//! `midas flow` — the ported midflow release/branch flow.

use crate::core::exit::{CliError, CliResult};
use crate::core::{prompt_line, Ctx};
use crate::flow::config::{
    pscale_branch_from_git, seed_by_default, slugify, valid_branch_type, validate_slug,
    BRANCH_TYPES,
};
use crate::flow::state::ActiveState;
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
    /// Rebase the current branch on origin/<trunk> and push.
    Sync,
    /// Open a PR with the team template.
    Pr {
        #[arg(long, short = 'd')]
        draft: bool,
        /// PR title (defaults to the last commit subject)
        #[arg(long)]
        title: Option<String>,
        /// PR body markdown (defaults to the what/why/test-plan template)
        #[arg(long)]
        body: Option<String>,
    },
    /// Fast-path fix branch off the trunk.
    Hotfix {
        slug: Option<String>,
        #[arg(long)]
        with_data: bool,
        #[arg(long)]
        no_data: bool,
    },
    /// Cut an annotated release tag from the trunk.
    Tag {
        /// Version, e.g. v0.4.0
        version: Option<String>,
        /// Tag message (defaults to "release <version>")
        #[arg(long)]
        message: Option<String>,
    },
    /// Operate on the active pscale branch / tunnel.
    Db {
        #[command(subcommand)]
        cmd: DbCmd,
    },
    /// Check your local flow setup (git/gh/pscale auth).
    Doctor,
}

#[derive(Subcommand)]
pub enum DbCmd {
    /// Open the pscale tunnel for the active branch (foreground).
    Connect,
    /// Print active branch / tunnel state.
    Status,
    /// Switch back to parent; optionally delete the paired pscale branch.
    End {
        #[arg(long)]
        force: bool,
    },
    /// Print the active pscale branch name.
    CurrentBranch,
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
        } => start(ctx, &cfg, branch_type, slug, with_data, no_data, None),
        FlowCmd::Sync => sync(ctx, &cfg),
        FlowCmd::Pr { draft, title, body } => pr(ctx, &cfg, draft, title, body),
        FlowCmd::Hotfix {
            slug,
            with_data,
            no_data,
        } => {
            ctx.out.banner("Hotfix");
            ctx.out
                .warn("production is broken — add a regression test if you can.");
            start(
                ctx,
                &cfg,
                Some("fix".into()),
                slug,
                with_data,
                no_data,
                Some("fix"),
            )
        }
        FlowCmd::Tag { version, message } => tag(ctx, &cfg, version, message),
        FlowCmd::Db { cmd } => db(ctx, &cfg, cmd),
        FlowCmd::Doctor => crate::cmd::doctor::run(ctx, true),
    }
}

#[allow(clippy::too_many_arguments)]
fn start(
    ctx: &Ctx,
    cfg: &FlowConfig,
    branch_type: Option<String>,
    slug: Option<String>,
    with_data: bool,
    no_data: bool,
    locked_type: Option<&str>,
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
    let branch_type = match locked_type {
        Some(t) => t.to_string(),
        None => match branch_type {
            Some(t) => t,
            None => prompt_line(
                &ctx.out,
                &ctx.global,
                &format!("Branch type [{}]", BRANCH_TYPES.join("/")),
                None,
            )?,
        },
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

    let state = ActiveState {
        pscale_branch: pscale_branch.clone(),
        git_branch: git_branch.clone(),
        port: cfg.port,
        db: cfg.db.clone(),
        org: cfg.org.clone(),
        parent: cfg.parent.clone(),
        created_at: now_rfc3339(),
        data_isolated: isolated,
    };
    crate::flow::state::write_state(cfg, &state)?;
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

fn pr(
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
            "worktree is dirty — commit before opening a PR",
        ));
    }
    if !git::has_upstream() {
        ctx.out.info("pushing branch — no upstream yet");
        git::push()?;
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
    ctx.out
        .data(&json!({ "url": url, "draft": draft }), |_| url.clone());
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

fn db(ctx: &Ctx, cfg: &FlowConfig, cmd: DbCmd) -> CliResult {
    use crate::flow::state::read_state;
    match cmd {
        DbCmd::Connect => {
            pscale::ensure_auth()?;
            let state = read_state(cfg)?;
            let (branch, port) = match &state {
                Some(s) => (
                    s.pscale_branch.clone(),
                    if s.port != 0 { s.port } else { cfg.port },
                ),
                None => (pscale_branch_from_git_or_parent(cfg, None), cfg.port),
            };
            ctx.out.info(format!(
                "opening tunnel: {}/{branch} → 127.0.0.1:{port}",
                cfg.db
            ));
            pscale::connect(cfg, &branch, port)?;
            Ok(())
        }
        DbCmd::Status => {
            let state = read_state(cfg)?;
            match state {
                None => {
                    ctx.out.info(format!(
                        "no active feature branch — running on parent ({})",
                        cfg.parent
                    ));
                    ctx.out
                        .data(&json!({ "active": false, "parent": cfg.parent }), |_| {
                            format!("on parent ({})", cfg.parent)
                        });
                }
                Some(s) => {
                    ctx.out.data(&s, |_| {
                        format!(
                            "{} → {} (isolated: {})",
                            s.git_branch, s.pscale_branch, s.data_isolated
                        )
                    });
                }
            }
            Ok(())
        }
        DbCmd::End { force } => db_end(ctx, cfg, force),
        DbCmd::CurrentBranch => {
            let state = read_state(cfg)?;
            let branch = match state {
                Some(s) => s.pscale_branch,
                None => pscale_branch_from_git_or_parent(cfg, None),
            };
            ctx.out
                .data(&json!({ "branch": branch }), |_| branch.clone());
            Ok(())
        }
    }
}

fn db_end(ctx: &Ctx, cfg: &FlowConfig, force: bool) -> CliResult {
    use crate::flow::state::{clear_state, read_state};
    let state = match read_state(cfg)? {
        None => {
            ctx.out.info("no active feature branch");
            return Ok(());
        }
        Some(s) => s,
    };
    let parent = if state.parent.is_empty() {
        cfg.parent.clone()
    } else {
        state.parent.clone()
    };

    ctx.out.step(format!("git checkout {parent}"));
    git::checkout(&parent)?;

    if force && !state.data_isolated {
        return Err(CliError::expected(format!(
            "refusing --force: this flow is git-only (no paired pscale branch). target is the shared parent {:?}",
            state.pscale_branch
        )));
    } else if force {
        ctx.out.step(format!(
            "pscale branch delete {} {}",
            cfg.db, state.pscale_branch
        ));
        pscale::delete_branch(cfg, &state.pscale_branch)?;
    } else if state.data_isolated {
        ctx.out.info(format!(
            "leaving pscale branch {} alive — pass --force to delete",
            state.pscale_branch
        ));
    }

    env::clear_api_env_local(cfg)?;
    clear_state(cfg)?;
    ctx.out.success("done");
    Ok(())
}

fn pscale_branch_from_git_or_parent(cfg: &FlowConfig, _state: Option<()>) -> String {
    if let Ok(branch) = git::current_branch() {
        for t in BRANCH_TYPES {
            let prefix = format!("{t}/");
            if branch.starts_with(&prefix) && branch.len() > prefix.len() {
                return pscale_branch_from_git(&branch);
            }
        }
    }
    cfg.parent.clone()
}

/// Minimal RFC3339 UTC timestamp from the system clock, no chrono dependency.
fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // civil-from-days (Howard Hinnant's algorithm)
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (hh, mm, ss) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}
