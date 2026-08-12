# Playbook — adopting the DOC family

How an existing repo brings its prose under [`../docs.md`](../docs.md) without a big-bang rewrite.
The target shape is `docs/<kind>.<scope>.<slug>[.<date>].md`, sorted by lifecycle. This playbook is
the **method** that gets a repo with an accumulated `docs/` there.

Proven end-to-end on midian (52 documents, 37 of which turned out to be spent) and on this repo.

The mechanical half — renaming files, injecting frontmatter — is minutes of scripting. The
expensive half is **classification**: deciding whether each document is current truth, a spent
record, or a decision worth keeping. That part requires reading the code to find out whether the
work described actually shipped. Budget accordingly: on midian the renames took ten minutes and
the classification took the rest of the afternoon.

`DOC-0001`/`0002`/`0004` are `hard`, so they can't be ledgered — `midas adopt` will list them as
the worklist, not offer to waive them. `DOC-0003` is `ledgered`, which is your relief valve if
code comments still point at plans mid-migration.

## Step 0 — opt in, and see the damage

Declare your scope vocabulary. This is the switch: without it every `DOC` entry evaluates
`skipped`.

```toml
[docs]
scopes = ["repo", "api", "web", "db", "ops"]
```

Pick scopes that name **subsystems you actually have**. Resist a scope per topic — if you need
more than about seven, you are describing topics, not subsystems.

```sh
midas check --json | jq '.mechanical.results[] | select(.id | startswith("DOC"))'
```

Everything fails. That is expected and it is the worklist.

## Step 1 — classify before you move anything

For each document, answer one question: **has the work it describes already happened?**

Do not trust the document's own status line. On midian, four plans claimed to be pending whose
components had already been deleted, and one `note` claimed thirteen open tickets of which nine
had shipped. Check the tree, not the prose.

| If the doc… | It is a | Goes to |
| ----------- | ------- | ------- |
| describes how something works right now | `ref` | `docs/` |
| records a decision and why, and the decision stuck | `adr` | `docs/decisions/` |
| proposes work that hasn't happened yet | `plan` | `docs/plans/` |
| described work that shipped, or was abandoned | `note` | `docs/archive/` |

Two rules of thumb that resolve most of the ambiguity:

- **A shipped plan is not a `ref`.** It is a `note`, unless it contains a decision that still
  binds — then lift that decision into an `adr` and archive the rest.
- **If it has a date in the title, it is almost certainly a `note`.** Audits, postmortems, cycle
  plans and "pass over X" documents are point-in-time by construction.

Expect the archive to be the biggest directory. That is the correct outcome, not a failure of
the migration — most documentation is a record of a moment.

## Step 2 — move mechanically

Write the old→new mapping as data, then execute it, so the migration is reviewable as a table
rather than a pile of `git mv`. Use `git mv` so history follows the rename.

Set `last_reviewed` on a `ref` to **the date the content last actually changed** (`git log -1
--format=%cs -- <file>`), not today. Stamping today's date on a document you have not re-read is
the one move that defeats `DOC-0004` — it converts a stale doc into a doc that *claims* to be
fresh, which is worse than where you started.

## Step 3 — declare sources on canon docs

Every `canon: true` doc needs `sources:` — the globs it describes:

```yaml
canon: true
sources:
  - app/api/src/modules/**
```

Keep them **narrow**. `sources: ["**"]` makes the doc fail on every commit and trains everyone to
bump the date reflexively. If a doc genuinely describes the whole repo, that is a sign it should
be several docs.

Then check the drift: `midas check` will now tell you which canon docs describe code that has
moved since they were last read. On midian this immediately surfaced a canon glossary documenting
a backend that had been deleted six weeks earlier.

## Step 4 — repoint the citations

`DOC-0003` scans source files for references to `docs/plans/` and `docs/archive/`. Fix them by
pointing at the `ref` or `adr` that survived, or by deleting the reference — a comment that
explains itself does not need a citation at all.

Search wider than you expect. On midian the references lived in `.rs`, `.ts`, `.svelte`, `.css`,
`.mjs`, an iOS `.entitlements` file, a husky hook and `midas.toml`.

If you can't finish this in one pass, ledger it and come back:

```sh
midas deviate DOC-0003 --reason "3 CSS comments still cite the archived design pass; MID-xxx"
```

## Step 5 — write the local rules down

Add `docs/AGENTS.md` — capped at 80 lines by `AGT-0009`, with `owner`/`last_reviewed`/`canon: true`.
It states the grammar, the four kinds, and the frontmatter contract for **your** scopes. `docs/`
and `README.md` are outside DOC's corpus, so this file is where a newcomer learns the local shape.

A `docs/archive/AGENTS.md` earns its place too: one short page saying "nothing here describes the
system as it is now" prevents an agent citing a three-month-old audit as current.

## Step 6 — scaffold from here on

```sh
midas touch doc ref api rate-limiting
```

Never hand-roll a new document (`AGT-0002`). The encoding is not something to half-remember, and
the whole value of the family evaporates the first time a doc is named by guess.

## What you should end up with

On midian: 13 `ref`, 4 `adr`, 8 `plan`, 37 `note`. If your archive isn't the largest directory,
you were probably too generous about what counts as current — go back to step 1 and re-ask
whether the work already shipped.
