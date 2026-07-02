# `midas` CLI — build blueprint

The implementation sketch for the `midas` binary. Design rules: `SPEC.md §5` (surface), `§7` (drift /
embed / manifest), `§8` (enforcement). Conventions it must itself obey: `standards/cli/conventions.md`.
This was the build blueprint; the core (`flow` / `check` / `sync` / `doctor` on the internal `core`
contract kernel) is now built — see `BUILD.md` for live status. Deferred surfaces (`upgrade` / codemods,
scaffolding, the semantic pass) remain indicative.

## Crate layout

```
midas/
├── Cargo.toml                # workspace (one member: cli)
├── registry/history/         # frozen per-version snapshots (drift's offline diff base)
├── cli/                      # the one-stop `midas` binary
│   ├── Cargo.toml            # [[bin]] name = "midas"
│   └── src/
│       ├── main.rs           # clap derive root; dispatch; map Outcome/Error → exit code
│       ├── manifest.rs       # typed midas.toml loader
│       ├── registry.rs       # embedded conventions.json model
│       ├── core/             # the CLI contract kernel — built
│       │   └── {global,output,exit,confirm,prompt,config,style,log}.rs + mod.rs
│       ├── flow/             # ported midflow → midas flow (git, gh, pscale, env, config)
│       ├── checks/           # mechanical check kinds (banned-call, file-structure, banned-file, managed-block)
│       └── cmd/{flow,check,drift,sync,doctor,add,new,dev,migrate,touch,templates}.rs   # shipped · upgrade/gen deferred
└── registry/                 # conventions.json (embedded at build); codemods later
```

`midas` builds every command on its internal `core` kernel for everything in `standards/cli` (global
flags, `Output` writer, exit-code mapping, `confirm`, config loader, tty/color, tracing). So the
agent-runnable contract is enforced once, centrally, not re-implemented per command.

## Global flags (from the `core` kernel)

`--json` · `--yes` · `--quiet` · `--verbose` · `--no-color`. Every subcommand inherits them.

## Command surface

| Command | Key flags | stdout (`--json`) | Exit | Status |
| --- | --- | --- | --- | --- |
| `midas flow <verb>` | start·sync·ship·tag·end·status | per-subcommand | 0 / 1 / 2 | **shipped** |
| `midas check` | (globals only) | `{version, root, mechanical:{…}, semantic:{delegated,…}}` | 0 / 2 / 3 | **shipped** (mechanical) |
| `midas sync` | `--check` | files touched + block version | 0 / 2 | **shipped** |
| `midas doctor` | — | env diagnosis | 0 / 2 | **shipped** |
| `midas touch state\|migration\|component\|module <name>` | `--dir`, `--ui`, `--no-wire`, `--force` | stamped file paths (+ `pub mod` wiring for `module`) | 0 / 2 / 3 | **shipped** |
| `midas touch handler\|pane …` | kind-specific | stamped file paths | 0 / 3 | next |
| `midas touch project <name>` | `--profile`, `--dir`, `--force` | created project file list | 0 / 2 / 3 | **shipped** (incl. `rust-service` + `svelte-app` skeletons) |
| `midas dev [names…]` | (globals only) | streamed prefixed process output | 0 / 1 / 3 | **shipped** (concurrent runner + pscale tunnel; auto-migrate; `[dev]` in midas.toml) |
| `midas migrate [apply\|status]` | (globals only) | `{newly_applied, states[]}` | 0 / 1 / 2 / 3 | **shipped** (forward-only runner over the local tunnel; `_migrations` checksum ledger; OPS-0008/0009) |
| `midas setup` / `teardown` | `--no-db` / `--yes` | bootstrapped / torn down | 0 / 1 | later |
| `midas gen types` | `--out` | written path | 0 / 1 | deferred |
| `midas upgrade` | `--to <ver>`, `--dry-run` | applied codemods + residuals | 0 / 1 | deferred |

Exit codes per `CLI-0004`: `0` ok · `1` tool error · `2` expected-negative (drift/dirty/no) · `3`
usage · `4` advisory. `4` stays in the shared taxonomy for CLIs that have an in-process advisory arm,
but **`midas check` never emits it** — it's mechanical-only (`0`/`2`/`3`, `1` on tool error) and owns
the CI gate at exit `2`. The semantic / review-tier arm is **delegated to an external review agent,
out-of-process**, so it can't block the gate; `[check] semantic_strict` is surfaced in `--json` for
that agent / CI to escalate on its own.

