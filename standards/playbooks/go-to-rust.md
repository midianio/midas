# Playbook — Go → Rust at parity

How any @midian Go service ports to Rust without a behaviour change the frontend can see. The target
shape is the steady-state backend convention set in [`../backend/`](../backend/) — this playbook is the
**method** that gets an existing Go service there safely: a standards-first, escalating-chunk rewrite
gated by one cheap, ruthless quality check (live-diff vs. the Go oracle), ending in a full swap.

Proven on midian's `app/api` (~127 routes). It is re-runnable: a sibling Go service *(e.g. prayer)*
ports by following the same loop, pointing the route lists and DSN at its own terms.

Full swap at the end — **no gateway, no shadow/canary.** Those are scale-up concerns; for a beta-scale
service the live-diff gate plus an instant-rollback deployable is enough.

## Setup (once)

- **DB**: connect to a dev branch through the project's DB proxy — one DSN both runtimes share.
  *(midian: `pscale connect application dev --port 3309` →
  `mysql://root@127.0.0.1:3309/application?ssl-mode=disabled`.)* Note the Go DSN form (go-sql-driver)
  must be converted to a sqlx `mysql://` URL; in the new service you write `mysql://` directly.
- **Go oracle**: run the existing Go service locally against the *same* proxy DSN, on a different port
  *(midian: `app/api`, env from `app/api/.env`, port 8081)*. This is the parity oracle for the
  live-diff gate.
- **Token**: mint a test token from the auth provider for a dev user that has data
  *(midian: Clerk — `POST /v1/sessions {user_id}` → `POST /v1/sessions/{id}/tokens`,
  `Content-Type: application/json`)*. Both runtimes verify the same token.

## Per-chunk loop

0. **Seam check.** Before porting routes, stand up any cross-cutting seam this chunk *triggers* — build
   the not-yet-built ones, apply the already-built ones — so every route in the chunk inherits it. This
   is how deferred cross-cutting work is picked up at the moment a route first needs it, instead of
   retrofitted. (Seam designs live in [`../backend/`](../backend/): the `Http`/`Tasks`/`with_tx`/`ids`
   seams in `conventions.md`, `access::require`, `RequirePlan`/`usage::guard`, telemetry, OpenAPI.)
1. Pick the next chunk off the **escalation ladder**.
2. For each route: read the Go handler + service + schema → write Rust (model / service / handler) per
   [`../backend/conventions.md`](../backend/conventions.md).
3. Wire routes.
4. **Parity gate (live-diff).** Hit Go and Rust back-to-back — same token, same dev DB — for each
   route; **deep-equal** the JSON (parse both to `serde_json::Value` and `assert_eq!`; object equality
   is order-insensitive, strict on presence/type/value) and assert the status codes match. SSE is
   **byte-exact**. **Writes**: read-back diff — write via Rust then read the row, write via Go then read
   the row, compare; double-writing on dev is safe.
5. `cargo test` + `cargo clippy -- -D warnings` green.
6. Commit.

Escalate only after the prior chunk is fully parity-green — each rung trusts the standards more, so
agents can run wider.

## The parity gate — what "equal" means

The Go service is the oracle; the Rust port matches its observable bytes, then improves under a
documented deviation only. Rules that make the diff trustworthy:

- **Match the Go query's exact `WHERE`/`ORDER BY`, even where Go looks wrong.** If Go's `ListAll`
  doesn't filter `deleted_at` and orders `updated_at DESC`, the port does the same. **Parity beats
  intuition** — a "fix" that changes a byte fails the gate. Genuine Go bugs get reproduced first, then
  fixed as a *separate, documented deviation* (below), never silently folded into the port.
- **The wire envelope mirrors the Go response helper** *(midian: `lib/utils/response.Ok` →
  `{data,code,timestamp,count}`)* — same fields, same `count` semantics. The Rust helper
  (`response::ok`/`ok_list`, `BE-0002`) is verified against it.
