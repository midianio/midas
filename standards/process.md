# Process & Ops

The L4 layer: how a change goes from clone to production. **Extracted** from the midian repo — the
`midflow` Go CLI (`scripts/midflow/`), the `.github/workflows/`, the monorepo `turbo`/`bun` scripts,
and `db/`. Where a rule names the `midas` CLI (`setup`/`check`/`gen`), that command will own/enforce
the step a human or ad-hoc script does today.

Each rule carries a stable **OPS-####** id and an enforcement tier: **[check]** = mechanical (a
script/CI job can decide pass/fail) · **[review]** = semantic (a human judges it). Entries flagged
**[gap]** are the standard's target where the repo doesn't yet enforce it — stated so `midas` can
close the loop.

## Stack of record

Monorepo: **Bun** workspaces (`app/web`, `app/api`, `app/db-tunnel`) orchestrated by **turbo**
(`package.json:5-10`, `turbo.json`). Backend is **Rust/axum** at `app/api` — the port from the earlier
Go/Chi backend is complete and merged; `app/api/package.json:6` runs `cargo run --bin server`. DB is
**PlanetScale/Vitess (MySQL)**, reached locally through a `pscale` tunnel. Native via **Capacitor** +
**fastlane**.

## Setup & bootstrap

One-time, one path. `midas setup` will own this end-to-end (today it's three manual steps in
`README.md:41-50`).

- **OPS-0005 [review]** — Bootstrap is `scripts/setup.sh` → `bun install` → installing the `midas`
  binary → `midas doctor`. `setup.sh` registers the `merge.ours.driver` git driver so conflicts in
  generated `db/gen/**` are resolved by regenerating, never hand-merged (`scripts/setup.sh:6-8`).
  *`bun run setup` is a different thing* — it's `bun install && turbo run build` (`package.json:12`),
  the deps+build step, not the git/tooling config.
- **OPS-0005 [review]** — `midas doctor` is the readiness gate: `git`/`gh`/`go`/`pscale` on PATH,
  `gh`/`pscale` authed, `$GOPATH/bin` on PATH, git identity set (`internal/cmd/doctor.go:40-50`). It
  probes the *active* gh account via `gh api user`, not `gh auth status`, because the latter fails on
  any stale configured account even when the active token works (`doctor.go:104-123`). `midas check`
  should subsume these probes.
- **OPS-0012 [check]** — Required dev keys are filled into the gitignored `app/web/.env.local` from
  1Password (`README.md:52-72`); the MySQL URL is **not** among them — midflow injects it
  (OPS-0006). Never commit `.env.local`; rotate immediately on any leak (`README.md:218-219`).

## Local dev

- **OPS-0006 [check]** — Dev DB is a `pscale connect application dev --port 3309` tunnel on
  `127.0.0.1:3309` (`internal/flow/config.go:9-22`; `app/api/scripts/parity.sh:39`). `bun run dev`
  fans out web + api + tunnel via turbo (`package.json:14`, `turbo.json:33-37`); the api's `dev`
  script blocks on port 3309 before booting (`app/api/package.json:6`).
- **OPS-0006 [check]** — `midas flow` **owns** the connection string. `flow start`/`flow end` write
  `MYSQL_DATABASE_URL` into `app/api/.env.local` between `# >>> midas >>>` / `# <<< midas <<<`
  markers (extracted from `internal/flow/env.go:14-64`), scoped to the active branch. Never hand-edit
  that block — it's rewritten on every `flow start` and stripped on `flow end`.
- **OPS-0006 [check]** — Env load order is first-wins and never overrides a real process var:
  `ENV_FILE` → crate `.env` (committed dev creds) → `.env.local` (the midas tunnel block)
  (`app/api/src/config.rs:101-112`). The committed `.env` is the source of dev secrets; `.env.local`
  only carries the per-branch DB URL. Docker injects real env instead (`docker-compose.yml:23-27`).