## `midas.toml` (the lockfile) — full schema

```toml
[standard]
version = "0.4.1"          # pins midas (binary + embedded rules + package versions); = a git tag
profile = "app"            # service | app | cli | library | pipeline

[stack]                    # per-layer current/target; a layer is checked vs its CURRENT stack
backend  = { current = "go", target = "rust" }
frontend = { current = "svelte" }

[layout]                   # where each layer lives; the registry's check globs are layer-relative
backend  = "app/api"       # (these are the defaults — omit unless the repo's shape differs)
frontend = "app/web"

[check]
semantic_strict = false    # opt-in; surfaced in --json for the external review agent / CI to gate on
                           # (midas check itself never blocks on semantic concerns)
[check.allow]              # per-project allow-list: convention id → extra allow_in globs
"BE-0016" = ["app/api/src/modules/sharelink/service.rs"]

[flow]                     # ported-midflow config (was hardcoded in midflow); defaults reproduce it
trunk         = "dev"      # midflow MainBranch (this repo overrides to "main")
pscale_org    = "midian"
pscale_db     = "application"
pscale_parent = "dev"
pscale_region = "us-east"
tunnel_port   = 3309
# api_env_local / env_marker — overridable per repo (env_marker defaults to "midas")

[dev]                      # `midas dev` — concurrent process runner (+ optional pscale tunnel)
tunnel  = true             # raise the pscale tunnel first (migrate = false to skip auto-migrations)
processes = [
  { name = "api", cmd = "cargo run", cwd = "app/api" },
  { name = "web", cmd = "bun run dev", cwd = "app/web" },
]

[deviations]               # ledgered escape hatches: convention id → reason
"FE-0004" = "web-only — no Capacitor adapter switch"
# a deviation entry against a `hard`-escape rule is itself a check failure
```

## `registry/conventions.json` — the embedded catalog

The machine-readable mirror of `standards/`. One object per convention; `midas check` reads this (from
the binary, not disk):

```jsonc
{
  "version": "0.4.1",
  "conventions": [
    {
      "id": "BE-0010",
      "title": "Outbound HTTP only through the pooled Http seam",
      "layer": "backend", "stack": "rust",
      "tier": "check", "escape": "hard",
      "check": { "kind": "banned-call",
                 "pattern": "reqwest::Client::new", "allow_in": ["src/http.rs"],
                 "globs": ["src/**/*.rs"] },
      "doc": "backend/conventions.md#be-0010"
    },
    {
      "id": "FE-0009", "title": "No business logic in components",
      "layer": "frontend", "stack": "svelte",
      "tier": "review", "escape": "hard",
      "doc": "frontend/conventions.md#fe-0009"
    }
  ]
}
```

Check globs/paths are **layer-relative** (`src/**/*.rs`, not `app/api/src/**/*.rs`) — the project's
`midas.toml [layout]` maps each layer onto the repo (defaults: `backend = "app/api"`,
`frontend = "app/web"`), so the same registry checks midian's monorepo, a scaffolded service, and
any other shape.

**Mechanical check kinds** (`tier: check`): `banned-call` (regex/substring + allow-list + globs),
`file-structure` (paths must / must-not exist), `banned-file` (paths that must be gitignored —
OPS-0012), and `managed-block` (the version-stamped agent-doc block, AGT-0001) are **implemented**;
`artifact-hash` (generated file in sync with its source — `.sqlx`, OpenAPI, TS client) is carried in
the registry (BE-0014/FE-0006/OPS-0003) but deferred — the engine reports it `skipped`;
`provenance-drift` and `clippy` exist in the engine's vocabulary but no entry uses them yet (clippy
runs directly in CI via `[lints]`). A rule is `check`-tier **only if it carries a real spec**.
**Semantic** (`tier: review`): delegated to the **external review agent** (not run by `midas`),
prompt in `standards/review-agent-prompt.md`.

## Semantic pass — delegated, not embedded

`midas` ships **no agent and no adapter.** The semantic (`review`-tier) pass is **inverted** from the
earlier design: the agent platform is the host, `midas` is a tool it invokes. Whatever review agent
the team already runs — Cursor, Claude, CodeRabbit, Copilot:

