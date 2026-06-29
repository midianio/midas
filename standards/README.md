# Standards

The conventions, by layer. Each rule is an **entry with a stable ID** (`SPEC.md §6`) carrying two
orthogonal attributes:

- **Tier** — how it's enforced: `check` (mechanical → `midas check`) or `review` (semantic →
  human/agent review; `SPEC.md §8`).
- **Escape** — how it may be deviated from: `hard` (no deviation), `ledgered` (allowed if recorded in
  `midas.toml [deviations]`), `advisory` (recommended).

| Layer | Doc | Status |
| --- | --- | --- |
| L1 · Stack & tooling | [`stack.md`](./stack.md) | written |
| L2 · Backend (Rust) | [`backend/`](./backend/) + Go→Rust method in [`playbooks/`](./playbooks/) | written |
| L2 · Frontend (Svelte) | [`frontend/conventions.md`](./frontend/conventions.md) | written |
| L2 · CLI (Rust) | [`cli/conventions.md`](./cli/conventions.md) | written |
| L4 · Process & ops | [`process.md`](./process.md) | written |
| L5 · Agent playbook | [`agents.md`](./agents.md) | written |

## Seed convention catalog

The conventions extractable from the midian apps + the CLI/agent layers, as IDed entries. This is the
prose form of `registry/conventions.json` — the catalog `midas check` embeds and enforces. Not
exhaustive; promotion of new entries follows `SPEC.md §9`.

### Stack — `STK-`
| ID | Rule | Tier | Escape |
| --- | --- | --- | --- |
| STK-0001 | Backend = Rust (axum 0.8 · tokio · sqlx 0.8 · utoipa). | check | ledgered |
| STK-0002 | Frontend = SvelteKit 2 + Svelte 5 runes; package manager = Bun. | check | ledgered |
| STK-0003 | Native/PWA = Capacitor 8 via static-adapter switch. | review | ledgered (web-only) |
| STK-0004 | Primary DB = PlanetScale/Vitess (MySQL); migrations forward-only. | check | ledgered |
| STK-0005 | Auth = Clerk; telemetry = PostHog; deploy = Railway. | review | ledgered |

### Backend — `BE-` (full prose in [`backend/`](./backend/))
| ID | Rule | Tier | Escape |
| --- | --- | --- | --- |
| BE-0001 | Handlers are `async fn(State<AppState>, extractors…) -> Result<Json<T>, AppError>`; thin. | review | hard |
| BE-0002 | One wire envelope `{data,code,timestamp,count}`; helpers `response::ok/ok_list`. | check | hard |
| BE-0003 | Errors via one `AppError` enum + `IntoResponse`; never leak internals; details logged. | check | hard |
| BE-0004 | Auth via the `RequireAuth` extractor; no fallback identity (any failure → 401). | check | hard |
| BE-0005 | Authz via the central `access::require` seam; never scatter `WHERE user_id = ?`. | check | hard |
| BE-0006 | Feature gating via `RequirePlan` + `usage::guard`/`Pass`; no hand-rolled per-handler checks. | check | hard |
| BE-0007 | Forward-only migrations; never edit an applied migration in place. | check | hard |
| BE-0008 | camelCase on the wire, snake_case in code (`#[serde(rename_all="camelCase")]`). | check | hard |
| BE-0009 | Opaque columns stay opaque (`Option<String>`/`Value`); never strict-type passthrough JSON. | review | hard |
| BE-0010 | Outbound HTTP only through the pooled `Http` seam; never `reqwest::Client::new()` in a handler. | check | hard |
| BE-0011 | Background work via the `Tasks` tracker (awaited on shutdown); not bare `tokio::spawn`. | check | hard |
| BE-0012 | Logs via `tracing::{info,warn,error}!`; clippy denies `print_stdout`/`print_stderr`. | check | hard |
| BE-0013 | Telemetry only through the vendor-neutral ports (`st.telemetry.*`); never a raw vendor SDK call. | check | hard |
| BE-0014 | API contract is generated (utoipa → OpenAPI → TS); no central registry to drift. | check | ledgered |
| BE-0015 | SSE is byte-exact (`event:`/`data:` framing, 15s heartbeat, five headers, no `[DONE]`). | check | hard |
| BE-0016 | IDs via `ids::generate()`; don't inline `uuid`. | check | hard |
| BE-0017 | Resilient boot: liveness independent of DB (`/ping` works without a pool); ordered graceful shutdown. | review | hard |
| BE-0018 | Prefer compile-checked `query!`/`query_as!`; commit the `.sqlx` offline cache (artifact-commit overlaps OPS-0003). | check | ledgered |
| BE-0019 | No N+1: batch-hydrate computed/related fields via one grouped `IN (…)` query. | review | hard |

