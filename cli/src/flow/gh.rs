//! Shell-outs to the GitHub `gh` CLI. Ported from midflow.

use crate::flow::pr::PrInfo;
use crate::proc::{capture, on_path};
use anyhow::{bail, Context, Result};
use serde::Deserialize;

pub fn ensure_installed() -> Result<()> {
    if on_path("gh").is_none() {
        bail!("gh CLI not found on PATH — install from https://cli.github.com");
    }
    Ok(())
}

/// `gh auth status` returns non-zero whenever *any* configured account has stale tokens; checking
/// the active account directly with `gh api user` only succeeds if the active token is usable.
pub fn ensure_authed() -> Result<()> {
    if capture("gh", &["api", "user", "--jq", ".login"]).is_err() {
        bail!("gh not authenticated — run `gh auth login`");
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhPr {
    url: String,
    number: u64,
    base_ref_name: String,
    head_ref_name: String,
    is_draft: bool,
}

impl From<GhPr> for PrInfo {
    fn from(p: GhPr) -> Self {
        PrInfo {
            number: p.number,
            url: p.url,
            base: p.base_ref_name,
            head: p.head_ref_name,
            draft: p.is_draft,
        }
    }
}

/// Open PR whose head is `branch`. `Err` is an API/permission failure (callers must not
/// report success). `Ok(None)` means there is no open PR.
pub fn existing_pr_info(branch: &str) -> Result<Option<PrInfo>> {
    let raw = capture(
        "gh",
        &[
            "pr",
            "list",
            "--head",
            branch,
            "--state",
            "open",
            "--json",
            "url,number,baseRefName,headRefName,isDraft",
        ],
    )
    .context("gh pr list failed — cannot inspect the existing PR")?;
    let rows: Vec<GhPr> = serde_json::from_str(&raw).context("parse gh pr list")?;
    Ok(rows.into_iter().next().map(PrInfo::from))
}

/// Open PRs targeting `base`. Network/API failure is `Err` so callers can degrade to a warning.
pub fn open_prs_for_base(base: &str) -> Result<Vec<PrInfo>> {
    let raw = capture(
        "gh",
        &[
            "pr",
            "list",
            "--base",
            base,
            "--state",
            "open",
            "--json",
            "url,number,baseRefName,headRefName,isDraft",
        ],
    )
    .context("gh pr list failed — cannot list overlapping PRs")?;
    let rows: Vec<GhPr> = serde_json::from_str(&raw).context("parse gh pr list")?;
    Ok(rows.into_iter().map(PrInfo::from).collect())
}

/// Paths changed on PR `number`. Used for overlap; failure is `Err` (advisory at the call site).
pub fn pr_changed_files(number: u64) -> Result<Vec<String>> {
    let raw = capture("gh", &["pr", "diff", &number.to_string(), "--name-only"])
        .with_context(|| format!("gh pr diff {number} --name-only failed"))?;
    Ok(raw
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect())
}

/// Retarget an existing PR. Failure is `Err` — callers must not report the ship as successful.
pub fn set_pr_base(number: u64, base: &str) -> Result<()> {
    capture("gh", &["pr", "edit", &number.to_string(), "--base", base])
        .map(|_| ())
        .with_context(|| format!("gh pr edit {number} --base {base} failed"))
}

/// True when a *merged* PR exists with `branch` as its head — the squash-merge-safe "is this work
/// landed?" probe (`git branch --merged` can't see a squash). False on lookup failure.
pub fn merged_pr_exists(branch: &str) -> bool {
    capture(
        "gh",
        &[
            "pr", "list", "--head", branch, "--state", "merged", "--json", "number", "--jq",
            "length",
        ],
    )
    .map(|n| n.parse::<u64>().map(|n| n > 0).unwrap_or(false))
    .unwrap_or(false)
}

/// `gh pr merge --auto --squash` on the open PR whose head is `branch`.
pub fn enable_auto_merge(branch: &str) -> Result<()> {
    capture("gh", &["pr", "merge", branch, "--auto", "--squash"]).map(|_| ())
}

/// `gh pr create`; returns the PR URL printed on stdout.
pub fn create_pr(title: &str, body: &str, base: &str, draft: bool) -> Result<String> {
    let mut args = vec![
        "pr", "create", "--base", base, "--title", title, "--body", body,
    ];
    if draft {
        args.push("--draft");
    }
    capture("gh", &args)
}
