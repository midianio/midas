# API Contract (OpenAPI) — generated, drift-free

The API contract is a **generated build artifact**, not tribal knowledge, and the frontend's
TypeScript types are generated from it — so the frontend contract can't silently drift from the
backend. This is `BE-0014`.

**Stack:** [`utoipa`](https://docs.rs/utoipa) (derive the spec from handler annotations) +
[`utoipa-axum`](https://docs.rs/utoipa-axum) (`OpenApiRouter` — auto-collects paths *and* schemas) →
[`openapi-typescript`](https://www.npmjs.com/package/openapi-typescript) (spec → TS client). Canonical:
`src/openapi.rs`.

## Why it's drift-free by construction

`utoipa-axum`'s `OpenApiRouter` reads each route's path, params, and referenced schemas from the
handler's own `#[utoipa::path]` — there is **no central registry to forget to update**. `src/openapi.rs`
holds only top-level metadata + the security scheme; routes self-register via `routes!(...)`. The single
source of truth for a route's shape is the handler next to it, and the envelope schema (`ApiResponse<T>`)
is the *same struct* that serializes on the wire (`response.rs`, `BE-0002`) — so the doc cannot lie
about the bytes.

## Per-route checklist (every slice)

1. `[check]` **DTOs derive `ToSchema`** — on the response structs and any nested structs. serde attrs
   are honored, so `#[serde(rename_all = "camelCase")]` → camelCase schema properties (matches the
   wire, `BE-0008`). Opaque columns stay `Option<String>`/`serde_json::Value` and document as
   `string`/free-form — never strict-typed (`BE-0009`).
2. `[check]` **Annotate the handler** with `#[utoipa::path(...)]`: `method`, `path` (OpenAPI `{id}`
   form, identical to the axum route), `tag`, `operation_id` (camelCase → clean TS method names),
   `security(("clerk_jwt" = []))` for authed routes, `params(...)`, and `responses(...)`.
   - Success body: `body = inline(ApiResponse<T>)` (list: `inline(ApiResponse<Vec<T>>)`). `inline`
     keeps the generic envelope working without alias boilerplate.
   - Document the real error statuses with `body = ErrorBody` (`{status, code}`). The typed 402 bodies
     (see [feature-gating](./feature-gating.md)) are bespoke — document those explicitly where the
     route can return them.
3. `[check]` **Register it** in `openapi::router()` via `routes!(module::handler::fn_name)`. That's the
   only wiring step.

The security scheme name *(midian: `clerk_jwt`)* tracks the auth provider; it's metadata, defined once
in `src/openapi.rs`.

## Generating the spec / TS client

- Spec only (no DB, no server — built from types): `cargo run --example export_openapi` → `openapi.json`.
- Spec **+** TS types: `scripts/gen-types.sh [out.ts]` (export, then `openapi-typescript`).
- Live spec served at **`GET /openapi.json`**.

## CI drift guard

`[check]` Run `cargo run --example export_openapi` in CI and fail if the committed `openapi.json`
changed but wasn't regenerated — the same loop as the committed `.sqlx` cache (`BE-0018`). Optionally
regenerate the TS client and fail on a dirty tree, so a Rust handler change that breaks the frontend
contract is caught at PR time, not in the browser.

## Optional: a docs UI

Intentionally **not** wired to a CDN (a `<script src=…>` without SRI is a supply-chain exposure, and a
docs page served from the API origin shouldn't pull third-party JS). For an interactive UI, add the
`utoipa-swagger-ui` crate — it **vendors** the assets into the binary at build time (no runtime CDN,
nothing to hash) — and mount it at `/docs`.
