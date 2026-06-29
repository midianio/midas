//! Minimal ANSI styling — no external dependency. The `color` flag is resolved once (from
//! `--no-color` / `NO_COLOR` / TTY detection) in [`crate::output::Output::new`].

#[derive(Clone, Copy, Debug)]
pub struct Style {
    color: bool,
}

impl Style {
    pub fn new(color: bool) -> Self {
        Self { color }
    }

    pub fn enabled(&self) -> bool {
        self.color
    }

    fn wrap(&self, code: &str, s: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }

    pub fn bold(&self, s: &str) -> String {
        self.wrap("1", s)
    }
    pub fn dim(&self, s: &str) -> String {
        self.wrap("2", s)
    }
    pub fn red(&self, s: &str) -> String {
        self.wrap("31", s)
    }
    pub fn green(&self, s: &str) -> String {
        self.wrap("32", s)
    }
    pub fn yellow(&self, s: &str) -> String {
        self.wrap("33", s)
    }
    pub fn blue(&self, s: &str) -> String {
        self.wrap("34", s)
    }
    pub fn cyan(&self, s: &str) -> String {
        self.wrap("36", s)
    }
}
