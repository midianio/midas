//! `midian-cli` — the shared core every midian CLI is built on.
//!
//! It makes the agent-runnable contract (CLI-0001..0005, see `standards/cli/conventions.md`)
//! *structural* rather than per-command discipline:
//!
//! - **CLI-0001** non-interactive by default — [`confirm`] is the single prompt chokepoint; with no
//!   TTY and no `--yes` it errors with exit 3 instead of hanging.
//! - **CLI-0002** dual output — [`Output::data`] serializes `--json` or renders human text.
//! - **CLI-0003** stdout = data, stderr = logs — [`Output`] is the only writer; data goes to stdout,
//!   every progress/warn/step line to stderr.
//! - **CLI-0004** typed exit codes — [`CliError`] maps to `0/1/2/3/4` via [`exit::finish`].
//! - **CLI-0005** every CLI depends on this crate, so the contract is enforced once, centrally.

pub mod config;
pub mod confirm;
pub mod exit;
pub mod global;
pub mod log;
pub mod output;
pub mod prompt;
pub mod style;

pub use confirm::confirm;
pub use exit::{CliError, CliResult};
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
