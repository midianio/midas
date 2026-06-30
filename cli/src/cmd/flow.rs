//! `midas flow` — the release/branch lifecycle: start · sync · ship · tag · end · status. Active
//! state is derived from the current git branch (the paired pscale branch is `pscale_branch_from_git`),
//! so there is no state file to keep in sync.
//!
//! The daily loop is `start → ship → end`: `start` cuts the branch, `ship` is the opinionated "send
//! it" (rebase on trunk, push, open-or-update the PR in one shot), and `end` cleans up. `sync` is the
//! lower-level rebase-only catch-up for mid-work; `ship` already does it as its first step.

use crate::core::exit::{CliError, CliResult};
use crate::core::{prompt_line, Ctx};
use crate::flow::config::{
    pscale_branch_from_git, seed_by_default, slugify, valid_branch_type, validate_slug,
    BRANCH_TYPES,
};
use crate::flow::{env, gh, git, pscale, FlowConfig};
use crate::manifest::Manifest;
use clap::Subcommand;
use serde_json::json;

#[derive(Subcommand)]
pub enum FlowCmd {
    /// Create a feature branch off the trunk (feat/fix get a seeded paired pscale branch).
    Start {
        /// Branch type: feat | fix | chore | docs | spike
        branch_type: Option<String>,
        /// Short slug for the branch name
        slug: Option<String>,
        /// Force a seeded paired pscale branch
        #[arg(long)]
        with_data: bool,
        /// Force git-only (no paired pscale branch)
        #[arg(long)]
        no_data: bool,
    },
    /// Rebase the current branch on origin/<trunk> and push (mid-work catch-up).
    Sync,
    /// Send it: rebase on trunk, push, then open or update the PR (the daily "I'm ready" button).
    #[command(visible_alias = "pr")]
    Ship {
        #[arg(long, short = 'd')]
        draft: bool,
        /// PR title (defaults to the last commit subject)
        #[arg(long)]
        title: Option<String>,
        /// PR body markdown (defaults to the what/why/test-plan template)
        #[arg(long)]
        body: Option<String>,
    },
    /// Cut an annotated release tag from the trunk.
    Tag {
        /// Version, e.g. v0.4.0
        version: Option<String>,
        /// Tag message (defaults to "release <version>")
        #[arg(long)]
        message: Option<String>,
    },
    /// Switch back to the parent branch; optionally delete the paired pscale branch.
    End {
        #[arg(long)]
        force: bool,
    },
    /// Print the active branch / paired pscale-branch state (--json for scripting).
    Status,
}

const PR_TEMPLATE: &str = "## What\n- %s\n\n## Why\n-\n\n## Test plan\n- [ ] ran locally\n- [ ] tested on mobile viewport (if UI)\n- [ ] type-check + lint pass\n";

pub fn run(ctx: &Ctx, manifest: &Manifest, cmd: FlowCmd) -> CliResult {
    let cfg = FlowConfig::from_manifest(manifest);
    match cmd {
        FlowCmd::Start {
            branch_type,
            slug,
            with_data,
            no_data,
        } => start(ctx, &cfg, branch_type, slug, with_data, no_data),
        FlowCmd::Sync => sync(ctx, &cfg),
        FlowCmd::Ship { draft, title, body } => ship(ctx, &cfg, draft, title, body),
        FlowCmd::Tag { version, message } => tag(ctx, &cfg, version, message),
        FlowCmd::End { force } => end(ctx, &cfg, force),
        FlowCmd::Status => status(ctx, &cfg),
    }
}

