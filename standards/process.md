# Process & Ops

The L4 layer: how a change goes from clone to production. **Extracted** from the midian repo — its
`.github/workflows/`, the Bun workspace scripts, `db/`, and `README.md` — and from the `midas` CLI
itself (`cli/src/`), which now owns the flow. The `midflow` Go CLI that originally carried these
rules is **retired**; `midas flow` replaces it entirely, and no midian repo uses Go any more. Where a
rule names a `midas` command, `check` · `doctor` · `flow` · `dev` · `migrate` · `touch` · `sync` ·
`drift` · `explain` · `conventions` · `deviate` · `adopt` ship today; `setup` and `gen` are named
targets that do **not** exist yet — they mark the step a human or ad-hoc script still does.

Each rule carries a stable **OPS-####** id and an enforcement tier: **[check]** = mechanical (a
script/CI job can decide pass/fail) · **[review]** = semantic (a human judges it). Entries flagged
**[gap]** are the standard's target where the repo doesn't yet enforce it — stated so `midas` can
close the loop.

## Stack of record

Monorepo: **Bun** workspaces (`app/web`, `app/api`, `scripts/store_assets`) with **no** task runner —
turbo is gone; `midas dev` orchestrates dev processes from `midas.toml [dev]`, and the root
`package.json` scripts are thin `bun run --filter=…` delegations. Backend is **Rust/axum** at
`app/api` — the port from the earlier Go/Chi backend is complete and merged, and the Go source is
deleted. DB is **PlanetScale/Vitess (MySQL)**, reached locally through a `pscale` tunnel. Native via
**Capacitor** + **fastlane**. There is no Go toolchain, no `go.mod`, no `db/gen/`, and no `gofmt`
gate anywhere in the stack.

## Setup & bootstrap

One-time, one path. `midas setup` will own this end-to-end (today it's three manual steps in
`README.md`).

- **OPS-0005 [review]** — Bootstrap is `scripts/setup.sh` → `bun install` → installing the `midas`
  binary → `midas doctor`. `setup.sh` is a repo-local hook for any git/tooling config a project
  needs; it may legitimately be a no-op (it is one in midian today). *`bun run setup` is a different
  thing* — the deps+build step, not the git/tooling config.
