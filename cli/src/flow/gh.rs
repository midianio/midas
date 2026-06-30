//! Shell-outs to the GitHub `gh` CLI. Ported from midflow.

use crate::proc::{capture, on_path};
use anyhow::{bail, Result};

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

/// The URL of the open PR whose head is `branch`, if one exists. Lets `ship` be idempotent:
/// create the PR the first time, then no-op (the push already updated it) on later runs. Returns
/// `None` both when there is no open PR and when the lookup itself fails — callers treat either as
/// "no PR yet" and fall through to `create_pr`.
pub fn existing_pr(branch: &str) -> Option<String> {
    capture(
        "gh",
        &[
            "pr", "list", "--head", branch, "--state", "open", "--json", "url", "--jq", ".[0].url",
        ],
    )
    .ok()
    .filter(|s| !s.is_empty())
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
