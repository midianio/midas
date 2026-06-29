use crate::core::global::GlobalArgs;

/// Initialize `tracing` to **stderr** (CLI-0003/0009): logs never pollute a `--json` stdout parse.
/// Level: `--quiet` → error, default → info, `-v` → debug, `-vv` → trace. `RUST_LOG` overrides.
pub fn init(g: &GlobalArgs) {
    use tracing_subscriber::{fmt, EnvFilter};

    let default = if g.quiet {
        "error"
    } else {
        match g.verbose {
            0 => "info",
            1 => "debug",
            _ => "trace",
        }
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    let _ = fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .try_init();
}
