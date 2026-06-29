use crate::core::exit::CliError;
use crate::core::global::GlobalArgs;
use crate::core::output::Output;
use std::io::{BufRead, IsTerminal, Write};

/// Free-text prompt with the same agent-safe contract as [`crate::core::confirm`]:
///
/// - interactive (TTY) → prompts on stderr, reads stdin; empty input takes `default` if any.
/// - **not** a TTY → returns `default` if one was given, else a usage error (exit 3) so an agent
///   never hangs on a prompt it can't answer.
pub fn prompt_line(
    out: &Output,
    _g: &GlobalArgs,
    label: &str,
    default: Option<&str>,
) -> Result<String, CliError> {
    let interactive = std::io::stdin().is_terminal() && std::io::stderr().is_terminal();
    if !interactive {
        return match default {
            Some(d) => Ok(d.to_string()),
            None => Err(CliError::usage(format!(
                "{label} required — pass it as an argument (no TTY to prompt)"
            ))),
        };
    }

    let hint = default.map(|d| format!(" ({d})")).unwrap_or_default();
    {
        let mut e = std::io::stderr().lock();
        let _ = write!(e, "  {} {label}{hint}: ", out.style.cyan("?"));
        let _ = e.flush();
    }
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(CliError::from)?;
    let v = line.trim().to_string();
    if v.is_empty() {
        match default {
            Some(d) => Ok(d.to_string()),
            None => Err(CliError::usage(format!("{label} is required"))),
        }
    } else {
        Ok(v)
    }
}
