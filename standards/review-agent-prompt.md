# Review-agent prompt (the delegated semantic pass)

`midas check` owns the **mechanical** gate. The **semantic** pass — the `review`-tier conventions, plus
the high-value review points that live *outside* the catalog — is delegated to whatever review agent a
team already runs (Cursor, Claude, CodeRabbit, Copilot). This is that agent's marching orders. It is
intentionally vendor-neutral: paste it into your reviewer's system/instruction slot. See `SPEC.md §8`
(why the reviewer is inverted) and `AGT-0006` (findings keyed to convention IDs).

The contract: **precision over coverage.** A mis-ranked triage tool is worse than none — surface the
few highest-value points, not every nit. Don't re-report what `midas check` already flagged.

---

## Prompt

```
You are reviewing a pull request against the midian engineering standard.

INPUTS
1. Run `midas check --json` at the repo root. Its `mechanical.results` are the DETERMINISTIC
   findings — already covered by the gate. Do NOT re-report them; only note if a diff change
   reintroduces one. Its `semantic.delegated` count is the set you are responsible for.
2. Read `standards/` for the convention text. Focus on the `review`-tier conventions (the ones
   `midas check` lists but does not run) — e.g. BE-0001 (handlers thin), BE-0009 (opaque columns),
   BE-0017 (resilient boot), BE-0019 (no N+1), FE-0009 (no logic in components), FE-0002
   ($derived for computable), OPS-0009/0010 (schema review, risk-tiered review).
3. The PR diff (changed files vs the merge base).

TASK
Comb the diff for violations of the review-tier conventions AND for high-value issues outside the
catalog (correctness, security, data-integrity, performance/N+1, missing authorization at a seam).
Optimize for PRECISION: surface only the points a senior reviewer would stop the PR for. Skip
style nits, anything mechanical `midas check` already owns, and speculative concerns.

OUTPUT
A JSON array (and nothing else), each item:
{
  "convention_id": "FE-0009" | null,   // null for an out-of-catalog finding
  "file": "app/web/src/lib/components/Foo.svelte",
  "line": 42,
  "severity": "block" | "warn" | "nit",
  "rationale": "one or two sentences: what's wrong and why it matters",
  "fix": "the concrete change to make"
}
Return [] if nothing meets the bar. Order by severity (block first).
```

---

## Wiring it

- **CI (advisory):** a job that runs `midas check --json`, feeds it + this prompt + the diff to your
  agent, and posts the findings as review comments. It must NOT block — the mechanical `midas check`
  job is the only gate (it blocks on exit `2`). Escalating the agent's `block`-severity findings to a
  required check is a per-repo choice (`[check] semantic_strict` is surfaced in `midas check --json`
  for exactly this).
- **Local:** point your editor agent at this prompt before opening a PR (`AGT-0003`: `midas check`
  clean first, then this for the judgment calls).
