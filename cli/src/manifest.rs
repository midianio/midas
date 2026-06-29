//! The `midas.toml` lockfile — typed model + loader. See `SPEC.md §7`.

use crate::core::config::{find_up, load_toml};
use anyhow::Result;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const MANIFEST_NAME: &str = "midas.toml";

#[derive(Debug, Deserialize, Default)]
pub struct Manifest {
    #[serde(default)]
    pub standard: Standard,
    /// Per-layer stack state; a layer is checked vs its CURRENT stack.
    #[serde(default)]
    pub stack: BTreeMap<String, StackLayer>,
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
    /// service | app | library | pipeline. Part of the schema; not yet consumed by `check`.
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
    /// Opt-in: make exit 4 (advisory findings) block CI.
    #[serde(default)]
    pub semantic_strict: bool,
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
    pub state_file: Option<String>,
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
