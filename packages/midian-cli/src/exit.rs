use crate::output::Output;
use std::fmt;

/// Result type every command returns. `Ok(())` → exit 0.
pub type CliResult = Result<(), CliError>;

/// Typed command failure → typed process exit code (CLI-0004). A script/agent branches on the
/// code: `2` means "ran fine, the answer is no/dirty/drift" — distinct from `1` "the tool broke".
#[derive(Debug)]
pub enum CliError {
    /// Internal / tool failure (a bug, an IO error). **Exit 1.**
    Tool(anyhow::Error),
    /// Expected negative result — drift found, "no", dirty worktree. A *clean* non-zero. **Exit 2.**
    Expected(String),
    /// Usage error — bad args, or would-prompt with no TTY and no flag. **Exit 3.**
    Usage(String),
    /// Advisory finding — non-blocking (e.g. semantic concerns). **Exit 4.**
    Advisory(String),
}

impl CliError {
    pub fn code(&self) -> i32 {
        match self {
            CliError::Tool(_) => 1,
            CliError::Expected(_) => 2,
            CliError::Usage(_) => 3,
            CliError::Advisory(_) => 4,
        }
    }

    pub fn tool(e: impl Into<anyhow::Error>) -> Self {
        CliError::Tool(e.into())
    }
    pub fn expected(msg: impl Into<String>) -> Self {
        CliError::Expected(msg.into())
    }
    pub fn usage(msg: impl Into<String>) -> Self {
        CliError::Usage(msg.into())
    }
    pub fn advisory(msg: impl Into<String>) -> Self {
        CliError::Advisory(msg.into())
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CliError::Tool(e) => write!(f, "{e:#}"),
            CliError::Expected(m) | CliError::Usage(m) | CliError::Advisory(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for CliError {}

impl From<anyhow::Error> for CliError {
    fn from(e: anyhow::Error) -> Self {
        CliError::Tool(e)
    }
}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        CliError::Tool(e.into())
    }
}

/// Map a finished command result to a process exit code. Prints a single human line to stderr for
/// `Tool`/`Usage` failures; `Expected`/`Advisory` are assumed already surfaced via [`Output`].
pub fn finish(out: &Output, r: CliResult) -> i32 {
    match &r {
        Ok(()) => 0,
        Err(e) => {
            match e {
                CliError::Tool(_) => out.error(format!("{e}")),
                CliError::Usage(m) => out.error(format!("usage: {m}")),
                CliError::Expected(_) | CliError::Advisory(_) => {}
            }
            e.code()
        }
    }
}
