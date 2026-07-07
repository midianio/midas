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

/// True when `refs/tags/<tag>` exists locally.
pub fn tag_exists(tag: &str) -> bool {
    capture("git", &["rev-parse", "--verify", &format!("refs/tags/{tag}")]).is_ok()
}

/// True when `origin` has `refs/tags/<tag>`.
pub fn remote_tag_exists(tag: &str) -> bool {
    capture("git", &["ls-remote", "--tags", "origin", &format!("refs/tags/{tag}")])
        .map(|out| !out.trim().is_empty())
        .unwrap_or(false)
}

pub fn delete_local_tag(tag: &str) -> Result<()> {
    inherit("git", &["tag", "-d", tag])
}

/// Delete a tag on `origin` (self-heal a broken release tag before re-pushing).
pub fn delete_remote_tag(tag: &str) -> Result<()> {
    inherit("git", &["push", "origin", &format!(":refs/tags/{tag}")])
}

/// Stage explicit paths and commit. Refuses when there is nothing to commit.
pub fn commit_paths(message: &str, paths: &[std::path::PathBuf]) -> Result<()> {
    if paths.is_empty() {
        bail!("nothing to commit");
    }
    let path_strs: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let mut args = vec!["add"];
    for p in &path_strs {
        args.push(p.as_str());
    }
    inherit("git", &args)?;
    inherit("git", &["commit", "-m", message])
}

/// All local branch names (short refs).
pub fn local_branches() -> Result<Vec<String>> {
    let out = capture(
        "git",
        &["for-each-ref", "--format=%(refname:short)", "refs/heads/"],
    )?;
    Ok(out.lines().map(str::to_string).collect())
}

/// Local branches whose tips are ancestors of origin/<trunk> (misses squash merges — callers pair
/// this with a merged-PR lookup). Empty on error (e.g. no origin/<trunk> yet).
pub fn merged_branches(trunk: &str) -> Vec<String> {
    capture(
        "git",
        &[
            "branch",
            "--merged",
            &format!("origin/{trunk}"),
            "--format=%(refname:short)",
        ],
    )
    .map(|out| out.lines().map(str::to_string).collect())
    .unwrap_or_default()
}

/// `git branch -D` — forced, because squash-merged branches are never ancestors of trunk.
pub fn delete_local_branch(branch: &str) -> Result<()> {
    inherit("git", &["branch", "-D", branch])
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
