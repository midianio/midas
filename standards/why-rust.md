# Why Rust — the backend rationale behind `STK-0001`

`standards/stack.md` pins the backend/CLI language in one line: *"Type-safety, one static binary,
graceful-shutdown control, no GC pauses on streaming."* That's the choice; this doc is the argument
behind it — expanded because the choice is now load-bearing on more than server ergonomics: most code
landing in a midian repo is AI-authored, and that changes what "safe default" means.

Source: [Why Rust Is the AI Language of the Future](https://hyperdev.matsuoka.com/p/why-rust-is-the-ai-language-of-the)
(Matsuoka, HyperDev). Claims below are attributed to it; the mapping to this repo's mechanisms is ours.

## The core argument: the compiler is a reviewer that never gets tired

The article's central claim isn't performance — it's that Rust turns human code review from the
*primary* safety net into a *secondary* one. When an LLM generates a diff, a human (or another agent)
skimming it is the same bottleneck that misses data races and lifetime bugs in human-written code, just
faster. Rust's compiler performs exhaustive checks — borrow/lifetime, exhaustive-match, `Send`/`Sync` —
on every candidate diff, agent-authored or not, before it can run.

That's not hypothetical here: `AGT-0001`–`AGT-0009` (`standards/agents.md`) already assume most authors
are agents and lean on `midas check` plus a semantic reviewer to catch what a linter can't. Rust adds a
**third arm that runs before either of them, on every build, for free**: `cargo build` rejects whole
classes of bugs (use-after-free, data races, null derefs) that would otherwise need a human or a
review-tier convention to catch. Three mechanical, `check`-tier backend rules most directly encode this:

- **`BE-0012`** — banned-call grep denies `print!`/`eprintln!` in `midas check`, and CI's
  `cargo clippy -- -D warnings` denies `print_stdout`/`print_stderr` independently; a mistake that in a
  dynamically-typed stack ships silently is a compile failure here.
- **`BE-0010`, `BE-0016`** — the `Http`/`ids` seams are enforced by grep-able bans, but the reason a
  bypass is *catchable at all* is that Rust's type system makes "wrong seam" a type error, not a
  runtime surprise three services downstream.

Microsoft's internal research (cited in the article) attributes roughly 70% of their historical CVEs to
memory-safety issues — the class Rust's borrow checker eliminates at compile time. For a codebase where
an increasing share of commits are agent-generated and reviewed at agent speed, moving that category of
bug from "found in review or production" to "doesn't compile" is the whole point.

## Performance and resource cost

- Rust lands within a few percent of C on CPU-bound workloads while paying none of C's memory-safety
  tax — no GC, no runtime bounds-check-everywhere tax paid at the language level.
- The article's examples are directly analogous to this stack's shape: Hugging Face's `tokenizers`
  (10–100x over pure Python) and Polars vs. pandas are both "hot path rewritten under a Python-facing
  surface" — the same shape as `stack.md`'s Dagster/Rust split (below).
- Practically for midian: `axum`/`tokio` gives no-GC-pause streaming (SSE, `BE-0015`) and predictable
  tail latency under load, which is the literal justification already in `stack.md`'s table — the
  article supplies the general case, midian's SSE routes are the specific one.

## Zero-cost abstractions — complexity absorbed at compile time, not deferred to runtime

`async`/`await` in Rust compiles to a state machine with no allocation or scheduler overhead beyond what
tokio needs to run it — the abstraction costs nothing you didn't already ask for. This is the same
philosophy `stack.md` applies architecturally: where a crate uses `sqlx`'s `query!` macro, it verifies
that SQL against the live schema **at compile time**, so a query with a typo'd column or a type mismatch
is a build failure, not a 2 a.m. page — the compiler's guarantee, not `midas check`'s. `BE-0018`
recommends adopting `query!` (`review`-tier, `ledgered`); it isn't gate-enforced, and midian currently
deviates from it (runtime-checked `sqlx::query` instead — see `midas.toml [deviations]`), so the
guarantee only applies where the macro is actually used. That's still a categorically different failure
mode than a dynamically-typed stack, where the same class of bug needs a runtime test *and* a human
catching the gap in that test.

## Rust doesn't replace Python here — it's already partitioned that way

The article's other main point: Python keeps winning research/experimentation (PyTorch's ecosystem
gravity is real), while production infrastructure increasingly moves to Rust — not a replacement, a
specialization by workload. `stack.md` already encodes exactly this split, independently of the
article, as a `hard` rule:

> **LLM / data pipeline: Dagster.** Generation/heavy data work belongs in the pipeline; the API only
> *serves* generated data. `hard` boundary — LLM generation does not belong in the request path of the
> serving backend.

Read against the article, this isn't a coincidence — it's the same "experiment in the loose language,
serve from the strict one" boundary the industry is converging on, applied to this repo's own
pipeline/serving split. It's a load-bearing reason `STK-0001` and the Dagster row are both `hard`/near-
`hard`: swapping either would blur a boundary that's earning its keep twice.

## What the article doesn't paper over — and how this repo already answers it

The article is explicit that Rust's costs are real, not marketing. Each has a concrete answer already
built into `midas`, not asserted here:

| Cost (per the article) | This repo's answer |
| --- | --- |
| Steep learning curve, ownership model | `midas touch` scaffolds conformant Rust (`BE-`/`CLI-` conventions) so authors — human or agent — inherit working patterns instead of learning ownership from a blank file. |
| Ecosystem gaps vs. Python's ML libraries | Already partitioned away — see the Dagster/Rust split above; Rust never has to touch the ML ecosystem. |
| Slower iteration (compile time) | `rust-toolchain.toml` pins a known-fast toolchain; the workspace ships templates (`templates/rust-service`) tuned for build speed rather than discovering the tradeoff per-project. |
| Small talent pool | Mitigated, not solved — `standards/backend/conventions.md` + `midas check` reduce how much "knowing Rust" an author needs versus "knowing the seams." |
| Not every workload justifies it | `stack.md`'s swap condition is explicit: *"a throwaway spike or FaaS glue job where Rust's build cost isn't worth it"* — the standard already carves out the exception rather than pretending it doesn't exist. |

## Industry direction, for context not proof

Microsoft has stated a goal of eliminating new C/C++ in favor of Rust by 2030; the Rust Foundation
positions the language for "ultra-reliable" systems generally. This repo doesn't lean on that as
justification — the compile-time-correctness and pipeline/serving arguments above stand on their own —
but it's evidence the bet isn't idiosyncratic to this repo.

## Bottom line

`STK-0001` isn't a taste preference carried over from midian's Go→Rust port
(`standards/playbooks/go-to-rust.md`). It's that the property Rust's compiler enforces — reject the
whole bug class before it runs, not after a human or an agent reviews it — matters more, not less, as
the share of code written by an agent instead of a person goes up. Python stays the right tool where
this repo already puts it (the Dagster pipeline); Rust stays the right tool where this repo already puts
it (everything serving a request). The article's argument is that this split is where the whole industry
is heading; `stack.md` already encodes it as the default.
