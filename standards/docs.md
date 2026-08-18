# Documentation

How a repo's prose stays trustworthy. The principle: **a document's directory says how much to
trust it, and its filename says what it is** — so neither a human nor an agent has to open a file
to know whether it describes reality. Entries are keyed `DOC-####` with an enforcement tier.

The failure this exists to prevent is not messy docs. It is a document that *reads* as current,
is *cited* as current, and is wrong — the most expensive artifact a repo can contain, because it
is confidently misleading in exactly the situation where someone is trying to learn.

## The corpus (`docs/`)

`DOC` governs `docs/` and nothing else. A repo opts in by declaring its scope vocabulary:

```toml
[docs]
scopes = ["repo", "api", "web", "db", "ops"]
```

No `[docs] scopes` means no opt-in and every `DOC` entry evaluates `skipped`. There is no
configurable docs root: a repo that wants its prose governed puts it in `docs/`. That is the
whole adoption switch, and it is why this repo moved its own `plans/` under `docs/`.

Two exclusions, neither of them a special case. `AGENTS.md` at any depth stays `AGT-0009`'s — it
is an *instruction* file, not documentation. `README.md` is a rendered entry point, exempt at any
depth.

## The grammar (`DOC-0001` `[check]` `hard`)

```
docs/<kind>.<scope>.<slug>[.<YYYY-MM-DD>].md
```

Fields split on `.`, words inside a field on `-`, so the boundaries are never ambiguous — the
thing that makes `nav-overhaul-plan.md` unparseable.

**Everything in the name is immutable.** Mutable state — status, review dates, supersession —
lives in frontmatter. A plan going from draft to shipped is never a rename and never breaks a
link. This is the load-bearing rule: the moment a filename encodes something that changes, the
encoding starts lying or the links start rotting, and you get to pick which.

| Field | Values |
| ----- | ------ |
| `kind` | `ref` · `adr` · `plan` · `note` — fixed by the standard |
| `scope` | the repo's own subsystems, from `[docs] scopes` |
| date | required for `adr` and `note`; forbidden for `ref` and `plan` |

`kind` is fixed because lifecycle means the same thing in every repo; `scope` cannot be, because
subsystems don't. A doc's `kind` also fixes where it lives, and frontmatter `kind`/`scope` must
agree with the filename — so the name can't drift from the contents.

| Kind | Lives in | Is | Trust it? |
| ---- | -------- | -- | --------- |
| `ref` | `docs/` | how the system works **today** | Yes — a wrong `ref` is a bug |
| `adr` | `docs/decisions/` | one decision + why, frozen at its date | Yes, for *why*. Supersede, never edit |
| `plan` | `docs/plans/` | intended work, not yet true | No — it states intent, not reality |
| `note` | `docs/archive/` | what was true on its date | No — history only |

## State (`DOC-0002` `[check]` `hard`)

Every doc carries `kind`, `scope`, `status`, `owner`, plus per kind:

- **`ref`** → `last_reviewed: YYYY-MM-DD`; `status: current | needs-review`; `canon: true` when
  agents are expected to load and trust it.
- **`adr`** → `decided: YYYY-MM-DD`; `status: accepted | superseded`, plus `superseded_by:` when
  retired.
- **`plan`** → `status: draft | in-flight | shipped | abandoned`. The tracker owns the task list;
  a plan holds the design.
- **`note`** → `recorded: YYYY-MM-DD`; `status: historical`. Immutable — correct the record by
  adding a newer note, never by editing a dated one.

A `canon: true` doc must also declare `sources:` — the globs it describes. That is what makes
staleness computable rather than a matter of opinion.

## Citations (`DOC-0003` `[check]` `ledgered`)

Source files may cite `docs/ref.*` and `docs/decisions/` only — never `docs/plans/` or
`docs/archive/`. Those two move by design, so a code comment pointing at one is a dangling
reference with a delay fuse: it breaks on the day the plan ships, in a file nobody touched.

`ref` and `adr` are the two kinds that *don't* move — a `ref` is kept true, an `adr` is frozen —
which is exactly why they are the only legal citation targets.

Ledgered rather than hard, because it is the one entry that can fail on a file nobody edited:
archive a doc, and an untouched comment becomes a violation. A repo mid-cleanup can ledger it.

**Applied migrations are exempt.** `db/migrations/**` is skipped, because the only way to satisfy
this entry inside one would be to edit a migration that has already run — which `BE-0007` forbids
outright, and which trips the runner's checksum guard. Where a `hard` convention and this one
disagree, this one yields: a stale comment in a frozen file is a smaller problem than a schema
history you can no longer replay.

## Drift (`DOC-0004` `[check]` `hard` on canon)

A `canon: true` doc is stale when any glob in its `sources:` changed **after** its
`last_reviewed` date. Blocking for canon docs, inert for everything else. The same
run also flags a doc that lists another *already stale* canon doc in `sources:` —
fixing the first one rewrites it, which would fail the next check. Report both so
one commit re-reads the cascade.

The trigger is deliberately *change*, not the calendar. A 200-day-old doc about a subsystem
nobody touched is fine; a 3-day-old doc about a module that moved yesterday is not. Calendar
expiry fails PRs unrelated to the doc and — worse — rewards bumping the date without re-reading,
which launders staleness into the appearance of freshness and makes the signal worse than absent.

`last_reviewed` means **someone re-read it**. Bumping it as a side effect of an unrelated edit is
the one way to defeat this entry, and no check can catch it. That one is on the reviewer.

Dates come from committed history, so a doc never fails for a change that hasn't landed.
**CI needs full history.** Drift is computed from git log, so a shallow checkout (the
`actions/checkout` default) can only see the head commit and would date every path to today. The
check detects a shallow repository and reports nothing rather than inventing dates — so a CI job
that wants this enforced must fetch full history (`fetch-depth: 0`). Silence here means "could not
tell", not "clean".

## Adopting

An existing repo migrates via [`playbooks/adopt-docs.md`](./playbooks/adopt-docs.md). The renames
are minutes; the classification — has this work already shipped? — is the real cost.

## Authoring

Scaffold with `midas touch doc <kind> <scope> <slug>` — never hand-rolled (`AGT-0002`). The
encoding is not something to half-remember, and a scaffolded doc is conformant on creation.

## Rules

- **`DOC-0005` `[review]`** — A shipped plan becomes a `note` or an `adr`, never a stale plan.
  When work lands, archive the design or promote its decision. A finished plan that still reads
  as pending is the single most common failure in this whole area.
- **`DOC-0006` `[review]`** — `docs/` root stays small enough to re-read in an hour. If the `ref`
  count climbs past ~15, something in it is really a `note`.
- **`DOC-0007` `[review]`** — Deleting beats archiving when git already has it. Archive only what
  a future reader would otherwise re-derive from scratch.