- **OPS-0005 [review]** — `midas doctor` is the readiness gate. Its actual probe set is: `git` on
  PATH, `gh` on PATH, `gh` authenticated, `git user.name`/`user.email` set, inside a git repo, agent
  docs carry the current midas managed block (AGT-0001's local face), and `pscale` authenticated
  (`cli/src/cmd/doctor.rs`). The pscale probe is **warn**, not fail — everything else fails the gate.
  It probes the *active* gh account via `gh api user`, not `gh auth status`, because the latter fails
  on any stale configured account even when the active token works. `--fix` remediates only the
  fixable subset (today: re-syncing a missing/stale agent-docs block). It does **not** check for a Go
  toolchain or `$GOPATH/bin` — there is no Go in the stack. `midas check` should subsume these probes.
- **OPS-0012 [check]** — Required dev keys are filled into the gitignored `app/web/.env` and
  `app/api/.env` from 1Password (`README.md`, "Env files"); the MySQL URL is **not** among them —
  `midas flow` injects it (OPS-0006). Never commit an env file; rotate immediately on any leak
  (`README.md`, "Things that will save you pain").

## Local dev

- **OPS-0006 [check]** — Dev DB is a `pscale connect application dev --port 3309` tunnel on
  `127.0.0.1:3309`. The org/db/parent/port defaults live in `cli/src/flow/config.rs`
  (`FlowConfig::default`) and midian pins them explicitly in `midas.toml`'s `[flow]` block.
  `bun run dev` is `midas dev`, which reads `[dev]` in `midas.toml`: raise the tunnel → apply pending
  migrations (`migrate = true`) → run the `api` and `web` processes with watch-and-restart. `midas dev
  db` is the tunnel-only invocation; the tunnel always comes up whichever subset you name. A held port
  fails and names the holder unless you pass `--kill-ports`. (`app/api/package.json` still carries a
  `dev` script that blocks on port 3309 before `cargo run`, but `midas dev` invokes `cargo run`
  directly per `midas.toml` and does not use it.)
- **OPS-0006 [check]** — `midas flow` **owns** the connection string. `flow start`/`flow end` write
  `MYSQL_DATABASE_URL` into `app/api/.env.local` between `# >>> midas >>>` / `# <<< midas <<<`
  markers (`cli/src/flow/env.rs`), scoped to the active branch; the path and marker are configurable
  via `[flow] api_env_local` / `[flow] env_marker`. Never hand-edit that block — it's rewritten on
  every `flow start` and stripped on `flow end`.
- **OPS-0006 [check]** — Env load order is first-wins and never overrides a real process var:
  `ENV_FILE` → crate `.env` (dev creds) → `.env.local` (the midas tunnel block)
  (`app/api/src/config.rs`, `Config::from_env`). The crate's `.env` is the source of dev secrets;
  `.env.local` only carries the per-branch DB URL. Container/prod injects real env vars instead.
- **OPS-0006 [check]** — One `MYSQL_DATABASE_URL` serves both consumers of the tunnel. It is written
  in the go-sql-driver DSN form (`user@tcp(host:port)/db?…`) — a carry-over from the retired Go
  backend, kept because both readers normalize it. The api's `config.rs::normalize_mysql_dsn` converts
  it to the `mysql://…?ssl-mode=…` URL sqlx needs; a DSN with no `tls=` → `ssl-mode=disabled` (what
  the tunnel wants) (`app/api/src/config.rs`). `midas` carries its own `normalize_mysql_dsn` for the
  migrate runner, which drops the query params and lets sqlx pick its own
  (`cli/src/flow/config.rs`). Two implementations of one conversion is a known duplicated seam.

## Release & branch flow — `midas flow`

The flow is a CLI, not a wiki page: `start` → commit → `rebase` → `ship` → squash-merge → (promote)
→ `tag`, with `end` · `status` · `clean` around the edges. (`ship` keeps `pr` as an alias; the old
midflow names `sync` and `db end --force` are gone — they are `rebase` and `flow end --delete-data`
now.) **`dev` is the integration trunk** — every feature PRs into it; `main` is production
(`README.md`, "Git workflow with `midas flow`"). The trunk is configuration, not a constant:
`[flow] trunk`, defaulting to `dev` (`cli/src/flow/config.rs`), and the CLI's help text says
"trunk" throughout rather than "main".

- **OPS-0007 [check]** — Branches are `<type>/<slug>` cut off `origin/<trunk>`, type ∈
  `feat | fix | chore | docs | spike` (`cli/src/flow/config.rs`, `BRANCH_TYPES`). Slugs are
  lowercased, hyphenated, ≤60 chars (`slugify`, same file). `start` refuses on a dirty worktree
  (`cli/src/cmd/flow.rs`).
- **OPS-0001 [review]** — Go through `midas flow` for branch/rebase/PR/tag; don't hand-roll the git
  dance. `rebase` = `fetch --prune` + `rebase origin/<trunk>` + `push --force-with-lease` (with
  confirm), and prints conflict guidance instead of leaving you stranded. `ship` rebases, pushes,
  then runs `gh pr create --base <trunk>` with the what/why/test-plan template prefilled and the
  title defaulting to the last commit subject; it refuses to ship from the trunk or `main`
  (`cli/src/cmd/flow.rs`; `cli/src/flow/gh.rs`). `--draft`, `--auto-merge`, `--title`, `--body`
  override.
- **OPS-0007 [check]** — `feat`/`fix` default to a **paired pscale branch seeded from `dev`** via
  Data Branching (`pscale branch create --seed-data --wait`); `chore`/`docs`/`spike` are git-only and
  the tunnel hits shared `dev` (`cli/src/flow/config.rs`, `seed_by_default`;
  `cli/src/flow/pscale.rs`). `--with-data`/`--no-data` override. There is **no `hotfix` branch
  type**. Seeded branches inherit parent cluster size (PS-10 min) and cost money — end them
  (OPS-0009).
- **OPS-0001 [review]** — **Hotfix** path is `midas flow start fix <slug>` — a `fix/` branch off
  `dev`, which gets a seeded paired pscale branch by default. For a fire so urgent PR-and-merge is
  too slow, revert the bad commit on `main` with a *new* commit and tell the team — never force-push
  (`README.md`, "Hotfixes").
- **OPS-0010 [review]** — Squash-merge to `dev`; the squash subject reads as a changelog line. Review
  is risk-tiered: features / schema / auth / payments / data-writes **wait for review**; a fix-with-a-
  test or dep bump may self-merge after 24h of silence; docs/scaffold self-merge (`README.md`,
  "Review" / "Merging").
- **OPS-0007 [review]** — Release tags are semver `vX.Y.Z`, annotated, cut from a clean **trunk** and
  pushed after confirm (`cli/src/cmd/flow.rs`), used to drive native builds. *The old "code cuts from
  `dev` but the README says main" discrepancy is resolved: `README.md` now states `midas flow tag`
  cuts from the configured trunk (`dev` in this repo), not `main`.*

## DB & migrations

- **OPS-0008 [check]** — Migrations are **forward-only**, numbered `NNN_short_name.sql` in
  `db/migrations/`, applied in lexical order by `midas migrate apply` (and automatically by `midas
  dev` once the tunnel is up; `midas migrate status` is the read-only view). The runner records each
  file in a `_migrations` ledger keyed by **filename** with a SHA-256 **checksum**, so re-runs are
  no-ops and **editing an applied migration is rejected at runtime** (the BE-0007 guard). One DDL set
  per file, **no `BEGIN`/`COMMIT`** (Vitess forbids DDL-in-txn; the runner applies each file with
  `sqlx::raw_sql`, never wrapped in a transaction) — a mid-file failure leaves partial state and
  writes no ledger row, so **fix forward, never edit an applied file** (`cli/src/flow/migrate.rs`).
  Scaffold with `midas touch migration <slug>`. Mirrors **BE-0007**. *(Cutover from the retired Go
  runner is automatic: the ledger shape is a drop-in match, and on first run the runner adds the
  nullable `checksum` column to an inherited `_migrations` table and backfills it
  trust-on-first-sight; no manual step.)*
- **OPS-0009 [review]** — Schema reaches an integration/prod branch only through a **PlanetScale deploy
  request reviewed in the PS UI** — never apply migrations directly to it. When a PR touches
  `db/migrations/**`, CI ensures the pscale branch, raises a short-lived `pscale connect` tunnel, runs
  `midas migrate apply --yes` against `127.0.0.1` (the local-only guard holds — CI applies *through
  the tunnel*, never to a remote host), and opens/refreshes the DR
  (`.github/workflows/db-deploy-request.yml`). **DR approval is deliberately *not* cascaded from
  GitHub PR review** — it's a separate manual click. On merge, CI deploys the DR and deletes the
  pscale branch (`.github/workflows/db-deploy.yml`). The legacy Go `db/cmd/migrate` binary is gone
  along with the rest of the Go tree; `midas migrate` is the single runner for both local dev and CI.
- **OPS-0009 [review]** — Clean up paired pscale branches: `midas flow end [--delete-data]`
  (`--force` was the midflow spelling and no longer exists). `--delete-data` deletes the derived
  paired branch when it exists (nothing to delete on a git-only flow), and a hardcoded protected-set
  (`main`/`dev`/the configured parent) can never be deleted by any code path
  (`cli/src/cmd/flow.rs`; `cli/src/flow/pscale.rs`). `midas flow clean` is the janitor for the ones
  you forgot — it prunes local branches whose PR merged plus their paired pscale branches, with
  `--dry-run` to look first.
- **OPS-0004 [review]** — Destructive prod data ops (TRUNCATE/DELETE/DROP, manual deploys) are handed
  to a human with the exact commands — tooling and agents never execute them. `midas migrate` is
  dev/preview-only by construction (`is_local_mysql_url` in `cli/src/flow/config.rs` refuses any
  non-loopback target); prod is DR-gated (OPS-0009).

## CI gates

Lint/format/typecheck runs on every PR and on push to `main` (`.github/workflows/lint.yml`).
`midas check` is the local mirror of this job set.

- **OPS-0002 [check]** — Frontend: `bun run format:check` (Prettier), `app/web` `bun run lint`
  (ESLint) + `bun run check` (svelte-check / strict TS) — three separate jobs in `lint.yml`, all
  blocking (`app/web/package.json`).
- **OPS-0002 [check]** — Context lint: `scripts/context-scan.sh --ci` blocks if a canonical
  `AGENTS.md`/`SKILL.md` lacks `owner`/`last_reviewed`/`canon:true` frontmatter, or a nested
  `AGENTS.md` exceeds 80 lines (`lint.yml`, `context-checks` job; `scripts/context-scan.sh`). The
  `docs/` corpus is out of scope here — it belongs to the DOC family.
- **OPS-0002 [check]** — Backend: a dedicated Rust workflow blocks on `cargo clippy --all-targets --
  -D warnings` then `cargo test`, run from `app/api` (`.github/workflows/api-rust.yml`). The crate
  also forbids `unsafe_code` and warns `clippy::all` at the source level (`app/api/Cargo.toml`,
  `[lints]`) — the workflow is what actually gates a PR on it. The required-status-check context is
  the **job name** `clippy + test`; renaming that job silently stops the check from reporting.
- **OPS-0002 / AGT-0003 [check]** — `midas` conformance: a `mechanical` job blocks on `midas check`
  then `midas sync --check` (agent docs current); a `semantic` job runs `midas check --json` and
  posts the review-tier convention set to the job summary for a delegated reviewer — advisory,
  `continue-on-error: true`, never blocks (`.github/workflows/midas.yml`).
- **OPS-0002 [check] [gap]** — `bun run test` (`bun run --filter=@midian/web test` — Playwright +
  vitest) and the dual-adapter web build (`vite build` and the `cap:build` script's
  `CAPACITOR_BUILD=1 VITE_CAPACITOR_BUILD=1 vite build`) are not gated in CI today; midian's
  `docs/archive/note.repo.plan-001-ci-test-gates.2026-06-11.md` is where this landed.

## Generated artifacts — regenerate, commit, drift-guard

The contract is: anything generated from another source of truth is committed, and CI fails on drift.
`midas gen` will own producing them.

- **OPS-0003 [check]** — The API contract is generated from the Rust handlers' `#[utoipa::path]`
  annotations: `cargo run --example export_openapi` → `openapi.json` (no DB/server needed), then
  `openapi-typescript` → the TS client (`app/api/scripts/gen-types.sh`;
  `app/api/examples/export_openapi.rs`; `app/web` `gen:api-types`). `midas check` (`artifact-hash`)
  mechanically requires both `openapi.json` and the generated TS client to be **committed** —
  tracked, not gitignored (`registry/conventions.json`; `cli/src/cmd/check.rs`). This is the
  **FE-0006** producer. A regenerate-and-diff freshness guard plus an `oasdiff` breaking-change gate
  both run in `api-rust.yml` today; `midas check` covers the commit-status half.
- **OPS-0003 [check] [gap]** — sqlx is used in its **runtime** form (`sqlx::query`/
  `query_as::<_,T>`, no `query!` macros), so builds need no DB and no cache. The documented target
  (midian's `docs/decisions/adr.api.rust-backend-port.2026-06-25.md`) is to adopt compile-time
  `query!` + commit the **`.sqlx`** offline cache (`cargo sqlx prepare`) so CI/prod build without a
  DB; a schema change then means regenerate-and-commit the cache, drift-guarded like the above.
  (`BE-0018`, ledgered in midian's `midas.toml` — the compiler enforces `query!` call sites are valid
  where they're used; it can't enforce that runtime `sqlx::query` was never chosen instead.)
- **OPS-0003 [obsolete]** — *This entry described the Go→Rust parity harness: `parity.sh` booting a
  Go oracle alongside the Rust server against the same dev tunnel and deep-diffing every route's
  JSON, with `record-goldens.sh` capturing Go responses into `tests/goldens/`. The port is complete
  and the Go tree is deleted, so the harness cannot run: `parity.sh` still exists on disk but shells
  out to a prebuilt `/tmp/go-oracle` binary that no repo can produce, and `tests/goldens/` does not
  exist. The id is kept for stability; the rule it carried (parity harnesses are dev tools, not
  committed artifacts) is subsumed by the two entries above.*

## Testing

Full conventions live in `backend/`/`frontend/`; the process rules:

- **OPS-0002 [review]** — A new handler/module/business-logic change needs a test; a bug fix ships with
  the regression test that proves it (`README.md`, "Review" / "Hotfixes"). Reviewers may self-merge a
  *fix-with-a-test*; a feature waits (OPS-0010).
- **OPS-0002 [review]** — Mock the network edge, hit the real thing for contract truth. The Rust proof
  tests mint tokens from a throwaway local keypair with no network and `#[ignore]` the live-token
  check (`app/api/tests/auth_verify.rs`); the integration suite in `app/api/tests/` runs against real
  behavior rather than hand-rolled stubs. Don't assert against a hand-rolled DB stub when a proxy to
  `dev` is one command away (`midas dev db`).

## Pre-commit, secrets, deploy

- **OPS-0011 [check]** — Husky pre-commit runs `lint-staged` → Prettier, then `midas -y --no-color
  check` (`.husky/pre-commit`; the midas step is skipped cleanly when the binary isn't installed, so
  CI stays the hard gate). Don't bypass with `--no-verify`; fix the lint or fix the hook in its own
  PR (`README.md`, "Things that will save you pain").
- **OPS-0012 [check]** — `.env`/`.env.*` are gitignored except `.env.example`/`.env.test`
  (`.gitignore`); `app/api/.env` holds dev-only creds, has no template, and is **not** tracked — you
  get it from a teammate or 1Password. Never force-push `main`/`dev` — revert with a new commit
  (`README.md`). `gh secret set` is how CI secrets land (`PSCALE_SERVICE_TOKEN_ID` /
  `PSCALE_SERVICE_TOKEN`, mapped by the workflows to the `PLANETSCALE_*` names the `pscale` CLI
  reads: `db/README.md`).
- **OPS-0013 [review]** — Native ships through a **manual** fastlane run: `deploy-ios.yml` /
  `deploy-android.yml` are `workflow_dispatch`-only (push triggers commented out), set
  `CAPACITOR_BUILD=true`, and derive the build number from the commit count over full history
  (`fetch-depth: 0` + fastlane's `number_of_commits`). Local native rebuilds go through the
  `cap:build:*` scripts that set the flag (`app/web/package.json`) — never a bare `vite build`
  (mirrors **FE-0004**). *Container caveat: `app/api/Dockerfile` and `app/web/Dockerfile` exist, but
  midian has no tracked `docker-compose.yml`, so the root `docker:up` script (`docker compose up
  --build`) has nothing to compose — treat the container path as unwired until a compose file lands.*

## Deviation journal

- **OPS-0014 [review]** — Every `midas.toml [deviations]` entry has a **tracked path to
  resolution**, not just a reason frozen at ledger time. A `ledgered`/`advisory` escape records *why*
  a rule is violated right now (`BE-0018`'s reason cites the concrete migration blocker); the journal
  is where *when it gets fixed* lives, since `midas.toml` itself has no room for that. Reference
  implementation: midian's `docs/archive/note.repo.midas-conformance-journal.2026-07-09.md` — one line per landed change or
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
| OPS-0005 | One-command bootstrap (`scripts/setup.sh`→`bun install`→install `midas`→`midas doctor`); `midas setup` owns it. | review | advisory |
| OPS-0006 | Local dev = pscale proxy `:3309` + dotenv chain (`ENV_FILE`→`.env`→`.env.local`); `midas flow` owns the `.env.local` tunnel block — don't hand-edit. | review | hard |
| OPS-0007 | Branch `<type>/<slug>` off `dev` (trunk); `main` = prod; tags semver `vX.Y.Z`. | review | ledgered |
| OPS-0008 | Migrations forward-only, numbered `NNN_`, one DDL set/file, no txn; fix-forward. (= BE-0007) | review | hard |
| OPS-0009 | Schema→prod only via PS deploy request reviewed in the UI; DR approval separate from GH review; never run `migrate` at prod. | review | hard |
| OPS-0010 | Squash-merge to `dev`; risk-tiered review (features/schema/auth/payments wait; low-risk self-merge after 24h). | review | advisory |
| OPS-0011 | Husky + lint-staged pre-commit not bypassed (`--no-verify`). | review | hard |
| OPS-0012 | Never commit `.env.local`/secrets; rotate on leak; never force-push `main`/`dev`. | check | hard |
| OPS-0013 | Native ships via manual fastlane `workflow_dispatch`; static SPA via `CAPACITOR_BUILD`; build no. = commit count. | review | ledgered (web-only) |
| OPS-0014 | Every `[deviations]` entry has a tracked path to resolution in a conformance journal. | review | advisory |

> IDs are stable once published. The remaining **[gap]** entries (`.sqlx` commit-and-guard, web
> test/build gates) are the standard's near-term target, not current enforcement — `midas check`/`gen`
> should close them. The OpenAPI/TS regenerate-and-diff guard is no longer a gap: `api-rust.yml` gates
> it today.