fn start(
    ctx: &Ctx,
    cfg: &FlowConfig,
    branch_type: Option<String>,
    slug: Option<String>,
    with_data: bool,
    no_data: bool,
) -> CliResult {
    git::ensure_repo()?;
    if !git::is_clean()? {
        return Err(CliError::expected(
            "worktree is dirty — commit or stash before starting a new branch",
        ));
    }
    pscale::ensure_auth()?;

    if with_data && no_data {
        return Err(CliError::usage(
            "--with-data and --no-data are mutually exclusive",
        ));
    }

    // Resolve branch type.
    let branch_type = match branch_type {
        Some(t) => t,
        None => prompt_line(
            &ctx.out,
            &ctx.global,
            &format!("Branch type [{}]", BRANCH_TYPES.join("/")),
            None,
        )?,
    };
    if !valid_branch_type(&branch_type) {
        return Err(CliError::usage(format!(
            "invalid branch type {branch_type:?} (must be one of: {})",
            BRANCH_TYPES.join(", ")
        )));
    }

    // Resolve slug.
    let raw_slug = match slug {
        Some(s) => s,
        None => prompt_line(&ctx.out, &ctx.global, "Branch slug", None)?,
    };
    let slug = slugify(&raw_slug);
    validate_slug(&slug).map_err(CliError::tool)?;

    let git_branch = format!("{branch_type}/{slug}");
    if git::branch_exists(&git_branch) {
        return Err(CliError::expected(format!(
            "branch {git_branch:?} already exists locally"
        )));
    }

    let isolated = if with_data {
        true
    } else if no_data {
        false
    } else {
        seed_by_default(&branch_type)
    };

    let pscale_branch = if isolated {
        pscale_branch_from_git(&git_branch)
    } else {
        cfg.parent.clone()
    };

    ctx.out.banner(format!("Starting {git_branch}"));

    if isolated {
        ctx.out.step(format!(
            "pscale branch create {} {} --from {} --seed-data",
            cfg.db, pscale_branch, cfg.parent
        ));
        if pscale::branch_exists(cfg, &pscale_branch) {
            ctx.out.info(format!(
                "pscale branch {pscale_branch} already exists — reusing"
            ));
        } else {
            ctx.out.info(format!(
                "seeding from {} — this can take a few minutes",
                cfg.parent
            ));
            pscale::create_branch(cfg, &pscale_branch, true)?;
        }
    } else {
        ctx.out.info(format!(
            "git-only flow — local tunnel will hit shared `{}` branch",
            cfg.parent
        ));
        ctx.out
            .hint("pass --with-data to create a seeded paired pscale branch");
    }

    ctx.out.step(format!("git fetch origin {}", cfg.parent));
    git::fetch_branch(&cfg.parent)?;
    ctx.out.step(format!(
        "git checkout -b {git_branch} origin/{}",
        cfg.parent
    ));
    git::checkout_new_from(&git_branch, &format!("origin/{}", cfg.parent))?;

    env::write_api_env_local(cfg)?;

    ctx.out.success(format!("on branch {git_branch}"));
    ctx.out.info("start tunnel + dev: bun run dev");

    ctx.out.data(
        &json!({ "gitBranch": &git_branch, "pscaleBranch": &pscale_branch, "dataIsolated": isolated }),
        |_| git_branch.clone(),
    );
    Ok(())
}

fn sync(ctx: &Ctx, cfg: &FlowConfig) -> CliResult {
    git::ensure_repo()?;
    if !git::is_clean()? {
        return Err(CliError::expected(
            "worktree is dirty — commit or stash before syncing",
        ));
    }
    let branch = git::current_branch()?;
    if branch == cfg.trunk {
        return Err(CliError::usage(format!(
            "on {} — sync is for feature branches; run `git pull` instead",
            cfg.trunk
        )));
    }

    ctx.out
        .banner(format!("Syncing {branch} with origin/{}", cfg.trunk));
    ctx.out.step("git fetch origin --prune");
    git::fetch()?;

    let (ahead, behind) = git::ahead_behind(&cfg.trunk)?;
    if behind == 0 {
        ctx.out
            .success(format!("already up to date (ahead {ahead}, behind 0)"));
        ctx.out.data(
            &json!({ "rebased": false, "ahead": ahead, "behind": 0 }),
            |_| "up to date".into(),
        );
        return Ok(());
    }

    ctx.out
        .info(format!("ahead {ahead}, behind {behind} — rebasing"));
    ctx.out.step(format!("git rebase origin/{}", cfg.trunk));
    rebase_onto_trunk(cfg)?;

    if !git::has_upstream() {
        ctx.out.info("branch has no upstream yet — pushing");
        git::push()?;
        ctx.out.success(format!("pushed {branch}"));
        ctx.out
            .data(&json!({ "rebased": true, "pushed": true }), |_| {
                "synced".into()
            });
        return Ok(());
    }

    if ctx.confirm("Rebase clean. Push --force-with-lease?", true)? {
        ctx.out.step("git push --force-with-lease");
        git::push_force_with_lease()?;
        ctx.out.success(format!("synced {branch}"));
        ctx.out
            .data(&json!({ "rebased": true, "pushed": true }), |_| {
                "synced".into()
            });
    } else {
        ctx.out
            .info("skipped push — local and origin will diverge until you push");
        ctx.out
            .data(&json!({ "rebased": true, "pushed": false }), |_| {
                "rebased".into()
            });
    }
    Ok(())
}

