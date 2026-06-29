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
