use clap::Args;
use std::path::PathBuf;

/// Flags every midian CLI command inherits (CLI-0001..0004). Marked `global` so they may appear
/// before *or* after the subcommand: `midas --json check` and `midas check --json` both work.
#[derive(Args, Debug, Clone, Default)]
pub struct GlobalArgs {
    /// Emit machine-readable JSON to stdout (stable, documented schema).
    #[arg(long, global = true)]
    pub json: bool,

    /// Project root for project-scoped commands (default: the nearest midas.toml walking up from
    /// the cwd, else the git toplevel, else the cwd).
    #[arg(long, global = true, value_name = "DIR", value_parser = parse_root)]
    pub root: Option<PathBuf>,

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

/// Validate `--root` at parse time so a typo'd path is a clap-level usage error.
fn parse_root(s: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(s);
    if p.is_dir() {
        Ok(p)
    } else {
        Err(format!("not a directory: {s}"))
    }
}