/// Run `git rebase origin/<trunk>` and turn a conflict into a friendly, recoverable error listing
/// the conflicted files. Shared by `sync` and `ship`.
fn rebase_onto_trunk(cfg: &FlowConfig) -> CliResult {
    if git::rebase_onto(&cfg.trunk).is_err() {
        let conflicts = git::conflicted_files();
        let mut msg = String::from("rebase produced conflicts");
        if !conflicts.is_empty() {
            msg.push_str(":\n");
            for c in &conflicts {
                msg.push_str(&format!("    - {c}\n"));
            }
        }
        msg.push_str(
            "\nresolve them, then `git add` + `git rebase --continue`, or `git rebase --abort`.",
        );
        return Err(CliError::expected(msg));
    }
    Ok(())
}

/// The opinionated daily "send it" button: rebase the feature branch on trunk, push it, then open a
/// PR (or no-op if one is already open — the push has already updated it). Folds what used to be a
/// separate `sync` + `pr` into one step; `sync` remains for a rebase-only catch-up.
fn ship(
    ctx: &Ctx,
    cfg: &FlowConfig,
    draft: bool,
    title: Option<String>,
    body: Option<String>,
) -> CliResult {
    git::ensure_repo()?;
    gh::ensure_installed()?;
    gh::ensure_authed()?;

    let branch = git::current_branch()?;
    if branch == cfg.trunk || branch == "main" {
        return Err(CliError::usage(format!(
            "on {branch} — switch to a feature branch first (try `midas flow start`)"
        )));
    }
    if !git::is_clean()? {
        return Err(CliError::expected(
            "worktree is dirty — commit before shipping",
        ));
    }

    ctx.out.banner(format!("Shipping {branch}"));

    // 1. Rebase on trunk (skip when already up to date).
    ctx.out.step("git fetch origin --prune");
    git::fetch()?;
    let (ahead, behind) = git::ahead_behind(&cfg.trunk)?;
    if behind > 0 {
        ctx.out
            .info(format!("behind {behind} — rebasing on origin/{}", cfg.trunk));
        ctx.out.step(format!("git rebase origin/{}", cfg.trunk));
        rebase_onto_trunk(cfg)?;
    } else {
        ctx.out
            .info(format!("up to date with origin/{} (ahead {ahead})", cfg.trunk));
    }

    // 2. Push (force-with-lease after a possible rebase; plain push to set upstream the first time).
    if git::has_upstream() {
        ctx.out.step("git push --force-with-lease");
        git::push_force_with_lease()?;
    } else {
        ctx.out.info("branch has no upstream yet — pushing");
        git::push()?;
    }

    // 3. Open the PR, or no-op if one is already open.
    if let Some(url) = gh::existing_pr(&branch) {
        ctx.out.success(format!("PR updated: {url}"));
        ctx.out
            .data(&json!({ "url": url, "created": false }), |_| url.clone());
        return Ok(());
    }

    let default_title = git::last_commit_subject().unwrap_or_default();
    let slug = branch.split_once('/').map(|(_, s)| s).unwrap_or(&branch);
    let default_body = PR_TEMPLATE.replacen("%s", slug, 1);

    let title = match title {
        Some(t) => t,
        None => prompt_line(&ctx.out, &ctx.global, "PR title", Some(&default_title))?,
    };
    let body = body.unwrap_or(default_body);

    ctx.out.step(format!(
        "gh pr create --base {}{}",
        cfg.trunk,
        if draft { " --draft" } else { "" }
    ));
    let url = gh::create_pr(&title, &body, &cfg.trunk, draft)?;
    ctx.out.success(format!("PR opened: {url}"));
    ctx.out.data(&json!({ "url": url, "created": true, "draft": draft }), |_| {
        url.clone()
    });
    Ok(())
}

