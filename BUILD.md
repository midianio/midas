# midas CLI — build progress

> Live build log for the `midas` binary. Maintained across autonomous loop iterations so any
> session can resume. Scope follows the grill decisions (handoff 2026-06-25, decisions #3–#7):
> deterministic scaffolding + **mechanical** `check` + ported `flow`. Deferred: `upgrade`/codemods,
> the in-binary Cursor semantic adapter (inverted — `check` is mechanical-only; an external agent
> invokes `midas check --json` + reads `standards/`), shared-package workspaces (vendor-with-provenance).

## Target (v1)

A Cargo workspace (one member) producing one static `midas` binary, built on its internal `core`
contract kernel.

```
midas/
├── Cargo.toml                 workspace (one member: cli)
├── cli/                       the one-stop `midas` binary
│   ├── build.rs               embeds registry/conventions.json + stamps version
│   └── src/{main, manifest, registry, core/*, flow/*, cmd/*, checks/*}
│       └── core/              agent-runnable contract kernel (CLI-0001..0005 by construction)
├── registry/conventions.json  machine-readable catalog (mirror of standards/README.md)
├── packages/                  graduated shared seams (empty until a seam earns it)
└── templates/                 scaffolding skeletons (Phase 2)
```

## Status

Legend: ✅ done · 🚧 in progress · ⬜ todo · ⏸ deferred

### Core kernel — `cli/src/core/` ✅ (compiles, clippy-clean)
- ✅ `global.rs` GlobalArgs (`--json/--yes/--quiet/--verbose/--no-color`)
- ✅ `output.rs` Output writer (data→stdout, logs/progress→stderr, `--json`)
- ✅ `exit.rs` CliError → exit codes 0/1/2/3/4
- ✅ `confirm.rs` + `prompt.rs` TTY-gated (non-TTY + no flag → exit 3)
- ✅ `config.rs` find-up + typed toml loader
- ✅ `style.rs` minimal ANSI (no dep)
- ✅ `log.rs` tracing → stderr

### Binary — `cli` (`midas`) ✅ (builds, runs, clippy-clean)
- ✅ `main.rs` clap root + dispatch + exit mapping
- ✅ `manifest.rs` typed `midas.toml`
- ✅ `registry.rs` embedded conventions.json model
- ✅ `flow/` ported midflow (git, gh, pscale, state, env, config)
- ✅ `cmd/flow.rs` start·sync·pr·hotfix·tag·db·doctor
- ✅ `cmd/check.rs` mechanical engine (verified vs live midian: clean; vs fixture: catches 3, exit 2)
- ✅ `checks/` banned-call · file-structure (artifact-hash/provenance/clippy = deferred → Skipped)
- ✅ `cmd/sync.rs` managed-block writer
- ✅ `cmd/doctor.rs` env diagnosis
- ✅ `cmd/add.rs` + `cmd/new.rs` scaffolding; `cmd/templates.rs` embedded skeletons
- ⏸ `cmd/gen.rs` TS types from OpenAPI

### Registry
- ✅ `registry/conventions.json` — full 60-entry catalog; **6 grounded mechanical checks** verified
  clean on live midian (FE-0001 file-structure, FE-0010/FE-0012 + BE-0010/0012/0016 banned-call).
  Grounding rejected BE-0011 (`tokio::spawn`, ~10 legit streaming uses) and FE-0003 (`new Set/Map`,
  mostly derived/dedup/consts) as false-positive-prone → correctly left `review`-tier.

### Tests ✅
- ✅ 16 `assert_cmd` tests: `--help`, `--version`, `--json` shape (no stdout log noise), exit codes
  0/2/3, doctor json, check clean/violation/ledgered, sync missing→present, add state/migration/module,
  new (incl. the rust-service skeleton + token substitution).

### Scaffolding ✅ (`midas add`)
- ✅ `add state <name>` → `lib/state/<name>.svelte.ts` (FE-0001 singleton; Pascal/camel derived)
- ✅ `add migration <slug>` → `db/migrations/NNN_<slug>.sql` (auto-numbered, OPS-0008)
- ✅ `add component <name>` → `lib/components[/ui]/<Pascal>.svelte` (Svelte 5 `$props`, FE-0011; `--ui`)
- ✅ `add module <name>` → `modules/<name>/{mod,model,service,handler}.rs` (BE-0001/0002/0004/0008 +
     utoipa) **and** idempotent `pub mod <name>;` wiring into `modules/mod.rs` (`--no-wire` to skip)
- ✅ `new <name> --profile app|service|cli|library|pipeline` → whole conformant project (`midas.toml`
     version-pinned, agent docs w/ synced block, starter CI, dir shape); refuses non-empty dir; the
     scaffolded project passes its own `midas check`. **Service profile also lays down the runnable
     `rust-service` skeleton** (below).
- ⬜ `add handler`/`pane`

### Templates ✅ (`rust-service` + `svelte-app`)
- ✅ `templates/rust-service/` — minimal **conformant, compiling** axum service, embedded via
     `include_str!` and laid down under `app/api/` by `midas new --profile service`. Distills the
     canonical seams: `response` (BE-0002 envelope), `error` (BE-0003 `AppError`), `ids` (BE-0016).
     Project tokens (`{{PKG}}`/`{{CRATE}}`) substituted at write time. **Verified end-to-end:** a
     generated project passes `midas check` (3 backend checks green) **and** `cargo build` + `clippy
     -D warnings` + runs (`/ping`→`pong`, `/hello`→ the envelope with a generated id).
- ✅ `templates/svelte-app/` — minimal **conformant, building** SvelteKit app (Svelte 5 runes,
     adapter-static), laid down under `app/web/` for `--profile app` (which also lays the backend).
     Distills the frontend seams: `state/app.svelte.ts` (FE-0001 runes singleton), `api.ts` (FE-0005
     typed wrapper), `utils.ts` `generateId` (FE-0010) + platform detection (FE-0012). `{{NAME}}`
     substituted. **Verified end-to-end:** a generated app passes `midas check` (6 checks, both
     layers) **and** `bun install` + `svelte-check` (0 errors) + `vite build`.
- Notes: sqlx (BE-0018) + utoipa/OpenAPI (BE-0014) on the backend, and auth/billing (Clerk, STK-0005)
  on both, are intentionally **TODO'd** in the starters so they build with no DB/codegen/keys;
  `cli-tool` template dropped (one-stop CLI).

### Reviewing (delegated semantic pass) ✅
- ✅ `standards/review-agent-prompt.md` — turnkey, vendor-neutral prompt operationalizing the inverted
     reviewer (AGT-0006): consumes `midas check --json` + `standards/`, outputs findings keyed to IDs,
     precision-over-coverage, advisory-not-blocking.

### Shipped & verified
- ✅ installed on PATH: `~/.cargo/bin/midas` (v0.1.0) — `midas <cmd>` works globally
- ✅ dogfood: repo has its own `midas.toml` (profile=cli, trunk=main); `midas check .` clean
- ✅ `.github/workflows/ci.yml` — fmt + clippy -D + test + `midas check` self
- ✅ `cargo fmt --check`, `cargo clippy -D warnings`, 17 tests — all green
- ✅ docs reconciled: SPEC/README/cli-README match built reality (inverted reviewer, mechanical-only
     gate exit 0/1/2/3, vendor-with-provenance default, upgrade/codemods deferred)

### Surface: flow · add · new (+ templates) · check · sync · doctor. Both code templates built + verified.
Remaining work needs a decision or touches another repo:
- **Template depth** — the `rust-service` + `svelte-app` starters are deliberately minimal. Growing
  them (sqlx + offline cache BE-0018, utoipa OpenAPI BE-0014, Clerk auth/billing STK-0005) needs
  DB/keys and a scope call. (A `cli-tool` template stays dropped: `midas` is the single one-stop CLI.)
- **`midas setup`/`teardown`** — midian-specific bootstrap (deps + pscale tunnel); needs the exact
  bootstrap steps to encode.
- **artifact-hash / provenance-drift / clippy** check kinds — registry carries them; engine reports
  `skipped`. clippy/artifact-hash add real gating but are stack-specific (deferred by the grill).
- **Wire `midas` into midian** — add a `midas.toml` + a `midas check` CI step to the midian repo
  (separate repo on `chore/rust-rewrite`; not touched autonomously).
- **First git commit** — this repo has none yet (held per commit policy; everything is ready).
- `add handler`/`pane`; `midas gen types`; `midas upgrade`/codemods (fleet-scale, deferred).

## Notes / decisions while building
- midflow config is midian-hardcoded; `midas flow` reads `[flow]` from `midas.toml` (defaults: org=midian, db=application, parent=dev, port=3309, region=us-east).
- Trunk branch in midflow = `dev` (`MainBranch="dev"`); README findings #1–#4 (007 plan) note doc/code drift — port faithfully, make trunk configurable.
- `feature/`/`feature-` prefixes in flow/config.go are dead constants; branch = `<type>/<slug>`, pscale = `<type>-<slug>`.
