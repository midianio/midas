//! `midas dev` — the one-command dev orchestrator. Runs the `[dev].processes` from `midas.toml`
//! concurrently with prefixed, color-coded streaming output, and (when `[dev].tunnel = true`) raises
//! the pscale tunnel — using the `[flow]` config + the paired branch for the current git branch —
//! before the processes start. One Ctrl-C tears the whole group down (each process leads its own
//! process group, so `cargo run`'s child server is killed too, not orphaned).

use crate::core::exit::{CliError, CliResult};
use crate::core::Ctx;
use crate::flow::config::{pscale_branch_from_git, FlowConfig};
use crate::manifest::{DevProcess, Manifest};
use std::io::{BufRead, BufReader, IsTerminal};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// ANSI fg colors cycled across processes; the tunnel always gets the first (blue).
const COLORS: &[&str] = &["34", "36", "35", "32", "33", "31"]; // blue cyan magenta green yellow red

pub fn run(ctx: &Ctx, only: Vec<String>) -> CliResult {
    let cwd = std::env::current_dir().map_err(CliError::tool)?;
    let (manifest, root) = match Manifest::find(&cwd).map_err(CliError::tool)? {
        Some((m, r)) => (m, r),
        None => {
            return Err(CliError::usage(
                "no midas.toml found — run from a midas project",
            ))
        }
    };

    // Build the run list: the tunnel (if enabled) first, then the configured processes.
    let mut procs: Vec<DevProcess> = Vec::new();
    let cfg = FlowConfig::from_manifest(&manifest);
    let mut tunnel_port: Option<u16> = None;
    if manifest.dev.tunnel {
        let branch = tunnel_branch(&manifest, &cfg);
        ctx.out.step(format!(
            "tunnel → {} branch {branch} on :{}",
            cfg.db, cfg.port
        ));
        procs.push(DevProcess {
            name: "db".into(),
            cmd: format!(
                "pscale connect {} {} --org {} --port {}",
                cfg.db, branch, cfg.org, cfg.port
            ),
            cwd: None,
        });
        tunnel_port = Some(cfg.port);
    }
    procs.extend(manifest.dev.processes.iter().cloned());

    // Optional positional filter: `midas dev api web` runs only those (the tunnel always runs).
    if !only.is_empty() {
        procs.retain(|p| p.name == "db" || only.iter().any(|o| o == &p.name));
    }
    if procs.is_empty() {
        return Err(CliError::usage(
            "no [dev] processes configured in midas.toml (add a [dev] section with `processes`)",
        ));
    }

    // Preflight: a JS process whose deps aren't installed dies with `vite: command not found` (127).
    // Install them once, up front, so `midas dev` works straight after `midas touch project`.
    ensure_js_deps(ctx, &procs, &root)?;

    let color = !ctx.global.no_color
        && std::env::var_os("NO_COLOR").is_none()
        && std::io::stderr().is_terminal();
    let width = procs.iter().map(|p| p.name.len()).max().unwrap_or(3);

    // Ctrl-C just flips the flag; the main loop owns teardown (no killing in signal context).
    let shutdown = Arc::new(AtomicBool::new(false));
    {
        let s = shutdown.clone();
        ctrlc::set_handler(move || s.store(true, Ordering::SeqCst)).map_err(CliError::tool)?;
    }

    let mut children: Vec<(String, Child)> = Vec::new();
    let mut group_pids: Vec<i32> = Vec::new();
    let mut readers: Vec<thread::JoinHandle<()>> = Vec::new();

    for (i, p) in procs.iter().enumerate() {
        let prefix = make_prefix(&p.name, width, COLORS[i % COLORS.len()], color);
        let mut child = spawn(p, &root)
            .map_err(|e| CliError::tool(anyhow::anyhow!("spawn {:?}: {e}", p.name)))?;
        group_pids.push(child.id() as i32);

        if let Some(out) = child.stdout.take() {
            readers.push(pipe(out, prefix.clone(), true));
        }
        if let Some(err) = child.stderr.take() {
            readers.push(pipe(err, prefix.clone(), false));
        }
        ctx.out.info(format!("started {}", p.name));
        children.push((p.name.clone(), child));

        // Gate the rest of the processes on the tunnel actually listening, then bring the schema up
        // to date before the app starts (so a fresh/seeded branch has its migrations applied).
        if i == 0 {
            if let Some(port) = tunnel_port {
                if wait_for_port(port, Duration::from_secs(20), &shutdown) {
                    if manifest.dev.migrate {
                        if let Err(e) = crate::cmd::migrate::apply_pending(ctx, &manifest, &root) {
                            ctx.out.error(format!("migrate: {e}"));
                            teardown(&group_pids, &mut children);
                            for r in readers {
                                let _ = r.join();
                            }
                            return Err(e);
                        }
                    }
                } else {
                    ctx.out.warn(format!(
                        "tunnel port :{port} not ready after 20s — starting anyway (skipped migrations)"
                    ));
                }
            }
        }
    }

    // Supervise: announce each process as it exits (so a crash is visible, not silent); stop when
    // Ctrl-C is pressed or every process has exited on its own.
    let mut reported = vec![false; children.len()];
    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        let mut all_done = true;
        for (idx, (name, child)) in children.iter_mut().enumerate() {
            match child.try_wait() {
                Ok(Some(status)) if !reported[idx] => {
                    reported[idx] = true;
                    match status.code() {
                        Some(0) => ctx.out.info(format!("{name} exited (0)")),
                        Some(c) => ctx.out.warn(format!("{name} exited ({c})")),
                        None => ctx.out.warn(format!("{name} terminated by signal")),
                    }
                }
                Ok(Some(_)) => {} // already reported
                _ => all_done = false,
            }
        }
        if all_done {
            break;
        }
        thread::sleep(Duration::from_millis(150));
    }

    if shutdown.load(Ordering::SeqCst) {
        ctx.out.step("shutting down");
    }
    teardown(&group_pids, &mut children);
    for r in readers {
        let _ = r.join();
    }
    Ok(())
}