fn tag(ctx: &Ctx, cfg: &FlowConfig, version: Option<String>, message: Option<String>) -> CliResult {
    git::ensure_repo()?;
    let branch = git::current_branch()?;
    if branch != cfg.trunk {
        return Err(CliError::usage(format!(
            "on {branch} — tags are cut from {} only",
            cfg.trunk
        )));
    }
    if !git::is_clean()? {
        return Err(CliError::expected(
            "worktree is dirty — release tags must be from a clean trunk",
        ));
    }

    let latest = git::latest_tag();
    ctx.out.banner("Tag release");
    if latest.is_empty() {
        ctx.out.info("no existing tags");
    } else {
        ctx.out.info(format!("latest tag: {latest}"));
    }

    let version = match version {
        Some(v) => v,
        None => prompt_line(&ctx.out, &ctx.global, "Version (e.g. v0.4.0)", None)?,
    };
    validate_version(&version)?;

    let default_msg = format!("release {version}");
    let message = match message {
        Some(m) => m,
        None => prompt_line(&ctx.out, &ctx.global, "Tag message", Some(&default_msg))?,
    };

    if !ctx.confirm(&format!("Create tag {version} and push to origin?"), true)? {
        ctx.out.info("aborted");
        return Err(CliError::expected("aborted"));
    }

    ctx.out.step(format!("git tag -a {version} -m {message:?}"));
    git::tag_annotated(&version, &message)?;
    ctx.out.step(format!("git push origin {version}"));
    git::push_tag(&version)?;
    ctx.out.success(format!("tagged and pushed {version}"));
    ctx.out
        .data(&json!({ "version": version }), |_| version.clone());
    Ok(())
}

fn validate_version(v: &str) -> Result<(), CliError> {
    let re = regex::Regex::new(r"^v\d+\.\d+\.\d+$").unwrap();
    if re.is_match(v) {
        Ok(())
    } else {
        Err(CliError::usage(
            "version must look like v1.2.3 (leading v, semver)",
        ))
    }
}

/// Print the active flow state, derived from the current git branch. On a feature branch
/// (`<type>/<slug>`) the paired pscale branch is `pscale_branch_from_git`; whether it physically
/// exists (best-effort live check) is reported as `dataIsolated`. Otherwise we're on the parent.
fn status(ctx: &Ctx, cfg: &FlowConfig) -> CliResult {
    git::ensure_repo()?;
    let git_branch = git::current_branch()?;
    if !is_feature_branch(&git_branch) {
        ctx.out.info(format!(
            "no active feature branch — running on parent ({})",
            cfg.parent
        ));
        ctx.out
            .data(&json!({ "active": false, "parent": cfg.parent }), |_| {
                format!("on parent ({})", cfg.parent)
            });
        return Ok(());
    }
    let pscale_branch = pscale_branch_from_git(&git_branch);
    let isolated = pscale::ensure_auth().is_ok() && pscale::branch_exists(cfg, &pscale_branch);
    ctx.out.data(
        &json!({
            "active": true,
            "gitBranch": git_branch,
            "pscaleBranch": pscale_branch,
            "dataIsolated": isolated,
            "parent": cfg.parent,
        }),
        |_| format!("{git_branch} → {pscale_branch} (isolated: {isolated})"),
    );
    Ok(())
}

/// Switch back to the parent branch and strip the managed `.env.local` block. With `--force`, also
/// delete the paired pscale branch derived from the current git branch (if it exists). The branch
/// name is derived from git, and `pscale::delete_branch` refuses `main`/`dev`/parent, so a stray
/// `--force` on a non-feature branch can't delete a shared branch.
fn end(ctx: &Ctx, cfg: &FlowConfig, force: bool) -> CliResult {
    git::ensure_repo()?;
    let git_branch = git::current_branch()?;
    if !is_feature_branch(&git_branch) {
        ctx.out.info("no active feature branch");
        return Ok(());
    }
    let pscale_branch = pscale_branch_from_git(&git_branch);

    ctx.out.step(format!("git checkout {}", cfg.parent));
    git::checkout(&cfg.parent)?;

    if force {
        pscale::ensure_auth()?;
        if pscale::branch_exists(cfg, &pscale_branch) {
            ctx.out
                .step(format!("pscale branch delete {} {}", cfg.db, pscale_branch));
            pscale::delete_branch(cfg, &pscale_branch)?;
        } else {
            ctx.out
                .info(format!("no paired pscale branch {pscale_branch} to delete"));
        }
    } else if pscale::ensure_auth().is_ok() && pscale::branch_exists(cfg, &pscale_branch) {
        ctx.out.info(format!(
            "leaving pscale branch {pscale_branch} alive — pass --force to delete"
        ));
    }

    env::clear_api_env_local(cfg)?;
    ctx.out.success("done");
    Ok(())
}

/// True when `branch` is a `<type>/<slug>` feature branch (non-empty slug after a known type prefix).
fn is_feature_branch(branch: &str) -> bool {
    BRANCH_TYPES.iter().any(|t| {
        let prefix = format!("{t}/");
        branch.starts_with(&prefix) && branch.len() > prefix.len()
    })
}
