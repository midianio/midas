use crate::core::exit::CliError;
use crate::core::global::GlobalArgs;
use crate::core::output::Output;
use std::io::{BufRead, IsTerminal, Write};

/// The single confirmation chokepoint (CLI-0001). This is what makes "non-interactive by default"
/// impossible to forget:
///
/// - `--yes` → returns `true` without prompting.
/// - interactive (stdin **and** stderr are TTYs) → prompts on stderr, reads stdin.
/// - **not** a TTY and `--yes` absent → fails with a usage error (exit 3) naming the flag, so an
///   agent never hangs waiting on a prompt that will never be answered.
pub fn confirm(
    out: &Output,
    g: &GlobalArgs,
    prompt: &str,
    default_yes: bool,
) -> Result<bool, CliError> {
    if g.yes {
        return Ok(true);
    }
    if !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(CliError::usage(format!(
            "would prompt: \"{prompt}\" — pass --yes to confirm non-interactively"
        )));
    }

    let suffix = if default_yes { "[Y/n]" } else { "[y/N]" };
    {
        let mut e = std::io::stderr().lock();
        let _ = write!(e, "  {} {prompt} {suffix} ", out.style.cyan("?"));
        let _ = e.flush();
    }

    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(CliError::from)?;

    let ans = line.trim().to_lowercase();
    Ok(match ans.as_str() {
        "" => default_yes,
        "y" | "yes" => true,
        _ => false,
    })
}
