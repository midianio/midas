//! `midas dev` — the one-command dev orchestrator. Runs the `[dev].processes` from `midas.toml`
//! concurrently with prefixed, color-coded streaming output, and (when `[dev].tunnel = true`) raises
//! the pscale tunnel — using the `[flow]` config + the paired branch for the current git branch —
//! before the processes start. One Ctrl-C tears the whole group down (each process leads its own
//! process group, so `cargo run`'s child server is killed too, not orphaned).
//!
//! A process with `watch` paths gets the watch-and-restart loop (for anything that doesn't
//! hot-reload itself, i.e. `cargo run`): any change under those paths kills the process's whole
//! tree and respawns it, debounced so a save-burst restarts once. A watched process that exits —
//! e.g. `cargo run` on a compile error — stays down until the next change instead of ending the
//! session. `--no-watch` disables all watchers for the run.
//!
//! Declared `port`s (and the tunnel's) are preflighted before anything spawns: a stale listener
//! would otherwise surface as a mid-startup `AddrInUse` panic, or worse, a dev server silently
//! hopping to another port while everything configured against the real one breaks. A busy port
//! fails the run naming its holder; `--kill-ports` kills the holders and proceeds.

use crate::core::exit::{CliError, CliResult};
use crate::core::Ctx;
use crate::flow::config::{pscale_branch_from_git, FlowConfig};
use crate::manifest::{DevProcess, Manifest};
use notify::Watcher;
use procgroup::Group;
use std::io::{BufRead, BufReader, IsTerminal};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

/// ANSI fg colors cycled across processes; the tunnel always gets the first (blue).
const COLORS: &[&str] = &["34", "36", "35", "32", "33", "31"]; // blue cyan magenta green yellow red

/// Restart no sooner than this after the last change event, so a burst of saves (editor
/// format-on-save, `git checkout`) restarts once, not once per file.
const DEBOUNCE: Duration = Duration::from_millis(300);

