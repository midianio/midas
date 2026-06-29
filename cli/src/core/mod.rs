//! `core` — the agent-runnable CLI contract, in one place.
//!
//! This is the kernel `midas` is built on. It makes the contract (CLI-0001..0005, see
//! `standards/cli/conventions.md`) *structural* rather than per-command discipline. It used to be a
//! separate `midian-cli` crate (back when the plan allowed for many midian CLIs); `midas` is now the
//! single one-stop CLI, so the kernel lives here as an internal module instead of a shared crate.
//!
//! - **CLI-0001** non-interactive by default — [`confirm`] is the single prompt chokepoint; with no
//!   TTY and no `--yes` it errors with exit 3 instead of hanging.
//! - **CLI-0002** dual output — [`Output::data`] serializes `--json` or renders human text.
//! - **CLI-0003** stdout = data, stderr = logs — [`Output`] is the only writer; data goes to stdout,
//!   every progress/warn/step line to stderr.
//! - **CLI-0004** typed exit codes — [`CliError`] maps to `0/1/2/3/4` via [`exit::finish`].
//! - **CLI-0005** one CLI, one kernel — every command is built on this module, so the contract is
//!   enforced once, centrally.
//!
//! A deliberately complete toolkit: some affordances (the exit-4 `advisory` result, the `--json`
//! introspection helpers, the full color palette) exist ahead of their first caller, so the whole
//! contract is on hand the moment a new command needs it — hence the module-level `dead_code` allow.
#![allow(dead_code)]

pub mod config;
pub mod confirm;
pub mod exit;
pub mod global;
pub mod log;
pub mod output;
pub mod prompt;
pub mod style;

pub use confirm::confirm;
pub use exit::CliError;
pub use global::GlobalArgs;
pub use output::Output;
pub use prompt::prompt_line;

/// Per-invocation context handed to every command: parsed global flags + the output writer.
#[derive(Clone)]
pub struct Ctx {
    pub global: GlobalArgs,
    pub out: Output,
}

impl Ctx {
    pub fn new(global: GlobalArgs) -> Self {
        let out = Output::new(&global);
        Self { global, out }
    }

    /// Prompt the user (or auto-yes / fail-fast per [`confirm`]).
    pub fn confirm(&self, prompt: &str, default_yes: bool) -> Result<bool, CliError> {
        confirm(&self.out, &self.global, prompt, default_yes)
    }
}
