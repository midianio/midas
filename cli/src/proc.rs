//! Thin process helpers shared by `flow`, `check` (clippy passthrough), and `doctor`.

use anyhow::{anyhow, bail, Result};
use std::path::PathBuf;
use std::process::Command;

/// Run `program args…`, capture stdout (trimmed). On failure the error carries combined output.
pub fn capture(program: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| anyhow!("{program}: {e}"))?;
    if !out.status.success() {
        let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
        combined.push_str(&String::from_utf8_lossy(&out.stderr));
        bail!("{program} {}: {}", args.join(" "), combined.trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Run `program args…` inheriting this process's stdio (interactive / streamed commands).
pub fn inherit(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|e| anyhow!("{program}: {e}"))?;
    if !status.success() {
        bail!(
            "{program} {} exited with status {}",
            args.join(" "),
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

/// Run and report (combined-output, success) — never errors. For probes like `pscale auth check`.
pub fn try_capture(program: &str, args: &[&str]) -> (String, bool) {
    match Command::new(program).args(args).output() {
        Ok(out) => {
            let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&out.stderr));
            (s.trim().to_string(), out.status.success())
        }
        Err(e) => (e.to_string(), false),
    }
}

/// Resolve a program on `PATH`, returning its absolute path. On Windows a bare name like `git`
/// is only on `PATH` as `git.exe`/`git.cmd`, so each `PATHEXT` extension is tried too.
pub fn on_path(program: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into())
            .split(';')
            .filter(|e| !e.is_empty())
            .map(str::to_string)
            .collect()
    } else {
        Vec::new()
    };
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
        for ext in &exts {
            let candidate = dir.join(format!("{program}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}
