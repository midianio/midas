# midas

The shared engineering standard for midian and every future midian project — the one answer to
*"how do we build this?"*, for humans and AI agents alike. `midas` is both the **repo** (the source of
truth) and the **binary** (the `midas` CLI that runs the release flow, checks projects against the
standard, syncs the agent docs, and scaffolds conventional pieces).

It is a **kit, not a cage**: opinionated defaults you inherit instead of rediscover, with recorded
escape hatches when a project genuinely needs to deviate. Everything here is **extracted from running
code** (the midian Rust backend + Svelte frontend, and the `midflow` release CLI), not invented.

## Start here

1. **[`SPEC.md`](./SPEC.md)** — what the standard is, its architecture, the `midas` CLI, the drift /
   versioning model, enforcement, and the rollout. *Read this first.*
2. **[`standards/`](./standards/)** — the conventions, by layer (+ the seed catalog with IDs/tiers).
3. **[`cli/README.md`](./cli/README.md)** — the `midas` binary build blueprint.

## What's in here

| Path | What |
| --- | --- |
| `SPEC.md` | The meta-spec — architecture, CLI, drift/versioning, enforcement, rollout, open decisions. |
| `standards/stack.md` | L1 — canonical tech choices + swap conditions. |
| `standards/backend/` | L2 — Rust/axum/sqlx conventions *(split-lift from `midian/plans/rust-port/standards/`, Phase 0)*. |
| `standards/frontend/conventions.md` | L2 — SvelteKit / Svelte-5 / Capacitor conventions (authored, with refinements). |
| `standards/cli/conventions.md` | L2 — Rust CLI conventions (extracted by building `midas`). |
| `standards/process.md` | L4 — setup, CI, the midflow release flow, testing, migrations *(Phase 0)*. |
| `standards/agents.md` | L5 — the AI/agent playbook (delivery + the semantic-review contract). |
| `standards/playbooks/go-to-rust.md` | The reusable Go→Rust migration method *(prayer re-runs it; Phase 0)*. |
| `cli/` | The one-stop `midas` binary — built: `flow`/`check`/`sync`/`doctor`/`add`/`new` on its internal `core` contract kernel (`cli/src/core/`) *(blueprint in `cli/README.md`, live status in `BUILD.md`)*. |
| `templates/` | Runnable project skeletons embedded into the binary *(`rust-service` + `svelte-app` built & verified)*. |
| `packages/` | Home for **graduated** shared seams — empty for now; behavioral seams start vendored-with-provenance in each consumer *(→ Phase 3 to graduate)*. |
| `registry/` | Machine-readable convention catalog (`conventions.json`), embedded in the binary *(built; codemods later)*. |

## How a project consumes it

```sh
midas flow pr                   # the release/branch flow (the ported midflow)            — shipped
midas check                     # mechanical lint vs the pinned standard; report drift     — shipped
                                #   (review-tier conventions are delegated to your review agent)
midas sync                      # refresh the version-stamped agent managed-block in this repo — shipped
midas add module billing        # scaffold a conventional piece — state/migration/component/module — shipped
midas new my-app --profile app  # scaffold a conformant project (midas.toml, agent docs, CI)   — shipped
midas upgrade                   # carry the project to a newer standard version via codemods — deferred
```

Each project carries a `midas.toml` lockfile pinning its `midas` version (which governs the CLI, the
embedded rules, **and** the shared-package versions — one git tag), declaring per-layer stack state,
and ledgering intentional deviations. See `SPEC.md §5` (CLI), `§7` (drift/versioning), `§8`
(enforcement).

## Status

v1 (2026-06-25). Architecture resolved via a full design grill (see `SPEC.md`). **Phase 0 (extract)
docs complete:** the spec; stack; backend (+ the Go→Rust playbook); frontend; CLI; process; and agent
conventions; the CLI build blueprint; and the seed catalog (≈60 IDs with enforcement tiers).
**Built (Phase 1–2):** a Cargo workspace producing the one-stop `midas` binary on its internal `core`
contract kernel (`cli/src/core/`) — `midas flow` (ported midflow), mechanical `midas check` (reads the embedded
`registry/conventions.json`; `banned-call` + `file-structure` kinds; gates CI at exit `2`),
`midas sync`, `midas doctor`, and `midas add` (deterministic `state`/`migration`/`component`/`module`
scaffolding — `module` writes the 4-file backend skeleton + wires `pub mod`). The repo dogfoods its
own `midas.toml` and `midas check` runs clean on it. `midas new <name>` scaffolds a whole conformant
project (manifest + agent docs + CI + dir shape) — and lays down **runnable, conformant skeletons**:
`rust-service` (axum, `--profile service`) and `svelte-app` (SvelteKit/Svelte 5 runes, `--profile
app`), each verified to pass `midas check` and build (cargo / bun). The delegated semantic review is
turnkey via [`standards/review-agent-prompt.md`](./standards/review-agent-prompt.md). **Deferred:**
`midas upgrade` /
codemods; the in-binary semantic adapter (inverted — `midas check` is mechanical-only; the team's
review agent invokes `midas check --json` + reads `standards/`); shared-package workspaces
(vendor-with-provenance is the default — nothing is shared up front, not even the CLI kernel). Nothing is enforced in a project until the
conventions are real and both midian and prayer pass `midas check` — see `SPEC.md §10` and `BUILD.md`.
