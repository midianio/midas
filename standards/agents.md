# Agent Playbook

How AI agents stay on-spec in every midian repo. The principle: **an agent does not learn the
conventions per-repo — it reads the versioned standard and runs `midas`.** The same deterministic path
a human uses. Entries are keyed `AGT-####` with an enforcement tier.

Agents play **two** roles here, and the playbook serves both:

1. **Author** — an agent writing code in a midian repo, which must *follow* the standard.
2. **Reviewer** — the team's external review agent, which *enforces* the `review`-tier conventions a
   linter can't. It is **not** part of `midas`: the agent platform is the host, `midas` is a tool it
   invokes (`midas check --json`) plus docs it reads (`standards/`) — see `SPEC.md §8` and the
   turnkey prompt in [`review-agent-prompt.md`](./review-agent-prompt.md).

## Delivery — how the standard reaches each repo (`AGT-0001` `[check]`)

Each repo already has its own agent config (`midian/CLAUDE.md` is GitNexus-specific; every repo has
its own `AGENTS.md`, `.cursor`). The standard injects shared guidance **without clobbering it**:

- **`midas sync` writes a version-stamped managed block** into each repo's `CLAUDE.md` and
  `AGENTS.md`:

  ```
  <!-- midas:0.2.0 -->
  This repo conforms to the midas standard, pinned in midas.toml.
  • Conventions are the source of truth: run `midas check` before a PR; scaffold with `midas touch …`.
  • Don't hand-roll what `midas touch` stamps. Don't bypass the seams the conventions name.
  • Any CLI you build follows standards/cli (agent-runnable: --json, non-interactive, typed exits).
  <!-- /midas -->
  ```

  **Project-specific content lives outside the block, untouched.** The block names the pinned version
  and points at the conventions; a stale or missing block is `check`-tier drift — `midas check`
  fails it (AGT-0001, `managed-block` kind) and `midas sync` fixes it.
- **The skill bundle ships versioned**, installed by `midas setup` (planned): the `add-*` skills are
  **thin wrappers that shell out to `midas touch`**. Because the logic lives in the binary, the
  bundle is tiny and rarely changes (`SPEC.md §5`).

## Rules for an authoring agent

- **`AGT-0002` Scaffold through `midas touch`, never hand-roll `[review]`.** A new handler/module/state/
  migration/pane is created by the command, so humans and agents emit identical structure. Hand-built
  scaffolding is the anti-pattern.
- **`AGT-0003` `midas check` is clean (or ledgered) before a PR `[review]`.** Enforced in CI. Mechanical
  drift blocks; a deliberate deviation is recorded in `midas.toml [deviations]` with a reason — never
  left as silent drift.
- **`AGT-0004` The pinned standard wins conflicts `[review]`.** When a repo's older local doc disagrees
  with the standard at its pinned version, follow the standard. Surface the stale doc rather than
  honoring it.
- **`AGT-0005` Use the seams the conventions name `[review]`.** Don't reach around `access::require`,
  the `Http` client, `st.telemetry.*`, the `api<T>()` wrapper, `generateId()`, etc. The seams *are* the
  conventions.

## The reviewer contract — delegated, out-of-process

The `review`-tier conventions are enforced by **whatever review agent the team runs** (Cursor,
Claude, CodeRabbit, …). `midas` ships **no agent and no adapter** — `midas check` is mechanical-only
and merely *counts* the applicable review-tier conventions (`SPEC.md §8`). The reviewer:

- **Input:** the diff (changed files vs. merge base) + the `review`-tier slice of the convention
  catalog applicable to the touched stacks, read from `standards/`, plus the mechanical baseline
  from `midas check --json`. [`review-agent-prompt.md`](./review-agent-prompt.md) is the turnkey
  prompt.
- **Output (`AGT-0006` `[review]` on shape):** structured findings, one per concern —
  `{ convention_id, file, line, severity, rationale }` — keyed to convention IDs, not a parallel
  rubric. A reviewer that returns prose instead of structured findings violates the contract.
- **Disposition:** advisory by default. `midas.toml [check] semantic_strict` is surfaced verbatim in
  `midas check --json`; it's the signal for the review agent / CI to escalate its own findings to
  blocking. The mechanical gate never reads it.
- **Determinism stays with the mechanical arm** — the semantic pass is non-deterministic and
  out-of-process, so it can never block the `midas check` gate (exit `2` is mechanical-only).

## Why this design

- **Robust:** the managed block works on a fresh CI clone — no assumption that the `midas` repo is a
  sibling checkout.
- **Non-destructive:** each repo's existing agent setup (GitNexus, project rules) is preserved.
- **Git-visible + drift-checkable:** the injected block shows up in a diff and is policed by
  `midas check` (AGT-0001).
- **Two arms, one catalog:** mechanical (`midas check`) + semantic (the external reviewer) together
  cover the full catalog, and the reviewer's job is *enforcing the same IDs* a human reads.
- **Buy the commodity, build the core:** combing a diff for review points is fast-commoditizing —
  wrap whatever agent the team already runs. The defensible core is the deterministic mechanical
  checks and conformant-by-construction scaffolding (`SPEC.md §8`).

## Reuse

Project-agnostic. The managed-block mechanism, the skill bundle, and the reviewer contract are part
of `midas`; a new project gets the whole playbook from `midas touch project` + `midas sync`.
