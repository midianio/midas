# CLI Conventions

Portable conventions for Rust command-line tools. **Extracted by building `midas` itself** — `midas`
is to CLIs what `app/api` is to services: the reference implementation the rules come from. It began
as the `midflow`→`midas` port (a small Go release-flow CLI, reimplemented in Rust). `midas` is the
**single one-stop CLI** for the project — new capabilities land as subcommands, not as separate tools.

Canonical code: `midas/cli/` — the binary and its internal contract kernel (`cli/src/core/`).
Strip the midas-specifics and keep the patterns. Entries are keyed `CLI-####` with an enforcement
tier — `[check]` (mechanical) or `[review]` (semantic).

## Stack

Rust · **clap** (derive) · the internal **`core`** contract kernel (`cli/src/core/`) · `serde`/`serde_json` (for
`--json`) · `anyhow` (internal error context) → mapped to typed exit codes · `tracing` (to stderr) ·
`assert_cmd` (tests) · single static binary (musl) · `clippy -D warnings`.

## The hard rule: every command is agent-runnable (`CLI-0001`)

The reason midian CLIs exist is that **humans and agents run the same deterministic path**. A CLI
built the normal way silently breaks that — it prompts, prints TTY-only output, mixes data with logs,
and returns `1` for everything. An agent hits the first prompt and hangs. So agent-runnability is the
first commandment, four parts, all `hard`:

- **`CLI-0001` Non-interactive by default `[check]`.** No command *requires* a prompt to complete.
  Every destructive/ambiguous action has a flag path (`--yes`, explicit args). A prompt is an opt-in
  convenience for an interactive human; when stdin/stdout isn't a TTY, the command **must not
  prompt** — it fails with a usage error (exit `3`) naming the flag that would have answered it.
- **`CLI-0002` Dual output `[check]`.** Human-readable by default; `--json` emits a **stable,
  documented schema** on every command that returns data. The JSON schema is part of the CLI's
  contract and is versioned with the standard — agents pass `--json` and parse structurally.
- **`CLI-0003` stdout = data, stderr = everything else `[check]`.** Results to stdout; logs, progress,
  prompts, diagnostics to stderr. So `midas check --json | jq` is always clean, and a human's
  progress bar never corrupts an agent's parse.