pub fn run(ctx: &Ctx, only: Vec<String>, no_watch: bool, kill_ports: bool) -> CliResult {
    let start = crate::manifest::resolve_root(&ctx.global).map_err(CliError::tool)?;
    let (manifest, root) = match Manifest::find(&start).map_err(CliError::tool)? {
        Some((m, r)) => (m, r),
        None => {
            return Err(CliError::usage(
                "no midas.toml found — run from a midas project (or pass --root)",
            ))
        }
    };

    // Build the run list: the tunnel (if enabled) first, then the configured processes.
    let mut procs: Vec<DevProcess> = Vec::new();
    let cfg = FlowConfig::from_manifest(&manifest);
    let mut tunnel_port: Option<u16> = None;
    if manifest.dev.tunnel {
        let branch = tunnel_branch(ctx, &manifest, &cfg);
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
            watch: Vec::new(),
            port: Some(cfg.port),
        });
        tunnel_port = Some(cfg.port);
    }
    procs.extend(manifest.dev.processes.iter().cloned());
    if no_watch {
        for p in &mut procs {
            p.watch.clear();
        }
    }

    // Optional positional filter: `midas dev api web` runs only those (the tunnel always runs).
    if !only.is_empty() {
        procs.retain(|p| p.name == "db" || only.iter().any(|o| o == &p.name));
    }
    if procs.is_empty() {
        return Err(CliError::usage(
            "no [dev] processes configured in midas.toml (add a [dev] section with `processes`)",
        ));
    }

    // Preflight: every declared port must be free before anything spawns — fail (or, with
    // --kill-ports, reclaim) while nothing has started yet.
    ensure_ports_free(ctx, &procs, kill_ports)?;

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

    let mut group = Group::new().map_err(CliError::tool)?;
    let mut children: Vec<(String, Child)> = Vec::new();
    let mut readers: Vec<thread::JoinHandle<()>> = Vec::new();
    let mut prefixes: Vec<String> = Vec::new();

    // File watchers for `watch`ed processes: every relevant event sends the process index down one
    // channel; the supervise loop debounces and restarts. Watchers must outlive the loop.
    let (watch_tx, watch_rx) = mpsc::channel::<usize>();
    let mut watchers: Vec<notify::RecommendedWatcher> = Vec::new();

    for (i, p) in procs.iter().enumerate() {
        let prefix = make_prefix(&p.name, width, COLORS[i % COLORS.len()], color);
        let mut child = spawn(p, &root)
            .map_err(|e| CliError::tool(anyhow::anyhow!("spawn {:?}: {e}", p.name)))?;
        group.register(&child);

        if let Some(out) = child.stdout.take() {
            readers.push(pipe(out, prefix.clone(), true));
        }
        if let Some(err) = child.stderr.take() {
            readers.push(pipe(err, prefix.clone(), false));
        }
        ctx.out.info(format!("started {}", p.name));
        prefixes.push(prefix);
        children.push((p.name.clone(), child));

        if !p.watch.is_empty() {
            match make_watcher(p, &root, i, watch_tx.clone()) {
                Ok(Some(w)) => {
                    ctx.out.step(format!(
                        "watching {} → restart {}",
                        p.watch.join(", "),
                        p.name
                    ));
                    watchers.push(w);
                }
                Ok(None) => ctx.out.warn(format!(
                    "{}: no watch path exists ({}) — running without restart",
                    p.name,
                    p.watch.join(", ")
                )),
                Err(e) => ctx.out.warn(format!(
                    "{}: watch failed ({e}) — running without restart",
                    p.name
                )),
            }
        }

        // Gate the rest of the processes on the tunnel actually listening, then bring the schema up
        // to date before the app starts (so a fresh/seeded branch has its migrations applied).
        if i == 0 {
            if let Some(port) = tunnel_port {
                if wait_for_port(port, Duration::from_secs(20), &shutdown) {
                    if manifest.dev.migrate {
                        if let Err(e) = crate::cmd::migrate::apply_pending(ctx, &manifest, &root) {
                            ctx.out.error(format!("migrate: {e}"));
                            group.teardown(&mut children);
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

    // Supervise: announce each process as it exits (so a crash is visible, not silent); restart
    // watched processes on (debounced) file changes. Stop when Ctrl-C is pressed or — with no
    // watchers armed — every process has exited on its own. With watchers, an all-exited state
    // just waits for the next change (a compile error shouldn't end the session).
    let watching = !watchers.is_empty();
    let mut reported = vec![false; children.len()];
    let mut restart_at: Vec<Option<Instant>> = vec![None; children.len()];
    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }

        // Drain change events into per-process debounce deadlines.
        while let Ok(idx) = watch_rx.try_recv() {
            restart_at[idx] = Some(Instant::now() + DEBOUNCE);
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
        if all_done && !watching {
            break;
        }

        // Fire due restarts.
        for idx in 0..children.len() {
            let due = matches!(restart_at[idx], Some(t) if Instant::now() >= t);
            if !due {
                continue;
            }
            restart_at[idx] = None;
            let (name, child) = &mut children[idx];
            ctx.out.step(format!("{name} changed — restarting"));
            procgroup::kill_tree(child);
            group.unregister(child.id());
            match spawn(&procs[idx], &root) {
                Ok(mut next) => {
                    group.register(&next);
                    if let Some(out) = next.stdout.take() {
                        readers.push(pipe(out, prefixes[idx].clone(), true));
                    }
                    if let Some(err) = next.stderr.take() {
                        readers.push(pipe(err, prefixes[idx].clone(), false));
                    }
                    *child = next;
                    reported[idx] = false;
                }
                Err(e) => ctx.out.error(format!("respawn {name}: {e}")),
            }
        }

        thread::sleep(Duration::from_millis(150));
    }

    if shutdown.load(Ordering::SeqCst) {
        ctx.out.step("shutting down");
    }
    group.teardown(&mut children);
    for r in readers {
        let _ = r.join();
    }
    Ok(())
}

/// Resolve the tunnel branch: explicit override, else the paired branch for the current git branch
/// (when one actually exists on pscale), else the `[flow]` parent. Git-only branch types
/// (`chore`/`docs`/`spike`) never get a paired pscale branch, so connecting to their derived name
/// would fail with "branch … does not exist" — those fall back to the parent (`dev` by default).
fn tunnel_branch(ctx: &Ctx, m: &Manifest, cfg: &FlowConfig) -> String {
    if let Some(b) = &m.dev.branch {
        return b.clone();
    }
    if let Ok(gb) = crate::proc::capture("git", &["branch", "--show-current"]) {
        let gb = gb.trim();
        if !gb.is_empty() && gb != cfg.trunk {
            let paired = pscale_branch_from_git(gb);
            if crate::flow::pscale::branch_exists(cfg, &paired) {
                return paired;
            }
            ctx.out.warn(format!(
                "pscale branch {paired:?} does not exist — falling back to {:?}",
                cfg.parent
            ));
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

/// Fail fast when a declared `port` (or the tunnel's) already has a listener — checked before
/// anything spawns, installs, or migrates. The alternative failure modes are strictly worse: the
/// api panics with `AddrInUse` mid-startup, and Vite silently hops to a free port while everything
/// configured against the declared one (ORIGIN, callback URLs) keeps pointing at the stale
/// listener. With `kill`, the holders get the TERM → grace → KILL ladder and the run proceeds
/// once each port frees up.
fn ensure_ports_free(ctx: &Ctx, procs: &[DevProcess], kill: bool) -> CliResult {
    /// One busy port's holders as `(pid, command)`; command may be empty when unresolvable.
    type Holders = Vec<(u32, String)>;
    let mut busy: Vec<(&str, u16, Holders)> = Vec::new();
    for p in procs {
        let Some(port) = p.port else { continue };
        if ports::listening(port) {
            busy.push((&p.name, port, ports::holders(port)));
        }
    }
    if busy.is_empty() {
        return Ok(());
    }

    if !kill {
        let mut msg = String::from("dev ports already in use:\n");
        for (name, port, holders) in &busy {
            msg.push_str(&format!("  :{port} ({name}) — {}\n", describe(holders)));
        }
        msg.push_str("stop them, or rerun with `midas dev --kill-ports` to take the ports");
        return Err(CliError::expected(msg));
    }

    for (name, port, holders) in &busy {
        if holders.is_empty() {
            return Err(CliError::tool(anyhow::anyhow!(
                ":{port} ({name}) is in use but its holder could not be identified — free it by hand"
            )));
        }
        for (pid, cmd) in holders {
            let what = if cmd.is_empty() {
                format!("pid {pid}")
            } else {
                format!("{cmd} (pid {pid})")
            };
            ctx.out
                .step(format!("freeing :{port} ({name}) — killing {what}"));
            ports::kill(*pid);
        }
        if !ports::wait_free(*port, Duration::from_secs(5)) {
            return Err(CliError::tool(anyhow::anyhow!(
                ":{port} ({name}) is still in use after killing its holder — free it by hand"
            )));
        }
    }
    Ok(())
}

/// Render a port's holders for humans: `vite (pid 3941), node (pid 3999)` — or `holder unknown`
/// when the platform lookup came back empty (no lsof, or a process owned by another user).
fn describe(holders: &[(u32, String)]) -> String {
    if holders.is_empty() {
        return "holder unknown".into();
    }
    holders
        .iter()
        .map(|(pid, cmd)| {
            if cmd.is_empty() {
                format!("pid {pid}")
            } else {
                format!("{cmd} (pid {pid})")
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Build a recursive watcher over the process's `watch` paths (resolved against its cwd), sending
/// the process index down `tx` on any mutating event. `Ok(None)` when no configured path exists.
fn make_watcher(
    p: &DevProcess,
    root: &Path,
    idx: usize,
    tx: mpsc::Sender<usize>,
) -> notify::Result<Option<notify::RecommendedWatcher>> {
    let base: PathBuf = match &p.cwd {
        Some(c) => root.join(c),
        None => root.to_path_buf(),
    };
    let paths: Vec<PathBuf> = p
        .watch
        .iter()
        .map(|w| base.join(w))
        .filter(|w| w.exists())
        .collect();
    if paths.is_empty() {
        return Ok(None);
    }
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            // Only content-affecting events; Access events would restart on every read.
            if matches!(
                event.kind,
                notify::EventKind::Create(_)
                    | notify::EventKind::Modify(_)
                    | notify::EventKind::Remove(_)
            ) {
                let _ = tx.send(idx);
            }
        }
    })?;
    for path in &paths {
        watcher.watch(path, notify::RecursiveMode::Recursive)?;
    }
    Ok(Some(watcher))
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
    // Put each child in its own teardown group, so teardown kills its whole tree (e.g. `cargo run`'s
    // child server), not just the shell.
    Group::prepare(&mut cmd);
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
        use std::io::Write;
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            // Raw child-output passthrough — not a print macro (CLI-0009): a closed pipe must not
            // panic the streamer, and the write is explicit about which channel it mirrors.
            if to_stdout {
                let _ = writeln!(std::io::stdout().lock(), "{prefix} {line}");
            } else {
                let _ = writeln!(std::io::stderr().lock(), "{prefix} {line}");
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

/// Per-platform "who is on this TCP port". `listening` is the cheap universal probe (a loopback
/// TCP connect, like `wait_for_port`); `holders` resolves the pids so the failure can name the
/// culprit. Lookup: macOS/BSD ship `lsof`; Linux reads `/proc` directly (lsof isn't always
/// installed) with `lsof` as fallback; Windows parses `netstat -ano` (always present).
mod ports {
    use std::net::TcpStream;
    use std::process::Command;
    use std::time::{Duration, Instant};

    /// True when something accepts on the port — probed on both loopback families, since a
    /// listener bound only to `::1`/dual-stack still collides with a server binding v4.
    pub fn listening(port: u16) -> bool {
        TcpStream::connect(("127.0.0.1", port)).is_ok() || TcpStream::connect(("::1", port)).is_ok()
    }

    /// Poll until nothing accepts on the port anymore (or timeout).
    pub fn wait_free(port: u16, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if !listening(port) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }

    /// `(pid, command)` pairs listening on the port; `command` may be empty when unresolvable,
    /// and the whole list empty when the platform lookup fails (holder owned by another user, no
    /// tool available) — callers must treat empty as "busy, holder unknown", not "free".
    #[cfg(all(unix, not(target_os = "linux")))]
    pub fn holders(port: u16) -> Vec<(u32, String)> {
        lsof_holders(port)
    }

    #[cfg(target_os = "linux")]
    pub fn holders(port: u16) -> Vec<(u32, String)> {
        let res = proc_holders(port);
        if !res.is_empty() {
            return res;
        }
        lsof_holders(port)
    }

    /// `lsof -Fpc` → machine-readable output: a `p<pid>` line then a `c<command>` line per process.
    #[cfg(unix)]
    fn lsof_holders(port: u16) -> Vec<(u32, String)> {
        let Ok(out) = Command::new("lsof")
            .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-Fpc"])
            .output()
        else {
            return Vec::new();
        };
        let mut res: Vec<(u32, String)> = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some(pid) = line.strip_prefix('p') {
                if let Ok(pid) = pid.parse::<u32>() {
                    if !res.iter().any(|(p, _)| *p == pid) {
                        res.push((pid, String::new()));
                    }
                }
            } else if let Some(cmd) = line.strip_prefix('c') {
                if let Some(last) = res.last_mut() {
                    if last.1.is_empty() {
                        last.1 = cmd.to_string();
                    }
                }
            }
        }
        res
    }

    /// No external tool on Linux: `/proc/net/tcp{,6}` maps the listening port to socket inodes,
    /// then each same-user `/proc/<pid>/fd` is scanned for those `socket:[inode]` links.
    #[cfg(target_os = "linux")]
    fn proc_holders(port: u16) -> Vec<(u32, String)> {
        let mut inodes: Vec<String> = Vec::new();
        for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
            let Ok(text) = std::fs::read_to_string(table) else {
                continue;
            };
            for line in text.lines().skip(1) {
                // `sl local_address rem_address st … inode …`; st 0A = LISTEN, port is hex.
                let cols: Vec<&str> = line.split_whitespace().collect();
                if cols.len() < 10 || cols[3] != "0A" {
                    continue;
                }
                let Some((_, hex_port)) = cols[1].rsplit_once(':') else {
                    continue;
                };
                if u16::from_str_radix(hex_port, 16) == Ok(port) {
                    inodes.push(cols[9].to_string());
                }
            }
        }
        if inodes.is_empty() {
            return Vec::new();
        }

        let mut res: Vec<(u32, String)> = Vec::new();
        let Ok(proc_dir) = std::fs::read_dir("/proc") else {
            return res;
        };
        for entry in proc_dir.flatten() {
            let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else {
                continue; // not ours to inspect
            };
            let holds = fds.flatten().any(|fd| {
                std::fs::read_link(fd.path()).is_ok_and(|t| {
                    let t = t.to_string_lossy();
                    inodes.iter().any(|i| t == format!("socket:[{i}]"))
                })
            });
            if holds {
                let cmd = std::fs::read_to_string(entry.path().join("comm"))
                    .map(|c| c.trim().to_string())
                    .unwrap_or_default();
                res.push((pid, cmd));
            }
        }
        res
    }

    #[cfg(windows)]
    pub fn holders(port: u16) -> Vec<(u32, String)> {
        // `netstat -ano -p TCP` lines: `TCP 0.0.0.0:8080 0.0.0.0:0 LISTENING 1234` (the local
        // address may also be `[::]:8080`). No command name without another lookup; pid suffices.
        let Ok(out) = Command::new("netstat").args(["-ano", "-p", "TCP"]).output() else {
            return Vec::new();
        };
        let suffix = format!(":{port}");
        let mut res: Vec<(u32, String)> = Vec::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() >= 5
                && cols[0] == "TCP"
                && cols[1].ends_with(&suffix)
                && cols[3] == "LISTENING"
            {
                if let Ok(pid) = cols[4].parse::<u32>() {
                    if !res.iter().any(|(p, _)| *p == pid) {
                        res.push((pid, String::new()));
                    }
                }
            }
        }
        res
    }

    /// Kill one foreign pid (not a process group we own): TERM, a grace window, then KILL — the
    /// stale holder is usually a dev server that shuts down cleanly on TERM. Windows has no TERM
    /// equivalent a console server honors, so `taskkill /T /F` fells the holder's tree outright.
    #[cfg(unix)]
    pub fn kill(pid: u32) {
        unsafe { libc::kill(pid as i32, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            // Signal 0 probes liveness: an error (ESRCH) means the process is gone.
            if unsafe { libc::kill(pid as i32, 0) } != 0 {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    }

    #[cfg(windows)]
    pub fn kill(pid: u32) {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output();
    }
}

/// Per-platform teardown of each child and its descendants. On Unix every child leads its own
/// process group and we signal the group — SIGTERM, a grace window, then SIGKILL any survivors. On
/// Windows every child is assigned to one Job Object with kill-on-close, so terminating the job
/// kills the whole tree at once (and an abrupt exit of `midas` does too).
mod procgroup {
    use std::process::{Child, Command};

    /// Kill ONE child's whole tree and reap it — the watch-restart path (teardown handles the
    /// everything-at-once case). Unix: signal its process group with the same TERM → grace → KILL
    /// ladder as teardown. Windows: children share one Job (can't kill just one through it), so
    /// `taskkill /T /F` fells this child's tree.
    pub fn kill_tree(child: &mut Child) {
        #[cfg(unix)]
        {
            let pid = child.id() as i32;
            unsafe { libc::kill(-pid, libc::SIGTERM) };
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while std::time::Instant::now() < deadline {
                if matches!(child.try_wait(), Ok(Some(_))) {
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            unsafe { libc::kill(-pid, libc::SIGKILL) };
            let _ = child.wait();
        }
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &child.id().to_string(), "/T", "/F"])
                .output();
            let _ = child.wait();
        }
    }

    #[cfg(unix)]
    pub struct Group {
        pids: Vec<i32>,
    }

    #[cfg(unix)]
    impl Group {
        pub fn new() -> std::io::Result<Self> {
            Ok(Self { pids: Vec::new() })
        }

        /// Make the spawned child lead its own process group.
        pub fn prepare(cmd: &mut Command) {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        /// Record the child's pid (which is also its process-group id).
        pub fn register(&mut self, child: &Child) {
            self.pids.push(child.id() as i32);
        }

        /// Forget a reaped child's pid so teardown never signals a group id the OS may have
        /// recycled (the watch-restart path replaces children mid-run).
        pub fn unregister(&mut self, pid: u32) {
            self.pids.retain(|&p| p != pid as i32);
        }

        /// SIGTERM every group, wait briefly for a clean exit, then SIGKILL any survivors and reap.
        pub fn teardown(&self, children: &mut [(String, Child)]) {
            self.signal(libc::SIGTERM);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
            while std::time::Instant::now() < deadline {
                if children
                    .iter_mut()
                    .all(|(_, c)| matches!(c.try_wait(), Ok(Some(_))))
                {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            self.signal(libc::SIGKILL);
            for (_, c) in children.iter_mut() {
                let _ = c.wait();
            }
        }

        fn signal(&self, sig: i32) {
            for &pid in &self.pids {
                // Negative pid → signal the whole process group led by `pid`.
                unsafe { libc::kill(-pid, sig) };
            }
        }
    }

    #[cfg(windows)]
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};

    #[cfg(windows)]
    pub struct Group {
        job: HANDLE,
    }

    #[cfg(windows)]
    impl Group {
        pub fn new() -> std::io::Result<Self> {
            use windows_sys::Win32::System::JobObjects::{
                CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            };
            unsafe {
                let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if job.is_null() {
                    return Err(std::io::Error::last_os_error());
                }
                // Kill every process in the job when its last handle closes, so even an abrupt exit
                // of `midas` (panic, force-kill) tears the tree down instead of orphaning it.
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    job,
                    JobObjectExtendedLimitInformation,
                    (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == 0 {
                    let err = std::io::Error::last_os_error();
                    CloseHandle(job);
                    return Err(err);
                }
                Ok(Self { job })
            }
        }

        /// No pre-spawn setup needed on Windows.
        pub fn prepare(_cmd: &mut Command) {}

        /// No-op on Windows: job membership isn't revocable, and terminating a job that contains
        /// an already-dead process is harmless.
        pub fn unregister(&mut self, _pid: u32) {}

        /// Assign the child to the job; descendants it spawns inherit the job, so tearing the job
        /// down kills the whole tree. Done right after spawn — before the shell has launched its
        /// command — so the race against early grandchildren is negligible, and kill-on-close
        /// covers any that slip through.
        pub fn register(&mut self, child: &Child) {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::JobObjects::AssignProcessToJobObject;
            unsafe { AssignProcessToJobObject(self.job, child.as_raw_handle() as HANDLE) };
        }

        /// Terminate every process in the job (the whole tree) at once, then reap.
        pub fn teardown(&self, children: &mut [(String, Child)]) {
            use windows_sys::Win32::System::JobObjects::TerminateJobObject;
            unsafe { TerminateJobObject(self.job, 1) };
            for (_, c) in children.iter_mut() {
                let _ = c.wait();
            }
        }
    }

    #[cfg(windows)]
    impl Drop for Group {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.job) };
        }
    }
}