- **OPS-0006 [check]** — One `MYSQL_DATABASE_URL` serves Rust *and* the Go `db/` tooling: it's the
  go-sql-driver DSN form (`tcp(host:port)/db?…`), and `config.rs::normalize_mysql_dsn` converts it to
  the `mysql://…?ssl-mode=…` sqlx needs (`app/api/src/config.rs:167-206`). A DSN with no `tls=` →
  `ssl-mode=disabled` (what the tunnel wants).

## Release & branch flow — `midas flow`

The flow is a CLI, not a wiki page: `start` → commit → `sync` → `pr` → squash-merge → (promote)
→ `tag`. **`dev` is the integration trunk** — every feature PRs into it; `main` is production
(`README.md:122`). Inside midflow this is one constant: `git.MainBranch = "dev"`
(`internal/git/git.go:11`), so "main" in the CLI's own help text means `dev`.

- **OPS-0007 [check]** — Branches are `<type>/<slug>` cut off `origin/dev`, type ∈
  `feat | fix | chore | docs | spike` (`internal/cmd/start.go:15,187-188`). Slugs are lowercased,
  hyphenated, ≤60 chars (`internal/flow/state.go:71-92`). `start` refuses on a dirty worktree
  (`start.go:96-98`). `midas flow` reimplements this faithfully (its defaults reproduce midflow's).
- **OPS-0001 [review]** — Go through `midas flow` for branch/PR/sync/tag; don't hand-roll the git dance.
  `sync` = `fetch --prune` + `rebase origin/dev` + `push --force-with-lease` (with confirm), and
  prints conflict guidance instead of leaving you stranded (`internal/cmd/sync.go:33-87`). `pr` =
  `gh pr create --base dev` with the what/why/test-plan template prefilled, title defaulting to the
  last commit subject (`internal/cmd/pr.go:13-86`); it refuses to PR from `dev`/`main`
  (`pr.go:40-41`).
- **OPS-0007 [check]** — `feat`/`fix` (and `hotfix`) default to a **paired pscale branch seeded from
  `dev`** via Data Branching (`pscale branch create --seed-data --wait`); `chore`/`docs`/`spike` are
  git-only and the tunnel hits shared `dev` (`start.go:56-63,168-181`; `internal/flow/pscale.go:56-68`).
  `--with-data`/`--no-data` override. Seeded branches inherit parent cluster size (PS-10 min) and
  cost money — end them (OPS-0009).
- **OPS-0001 [review]** — **Hotfix** path is `midas flow start fix <slug>` — a `fix/` branch off
  `dev` (and a fix branch gets a seeded paired pscale branch by default). For a fire so urgent
  PR-and-merge is too slow, revert the bad commit on `main` with a *new* commit and tell the team —
  never force-push (`README.md:204-211,218`).
- **OPS-0010 [review]** — Squash-merge to `dev`; the squash subject reads as a changelog line. Review
  is risk-tiered: features / schema / auth / payments / data-writes **wait for review**; a fix-with-a-
  test or dep bump may self-merge after 24h of silence; docs/scaffold self-merge (`README.md:187-201`).
- **OPS-0007 [review]** — Release tags are semver `vX.Y.Z` cut from a clean trunk and pushed after
  confirm (`internal/cmd/tag.go:12,29-38,80-89`), used to drive native builds. *Note the code cuts from
  `git.MainBranch` (= `dev`) while `README.md:172` says "from main" — reconcile before relying on it.*

## DB & migrations

- **OPS-0008 [check]** — Migrations are **forward-only**, numbered `NNN_short_name.sql` in
  `db/migrations/`, applied in lexical order by `midas migrate` (and automatically by `midas dev` once
  the tunnel is up). The runner records each file in a `_migrations` ledger keyed by version with a
  SHA-256 **checksum**, so re-runs are no-ops and **editing an applied migration is rejected at runtime**
  (the BE-0007 guard). One DDL set per file, **no `BEGIN`/`COMMIT`** (Vitess forbids DDL-in-txn; the
  runner applies each file with `sqlx::raw_sql`, never wrapped in a transaction) — a mid-file failure
  leaves partial state and writes no ledger row, so **fix forward, never edit an applied file**.
  Scaffold with `midas touch migration <slug>`. Mirrors **BE-0007**. *(Cutover from the retired Go
  runner is automatic: on first run the runner adds the `checksum` column to an inherited `_migrations`
  table and backfills it trust-on-first-sight; no manual step.)*
- **OPS-0009 [review]** — Schema reaches an integration/prod branch only through a **PlanetScale deploy
  request reviewed in the PS UI** — never apply migrations directly to it. When a PR touches
  `db/migrations/**`, CI ensures the pscale branch, raises a short-lived `pscale connect` tunnel, runs
  `midas migrate` against `127.0.0.1` (the local-only guard holds — CI applies *through the tunnel*,
  never to a remote host), and opens/refreshes the DR. **DR approval is deliberately *not* cascaded
  from GitHub PR review** — it's a separate manual click. On merge, CI deploys the DR and deletes the
  pscale branch. The legacy Go `db/cmd/migrate` binary is retired; `midas migrate` is the single runner
  for both local dev and CI.
- **OPS-0009 [review]** — Clean up paired pscale branches: `midas flow end [--force]`. `--force` deletes
  the derived paired branch when it exists (nothing to delete on a git-only flow), and a hardcoded
  protected-set (`main`/`dev`/parent) can never be deleted by any code path
  (`internal/cmd/db.go:114-133`; `internal/flow/pscale.go:75-93`).
- **OPS-0004 [review]** — Destructive prod data ops (TRUNCATE/DELETE/DROP, manual deploys) are handed
  to a human with the exact commands — tooling and agents never execute them. `midas migrate` is
  dev/preview-only by construction (it refuses any non-loopback target); prod is DR-gated (OPS-0009).

## CI gates

Lint/format/typecheck runs on every PR and on push to `main` (`.github/workflows/lint.yml:3-6`).
`midas check` is the local mirror of this job set.

- **OPS-0002 [check]** — Frontend: `bun run format:check` (Prettier), `app/web` `bun run lint`
  (ESLint) + `bun run check` (svelte-check / strict TS) (`lint.yml:8-61`; `app/web/package.json:13,18`).
- **OPS-0002 [check]** — Go (`db/`): `gofmt -l` must be empty (`lint.yml:63-82`); also enforced
  pre-commit (OPS-0011).
- **OPS-0002 [check]** — Context lint: `scripts/context-scan.sh --ci` blocks if a canonical
  `AGENTS.md`/`SKILL.md`/`ARCHITECTURE.md` lacks `owner`/`last_reviewed`/`canon:true` frontmatter, or a
  nested `AGENTS.md` exceeds 80 lines (`lint.yml:84-90`; `scripts/context-scan.sh:1-20`).
- **OPS-0002 [check]** — Backend: a dedicated Rust workflow blocks on `cargo clippy --all-targets --
  -D warnings` then `cargo test`, run from `app/api` (`.github/workflows/api-rust.yml`). The crate
  also forbids `unsafe_code` and warns `clippy::all` at the source level
  (`app/api/Cargo.toml:17-22`) — the workflow is what actually gates a PR on it.
- **OPS-0002 / AGT-0003 [check]** — `midas` conformance: a `mechanical` job blocks on `midas check`
  then `midas sync --check` (agent docs current); a `semantic` job runs `midas check --json` and
  posts the review-tier convention set to the job summary for a delegated reviewer — advisory,
  `continue-on-error: true`, never blocks (`.github/workflows/midas.yml`).
- **OPS-0002 [check] [gap]** — `bun run test` (turbo: vitest + Playright) and the dual-adapter web
  build (`vite build` and `CAPACITOR_BUILD=1 vite build`) are not gated in CI today; the planned
  `plans/001-ci-test-gates.md` is where this lands.

## Generated artifacts — regenerate, commit, drift-guard

The contract is: anything generated from another source of truth is committed, and CI fails on drift.
`midas gen` will own producing them.

- **OPS-0003 [check]** — go-jet bindings (`db/gen/`) are checked in and **drift-guarded today**: after
  migrations land on `dev`, `db-codegen.yml` regenerates against the live `dev` schema and opens a PR
  if `db/gen/` changed (`.github/workflows/db-codegen.yml:13-17,73-106`). Commit a new migration **and**
  its regenerated bindings together (`db/README.md:133`).
- **OPS-0003 [check]** — The API contract is generated from the Rust handlers' `#[utoipa::path]`
  annotations: `cargo run --example export_openapi` → `openapi.json` (no DB/server needed), then
  `openapi-typescript` → the TS client (`app/api/scripts/gen-types.sh:14-20`;
  `app/api/examples/export_openapi.rs:9-15`). `midas check` (`artifact-hash`) mechanically requires
  both `openapi.json` and the generated TS client to be **committed** — tracked, not gitignored. This
  is the **FE-0006** producer. **[gap]** the byte-level regenerate-and-diff guard (same loop as
  `db/gen`, below) isn't wired into CI yet — only the commit-status half is enforced today.
- **OPS-0003 [check] [gap]** — sqlx is used in its **runtime** form (`sqlx::query`/
  `query_as::<_,T>`, no `query!` macros), so builds need no DB and no cache. The documented target
  (`plans/006-rust-backend-port.md:230-240`) is to adopt compile-time `query!` + commit the **`.sqlx`
  offline cache** (`cargo sqlx prepare`) so CI/Railway build without a DB; a schema change then means
  regenerate-and-commit the cache, drift-guarded like the above. (`BE-0018`, ledgered — the compiler
  enforces `query!` call sites are valid where they're used; it can't enforce that runtime `sqlx::query`
  was never chosen instead.)
- **OPS-0003 [check]** — Parity harnesses are dev tools, not committed artifacts: `parity.sh` boots the
  Go oracle + Rust server against the same dev tunnel, mints a Clerk token, and deep-diffs every route's
  JSON (`app/api/scripts/parity.sh`); `record-goldens.sh` captures Go responses into `tests/goldens/`.
  They source the sibling crate's `.env` for the Clerk secret (`parity.sh:30`).

## Testing

Full conventions live in `backend/`/`frontend/`; the process rules:

- **OPS-0002 [review]** — A new handler/module/business-logic change needs a test; a bug fix ships with
  the regression test that proves it (`README.md:192`, hotfix reminder `hotfix.go:13`). Reviewers may
  self-merge a *fix-with-a-test*; a feature waits (OPS-0010).
- **OPS-0002 [review]** — Mock the network edge, hit the real thing for contract truth. The Rust proof
  tests use a local Clerk keypair and `#[ignore]` the live-token check (`app/api/README.md:11-16`);
  cross-stack parity diffs run against the **real** dev DB via the pscale proxy, not a fake
  (`parity.sh`). Don't assert against a hand-rolled DB stub when a proxy to `dev` is one command away.

## Pre-commit, secrets, deploy

- **OPS-0011 [check]** — Husky pre-commit runs `lint-staged` → Prettier (+ `gofmt` on `*.go`)
  (`.husky/pre-commit`; `package.json:59-66`). Don't bypass with `--no-verify`; fix the lint or fix the
  hook in its own PR (`README.md:215`).
- **OPS-0012 [check]** — `.env`/`.env.*` are gitignored except `.env.example`/`.env.test`
  (`.gitignore:6-9`); the committed `app/api/.env` holds dev-only creds and is *not* tracked. Never
  force-push `main`/`dev` — revert with a new commit (`README.md:218`). `gh secret set` is how CI
  secrets land (`PSCALE_SERVICE_TOKEN_ID/SECRET` → `PLANETSCALE_*`: `db/README.md:178-193`).
- **OPS-0013 [review]** — Native ships through a **manual** fastlane run: `deploy-ios.yml` /
  `deploy-android.yml` are `workflow_dispatch`-only (push triggers commented out), set
  `CAPACITOR_BUILD=true`, and derive the build number from the commit count over full history
  (`deploy-ios.yml:3-21,47-76`). Local native rebuilds go through the `cap:build:*` scripts that set the
  flag (`app/web/package.json:22-24`) — never a bare `vite build` (mirrors **FE-0004**). The container
  path is `docker compose up --build` against multi-stage Dockerfiles (`docker-compose.yml`;
  `app/api/Dockerfile`).

## Deviation journal

- **OPS-0014 [review]** — Every `midas.toml [deviations]` entry has a **tracked path to
  resolution**, not just a reason frozen at ledger time. A `ledgered`/`advisory` escape records *why*
  a rule is violated right now (`BE-0018`'s reason cites the concrete migration blocker); the journal
  is where *when it gets fixed* lives, since `midas.toml` itself has no room for that. Reference
  implementation: midian's `plans/midas-conformance-journal.md` — one line per landed change or
  decision, a `MORNING TODO:` marker on anything deferred, safe to resume a session from git history
  plus that file alone. **In scope:** the journal-as-ledger-memory discipline. **Out of scope:** the
  overnight-unattended-loop machinery that happens to write it in midian — that's a workflow choice,
  not a convention; a repo can keep this journal by hand in a normal session. No mechanical check:
  whether an entry's resolution path is actually tracked (versus just asserted) is a judgment call for
  the reviewer, not a grep.

## Catalog (additions to `registry/conventions.json`)

OPS-0001..0004 are defined in `README.md`; this doc adds:

| ID | Rule | Tier | Escape |
| --- | --- | --- | --- |
| OPS-0005 | One-command bootstrap (`scripts/setup.sh`→`bun install`→midflow→`doctor`); `midas setup` owns it. | review | advisory |
| OPS-0006 | Local dev = pscale proxy `:3309` + dotenv chain (`ENV_FILE`→`.env`→`.env.local`); `midas flow` owns the `.env.local` tunnel block — don't hand-edit. | review | hard |
| OPS-0007 | Branch `<type>/<slug>` off `dev` (trunk); `main` = prod; tags semver `vX.Y.Z`. | review | ledgered |
| OPS-0008 | Migrations forward-only, numbered `NNN_`, one DDL set/file, no txn; fix-forward. (= BE-0007) | review | hard |
| OPS-0009 | Schema→prod only via PS deploy request reviewed in the UI; DR approval separate from GH review; never run `migrate` at prod. | review | hard |
| OPS-0010 | Squash-merge to `dev`; risk-tiered review (features/schema/auth/payments wait; low-risk self-merge after 24h). | review | advisory |
| OPS-0011 | Husky + lint-staged pre-commit not bypassed (`--no-verify`). | review | hard |
| OPS-0012 | Never commit `.env.local`/secrets; rotate on leak; never force-push `main`/`dev`. | check | hard |
| OPS-0013 | Native ships via manual fastlane `workflow_dispatch`; static SPA via `CAPACITOR_BUILD`; build no. = commit count. | review | ledgered (web-only) |
| OPS-0014 | Every `[deviations]` entry has a tracked path to resolution in a conformance journal. | review | advisory |

> IDs are stable once published. The **[gap]** entries (OpenAPI/TS regenerate-and-diff CI guard,
> `.sqlx` commit-and-guard, web test/build gates) are the standard's near-term target, not current
> enforcement — `midas check`/`gen` should close them.
