# Agent Playbook

How AI agents stay on-spec in every midian repo. The principle: **an agent does not learn the
conventions per-repo — it reads the versioned standard and runs `midas`.** The same deterministic path
a human uses. Entries are keyed `AGT-####` with an enforcement tier.

Agents play **two** roles here, and the playbook serves both:

1. **Author** — an agent writing code in a midian repo, which must *follow* the standard.
2. **Reviewer** — the agent *inside* `midas check`'s semantic pass, which *enforces* the `review`-tier
   conventions a linter can't (`SPEC.md §8`).

## Delivery — how the standard reaches each repo (`AGT-0001` `[check]`)

Each repo already has its own agent config (`midian/CLAUDE.md` is GitNexus-specific; every repo has
its own `AGENTS.md`, `.cursor`). The standard injects shared guidance **without clobbering it**:

- **`midas sync` writes a version-stamped managed block** into each repo's `CLAUDE.md`, `AGENTS.md`,
  and `.cursor` rules:

  ```
  <!-- midas:0.4.1 -->
  This repo conforms to the midas standard, pinned in midas.toml.
  • Conventions are the source of truth: run `midas check` before a PR; scaffold with `midas touch …`.
  • Don't hand-roll what `midas touch` stamps. Don't bypass the seams the conventions name.
  • Any CLI you build follows standards/cli (agent-runnable: --json, non-interactive, typed exits).
  <!-- /midas -->
  ```

  **Project-specific content lives outside the block, untouched.** The block names the pinned version
  and points at the conventions; a stale or missing block is `check`-tier drift (`midas check` flags
  it; `midas sync` fixes it).
- **The skill bundle ships versioned**, installed by `midas setup`: the `add-*` skills are **thin
  wrappers that shell out to `midas touch`**, plus the standard's review skill. Because the logic lives
  in the binary, the bundle is tiny and rarely changes (`SPEC.md §5`).

## Rules for an authoring agent

- **`AGT-0002` Scaffold through `midas touch`, never hand-roll `[review]`.** A new handler/module/state/
  migration/pane is created by the command, so humans and agents emit identical structure. Hand-built
  scaffolding is the anti-pattern.
- **`AGT-0003` `midas check` is clean (or ledgered) before a PR `[check]`.** Enforced in CI. Mechanical
  drift blocks; a deliberate deviation is recorded in `midas.toml [deviations]` with a reason — never
  left as silent drift.
- **`AGT-0004` The pinned standard wins conflicts `[review]`.** When a repo's older local doc disagrees
  with the standard at its pinned version, follow the standard. Surface the stale doc rather than
  honoring it.
- **`AGT-0005` Use the seams the conventions name `[review]`.** Don't reach around `access::require`,
  the `Http` client, `st.telemetry.*`, the `api<T>()` wrapper, `generateId()`, etc. The seams *are* the
  conventions.

## The reviewer contract — `midas check`'s semantic pass

The `review`-tier conventions are enforced by an agent through the **`AgentReviewer` seam**
(vendor-neutral; **Cursor SDK agent** is the shipped default adapter — `SPEC.md §8`).

- **Input:** the diff (changed files vs. merge base; `--all` for a full sweep) + the `review`-tier
  slice of the convention catalog applicable to the touched stacks.
- **Output (`AGT-0006` `[check]` on shape):** structured findings, one per concern —
  `{ convention_id, file, line, severity, rationale }`. The shape is fixed so `midas check` can render
  the `SEMANTIC` block and (optionally) gate on it. An agent reviewer that returns prose instead of
  structured findings is a contract violation.
- **Disposition:** **advisory by default** (exit `4`, non-blocking) — it always renders and is expected
  to be addressed; a project ratchets to `--semantic-strict` once it trusts the false-positive rate.
- **Determinism stays with the mechanical arm** — the agent never owns the blocking gate.

## Why this design

- **Robust:** the managed block works on a fresh CI clone — no assumption that the `midas` repo is a
  sibling checkout.
- **Non-destructive:** each repo's existing agent setup (GitNexus, project rules) is preserved.
- **Git-visible + drift-checkable:** the injected block shows up in a diff and is policed by
  `midas check`.
- **Two arms, one catalog:** mechanical (`midas check`) + semantic (this agent) together cover the
  full catalog, and the agent's job is *enforcing the same IDs* a human reads — not a parallel rubric.

## Reuse

Project-agnostic. The managed-block mechanism, the skill bundle, and the `AgentReviewer` contract are
part of `midas`; a new project gets the whole playbook from `midas setup` + `midas sync`.
