# Backend Conventions

Portable conventions for a Rust · axum · sqlx service — the steady state for a **new or already-ported**
backend. State each rule from first principles; the product specifics are pluggable seam examples
*(midian: …)*, swappable without touching the convention.

Canonical examples live in `app/api/src/`. *Origin: parity-verified during midian's Go→Rust port;
porting an existing Go service is a separate method — see [`../playbooks/go-to-rust.md`](../playbooks/go-to-rust.md).*

Every rule carries a stable **`BE-####` ID** and an **enforcement tier**:
`[check]` = mechanically verifiable by `midas check` (banned-call grep / file-structure /
artifact-hash — clippy runs in CI, not in `midas`), `[review]` = semantic (needs a human or agent to
judge). This hub covers the stack, structure,
handlers, envelope, errors, DB, the `AppState` seams, JSON, auth, and quality gates, and links to the
four deep-dive docs:
[authorization](./authorization.md) · [feature-gating](./feature-gating.md) ·
[observability](./observability.md) · [openapi](./openapi.md).

## Stack

axum 0.8 · tokio · sqlx 0.8 (MySQL, `runtime-tokio`, `tls-rustls`) · jsonwebtoken · reqwest ·
thiserror · tracing (+ OpenTelemetry/OTLP) · utoipa · `clippy -D warnings`. **rustls everywhere**, no
OpenSSL/native-tls, so Alpine/musl and slim Docker images build clean — this constrains crate feature
selection (see [observability](./observability.md) for the OTLP exporter's `grpc-tonic`/`tls-webpki-roots`
choice).

## Project structure

Single crate, lib + bin, one module per feature:

```
src/{config,db,error,sse,auth,routes}.rs
src/modules/<feature>/{model,service,handler}.rs
src/main.rs                # boot: config → db pool → router → /ping → graceful shutdown
```

- `BE-0017` `[review]` **Resilient boot, ordered shutdown.** Liveness must not depend on the DB: start
  listening before the pool is confirmed ready (pool is `Option`, `/ping` answers without a DB), so a
  slow/blipping database doesn't fail the health check. Shutdown drains in order — HTTP first, then the
  task tracker, then telemetry flush + tracer shutdown, then the pool — so nothing in flight is lost.
  (Canonical: `src/main.rs`. Shutdown sequence detailed in [observability](./observability.md).)

## HTTP handlers

- `BE-0001` `[review]` **Handlers are thin.** Signature is
  `async fn(State<AppState>, extractors…) -> Result<Json<T>, AppError>`; the body validates input,
  calls one service function, and wraps the result in the envelope. Fetching, business logic, and
  orchestration live in `service.rs`, not the handler. (Canonical: `src/modules/notes/handler.rs`.)
- Path params use axum 0.8 syntax: `Path(id): Path<String>`, route `"/notes/notes/{id}"`.
- `BE-0004` `[check]` **Auth via the `RequireAuth` extractor.** Add it as a handler arg
  (`auth: RequireAuth`) to require an authenticated caller; it yields `{ user_id, sid }` and records
  `user_id` on the request span. **No fallback identity** — any failure is a 401, never an anonymous
  default. Never hand-parse the `Authorization` header in a handler. (See *Auth* below; canonical
  `src/auth/mod.rs`.)

## Response envelope

- `BE-0002` `[check]` **One wire envelope, every JSON endpoint.**
  `{ "data": …, "code": 200, "timestamp": "<RFC3339, local offset, second precision>", "count": N }`
  where `count` = list length for arrays, else 1. Build it only through the helpers — `response::ok(x)`
  (count 1) / `response::ok_list(v)` (count = len) — so the shape can't fork per handler. Canonical:
  `src/response.rs`. The *same* `ApiResponse<T>` struct backs the OpenAPI schema, so the contract
  can't lie about the bytes (see [openapi](./openapi.md)).

## Errors — never leak internals

- `BE-0003` `[check]` **One `AppError` enum (`thiserror`) + one `IntoResponse`.** All fallible paths
  return `Result<_, AppError>`; the single `IntoResponse` is the only place a status code is chosen.
  Generic error envelope `{ "status": <text>, "code": <n> }`: `unauthorized`/401, `not found`/404,
  `bad request`/400, `internal server error`/500. The client gets a generic message; the cause is
  logged with structured fields. `sqlx::Error::RowNotFound` → 404.
- `[review]` **Never put a token, secret, or PII in an error value** — error messages are logged and
  captured to telemetry. (Scrub policy in [observability](./observability.md).)
- Typed 402 bodies (`UsageLimit` / `PlanRequired`) are the one sanctioned bespoke shape, handled at the
  top of `into_response` (see [feature-gating](./feature-gating.md)).