- **`CLI-0004` Typed, documented exit codes `[check]`.** Not just `0`/`1`. The shared scheme:

  | Exit | Meaning |
  | --- | --- |
  | `0` | success |
  | `1` | internal/tool error (a bug, an IO failure) |
  | `2` | **expected negative result** (drift found, check failed, "no") — a *clean* non-zero |
  | `3` | usage error (bad args, would-prompt-but-no-TTY) |
  | `4` | advisory finding (non-blocking; e.g. semantic concerns) |

  A script/agent branches on the code; `2` means "ran fine, answer is no/dirty" and is distinct from
  `1` "the tool broke". (Mirrors the backend parity harness's exit-code discipline.)

These four are enforced **by construction**, not per-command discipline — see the core kernel below.

## The `core` contract kernel (`CLI-0005` `[check]`)

Every command is built on the internal `core` kernel (`cli/src/core/`; clap derive + the machinery
that makes `CLI-0001…0004` structural) rather than re-implementing the contract per command. It owns:

- **Global flags** every command inherits: `--json`, `--root`, `--yes`, `--quiet`, `--verbose`,
  `--no-color`. `--root` is the single project-root override — one resolution rule for every
  command (explicit `--root` → nearest `midas.toml` walking up → git toplevel → cwd), never
  re-implemented per command.
- **An `Output` writer** — `out.data(value)` serializes to stdout as human text *or* `--json` per the
  flag; `out.progress(msg)`/`out.warn(msg)` go to stderr. A command never touches `println!` directly
  — `midas check` bans it by grep (`CLI-0009`), and CI's `clippy -D warnings` denies
  `print_stdout`/`print_stderr` independently, as in the backend.
- **Exit-code mapping** — commands return a `Result<Outcome, CliError>`; the crate maps `Outcome` /
  `CliError` to the typed codes above. Commands don't call `std::process::exit` themselves; only
  `main` does, via the kernel's `finish()`.
- **`confirm(prompt, flag)`** — prompts only when interactive; when `--yes` is set it returns true;
  when non-TTY and the flag is absent it errors with exit `3`. This is the single chokepoint that
  makes "non-interactive by default" impossible to forget.
- **TTY + color detection**, `tracing` wired to stderr respecting `--quiet`/`--verbose`/`RUST_LOG`.
- **The config loader** (below).

Because the contract lives in the kernel, a new command is agent-runnable the moment it compiles —
the same structural-correctness move as the backend's `response.rs`/`AppError`.

## Command structure (`CLI-0010` `[review]`)

- **Noun-first grouping** for multi-domain tools: `midas flow ship`, `midas touch module`, `midas check`.
  Subcommands are kebab-case; the noun is the area, the verb is the action.
- **No name may mean two things.** A verb reused across groups must be the *same operation* (or be
  renamed): `flow rebase` was born `flow sync` and collided with top-level `sync` (agent docs) —
  a muscle-memory trap. Renames keep the old spelling as a hidden alias for a release.
- **Flag naming:** `--root` = the project root a command *reads/operates on* (global, one
  resolver); `--dir` = the destination a scaffold *writes into*. Don't mint new spellings for
  either. Destructive side effects get named flags (`flow end --delete-data`), never a generic
  `--force` (reserved for "overwrite what's in the way").
- `--help` is complete and accurate on every (sub)command — clap derive gives this for free; keep doc
  comments on every arg. Declaration order is help order: group by usage rhythm (the daily loop,
  then the standards family, then setup/tooling), so `--help` teaches the mental model.
- Prefer explicit args over positional ambiguity for anything an agent generates.

## Config (`CLI-0006` `[review]`)

- **Project config** is read through the shared loader from `midas.toml` at the repo root (the same
  manifest `midas` uses) — one config surface for humans and agents. Respect the sibling-`.env`
  loading order projects already rely on.
- **Tool-global config** (rare) lives under the XDG config dir, never scattered dotfiles.
- Never read secrets from argv (they leak into process listings / shell history) — env or a file.

## Errors, logging, secrets (`CLI-0009` `[check]`+`[review]`)

- Internal errors carry `anyhow` context; the kernel maps them to exit `1` and prints a single
  human-actionable line to stderr (full chain under `--verbose`).
- Logs via `tracing` to **stderr** only; `--json` must never emit a non-JSON byte to stdout.
- **Never** put a token/secret/PII in output or an error message `[review]`. No `print!`/`eprintln!`
  `[check]` — use the `Output` writer.

## Distribution (`CLI-0007` `[review]`)

- **Single static binary** (musl, rustls — no OpenSSL), so it drops into CI and dev machines with no
  runtime deps (matches the backend Docker stance).
- **The binary embeds its standard version** — `midas --version` *is* the standard version it
  enforces (see `SPEC.md §7`). Self-update is `midas upgrade`; other CLIs version independently unless
  they ship rules.
- Built/released from the `midas` repo on the one SemVer git tag.

## Testing (`CLI-0008` `[check]`)

- **Snapshot-test the surface** with `assert_cmd`: assert the human output, the `--json`
  shape, **and the exit code** for the happy path, the negative path (exit `2`), and the usage-error
  path (exit `3`). The `--json` schema test is the contract guard — a breaking schema change fails CI.
- A command that prompts must have a test proving it runs to completion non-interactively with the
  flag and errors (exit `3`) without it under a simulated non-TTY.

## Quality gates

`cargo build`/`test`/`clippy -D warnings` green (clippy denies `print_stdout`/`print_stderr`) · the
`--json` snapshot tests pass · `--help` renders for every subcommand. A command that prompts with no
flag path, writes logs to stdout, or returns `1` for an expected-negative result is review-blocking.

## Reuse

Project-agnostic minus the `midas`-specific commands. The `core` kernel (`cli/src/core/`) keeps the
agent-runnable contract in one place, so it's enforced once, centrally, not re-litigated per command.
A future Rust CLI elsewhere would copy this kernel's shape rather than depend on it — `midas` is the
one CLI here, and the kernel is internal to its binary, not a shared package.
