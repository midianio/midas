//! The active-flow state file (`.midflow/active.json` by default). Wire-compatible with midflow.

use super::config::FlowConfig;
use super::git;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveState {
    #[serde(rename = "pscaleBranch")]
    pub pscale_branch: String,
    #[serde(rename = "gitBranch")]
    pub git_branch: String,
    pub port: u16,
    pub db: String,
    pub org: String,
    pub parent: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    /// True when `start --with-data` cloned data into a paired pscale branch. When false,
    /// `pscale_branch` is the shared parent and `db end --force` refuses to delete it.
    #[serde(rename = "dataIsolated", default)]
    pub data_isolated: bool,
}

fn state_path(cfg: &FlowConfig) -> Result<PathBuf> {
    Ok(git::repo_root()?.join(&cfg.state_file))
}

pub fn read_state(cfg: &FlowConfig) -> Result<Option<ActiveState>> {
    let p = state_path(cfg)?;
    match std::fs::read(&p) {
        Ok(data) if data.is_empty() => Ok(None),
        Ok(data) => Ok(serde_json::from_slice(&data).ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn write_state(cfg: &FlowConfig, state: &ActiveState) -> Result<()> {
    let p = state_path(cfg)?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut body = serde_json::to_string_pretty(state)?;
    body.push('\n');
    std::fs::write(&p, body)?;
    Ok(())
}

pub fn clear_state(cfg: &FlowConfig) -> Result<()> {
    let p = state_path(cfg)?;
    match std::fs::remove_file(&p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}