## Database (sqlx)

Primary DB is MySQL *(midian: PlanetScale/Vitess)*; the managed layer pools server-side, so the app
pool stays lean.

- `BE-0018` `[check]` **Prefer compile-checked queries; commit the offline cache.** Use
  `query!` / `query_as!` (verified against the schema at build time) over runtime `query_as::<_, Row>`;
  commit the `.sqlx` offline cache (`cargo sqlx prepare`) so CI builds without a live DB. The committed
  cache is CI-drift-guarded — regenerate it on any schema/query change, same loop as the OpenAPI/TS
  artifacts.
- **Pool**: max 5 / min 2 / lifetime 120s / idle 30s — a tuned default for a server-pooled DB; raise
  only with evidence. DSN is a sqlx `mysql://` URL; local dev points at the DB proxy
  *(midian: `pscale connect … → mysql://root@127.0.0.1:<port>/<db>?ssl-mode=disabled`)*.
- `[review]` **Every list query has a deterministic total `ORDER BY`** — stable output and stable
  pagination; an unordered list is a latent flake.
- **Transactions** through the seam (`db::with_tx`, below), never a hand-managed begin/commit.

## Shared infra — use the `AppState` seams, never hand-roll

`AppState` carries the shared infrastructure so handlers reach it through `State<AppState>` — no
globals, no per-handler construction.

- `BE-0010` `[check]` **Outbound HTTP only through the pooled `Http` seam.**
  `st.http.execute(Tier::Fast|Llm|Stream, retries, |c| c.get(url)…).await` — one pooled client, a
  per-tier timeout, and retry/backoff on 429/5xx/transient; `st.http.raw()` for streaming bodies.
  **Never** `reqwest::get` or `reqwest::Client::new()` in a handler/module — no pool, no timeout (the
  unbounded-connection footgun). (Canonical: `src/http.rs`.)
- `BE-0011` `[check]` **Background work via the `Tasks` tracker.** `st.tasks.spawn(async move { … })`
  for fire-and-forget that must outlive the response — it is **awaited on shutdown**, so a deploy
  doesn't drop in-flight work. Bare `tokio::spawn` for must-finish work is the anti-pattern.
- `[review]` **Multi-statement invariants go through `db::with_tx`.**
  `db::with_tx(pool, async |conn| { … Ok(v) }).await?` — commits on `Ok`, auto-rolls-back on any `?`.
  Use it whenever a write has more than one statement that must all land or none.
- `BE-0016` `[check]` **IDs via `ids::generate()`** (UUIDv4) — never inline `uuid::…` calls, so ID
  generation stays one swappable function.

## JSON rules

- `BE-0008` `[check]` **camelCase on the wire, snake_case in code.**
  `#[derive(Serialize)] #[serde(rename_all = "camelCase")]` on every response DTO. The frontend contract
  is camelCase; Rust stays idiomatic. (The same DTOs feed the generated TS client — see
  [openapi](./openapi.md) and the frontend's generated-types rule.)
- `BE-0009` `[review]` **Opaque columns stay opaque.** Passthrough JSON / rich-text columns are
  modelled as `Option<String>` / `serde_json::Value`, never strict-typed — strict-typing drops unknown
  nodes on the next write = silent data loss. Parse only the fields you actually branch on (e.g. `tags`
  → `Vec<String>`). *(midian: note `content` is opaque TipTap rich-text; desk `params/appearance/
  points/anchors` are opaque JSON.)*
- **Epoch-ms timestamps are wire numbers.** A `bigint` epoch-millis column is JSON `i64`, not an
  RFC3339 string. (The envelope's own `timestamp` is the one RFC3339 string — that's the response
  metadata, not a model field.)
- **List vs. detail share one row struct.** List views omit heavy fields — `SELECT NULL AS content` so
  one `FromRow` serves both; the detail query selects the real column. Avoids a second DTO that can
  drift.
- `BE-0019` `[review]` **No N+1 — batch-hydrate computed/related fields.** A per-row count or join is
  one grouped query over an `IN (…)` set, not a query per row. *(midian: `savesCount`, `sharedWith`.)*
- **Omit empty optionals from the wire** — `#[serde(skip_serializing_if = "Option::is_none")]` /
  `"Vec::is_empty"` — so absent ≠ `null`/`[]` where the contract distinguishes them.

## Auth

- `BE-0004` `[check]` **Local JWT verification via the auth provider's keys.** *(midian: Clerk — RS256
  + JWKS.)* The `RequireAuth` extractor (`FromRequestParts`) reads the token from `Authorization:
  Bearer …` or the session cookie, verifies it against cached JWKS, and yields `{ user_id (sub), sid }`.
  Provider-specific verification config lives in one place *(midian/Clerk: disable `aud` validation;
  validate the issuer by prefix `https://clerk.` / `.clerk.accounts`)*. JWKS is cached in `AppState`
  and refetched only on a `kid` miss (key rotation) — never per request. No fallback identity → any
  verification failure is a 401.

