//! `midas migrate` — apply the forward-only `db/migrations/NNN_*.sql` against the local pscale
//! tunnel, or report status. The runner itself lives in [`crate::flow::migrate`]; this is the thin
//! command surface (arg parsing, runtime, exit-code mapping, human/JSON output).
//!
//! Dev/preview-only by construction (OPS-0004/OPS-0009): the runner refuses any non-local target —
//! schema reaches prod only through a reviewed PlanetScale deploy request, never `midas migrate`.

use crate::core::exit::{CliError, CliResult};
use crate::core::Ctx;
use crate::flow::config::{is_local_mysql_url, FlowConfig};
use crate::flow::migrate::{self, MigrateError, Report};
use crate::manifest::Manifest;
use clap::Subcommand;
use std::path::Path;

#[derive(Subcommand)]
pub enum MigrateCmd {
    /// Apply every pending migration in order (the default).
    Apply,
    /// Show which migrations are applied vs pending (read-only).
    Status,
}

pub fn run(ctx: &Ctx, cmd: MigrateCmd) -> CliResult {
    let cwd = std::env::current_dir().map_err(CliError::tool)?;
    let (manifest, root) = match Manifest::find(&cwd).map_err(CliError::tool)? {
        Some((m, r)) => (m, r),
        None => return Err(CliError::usage("no midas.toml found — run from a midas project")),
    };
    let cfg = FlowConfig::from_manifest(&manifest);
    let url = resolve_url(ctx, &cfg)?;
    match cmd {
        MigrateCmd::Apply => {
            let report = block_on(migrate::apply(&url, &root)).map_err(|e| to_cli_err(ctx, e))?;
            print_apply(ctx, &report);
            Ok(())
        }
        MigrateCmd::Status => {
            let report = block_on(migrate::status(&url, &root)).map_err(|e| to_cli_err(ctx, e))?;
            print_status(ctx, &report);
            Ok(())
        }
    }
}

/// Apply pending migrations as part of `midas dev` (after the tunnel is up, before the app starts).
/// Prints a concise line through `ctx.out`; a drift/apply failure aborts the caller.
pub fn apply_pending(ctx: &Ctx, manifest: &Manifest, root: &Path) -> CliResult {
    let cfg = FlowConfig::from_manifest(manifest);
    let url = resolve_url(ctx, &cfg)?;
    let report = block_on(migrate::apply(&url, root)).map_err(|e| to_cli_err(ctx, e))?;
    if report.newly_applied.is_empty() {
        ctx.out.step("migrations up to date");
    } else {
        ctx.out
            .success(format!("applied {} migration(s)", report.newly_applied.len()));
    }
    Ok(())
}

/// Resolve the sqlx URL and enforce the local-only guard (OPS-0009).
fn resolve_url(ctx: &Ctx, cfg: &FlowConfig) -> Result<String, CliError> {
    let url = cfg.migrate_url();
    if !is_local_mysql_url(&url) {
        let msg = "refusing to migrate a non-local database — schema reaches prod only via a \
                   reviewed PlanetScale deploy request, never `midas migrate` (OPS-0009)";
        ctx.out.error(msg);
        return Err(CliError::expected(msg));
    }
    Ok(url)
}

fn block_on<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime")
        .block_on(fut)
}

/// `Drift` is a clean "no" (exit 2) — surface it via `ctx.out` since the harness won't print
/// `Expected` for us. `Failed` (exit 1) is printed by the top-level `finish`.
fn to_cli_err(ctx: &Ctx, e: MigrateError) -> CliError {
    match e {
        MigrateError::Drift(msg) => {
            ctx.out.error(&msg);
            CliError::expected(msg)
        }
        MigrateError::Failed(err) => CliError::tool(err),
    }
}

fn print_apply(ctx: &Ctx, report: &Report) {
    for file in &report.newly_applied {
        ctx.out.success(format!("applied {file}"));
    }
    let applied = report.states.iter().filter(|s| s.applied).count();
    if report.newly_applied.is_empty() {
        ctx.out
            .success(format!("up to date — {applied} migration(s) applied"));
    } else {
        ctx.out.success(format!(
            "{} applied this run — {applied} total",
            report.newly_applied.len()
        ));
    }
    ctx.out.data(report, |_| human_summary(report));
}

fn print_status(ctx: &Ctx, report: &Report) {
    let pending = report.states.iter().filter(|s| !s.applied).count();
    for s in &report.states {
        let mark = if s.applied { "✓" } else { "·" };
        let tail = if s.applied { "" } else { " (pending)" };
        ctx.out.info(format!("{mark} {}{tail}", s.version));
    }
    ctx.out.step(format!(
        "{} applied · {pending} pending",
        report.states.len() - pending
    ));
    ctx.out.data(report, |_| human_summary(report));
}

fn human_summary(report: &Report) -> String {
    let pending = report.states.iter().filter(|s| !s.applied).count();
    format!(
        "{} migrations · {} applied · {} pending",
        report.states.len(),
        report.states.len() - pending,
        pending
    )
}
