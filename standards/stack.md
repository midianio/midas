# L1 · Stack & tooling

The canonical choices, so "what do we build with?" has one answer. Each is a recommended default with
a stated swap condition — the escape hatch. Deviation is `ledgered` (record it in `midas.toml
[deviations]` with a reason) unless marked `hard`.

## Canonical choices

| Concern | Choice | Why | May swap when… |
| --- | --- | --- | --- |
| **Backend language/framework** | Rust · axum 0.8 · tokio | Type-safety, one static binary, graceful-shutdown control, no GC pauses on streaming. Proven on the Go→Rust port. | The service is a throwaway spike or a function-as-a-service glue job where Rust's build cost isn't worth it. |
| **DB access** | sqlx 0.8 (MySQL, `runtime-tokio`, `tls-rustls`), compile-checked `query!` + committed `.sqlx` cache | Compile-time query verification; rustls keeps the Docker image OpenSSL-free. | A project needs Postgres — keep sqlx, change the driver feature. |
| **API contract** | utoipa → OpenAPI → `openapi-typescript` | Contract is a generated artifact; frontend types can't silently drift from the backend. | Never for a service with a typed frontend client (that's the whole point). |
| **Frontend** | SvelteKit 2 + Svelte 5 **runes** | Fine-grained reactivity, small bundles, first-class static-adapter output for native/PWA. | A project is headless/CLI/no-UI. |
| **Package manager (JS)** | Bun | Fast installs + script runner; matches the existing toolchain. | CI constraint forces npm/pnpm. |
| **Styling** | Tailwind 4 (`@tailwindcss/vite`) + CSS-variable theming + `bits-ui`/shadcn-svelte primitives | Utility-first with a typed primitive layer; tokens as CSS vars. | — |
| **Native / PWA** | Capacitor 8 via static-adapter switch (`CAPACITOR_BUILD=1`) | One codebase → web, PWA, iOS, Android; no separate native app. | Web-only product (`ledgered`: FE-0004). |
| **Primary database** | PlanetScale / Vitess (MySQL) | Horizontal scale, branch-based schema workflow; migrations forward-only. | Relational needs that Vitess can't serve → Neki/Postgres; document the integrity trade-offs (no FK enforcement on Vitess). |
| **Auth** | Clerk (RS256 + JWKS, verified locally) | Managed auth, JWT verifiable at the edge without a round-trip. | Enterprise SSO requirement Clerk can't meet. |
| **Billing** | Clerk Billing (subscription state behind the `Billing` seam + `RequirePlan`/usage metering) | One vendor for identity *and* entitlement — the subscription is keyed to the same Clerk user, no second account system to reconcile. | A billing model Clerk can't express (usage-based invoicing, marketplaces) → a dedicated provider behind the same `Billing` seam. |
| **Telemetry** | PostHog (product analytics, error tracking, LLM obs, flags) behind vendor-neutral ports + OTLP for pillars | One vendor/one bill, but swap-safe by construction (adapter swap, not a refactor). | Per-capability swap (e.g. Sentry for errors) = a new adapter, not a stack change. |
| **LLM / data pipeline** | Dagster | Generation/heavy data work belongs in the pipeline; the API only *serves* generated data. | `hard` boundary — LLM generation does not belong in the request path of the serving backend. |
| **Deploy** | Railway (containers) | Simple container deploys; instant rollback by re-pointing. | Scale/compliance need a different host — the Dockerfile is portable. |
| **CI / repo** | GitHub + the midflow release flow | — | — |

## Cross-cutting rules

- **`hard` stack rules** (not ledgerable): LLM generation stays in the Dagster pipeline, never the
  serving backend (the "pipeline generates, Go/Rust serves" boundary); generated artifacts (`.sqlx`,
  OpenAPI, TS client) are committed and CI-drift-guarded.
- **rustls everywhere** (no OpenSSL/native-tls) so Alpine/musl and slim Docker images build clean —
  this constrains crate feature selection (e.g. OTLP uses `grpc-tonic` + `tls-webpki-roots`, not the
  reqwest/http-proto exporter).
- **A swap is a recorded decision, not a fork.** Swapping any `ledgered` choice means a `[deviations]`
  entry naming the convention ID and the reason — so `midas check` treats it as intentional, not drift.

## Versions

Pinned versions live in the `templates/` skeletons and the Cargo/npm manifests, not here — this doc
names the *choice*, the templates name the *version*, and `midas upgrade` moves versions forward. (Keeps
this doc from rotting every dependency bump.)
