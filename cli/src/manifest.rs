//! The `midas.toml` lockfile — typed model + loader. See `SPEC.md §7`.

use crate::core::config::{find_up, load_toml};
use crate::core::GlobalArgs;
use anyhow::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const MANIFEST_NAME: &str = "midas.toml";

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize, Default)]
pub struct Manifest {
    #[serde(default)]
    pub standard: Standard,
    /// Per-layer stack state; a layer is checked vs its CURRENT stack.
    #[serde(default)]
    pub stack: BTreeMap<String, StackLayer>,
    /// Where each layer lives, relative to the repo root (`[layout] backend = "app/api"`). The
    /// registry's check globs are layer-relative; this maps them onto the repo. Defaults to the
    /// midian monorepo shape (`backend = "app/api"`, `frontend = "app/web"`).
    #[serde(default)]
    pub layout: BTreeMap<String, String>,
    #[serde(default)]
    pub check: CheckCfg,
    #[serde(default)]
    pub flow: FlowCfg,
    /// `midas dev` orchestration: the concurrent process set + the optional pscale tunnel.
    #[serde(default)]
    pub dev: DevCfg,
    /// Ledgered escape hatches: convention id → reason.
    #[serde(default)]
    pub deviations: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Standard {
    #[serde(default)]
    pub version: String,
    /// service | app | cli | library | pipeline. Part of the schema; not yet consumed by `check`.
    #[serde(default)]
    #[allow(dead_code)]
    pub profile: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct StackLayer {
    pub current: Option<String>,
    /// The stack a layer is porting *to*; informational until migration tooling consumes it.
    #[allow(dead_code)]
    pub target: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub struct CheckCfg {
    /// Surfaced verbatim in `midas check --json` for the external review agent / CI to decide
    /// whether to escalate its own semantic findings to blocking. `midas check` never reads it —
    /// the mechanical gate stays deterministic (SPEC §8).
    #[serde(default)]
    pub semantic_strict: bool,
    /// Per-project allow-list: convention id → extra repo-relative `allow_in` globs, merged into
    /// that convention's mechanical check. The seam for a project-specific exception that doesn't
    /// belong in the org registry (e.g. midian's sharelink service inlining uuid, BE-0016).
    #[serde(default)]
    pub allow: BTreeMap<String, Vec<String>>,
}

/// `[flow]` overrides for the ported release flow. Every field is optional; omitted fields fall
/// back to the midian defaults (so a fresh midian checkout needs no `midas.toml` to run `flow`).
#[derive(Debug, Deserialize, Default)]
pub struct FlowCfg {
    pub trunk: Option<String>,
    pub pscale_org: Option<String>,
    pub pscale_db: Option<String>,
    pub pscale_parent: Option<String>,
    pub pscale_region: Option<String>,
    pub tunnel_port: Option<u16>,
    pub api_env_local: Option<String>,
    pub env_marker: Option<String>,
}

/// `[dev]` — `midas dev` runs `processes` concurrently with prefixed output and one-Ctrl-C teardown.
/// When `tunnel = true`, the pscale tunnel (configured under `[flow]`) is raised first and the
/// processes wait for its port before starting.
#[derive(Debug, Deserialize, Default)]
pub struct DevCfg {
    /// Raise the pscale tunnel (using the `[flow]` org/db/port) before the processes start.
    #[serde(default)]
    pub tunnel: bool,
    /// Apply pending `db/migrations/` once the tunnel is up, before the processes start. Only
    /// meaningful with `tunnel = true`. Defaults to on; set `migrate = false` to opt out.
    #[serde(default = "default_true")]
    pub migrate: bool,
    /// Tunnel branch override; defaults to the paired branch for the current git branch, else the
    /// `[flow]` parent (`dev`).
    pub branch: Option<String>,
    /// The processes to run concurrently.
    #[serde(default)]
    pub processes: Vec<DevProcess>,
}

/// One `midas dev` process: a labeled shell command, optionally run in a subdirectory.
#[derive(Debug, Deserialize, Clone)]
pub struct DevProcess {
    /// Short label shown as the output prefix (e.g. `api`, `web`).
    pub name: String,
    /// The command line, run via the shell (`sh -c` / `cmd /C`).
    pub cmd: String,
    /// Working directory relative to the manifest root (defaults to the root).
    #[serde(default)]
    pub cwd: Option<String>,
    /// Paths to watch (relative to `cwd`); any change restarts this process, debounced. The
    /// watch-and-restart loop for processes that don't hot-reload themselves (`cargo run`) —
    /// Vite/Bun processes don't need it. Disable for a run with `midas dev --no-watch`.
    #[serde(default)]
    pub watch: Vec<String>,
}

/// Resolve the project root every project-scoped command shares (one resolution rule, no
/// per-command drift): the global `--root` if given, else the directory of the nearest `midas.toml`
/// walking up from the cwd, else the git toplevel, else the cwd itself.
pub fn resolve_root(global: &GlobalArgs) -> Result<PathBuf> {
    if let Some(r) = &global.root {
        return Ok(r.clone());
    }
    let cwd = std::env::current_dir()?;
    if let Some(path) = find_up(&cwd, MANIFEST_NAME) {
        if let Some(dir) = path.parent() {
            return Ok(dir.to_path_buf());
        }
    }
    if let Ok(top) = crate::proc::capture("git", &["rev-parse", "--show-toplevel"]) {
        return Ok(PathBuf::from(top));
    }
    Ok(cwd)
}

impl Manifest {
    /// Find `midas.toml` walking up from `start`; load it if present. Returns `(manifest, root)`
    /// where `root` is the manifest's directory, or `None` when no manifest exists.
    pub fn find(start: &Path) -> Result<Option<(Manifest, PathBuf)>> {
        match find_up(start, MANIFEST_NAME) {
            Some(path) => {
                let manifest: Manifest = load_toml(&path)?;
                let root = path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("."));
                Ok(Some((manifest, root)))
            }
            None => Ok(None),
        }
    }
}
