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

/// `GIT_EDITOR=true` so continue does not open a pager/editor.
pub fn rebase_continue() -> Result<()> {
    let status = std::process::Command::new("git")
        .env("GIT_EDITOR", "true")
        .args(["-c", "core.editor=true", "rebase", "--continue"])
        .status()
        .map_err(|e| anyhow::anyhow!("git rebase --continue: {e}"))?;
    if !status.success() {
        anyhow::bail!(
            "git rebase --continue exited with status {}",
            status.code().unwrap_or(-1)
        );
    }
    Ok(())
}

pub fn rebase_in_progress() -> bool {
    let exists = |kind: &str| {
        capture("git", &["rev-parse", "--git-path", kind])
            .ok()
            .map(|p| std::path::Path::new(&p).exists())
            .unwrap_or(false)
    };
    exists("rebase-merge") || exists("rebase-apply")
}

/// Stage `n` of a conflicted path (`:2:` = ours / trunk during rebase, `:3:` = theirs).
/// Does not trim — file bytes must stay intact for the date-only comparison.
pub fn show_stage(stage: u8, path: &str) -> Result<String> {
    let spec = format!(":{stage}:{path}");
    let out = std::process::Command::new("git")
        .args(["show", &spec])
        .output()
        .map_err(|e| anyhow::anyhow!("git show {spec}: {e}"))?;
    if !out.status.success() {
        anyhow::bail!(
            "git show {spec} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// During rebase, `--ours` is the branch we are rebasing onto (trunk).
pub fn checkout_ours(path: &str) -> Result<()> {
    inherit("git", &["checkout", "--ours", "--", path])
}

pub fn add(path: &str) -> Result<()> {
    inherit("git", &["add", "--", path])
}

/// Paths that differ between `origin/<trunk>` and HEAD (empty when the branch has no unique diff).
pub fn diff_names_vs(trunk: &str) -> Result<Vec<String>> {
    let spec = format!("origin/{trunk}...HEAD");
    let out = capture("git", &["diff", "--name-only", spec.as_str()])?;
    Ok(out
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
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
    capture(
        "git",
        &["rev-parse", "--verify", &format!("refs/tags/{tag}")],
    )
    .is_ok()
}

/// True when `origin` has `refs/tags/<tag>`.
pub fn remote_tag_exists(tag: &str) -> bool {
    capture(
        "git",
        &["ls-remote", "--tags", "origin", &format!("refs/tags/{tag}")],
    )
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
    let path_strs: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
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

/// True when any path under `pathspec` differs between the working tree (incl. index) and
/// `origin/<base>` (falls back to local `<base>`). Used to warn when migrations changed on a
/// shared-parent session.
pub fn pathspec_changed_vs(base: &str, pathspec: &str) -> bool {
    let remote = format!("origin/{base}");
    let base_ref = if capture("git", &["rev-parse", "--verify", "--quiet", &remote]).is_ok() {
        remote
    } else {
        base.to_string()
    };
    capture("git", &["diff", "--name-only", &base_ref, "--", pathspec])
        .map(|out| out.lines().any(|l| !l.is_empty()))
        .unwrap_or(false)
}