- **reads `standards/`** for the `review`-tier convention text (each entry's `doc` points at it;
  the turnkey prompt is `standards/review-agent-prompt.md`),
- **calls `midas check --json`** for the mechanical baseline + the deviation ledger, then combs the
  diff itself.

`midas check`'s `--json` reports only a **delegated count** in its `semantic` block
(`{delegated, semantic_strict, note}`); it produces no findings of its own and never blocks the gate
on semantic concerns. Combing a diff for high-value review points is a fast-commoditizing capability
(CodeRabbit, Copilot, Cursor, Claude review) — we **buy/wrap it, not build it.** The defensible,
build-it-ourselves core is the deterministic mechanical engine + conformant-by-construction
scaffolding. (Earlier drafts embedded a `CursorReviewer` / `AgentReviewer` trait that `midas` drove;
that has been removed.)

## Embed mechanism

`registry.rs` embeds `registry/conventions.json` (+ the frozen `registry/history/*.json` snapshots)
via `include_str!`; the managed-block template lives in `cmd/sync.rs`. The **crate version is the
standard version** — a build-time test (`binary_version_equals_embedded_standard_version`) fails
when `cli/Cargo.toml` and the embedded registry diverge, so `midas --version` *is* the standard it
enforces — no checker/rules skew, no repo fetch to run `check` (`SPEC.md §7`). (Codemods will be
embedded the same way once `upgrade` lands.)

## Managed-block sync

`midas sync` writes/updates a delimited region in each repo's `CLAUDE.md` and `AGENTS.md`:

```
<!-- midas:0.4.1 -->  … generated content …  <!-- /midas -->
```

Algorithm: find the delimiters; replace the span (or append if absent); never touch bytes outside it.
`midas sync --check` (and `midas check`) flag a missing/stale-version block as `check`-tier drift.

## Distribution

Single static binary (musl, rustls — no OpenSSL). Released from this repo on the one SemVer git tag
(cargo-dist or equivalent). A future `midas upgrade` swaps the binary to the pinned/`--to` version and
runs `codemods/<from>-<to>/`.

## Build order

1. **`core` kernel** (`cli/src/core/`) — global flags, `Output`, exit-code mapping, `confirm`, config
   loader, tty, tracing. (Locks `standards/cli` `CLI-0001…0005` by construction.) ✅ built.
2. **`midas flow`** — port `scripts/midflow` (Go) faithfully into Rust subcommands; lift its hardcoded
   config into `[flow]`. (Defines the CLI standard in practice.) ✅ built (start·sync·ship·tag·end·status).
3. **embed** `registry/conventions.json` + version via `include_str!` + the lockstep-version test. ✅ built.
4. **`midas check` (mechanical)** — banned-call, file-structure, banned-file, managed-block
   implemented (artifact-hash carried but `skipped`); layer-relative globs via `[layout]`; per-project
   `[check.allow]`; the `[deviations]` ledger + escape policy + exit `0/2/3`. ✅ built (fires real
   checks on this repo and on fresh scaffolds; catches planted violations on a fixture; a ledgered
   deviation for a `hard` rule is itself an error).
5. **`midas sync`** (managed-block writer) + **`midas doctor`**. ✅ built.
6. **`midas touch`** — deterministic scaffolding. ✅ built (`state`·`migration`·`component`·`module` — `module` writes the 4-file backend skeleton + wires `pub mod`); `handler`/`pane` ⬜ next.
6b. **`midas touch project`** — whole-project scaffold (`midas.toml` + agent docs + CI + dir shape, profile-aware), embedding the runnable `rust-service` (`--profile service`) and `svelte-app` (`--profile app`) skeletons. ✅ built & verified.
7. **`midas upgrade` + codemods.** ⏸ deferred (fleet-scale; build-trigger is "the agent-first software
   factory becomes real"). The stable convention IDs + the `midas.toml` version pin are the cheap
   anchors kept meanwhile.
8. **Semantic pass** — runs out-of-process via the team's review agent (consumes `midas check --json`
   + reads `standards/`, prompt in `standards/review-agent-prompt.md`); **no in-binary adapter**.
   ⏸ no midas work beyond the `--json` contract.