- **Match Go's `omitempty`** with `#[serde(skip_serializing_if = "Option::is_none")]` /
  `"Vec::is_empty"`, field by field — an extra `null` or `[]` is a diff.
- **Whole-float JSON.** Go's `encoding/json` emits a whole `float64` with no trailing `.0`; serde
  appends `.0`. Normalize wire `f64`s (a `serialize_go_f64` helper) **or** mask the difference in the
  diff — pick one and apply it everywhere a float crosses the wire.

### Diff masking

Per-request, non-deterministic fields are masked before the deep-equal, or the gate never passes:

- The envelope **`timestamp`** (per-request RFC3339) — the default mask.
- Freshly **generated IDs** on write responses.
- Whole-float formatting, if you mask rather than normalize (above).

Keep the mask list short and explicit — every masked field is a field you are *not* checking, so add
one only when it's provably non-deterministic.

### Live oracle vs. golden recordings

The live dual-run (Go + Rust side by side) is the primary gate. Where keeping Go running is
inconvenient — post-cutover regression checks, CI — **record Go's responses to golden fixtures** once
and diff Rust against the recordings. Same masks apply. Live-run while a slice is in flux; golden once
it's stable.

### Documented deviations

A port is allowed to *improve* on Go only as an explicit, recorded deviation — never as an accidental
byte change:

- **Status-code fixes while centralizing authz.** Go muddles codes (e.g. 500 on a missing session);
  the port returns clean 401/403/404 through `access::require`. Note it as a deliberate write-path
  deviation as each write slice ports — reads stay parity-clean (Go's `WHERE id=? AND user_id=?`
  already returned 404 for a non-owner). See [`../backend/authorization.md`](../backend/authorization.md).
- **New functionality is exempt from parity by definition** — a feature that doesn't exist in Go has no
  oracle. Build it to the conventions, not the gate *(midian: `resource_grants` sharing — the first
  post-cutover Rust feature, built on the `access` seam)*.

## Escalation ladder

Chunk size grows as the standards earn trust; each chunk names the **seam triggers** loop-step 0 must
satisfy first.

- **Chunk 1 — by hand, sets the standards** (~4 read routes). The first slice is written carefully and
  becomes the worked example that validates `../backend/conventions.md` against reality. *(midian:
  `notes-read` ✅ · user read · pins read · wiki read.)*
- **Chunk 2 — full simple modules, incl. writes.** *(midian: `notes`, `pins`, `user`,
  `activity/visits`.)* **Seams:** build the validation extractor; apply `access::require` (writes) ·
  `db::with_tx` · `usage::guard` (metered writes) · `Tasks::spawn` · `ids::generate`.
- **Chunk 3 — read-heavy domain, parallel agents** (each following the conventions + gate). *(midian:
  read, library, wiki, search, social, journey, desk, communications.)* **Seams:** `RequirePlan`
  (premium route groups) · `access::require` (more resource kinds) · rate-limiting for
  public/optional-auth reads.
- **Chunk 4 — hard / streaming, parallel.** *(midian: `ai` + the SSE routes, chats, billing, sharelink,
  webhooks.)* **Carve-outs** that are visual/proxy-equivalent rather than byte-identical
  *(midian: OG-image PNG, TTS audio)* — diff the shape, not the bytes. **Seams:** build caching
  (embeddings) + circuit-breaker (`http`); apply `usage::guard` (AI) · `Tasks::spawn` (async events) ·
  `Billing::invalidate` (webhooks) · `http.execute(Tier::Llm|Stream)`.

## Cutover

All routes parity-green → deploy the Rust service → point the deploy target at it
*(midian: Railway)* → **delete the Go service.** Keep Go deployable but unrouted for a short window for
instant rollback — because it's a full swap, no per-route gateway is needed.

## Reuse

The target conventions ([`../backend/`](../backend/)) are project-agnostic. This playbook is the
re-runnable method: another @midian Go service ports by swapping in its own route lists, DSN, and
premium route groups, and following the same loop, gate, and ladder.