/// Resolve the tunnel branch: explicit override, else the paired branch for the current git branch,
/// else the `[flow]` parent.
fn tunnel_branch(m: &Manifest, cfg: &FlowConfig) -> String {
    if let Some(b) = &m.dev.branch {
        return b.clone();
    }
    if let Ok(gb) = crate::proc::capture("git", &["branch", "--show-current"]) {
        let gb = gb.trim();
        if !gb.is_empty() && gb != cfg.trunk {
            return pscale_branch_from_git(gb);
        }
    }
    cfg.parent.clone()
}

/// Install JS deps before starting: any process whose cwd has a `package.json` but no `node_modules`
/// gets a `bun install` first (each unique dir once). Runs synchronously with inherited stdio so the
/// user sees install progress, and fails loudly — a missing `bun` or a failed install is a clearer
/// stop than a downstream `command not found`.
fn ensure_js_deps(ctx: &Ctx, procs: &[DevProcess], root: &Path) -> CliResult {
    let mut installed: Vec<PathBuf> = Vec::new();
    for p in procs {
        let dir: PathBuf = match &p.cwd {
            Some(c) => root.join(c),
            None => root.to_path_buf(),
        };
        if !dir.join("package.json").exists() || dir.join("node_modules").exists() {
            continue;
        }
        if installed.contains(&dir) {
            continue;
        }
        installed.push(dir.clone());

        let label = p.cwd.as_deref().unwrap_or(".");
        ctx.out
            .step(format!("installing deps in {label} (bun install)"));
        let status = Command::new("bun")
            .arg("install")
            .current_dir(&dir)
            .status()
            .map_err(|e| {
                CliError::tool(anyhow::anyhow!(
                    "bun install in {label}: {e} — is bun installed? https://bun.sh"
                ))
            })?;
        if !status.success() {
            return Err(CliError::tool(anyhow::anyhow!(
                "bun install failed in {label} ({})",
                status
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".into())
            )));
        }
    }
    Ok(())
}

fn spawn(p: &DevProcess, root: &Path) -> std::io::Result<Child> {
    let dir: PathBuf = match &p.cwd {
        Some(c) => root.join(c),
        None => root.to_path_buf(),
    };
    let mut cmd = shell(&p.cmd);
    cmd.current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Each process leads its own group, so teardown kills its whole tree (e.g. `cargo run`'s server).
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn()
}

#[cfg(unix)]
fn shell(line: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c").arg(line);
    c
}

#[cfg(not(unix))]
fn shell(line: &str) -> Command {
    let mut c = Command::new("cmd");
    c.arg("/C").arg(line);
    c
}

/// Stream a child pipe to our stdout/stderr, one prefixed line at a time.
fn pipe<R: std::io::Read + Send + 'static>(
    stream: R,
    prefix: String,
    to_stdout: bool,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            if to_stdout {
                println!("{prefix} {line}");
            } else {
                eprintln!("{prefix} {line}");
            }
        }
    })
}

fn make_prefix(name: &str, width: usize, color: &str, enabled: bool) -> String {
    let label = format!("{name:>width$}");
    if enabled {
        format!("\x1b[1;{color}m{label}\x1b[0m \x1b[2m│\x1b[0m")
    } else {
        format!("{label} │")
    }
}

/// Poll until the tunnel port accepts a TCP connection (or timeout / shutdown).
fn wait_for_port(port: u16, timeout: Duration, shutdown: &AtomicBool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if shutdown.load(Ordering::SeqCst) {
            return true;
        }
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        thread::sleep(Duration::from_millis(200));
    }
    false
}

/// SIGTERM every process group, give them a moment, then SIGKILL any survivors. On non-Unix, fall
/// back to killing the direct children.
fn teardown(group_pids: &[i32], children: &mut [(String, Child)]) {
    #[cfg(unix)]
    {
        signal_groups(group_pids, libc::SIGTERM);
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if children
                .iter_mut()
                .all(|(_, c)| matches!(c.try_wait(), Ok(Some(_))))
            {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        signal_groups(group_pids, libc::SIGKILL);
    }
    #[cfg(not(unix))]
    {
        let _ = group_pids;
        for (_, c) in children.iter_mut() {
            let _ = c.kill();
        }
    }
    for (_, c) in children.iter_mut() {
        let _ = c.wait();
    }
}

#[cfg(unix)]
fn signal_groups(group_pids: &[i32], sig: i32) {
    for &pid in group_pids {
        // Negative pid → signal the whole process group led by `pid`.
        unsafe { libc::kill(-pid, sig) };
    }
}
