//! Shell-outs to the `pscale` CLI for the paired-branch lifecycle. Ported from midflow.

use super::config::FlowConfig;
use crate::proc::{inherit, on_path, try_capture};
use anyhow::{bail, Result};

pub fn ensure_auth() -> Result<()> {
    if on_path("pscale").is_none() {
        bail!("pscale CLI not found on PATH — install from https://planetscale.com/cli");
    }
    if !try_capture("pscale", &["auth", "check"]).1 {
        bail!("pscale CLI not authenticated — run `pscale auth login` and try again");
    }
    Ok(())
}

pub fn branch_exists(cfg: &FlowConfig, name: &str) -> bool {
    try_capture(
        "pscale",
        &["branch", "show", &cfg.db, name, "--org", &cfg.org],
    )
    .1
}

/// Create a pscale branch off the parent. `seed_data` clones parent data via Data Branching™
/// (upgrades cluster size to match parent — not free). `--wait` blocks until ready.
pub fn create_branch(cfg: &FlowConfig, name: &str, seed_data: bool) -> Result<()> {
    let mut args = vec![
        "branch",
        "create",
        &cfg.db,
        name,
        "--from",
        &cfg.parent,
        "--region",
        &cfg.region,
        "--org",
        &cfg.org,
        "--wait",
    ];
    if seed_data {
        args.push("--seed-data");
    }
    inherit("pscale", &args)
}

/// True when the current git branch has a paired PlanetScale branch (data-isolated). Shared parent
/// / trunk sessions are not isolated — auto-migrate and `migrate apply` gate on this.
pub fn is_data_isolated(cfg: &FlowConfig, git_branch: &str) -> bool {
    if git_branch.is_empty() || git_branch == cfg.trunk || git_branch == "main" {
        return false;
    }
    let paired = super::config::pscale_branch_from_git(git_branch);
    if paired == cfg.parent || paired == "main" || paired == "dev" {
        return false;
    }
    branch_exists(cfg, &paired)
}

/// Refuses to delete `main`/`dev`/the configured parent even with a tampered state file.
pub fn delete_branch(cfg: &FlowConfig, name: &str) -> Result<()> {
    if name == "main" || name == "dev" || name == cfg.parent {
        bail!("refusing to delete protected pscale branch {name:?}");
    }
    inherit(
        "pscale",
        &[
            "branch", "delete", &cfg.db, name, "--org", &cfg.org, "--force",
        ],
    )
}
