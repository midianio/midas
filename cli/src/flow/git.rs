//! Thin shell-outs to `git`. Ported from midflow's `internal/git`. Trunk-dependent operations take
//! the trunk branch as a parameter (midflow hard-codes `dev`; `midas` reads it from `[flow] trunk`).

use crate::proc::{capture, inherit};
use anyhow::{bail, Result};
use std::path::PathBuf;

pub fn ensure_repo() -> Result<()> {
    capture("git", &["rev-parse", "--git-dir"])
        .map(|_| ())
        .map_err(|_| anyhow::anyhow!("not inside a git repository"))
}

pub fn repo_root() -> Result<PathBuf> {
    Ok(PathBuf::from(capture(
        "git",
        &["rev-parse", "--show-toplevel"],
    )?))
}

pub fn current_branch() -> Result<String> {
    capture("git", &["rev-parse", "--abbrev-ref", "HEAD"])
}

pub fn is_clean() -> Result<bool> {
    Ok(capture("git", &["status", "--porcelain"])?.is_empty())
}

pub fn fetch() -> Result<()> {
    inherit("git", &["fetch", "origin", "--prune"])
}

pub fn fetch_branch(branch: &str) -> Result<()> {
    inherit("git", &["fetch", "origin", branch])
}

pub fn branch_exists(branch: &str) -> bool {
    capture(
        "git",
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_ok()
}

pub fn checkout(branch: &str) -> Result<()> {
    inherit("git", &["checkout", branch])
}

/// `--no-track` so the new branch doesn't inherit `ref` as its upstream (which would make
/// `git push` refuse).
pub fn checkout_new_from(branch: &str, base_ref: &str) -> Result<()> {
    inherit("git", &["checkout", "--no-track", "-b", branch, base_ref])
}

pub fn rebase_onto(trunk: &str) -> Result<()> {
    inherit("git", &["rebase", &format!("origin/{trunk}")])
}

/// (ahead, behind) commit counts for HEAD vs origin/trunk.
pub fn ahead_behind(trunk: &str) -> Result<(u32, u32)> {
    let out = capture(
        "git",
        &[
            "rev-list",
            "--left-right",
            "--count",
            &format!("HEAD...origin/{trunk}"),
        ],
    )?;
    let mut parts = out.split_whitespace();
    let ahead = parts.next().and_then(|s| s.parse().ok());
    let behind = parts.next().and_then(|s| s.parse().ok());
    match (ahead, behind) {
        (Some(a), Some(b)) => Ok((a, b)),
        _ => bail!("could not parse ahead/behind: {out:?}"),
    }
}

pub fn has_upstream() -> bool {
    capture(
        "git",
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .is_ok()
}

pub fn push() -> Result<()> {
    let branch = current_branch()?;
    inherit("git", &["push", "-u", "origin", &branch])
}

pub fn push_force_with_lease() -> Result<()> {
    let branch = current_branch()?;
    inherit("git", &["push", "--force-with-lease", "origin", &branch])
}

pub fn last_commit_subject() -> Result<String> {
    capture("git", &["log", "-1", "--pretty=%s"])
}

/// Latest tag, or empty string when there are none (not an error).
pub fn latest_tag() -> String {
    capture("git", &["describe", "--tags", "--abbrev=0"]).unwrap_or_default()
}

pub fn tag_annotated(version: &str, message: &str) -> Result<()> {
    inherit("git", &["tag", "-a", version, "-m", message])
}

pub fn push_tag(version: &str) -> Result<()> {
    inherit("git", &["push", "origin", version])
}

pub fn conflicted_files() -> Vec<String> {
    capture("git", &["diff", "--name-only", "--diff-filter=U"])
        .map(|out| {
            out.lines()
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
