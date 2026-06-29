# `midas` CLI — build blueprint

The implementation sketch for the `midas` binary. Design rules: `SPEC.md §5` (surface), `§7` (drift /
embed / manifest), `§8` (enforcement). Conventions it must itself obey: `standards/cli/conventions.md`.
This was the build blueprint; the core (`flow` / `check` / `sync` / `doctor` on the `midian-cli` core
crate) is now built — see `BUILD.md` for live status. Deferred surfaces (`upgrade` / codemods,
scaffolding, the semantic pass) remain indicative.

## Crate layout

```
midas/
├── Cargo.toml                # workspace (cli + packages/midian-cli)
├── cli/                      # the `midas` binary
│   ├── Cargo.toml            # [[bin]] name = "midas"
│   ├── build.rs              # embeds registry/conventions.json + managed-block templates + git tag as version
│   └── src/
│       ├── main.rs           # clap derive root; dispatch; map Outcome/Error → exit code
│       ├── manifest.rs       # typed midas.toml loader
│       ├── registry.rs       # embedded conventions.json model
│       ├── flow/             # ported midflow (git, gh, pscale, state, env, config)
│       ├── checks/           # mechanical check kinds (banned-call, file-structure; rest deferred)
│       └── cmd/{flow,check,sync,doctor}.rs   # shipped · add/new next · upgrade/gen deferred
├── packages/
│   └── midian-cli/           # shared core crate (every midian CLI depends on this) — built
│       └── src/{global,output,exit,confirm,prompt,config,style,log}.rs
└── registry/                 # conventions.json (embedded at build); codemods later
```

`midas` depends on `midian-cli` for everything in `standards/cli` (global flags, `Output` writer,
exit-code mapping, `confirm`, config loader, tty/color, tracing). So the agent-runnable contract is
inherited, not re-implemented.

## Global flags (from `midian-cli`)

`--json` · `--yes` · `--quiet` · `--verbose` · `--no-color`. Every subcommand inherits them.

## Command surface

| Command | Key flags | stdout (`--json`) | Exit | Status |
| --- | --- | --- | --- | --- |
| `midas flow <…>` | (ported midflow) | per-subcommand | 0 / 1 / 2 | **shipped** |
| `midas check` | (globals only) | `{version, root, mechanical:{…}, semantic:{delegated,…}}` | 0 / 2 / 3 | **shipped** (mechanical) |
| `midas sync` | `--check` | files touched + block version | 0 / 2 | **shipped** |
| `midas doctor` | — | env diagnosis | 0 / 2 | **shipped** |
| `midas add state\|migration\|component\|module <name>` | `--dir`, `--ui`, `--no-wire`, `--force` | stamped file paths (+ `pub mod` wiring for `module`) | 0 / 2 / 3 | **shipped** |
| `midas add handler\|pane …` | kind-specific | stamped file paths | 0 / 3 | next |
| `midas new <name>` | `--profile`, `--dir`, `--force` | created project file list | 0 / 2 / 3 | **shipped** (profile init; code `templates/` next) |
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
profile = "app"            # service | app | library | pipeline

[stack]                    # per-layer current/target; a layer is checked vs its CURRENT stack
backend  = { current = "go", target = "rust" }
frontend = { current = "svelte" }

[check]
semantic_strict = false    # opt-in; surfaced in --json for the external review agent / CI to gate on
                           # (midas check itself never blocks on semantic concerns)

[flow]                     # ported-midflow config (was hardcoded in midflow); defaults reproduce it
trunk         = "dev"      # midflow MainBranch (this repo overrides to "main")
pscale_org    = "midian"
pscale_db     = "application"
pscale_parent = "dev"
pscale_region = "us-east"
tunnel_port   = 3309
# api_env_local / state_file / env_marker — the paths midflow used; overridable per repo

