use crate::global::GlobalArgs;
use crate::style::Style;
use serde::Serialize;
use std::io::{IsTerminal, Write};

/// The single output chokepoint (CLI-0003): the command's **result** goes to stdout, everything
/// else (progress, steps, warnings, prompts, diagnostics) to **stderr**. In `--json` mode stdout
/// carries only serialized data, so `midas <cmd> --json | jq` is always clean.
#[derive(Clone, Copy)]
pub struct Output {
    json: bool,
    quiet: bool,
    pub style: Style,
}

impl Output {
    pub fn new(g: &GlobalArgs) -> Self {
        let color = !g.no_color
            && std::env::var_os("NO_COLOR").is_none()
            && std::io::stderr().is_terminal();
        Output {
            json: g.json,
            quiet: g.quiet,
            style: Style::new(color),
        }
    }

    pub fn is_json(&self) -> bool {
        self.json
    }

    /// Emit the command's primary result to **stdout**. `--json` → pretty JSON; otherwise the
    /// `human` rendering (computed lazily, so JSON callers pay nothing for it). The only method
    /// that writes to stdout.
    pub fn data<T, F>(&self, value: &T, human: F)
    where
        T: Serialize,
        F: FnOnce(&Style) -> String,
    {
        let mut w = std::io::stdout().lock();
        if self.json {
            let _ = serde_json::to_writer_pretty(&mut w, value);
            let _ = writeln!(w);
        } else {
            let _ = writeln!(w, "{}", human(&self.style));
        }
    }

    /// Write an already-shaped JSON value to stdout (only meaningful in `--json` mode; in human
    /// mode the caller should use [`Output::data`]).
    pub fn json_value(&self, v: &serde_json::Value) {
        let mut w = std::io::stdout().lock();
        let _ = serde_json::to_writer_pretty(&mut w, v);
        let _ = writeln!(w);
    }

    // ---- stderr channels (suppressed by --quiet; never collide with a --json stdout parse) ----

    fn line(&self, s: String) {
        if self.quiet {
            return;
        }
        let mut w = std::io::stderr().lock();
        let _ = writeln!(w, "{s}");
    }

    pub fn banner(&self, title: impl AsRef<str>) {
        self.line(format!("\n{}", self.style.bold(title.as_ref())));
    }
    pub fn step(&self, msg: impl AsRef<str>) {
        self.line(format!(
            "  {} {}",
            self.style.dim("›"),
            self.style.dim(msg.as_ref())
        ));
    }
    pub fn info(&self, msg: impl AsRef<str>) {
        self.line(format!("  {} {}", self.style.cyan("•"), msg.as_ref()));
    }
    pub fn warn(&self, msg: impl AsRef<str>) {
        self.line(format!("  {} {}", self.style.yellow("⚠"), msg.as_ref()));
    }
    pub fn success(&self, msg: impl AsRef<str>) {
        self.line(format!("  {} {}", self.style.green("✓"), msg.as_ref()));
    }
    pub fn hint(&self, msg: impl AsRef<str>) {
        self.line(format!("    {}", self.style.dim(msg.as_ref())));
    }

    /// Errors print even under `--quiet` (a failure must always be visible).
    pub fn error(&self, msg: impl AsRef<str>) {
        let mut w = std::io::stderr().lock();
        let _ = writeln!(w, "  {} {}", self.style.red("✗"), msg.as_ref());
    }
}
