use clap::Args;

/// Flags every midian CLI command inherits (CLI-0001..0004). Marked `global` so they may appear
/// before *or* after the subcommand: `midas --json check` and `midas check --json` both work.
#[derive(Args, Debug, Clone, Default)]
pub struct GlobalArgs {
    /// Emit machine-readable JSON to stdout (stable, documented schema).
    #[arg(long, global = true)]
    pub json: bool,

    /// Assume "yes" for every confirmation (required for non-interactive / agent use).
    #[arg(long, short = 'y', global = true)]
    pub yes: bool,

    /// Suppress progress and log output on stderr.
    #[arg(long, short = 'q', global = true)]
    pub quiet: bool,

    /// Increase log verbosity (repeatable: -v debug, -vv trace).
    #[arg(long, short = 'v', global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Disable ANSI color even on a TTY.
    #[arg(long, global = true)]
    pub no_color: bool,
}
