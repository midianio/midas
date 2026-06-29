# The midas standard — spec

> **Status:** v1 (2026-06-25). Architecture resolved via a full design grill. This is the
> meta-document: what the standard *is*, how it's structured, how projects consume it, how it's
> enforced, and how it evolves. The conventions live under [`standards/`](./standards/); the
> executable surface is the [`midas`](#5-the-midas-cli) CLI. `midas` is both the **repo** (the source
> of truth) and the **binary** (the tool).

---

## 1. Purpose

One answer to *"how do we build a midian project?"* — for humans and AI agents alike — that a new
repo **inherits** instead of rediscovers.

Four properties, non-negotiable:

1. **Extracted, not invented.** midian already converged on strong patterns. The standard *codifies
   what works*; every convention cites canonical code that runs in production. ([§2](#2-the-evidence-base))
2. **Dual-audience by construction.** The same rule is executable by a human typing a command and an
   agent calling a tool — because **skills/agents wrap the `midas` CLI** rather than reimplement it.
   One deterministic path; zero drift between "how Matt does it" and "how the agent does it." ([§5](#5-the-midas-cli))
3. **Recommended defaults + escape hatches.** Opinionated by default, but *deviation is a recorded
   act* — a project that breaks a rule ledgers which rule and why. Accidental drift and intentional
   deviation are distinguishable. ([§7](#7-drift--versioning))
4. **Versioned, embedded, drift-aware.** The standard is SemVer'd by git tag; the `midas` binary
   **embeds its version's rules** (binary-version == standard-version); `midas check` reports drift
   both directions; a later `midas upgrade` carries a project forward with codemods. ([§7](#7-drift--versioning))

**Non-goals:** a runtime framework every project boots through, or a rigid template you can't escape.
A kit, not a cage.

---

## 2. The evidence base

Not greenfield. The standard extracts two mature bodies of practice — and it already has **real
consumers**, not hypothetical ones.

**The portfolio (`/Users/mattrs/projects/midian.io/`):**

| Project | Stack | Role for the standard |
| --- | --- | --- |
| **midian** | Rust(`api-rs`) + Svelte(`web`), mid Go→Rust port | The reference. Conventions are extracted here. |
| **prayer** (`@midian/prayer`) | Go + Svelte (same `app/`+`db/`+`turbo`+`bun` monorepo), **slated to port to Rust** | The **validating second consumer**. Same frontend/process/agent stack *today*; backend conforms once it ports. |
| **orca** | Dagster / Python pipeline | **Partial** consumer — only the stack-agnostic layers (process/ops, agents) apply; the Rust/Svelte L2 conventions do not. |
| **scripts/midflow** | Go CLI | The existing release-flow tool — **ported into `midas`** ([§5](#5-the-midas-cli)). |

Why this matters: a "standard" extracted from a *single* codebase is just that codebase's
conventions wearing a hat. **prayer** is what converts midian's conventions into *the* standard — run
`midas check` against both and every disagreement is either a real convention to pin or a deviation
to ledger. And because prayer is Go→Rust like midian, the **migration playbook becomes a reusable
artifact** ([`standards/playbooks/go-to-rust.md`](./standards/playbooks/)), validated by its second run.

**Backend (Rust).** Six battle-tested docs in `midian/plans/rust-port/standards/`, live-diff-verified
against the Go oracle on Vitess: one `AppError` + one wire envelope; central auth (`RequireAuth`),
authz (`access::require`), feature-gating (`RequirePlan`/`usage::guard`); two-tier vendor-neutral
observability; `AppState` shared-infra seams (pooled HTTP, task tracker, `with_tx`, `ids`); a
generated OpenAPI→TS contract; clippy-denied `print*`. **Frontend (Svelte).** Strong de-facto
conventions in `midian/app/web`, now written up (+refined) in
[`standards/frontend/conventions.md`](./standards/frontend/conventions.md). **CLI.** No standard
existed; the `midflow`→`midas` port *defines* one ([`standards/cli/`](./standards/cli/)).

---

## 3. Architecture of the standard

**`midas` is one versioned source** — docs, the machine-readable registry, and the CLI source (the
binary and its internal contract kernel) — all moving on one SemVer git tag. Consumers pin that tag.
(Codemods and shared packages land later, [§7](#shared-code--packages).)

```
  ┌──────────────────────────  midas repo (the source of truth)  ──────────────────────────┐
  │  standards/   versioned conventions (the WHY)         registry/   machine-readable       │
  │  templates/   runnable project skeletons                 conventions.json (codemods later)     │
  │  packages/    graduated shared seams (none yet); seams start vendored-with-provenance    │
  │  cli/         the one-stop `midas` binary (+ internal core kernel) — EMBEDS its rules     │
  └──────────────────────────────────────────────────────────────────────────────────────────┘
        │ released as one tagged version  vX.Y.Z
        ▼
  midas binary vX.Y.Z   ── installed in each project + CI (self-contained; no repo fetch to run)
        │ governed by
        ▼
  each project's  midas.toml   ── pins `midas = "X.Y.Z"`, ledgers deviations, declares stack state
        │ depends on (git-tag pins → midas repo @ vX.Y.Z)
        ▼
  midian · prayer  ── run `midas flow/check/sync`; vendor shared seams with provenance
```

| Artifact | What | How consumed |
| --- | --- | --- |
| **`standards/`** | Versioned markdown: rules, rationale, canonical-code citations, enforcement tier. | Read by humans + agents; the managed block ([§8](#8-enforcement)) points each repo here. |
| **`registry/`** | `conventions.json` (ID → metadata, escape policy, enforcement tier, check id); codemod manifests later. | **Embedded into the `midas` binary** at build; drives `check`. |
| **`templates/`** | Runnable skeletons (`rust-service` + `svelte-app`, built & verified), embedded in the binary. | Laid down by `midas new --profile <p>`. |
| **`packages/`** | Home for **graduated** shared seams — empty until a behavioral seam earns it. Seams start **vendored-with-provenance** in each consumer, graduating by evidence. | Vendored seams copied + provenance-stamped; a graduated package imported via a **git-tag pin** ([§7](#shared-code--packages)). |
| **`cli/` → `midas`** | One Rust binary (absorbs midflow), built on its internal `core` contract kernel. The single one-stop CLI. | Run by humans; called by agents/skills. |

**Copy the shape, depend on the mechanism — staged by readiness.** Prose conventions (how to
structure a handler) ship as templates you copy and own. Behavioral seams ship as packages — but only
once they've *earned stability* ([§7](#shared-code--packages)). The line moves over time; provenance
markers track it.

---

## 4. The layers (content map)

| Layer | Doc | Applies to |
| --- | --- | --- |
| **L1 · Stack & tooling** | [`standards/stack.md`](./standards/stack.md) | all (per-profile) |
| **L2 · Backend (Rust)** | [`standards/backend/`](./standards/backend/) — conventions + 4 seam docs | Rust services (midian; prayer post-port) |
| **L2 · Frontend (Svelte)** | [`standards/frontend/conventions.md`](./standards/frontend/conventions.md) | Svelte apps (midian, prayer) |
| **L2 · CLI (Rust)** | [`standards/cli/conventions.md`](./standards/cli/) | every midian CLI (`midas` is the reference) |
| **L4 · Process & ops** | [`standards/process.md`](./standards/process.md) | all (incl. orca) |
| **L5 · Agent playbook** | [`standards/agents.md`](./standards/agents.md) | all |
| **Playbook · Go→Rust** | [`standards/playbooks/go-to-rust.md`](./standards/playbooks/) | a *method*, re-run when prayer ports |

The backend docs are **split**: durable Rust conventions in `backend/`; the Go→Rust *migration
method* in `playbooks/`. "Match Go exactly even when Go looks wrong" is correct for a port and wrong
for the standard — so it lives in the playbook, not the conventions.

---

## 5. The `midas` CLI

The executable spine — what makes the standard productive rather than aspirational. **One Rust
binary**, built from this repo, embedding its version's rules. It absorbs `midflow` (a small Go CLI at
`scripts/midflow`, reimplemented in Rust as part of standing up `midas` — the act that *defines* the
CLI standard, [`standards/cli/`](./standards/cli/)).

### Command surface

| Command | Does | Status |
| --- | --- | --- |
| `midas flow <…>` | The release/branch flow (the ported midflow). | **shipped** |
| `midas check` | Lint vs the pinned standard — **mechanical only**; review-tier conventions are delegated to an external agent ([§8](#8-enforcement)). The CI gate. | **shipped** |
| `midas sync` | Materialize the version-stamped managed block into the repo ([§8](#agent-playbook-delivery)). | **shipped** |
| `midas doctor` | Diagnose the dev environment. | **shipped** |
| `midas add state\|migration\|component\|module` | Stamp a conventional piece — deterministic bytes (the `add-*` skills promoted to commands). `module` scaffolds the 4-file backend module + wires `pub mod` into `modules/mod.rs`. | **shipped** |
| `midas add handler\|pane\|…` | The remaining kinds. | next |
| `midas new <name> --profile <p>` | Scaffold a conformant project: `midas.toml` (version-pinned), agent docs with the synced block, starter CI, dir shape — plus runnable skeletons: `rust-service` (`service`) and `rust-service` + `svelte-app` (`app`). | **shipped** |
| `midas dev [names…]` | Run `[dev].processes` concurrently with prefixed streaming output + one-Ctrl-C teardown (each process leads its own group). When `[dev].tunnel`, raises the pscale tunnel (reusing `[flow]` + the paired branch) first. Replaces `turbo run dev` + the tunnel sidecar. | **shipped** |
| `midas setup` / `midas teardown` | Bootstrap / tear down local dev (deps, pscale proxy, env, hooks). | later |
| `midas gen types` | Regenerate the TS client from the backend OpenAPI. | later |
| `midas upgrade [--to <ver>]` | Move to a newer standard version; run codemods; report residuals. | deferred |

### Skills wrap the CLI

> A skill becomes a **thin wrapper that shells out to `midas`.** The agent's `add-module` skill
> *runs* `midas add module`; a human runs the same command. One implementation of "how you add a
> module," hit by both audiences. The skill's job shrinks to *knowing when* to call and *interpreting*
> the result — not *being* the procedure. (Property #2 of [§1](#1-purpose), made concrete.)

### Agent-runnable is a hard rule

Every `midas` command obeys the CLI standard's first commandment (`CLI-0001`): **non-interactive by
default, `--json` with a stable schema, stdout=data / stderr=logs, typed exit codes.** An
interactive-only command is invisible to agents — which defeats the entire point. Enforced by
construction via `midas`'s internal `core` contract kernel (clap derive), shared by every command.
Full design: [`standards/cli/conventions.md`](./standards/cli/).

---

## 6. Convention entries — the unit of the standard

Every rule has a **stable ID** and an **enforcement tier**, so projects can ledger deviations and
tooling can reason about drift. Format:

```
### BE-0010 · Outbound HTTP only through the pooled Http seam
- Status:     adopted (since 0.1)
- Tier:       check          ← check (mechanical) | review (semantic)
- Escape:     hard           ← hard | ledgered | advisory
- Rule:       Never reqwest::Client::new() in a handler/module; use st.http.execute(Tier, …).
- Why:        Per-call timeout + retry/backoff; a bare client has neither (that was a live bug).
- Enforced:   midas check (AST: banned `reqwest::Client::new` outside src/http.rs)
- Canonical:  app/api/src/http.rs
```

ID prefixes: `STK-`, `BE-`, `FE-`, `CLI-`, `OPS-`, `AGT-`. **Enforcement tier** is `check`
(mechanically verifiable → `midas check`) or `review` (semantic → human/agent review, [§8](#8-enforcement)).
**Escape policy** is `hard` / `ledgered` / `advisory`. The machine-readable mirror is
`registry/conventions.json` (embedded in the binary). Seed catalog: [`standards/README.md`](./standards/README.md).

---

## 7. Drift & versioning

The package-manager model. midas.toml is the lockfile; `midas` is the resolver; the binary is the
materialized standard.

### The manifest — `midas.toml`

```toml
[standard]
version = "0.4.1"          # pins midas: CLI + embedded rules + shared-package versions (one git tag)
profile = "app"            # service | app | library | pipeline

[stack]                    # per-layer current/target; midas check runs a layer vs its CURRENT stack
backend.current  = "go"    # prayer: still Go …
backend.target   = "rust"  #         … porting to Rust (playbook applies)
frontend.current = "svelte"

[check]
semantic_strict = false    # opt-in; surfaced in `midas check --json` for the external review agent /
                           # CI to gate on — `midas check` itself never blocks on semantic concerns

[flow]                     # the ported-midflow config (was hardcoded in midflow); defaults reproduce it
trunk         = "dev"      # midflow's MainBranch (this repo overrides to "main")
pscale_org    = "midian"
pscale_db     = "application"
pscale_parent = "dev"
pscale_region = "us-east"
tunnel_port   = 3309
# api_env_local / state_file / env_marker — the paths midflow used; overridable per repo

[deviations]               # intentional, ledgered escape hatches: convention ID → reason
"FE-0004" = "web-only — no Capacitor adapter switch"
"BE-0012" = "Vitess can't enforce FKs; integrity is checked in the access seam instead"
```

`midas check` treats `[deviations]` as expected; anything else that fails a `check`-tier rule is real
drift. A `hard`-escape rule can't be ledgered away — a deviation entry against one is itself an error.
A layer whose `current` stack the standard doesn't cover (prayer's Go backend) is
**not-applicable** — not drift, not a deviation.

### Versioning — one tag governs everything

`midas` is SemVer'd by git tag; that tag governs the CLI binary, the embedded rules, *and* the shared
package versions. One knob.

- **patch** — clarified prose / advisory entry / doc fix. No action.
- **minor** — new `ledgered`/`hard` convention, template piece, or package version. `check` warns; a
  future `upgrade` offers codemods.
- **major** — a convention's meaning changed or a default flipped. A future `upgrade` is required;
  codemods may be partial (residuals reported).

### Three drift directions (all reported by `midas check`)

1. **Project-behind** — the standard moved ahead. "On 0.4.1; 0.5.0 adds OPS-0019" (a future `midas
   upgrade` carries it forward). Heads-up, never a hard fail (unless a new `hard` rule landed).
2. **Project-ahead / divergent** — the project violates a `check`-tier rule of its *own pinned*
   version. The real failure: fix it, or *ledger it* in `[deviations]` with a reason.
3. **Local invention** — a pattern the standard doesn't cover. `midas check --suggest` surfaces it as
   a **candidate convention to promote upstream** ([§9](#9-how-the-standard-evolves)).

### Embed, don't fetch

The binary **embeds** its version's `registry` + check-rules + managed-block templates. So `midas
check` is self-contained — install one binary in CI, no repo fetch, **no skew between the checker and
the rules it checks** (the same move as "the OpenAPI envelope *is* the wire struct"). midas-repo is
consulted by *authors* and by a future `midas upgrade` (to fetch the newer binary + codemods), not per check.

### Shared code — packages

Behavioral seams are **vendored-with-provenance by default**, not shared up front — both midian and
prayer are mid-port, so nothing is genuinely churn-free yet, and coupling cost scales with churn:

- **Nothing is shared from day one** — not even the CLI contract. The agent-runnable kernel
  (global flags, the `Output` writer, exit-code mapping, `confirm`, the config loader) lives *inside*
  the one `midas` binary (`cli/src/core/`), not as a shared crate: `midas` is the single one-stop CLI,
  so there's no second consumer to share it with. `packages/` starts **empty**; a seam lands there
  only by graduating (below).
- **Everything starts vendored.** Both the would-be primitives (`ids`/`generateId`, byte-exact
  SSE framing, the response envelope, `AppError`, the `Http` retry-tier client, the telemetry ports +
  scrub, `with_tx`, `Tasks`; frontend `generateId()`, platform detection, `screen`) *and* the
  still-moving seams (`access`, feature-gating, the `api<T>()` wrapper, the pane system) are copied
  into each consumer and stamped `// midas:provenance <conv-id> <sha>`. A (future) provenance-drift
  check flags when a vendored copy has **drifted from the canonical version** — propagation *signal*
  without release coupling.
- **Graduation is by evidence, not calendar:** a vendored seam becomes a real package once it has
  stayed unchanged across *both* consumers long enough to prove it settled. `"copy the shape and the
  mechanism, with provenance, until stable; then depend."`
- **No publish infra day one:** packages live in this repo (Cargo + npm workspaces); consumers add
  **git-tag deps** pointing at midas @ the pinned version.

---

## 8. Enforcement

Two arms, declared per convention by its tier — but they run in **different places.** `midas check`
runs the **mechanical** arm only and owns the blocking gate; the **semantic** arm is delegated to
whatever review agent the team runs (Cursor, Claude, CodeRabbit, …), which reads `standards/` and
consumes `midas check --json`. `midas check` partitions its output and **counts** the delegated
review-tier conventions without running them.

```
MECHANICAL  (deterministic · gates CI · exit 2)
  ✓ BE-0010  no raw reqwest client in handlers
  ✗ FE-0010  raw crypto.randomUUID()      app/web/src/lib/x.ts:42
SEMANTIC    (review-tier · delegated out-of-process — count only)
  · 7 review-tier conventions apply — see `standards/`; run your review agent over the diff
```

- **Mechanical (`check` tier)** — structure, banned-call (regex/substring + allow-list + globs),
  artifact-hash drift, vendored-copy provenance drift, clippy. Deterministic. **Owns the blocking
  exit code (`2`).** Implemented today: banned-call + file-structure; artifact-hash, provenance-drift,
  and clippy are carried in the registry but reported `skipped` for now. The drift model + deviation
  ledger apply here.
- **Semantic (`review` tier)** — not machine-checkable ("handler is thin", "no business logic in a
  component", "PII scrubbed at the boundary"). **Delegated out-of-process** to the team's review
  agent, which reads the convention in `standards/` and the mechanical baseline from `midas check
  --json`. `midas check` only *counts* the applicable review-tier conventions; it runs no agent.

### Inverted — `midas` is the tool, the agent is the host

The earlier design had `midas` embed an agent adapter and *drive* the semantic pass. That is
**inverted**: the agent platform is the **host**, `midas` is a **tool** it invokes (`midas check
--json`) plus **docs** it reads (`standards/`). Whatever review agent the team already runs — Cursor,
Claude, CodeRabbit, Copilot — combs the diff itself; the binary ships **no agent and no adapter.**
Why: combing a PR for high-value review points is a **fast-commoditizing** capability, so we
**buy/wrap it, not build it.** The defensible, build-it-ourselves core is the other half — the
deterministic, convention-specific mechanical checks plus conformant-by-construction scaffolding.
Because the semantic pass is non-deterministic *and* out-of-process, it can never block the `midas
check` gate.

### Determinism owns the gate

The semantic arm runs out-of-process and is non-deterministic, so it **can't block the `midas check`
gate** — only mechanical drift does. `midas check` emits `0/1/2/3`:

| Exit | Meaning | CI |
| --- | --- | --- |
| `0` | clean | pass |
| `1` | tool error | fail |
| `2` | **mechanical drift** | **fail (blocks)** |
| `3` | usage error | fail |

Exit `4` (advisory) stays in the shared CLI exit-code taxonomy (`standards/cli/conventions.md`) for
tools that *do* have an in-process advisory arm — but `midas check` never emits it. `[check]
semantic_strict` is surfaced in `--json` for the external review agent / CI to escalate its own
findings to blocking; `midas check` itself stays mechanical.

### Agent-playbook delivery

The conventions + the agent's marching orders need to reach every repo without clobbering its existing
agent config (`midian/CLAUDE.md` is GitNexus-specific; each repo has its own `AGENTS.md`, `.cursor`):

- **`midas sync` writes a version-stamped managed block** — `<!-- midas:0.4.1 -->…<!-- /midas -->` —
  into each repo's `CLAUDE.md` / `AGENTS.md` / `.cursor` rules: naming the pinned version, pointing at
  the conventions, instructing the agent to treat `midas check`/`add` as source of truth. **Project
  content lives outside the block, untouched.** A stale/missing block is `check`-tier drift.
- **Executable skills ship as a versioned bundle** (delivery via `midas setup`, planned) — thin
  wrappers over the binary, so the sync surface is tiny (logic lives in `midas`, not the skill). Full
  design: [`standards/agents.md`](./standards/agents.md).

---

## 9. How the standard evolves

- **Source of change is practice.** `midas check --suggest` surfaces local inventions; promoting one
  is the normal path to a new entry.
- **RFC-lite.** A change to a `hard`/`ledgered` entry is a short PR against a `proposed` entry
  (`proposed` → `adopted` → `deprecated`); each entry has an owner.
- **The standard governs itself.** This repo uses its own `midas flow` + `midas check`. Dogfooding is
  the test.
- **Deprecation is explicit + dated** — a `deprecated` entry names its replacement and removal version
  so a future `upgrade` can codemod toward the successor.
- **Packages graduate** vendored→shared **by evidence**, as they stabilize ([§7](#shared-code--packages)).

---

## 10. Rollout

Extract first, enforce last. Never gate before the conventions are real and *both* midian and prayer
pass them.

| Phase | Deliverable | Exit criterion |
| --- | --- | --- |
| **0 · Extract** | This spec; backend docs **split** into `backend/` + `playbooks/go-to-rust.md`; frontend, CLI, process, agents docs; seed `registry/conventions.json`. | `standards/` is the readable source of truth; midian + prayer practice catalogued. |
| **1 · Observe** | `midas check` (read-only) against **both midian and prayer**; `midas.toml` in each, pinning 0.1. No enforcement. | `check` runs clean (or every failure ledgered) on both — the disagreements are the signal. |
| **2 · Scaffold + CLI** | Build `midas` in Rust: `midas flow` (ported midflow), mechanical `midas check`, `midas sync`, `midas doctor`, `midas add` (`state`/`migration`/`component`/`module`), and `midas new` (profile-based project init, which lays down runnable `rust-service` + `svelte-app` skeletons) **shipped** on the internal `core` contract kernel. | Human + agent scaffold a piece via the identical command. |
| **3 · Share** | Every shared seam **vendored-with-provenance**, drift-flagged; a seam graduates from a vendored copy to a real `packages/` package only on evidence (stable across both consumers). | A vendored-seam divergence is flagged in both consumers; the first seam graduates. |
| **4 · Reconcile** *(deferred)* | `midas upgrade` + codemods; the external review agent wired against `midas check --json` (no in-binary adapter). | A version bump carries both projects forward with one command. |
| **5 · Prayer ports** | prayer re-runs `playbooks/go-to-rust.md` → its backend conforms to `backend/`. | The migration playbook is validated by a second run; the backend standard has two conformant consumers. |

---

**Resolved:** manifest = **`midas.toml`** (matches the tool, like `Cargo.toml`). The semantic reviewer
is **inverted** — `midas` is a tool the team's review agent invokes (`midas check --json`) plus docs it
reads (`standards/`); the binary ships no agent. `midas check` is mechanical-only and owns the gate
([§8](#8-enforcement), [`cli/README.md`](./cli/README.md)).

Still open:

1. **Conformance profiles axis** — `service | app | library | pipeline`, or finer (`app-web` vs
   `app-native`)?
2. **Package graduation criteria** — what evidence concretely promotes a vendored seam to a package (N
   releases unchanged across both consumers? a manual call?).
3. **Provenance-drift check** — carried in the registry but not yet implemented in the engine (reported
   `skipped`); design the canonical-version lookup that flags a drifted vendored copy.

---

## Appendix — repo layout

```
midas/
├── SPEC.md  README.md  BUILD.md
├── Cargo.toml               ← workspace (one member: the midas binary)       (built)
├── standards/
│   ├── README.md            ← layer map + seed catalog
│   ├── stack.md             ← L1
│   ├── backend/             ← L2: conventions.md + authorization/feature-gating/observability/openapi
│   ├── frontend/conventions.md   ← L2
│   ├── cli/conventions.md   ← L2 (the CLI standard, extracted from building midas)
│   ├── process.md           ← L4
│   ├── agents.md            ← L5
│   └── playbooks/go-to-rust.md   ← reusable migration method (prayer re-runs)
├── templates/               ← rust-service/ · svelte-app/ (both built)       embedded in the binary
├── packages/                ← graduated shared seams (empty until a seam earns it)
├── cli/                     ← the `midas` binary + internal core kernel (cli/src/core/)   (built)
└── registry/                ← conventions.json (codemods later) — embedded in the binary  (built)
```

Dirs reached by later phases are *named so the destination is unambiguous*, not created empty.