## Authorization → [`authorization.md`](./authorization.md)

- `BE-0005` `[check]`/`[review]` **One central access seam; never scatter `WHERE user_id = ?`.** Gate
  every owned-resource handler through `access::require(pool, &auth.user_id, kind, &id, min_role)` —
  `Viewer` for reads, `Editor` for edits, `Owner` for delete/share. It returns the correct status
  (**404** missing-or-no-access, no existence leak; **403** visible-but-insufficient; 401 is upstream
  at `RequireAuth`). Service fetchers take `id` only — access is checked centrally, so the `user_id`
  filter is *out* of the query. The ban on scattered filters is `[check]`; choosing the right minimum
  role per handler is `[review]`. Globally-shared content is exempt *(midian: passage/strong/chunk
  insights — keyed by ref, not owned)*. Full design, status-code table, and the grants growth path in
  the linked doc.

## Feature gating → [`feature-gating.md`](./feature-gating.md)

- `BE-0006` `[check]` **Two seams, no hand-rolled per-handler checks.** Plan-gated routes take the
  `RequirePlan` extractor (paid-or-402, yields `user_id`). Metered actions go
  `guard → do work → pass.commit`, where the `#[must_use]` `Pass` makes "increment once, after success,
  free-users-only" structural. 402 bodies are typed on `AppError`. Failure policy and the metering
  table in the linked doc.

## SSE — byte-exact

- `BE-0015` `[check]` **Frame the stream by hand, to the byte.**
  `event: <type>\ndata: <json of the Data payload only>\n\n`. **No `[DONE]` sentinel** — completion is
  `event: done`. `: heartbeat\n\n` every 15s. Five headers: `text/event-stream`;
  `no-cache, no-transform`; `keep-alive`; `X-Accel-Buffering: no`; `X-Content-Type-Options: nosniff`.
  Write the raw stream (not axum's `Sse` helper) so the bytes are guaranteed. (Canonical: `src/sse.rs`.)

## Observability → [`observability.md`](./observability.md)

- `BE-0012` `[check]` **Logs via `tracing::{info,warn,error}!` with structured fields** — never
  `println!`/`eprintln!`/bare `log`. `midas check` bans the print macros by grep; CI's
  `cargo clippy -- -D warnings` denies `print_stdout`/`print_stderr` independently. Each request
  already carries `request_id` + (post-auth) `user_id` on its span, inherited automatically.
- `BE-0013` `[review]` **Telemetry only through the vendor-neutral ports** (`st.telemetry.*` + the OTLP
  pillars) — never a raw vendor SDK call in a handler. Swapping a capability is an adapter swap, not a
  refactor. *(midian: PostHog Cloud behind the ports; LLM usage rides the AI-client choke point.)*
- `[review]` **PII scrubbed at the emit boundary** (allowlist, not blocklist); telemetry is fail-open
  and non-blocking. Full two-tier design in the linked doc.

## API contract → [`openapi.md`](./openapi.md)

- `BE-0014` `[check]` **The contract is a generated artifact.** `utoipa` derives the OpenAPI spec from
  each handler's `#[utoipa::path]` + `ToSchema` DTOs; `utoipa-axum`'s `OpenApiRouter` auto-collects
  paths *and* schemas, so there's no central registry to drift. Generate the spec
  (`cargo run --example export_openapi`) and the TS client (`scripts/gen-types.sh`); both are committed
  and CI-drift-guarded. Served live at `/openapi.json`. (`ledgered`.)

## Quality gates

- `cargo build` · `cargo test` · `cargo clippy -- -D warnings` all green, every change.
- `BE-0012` Clippy denies `print_stdout`/`print_stderr` — logging stays on `tracing`.
- Generated artifacts (`.sqlx`, `openapi.json`, the TS client) regenerated and committed; CI fails on a
  dirty tree (drift guard) — same loop as the frontend's generated API types.
- `BE-0007` `[check]` **Forward-only migrations.** Schema changes ship as new numbered files; never
  edit an applied migration in place, even on a spike branch — an edited migration diverges between
  environments that already applied the old file.
- **Porting an existing Go service?** Add the live-diff parity gate on top of these — see
  [`../playbooks/go-to-rust.md`](../playbooks/go-to-rust.md).