### Frontend — `FE-` (full prose in [`frontend/conventions.md`](./frontend/conventions.md))
| ID | Rule | Tier | Escape |
| --- | --- | --- | --- |
| FE-0001 | Global state = class-based runes singleton per domain, one exported instance, in `lib/state/<d>.svelte.ts`. | check | hard |
| FE-0002 | `$state` for source-of-truth, `$derived` for anything computable. | review | advisory |
| FE-0003 | Reactive collections use `SvelteSet`/`SvelteMap`/`SvelteDate`, never plain `Set`/`Map` in `$state`. | check | hard |
| FE-0004 | One codebase; native/PWA via the `CAPACITOR_BUILD` adapter switch + `cap:build:*` scripts. | check | ledgered (web-only) |
| FE-0005 | All backend calls through the typed `api<T>()` wrapper; never `fetch()` a backend route directly. | check | hard |
| FE-0006 | API types generated from OpenAPI; hand-written types only for client-only shapes. | check | ledgered |
| FE-0007 | Content nav = the pane system (`openTarget`/url-codec/registry), not `goto()` routing. | review | ledgered |
| FE-0008 | Cross-singleton communication is a direct method call, not a global DOM event. | check | advisory |
| FE-0009 | No fetch/mutation/orchestration logic in components — it lives in `state/`. | review | hard |
| FE-0010 | IDs via `generateId()`, never raw `crypto.randomUUID()`. | check | hard |
| FE-0011 | UI primitives in `components/ui/`, variants via `tailwind-variants`, class-merge via `cn()`. | review | advisory |
| FE-0012 | Platform detection through `utils.ts`/`screen.svelte.ts`, not ad-hoc userAgent sniffing. | check | advisory |

### CLI — `CLI-` (full prose in [`cli/conventions.md`](./cli/conventions.md))
| ID | Rule | Tier | Escape |
| --- | --- | --- | --- |
| CLI-0001 | Agent-runnable: non-interactive by default; every prompt has a flag path; no-TTY never prompts. | check | hard |
| CLI-0002 | Dual output: `--json` with a stable, documented schema on every data-returning command. | check | hard |
| CLI-0003 | stdout = data, stderr = logs/progress/prompts. | check | hard |
| CLI-0004 | Typed, documented exit codes (`0`/`1`/`2`/`3`/`4`). | check | hard |
| CLI-0005 | Built on one internal CLI contract kernel (`cli/src/core/`, clap derive); not re-implemented per command. | check | hard |
| CLI-0006 | Project config read through the shared loader from `midas.toml`; no secrets in argv. | review | ledgered |
| CLI-0007 | Single static binary (musl/rustls); the binary embeds its standard version. | review | advisory |
| CLI-0008 | Snapshot-tested surface (`assert_cmd`/`trycmd`): output, `--json` schema, exit codes. | check | hard |
| CLI-0009 | Logs via `tracing` to stderr; no `print!`/`eprintln!`; never emit secrets/PII. | check | hard |
| CLI-0010 | Noun-first subcommand grouping, kebab-case, complete `--help`. | review | advisory |

### Process & ops — `OPS-` (full prose in [`process.md`](./process.md))
| ID | Rule | Tier | Escape |
| --- | --- | --- | --- |
| OPS-0001 | Release/branch flow via midflow (now `midas flow`): PR → review → merge; dev/main split; hotfix path. | review | ledgered |
| OPS-0002 | Quality gates green before merge (build/test/lint/typecheck per stack). | check | hard |
| OPS-0003 | Generated artifacts (`.sqlx`, OpenAPI, TS client) regenerated & committed (CI drift guard). | check | hard |
| OPS-0004 | Destructive prod data ops are handed to a human with exact commands, not run by tooling/agents. | review | hard |
| OPS-0005 | One-command bootstrap (`midas setup` owns install → tunnel → doctor). | check | advisory |
| OPS-0006 | Local dev = pscale proxy + dotenv chain (`ENV_FILE`→`.env`→`.env.local`); midflow owns the `.env.local` tunnel block — don't hand-edit. | check | hard |
| OPS-0007 | Branch `<type>/<slug>` off `dev` (trunk); `main` = prod; tags semver. | check | ledgered |
| OPS-0008 | Migrations forward-only, numbered `NNN_`, one DDL set/file, fix-forward. *(cross-ref BE-0007 — same rule, process view)* | check | hard |
| OPS-0009 | Schema→prod only via a reviewed PlanetScale deploy request; never run `migrate` at prod. | review | hard |
| OPS-0010 | Squash-merge to `dev`; risk-tiered review (features/schema/auth/payments wait). | review | advisory |
| OPS-0011 | Husky + lint-staged pre-commit not bypassed (`--no-verify`). | check | hard |
| OPS-0012 | Never commit `.env.local`/secrets; never force-push `main`/`dev`. | check | hard |
| OPS-0013 | Native ships via manual fastlane dispatch; static SPA via `CAPACITOR_BUILD`. | review | ledgered (web-only) |

### Agent playbook — `AGT-` (full prose in [`agents.md`](./agents.md))
| ID | Rule | Tier | Escape |
| --- | --- | --- | --- |
| AGT-0001 | The version-stamped `midas` managed block is present + current in `CLAUDE.md`/`AGENTS.md`/`.cursor`. | check | hard |
| AGT-0002 | Scaffolding goes through `midas add`, never hand-rolled. | review | hard |
| AGT-0003 | `midas check` is clean (or ledgered) before a PR. | check | hard |
| AGT-0004 | On conflict between a stale local doc and the pinned standard, the standard wins. | review | hard |
| AGT-0005 | Use the seams the conventions name; don't reach around them. | review | hard |
| AGT-0006 | The semantic reviewer returns structured findings keyed to convention IDs. | check | hard |

> IDs are stable once published. Tier, status (`proposed`/`adopted`/`deprecated`), escape policy, and
> the enforcing check are mirrored in `registry/conventions.json` (embedded in the `midas` binary) —
> the form tooling reads. `OPS-` and `BE-` prose is being authored/lifted in Phase 0; IDs above are
> the contract those docs fill in.