[deviations]               # ledgered escape hatches: convention id → reason
"FE-0004" = "web-only — no Capacitor adapter switch"
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
      "status": "adopted", "tier": "check", "escape": "hard",
      "check": { "kind": "banned-call",
                 "pattern": "reqwest::Client::new", "allow_in": ["src/http.rs"],
                 "globs": ["app/api/src/**/*.rs"] },
      "doc": "backend/conventions.md#be-0010"
    },
    {
      "id": "FE-0009", "title": "No business logic in components",
      "layer": "frontend", "stack": "svelte",
      "status": "adopted", "tier": "review", "escape": "hard",
      "review": { "prompt_ref": "frontend/conventions.md#state" },
      "doc": "frontend/conventions.md#fe-0009"
    }
  ]
}
```

**Mechanical check kinds** (`tier: check`): `banned-call` (regex/substring + allow-list + globs) and
`file-structure` (paths must / must-not exist) are **implemented**; `artifact-hash` (generated file in
sync with its source — `.sqlx`, OpenAPI, TS client), `provenance-drift` (a `// midas:provenance <id>
<sha>` vendored file vs. its canonical version), and `clippy` (lint passthrough) are **carried in the
registry but deferred** — the engine reports them `skipped`. **Semantic** (`tier: review`): carries a
`prompt_ref` for the **external review agent** (not run by `midas`).

## Semantic pass — delegated, not embedded

`midas` ships **no agent and no adapter.** The semantic (`review`-tier) pass is **inverted** from the
earlier design: the agent platform is the host, `midas` is a tool it invokes. Whatever review agent
the team already runs — Cursor, Claude, CodeRabbit, Copilot:

- **reads `standards/`** for the `review`-tier convention text (and the `prompt_ref` each registry
  entry carries),
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

`build.rs` embeds `registry/conventions.json` and the managed-block templates via
`include_str!`/`include_dir!`, and stamps the **git tag as the version**. So `midas --version` *is* the
standard version it enforces — no checker/rules skew, no repo fetch to run `check` (`SPEC.md §7`).
(Codemods will be embedded the same way once `upgrade` lands.)

## Managed-block sync

`midas sync` writes/updates a delimited region in each repo's `CLAUDE.md`, `AGENTS.md`, `.cursor`
rules:

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

1. **`midian-cli` core** — global flags, `Output`, exit-code mapping, `confirm`, config loader, tty,
   tracing. (Locks `standards/cli` `CLI-0001…0005` by construction.) ✅ built.
2. **`midas flow`** — port `scripts/midflow` (Go) faithfully into Rust subcommands; lift its hardcoded
   config into `[flow]`. (Defines the CLI standard in practice.) ✅ built (start·sync·pr·hotfix·tag·db·doctor).
3. **embed** `registry/conventions.json` + version via `build.rs`. ✅ built.
4. **`midas check` (mechanical)** — banned-call + file-structure implemented (artifact-hash /
   provenance-drift / clippy carried but `skipped`); the `[deviations]` ledger + escape policy + exit
   `0/2/3`. ✅ built (clean on midian; catches planted violations on a fixture; refuses a ledgered
   deviation for a `hard` rule).
5. **`midas sync`** (managed-block writer) + **`midas doctor`**. ✅ built.
6. **`midas add`** — deterministic scaffolding. ✅ built (`state`·`migration`·`component`·`module` — `module` writes the 4-file backend skeleton + wires `pub mod`); `handler`/`pane` ⬜ next.
6b. **`midas new`** — whole-project scaffold (`midas.toml` + agent docs + CI + dir shape, profile-aware). ✅ built; runnable code `templates/` (rust-service/svelte-app/cli-tool) ⬜ next (gated on the package-sharing story, SPEC §7).
7. **`midas upgrade` + codemods.** ⏸ deferred (fleet-scale; build-trigger is "the agent-first software
   factory becomes real"). The stable convention IDs + the `midas.toml` version pin are the cheap
   anchors kept meanwhile.
8. **Semantic pass** — runs out-of-process via the team's review agent (consumes `midas check --json`
   + reads `standards/`); **no in-binary adapter**. ⏸ no midas work beyond the `--json` contract +
   the registry `prompt_ref`s.
