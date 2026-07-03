mod checks;
mod cmd;
mod core;
mod flow;
mod manifest;
mod proc;
mod registry;

use crate::core::exit::{finish, CliResult};
use crate::core::{Ctx, GlobalArgs};
use clap::{CommandFactory, Parser, Subcommand};
use cmd::flow::FlowCmd;
use cmd::migrate::MigrateCmd;
use cmd::new::Profile;
use cmd::touch::TouchCmd;
use manifest::Manifest;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "midas",
    version,
    about = "The midian engineering standard — CLI",
    long_about = None,
)]
struct Cli {
    #[command(flatten)]
    global: GlobalArgs,
    #[command(subcommand)]
    command: Commands,
}

/// Declaration order is help order (clap keeps it), grouped by rhythm: the daily loop
/// (dev · flow · migrate), the standards family (check · drift · sync · explain · conventions ·
/// deviate), then setup & tooling (touch · adopt · doctor · completions).
#[derive(Subcommand)]
enum Commands {
    /// Run the project's dev processes concurrently (+ the pscale tunnel) — `[dev]` in midas.toml.
    Dev {
        /// Run only these named processes (the tunnel always runs); default: all.
        only: Vec<String>,
        /// Disable the watch-and-restart loop for processes that declare `watch` paths.
        #[arg(long)]
        no_watch: bool,
    },
    /// Release / branch flow: start · rebase · ship · tag · end · status · clean.
    Flow {
        #[command(subcommand)]
        cmd: FlowCmd,
    },
    /// Apply forward-only migrations (`db/migrations/`) to the local tunnel, or show status.
    /// Create one with `midas touch migration`.
    Migrate {
        /// Defaults to `apply` when omitted.
        #[command(subcommand)]
        cmd: Option<MigrateCmd>,
    },
    /// Lint the repo against the pinned standard — the mechanical conformance gate.
    Check {
        /// Scan only files changed vs origin/<trunk> (+ untracked) — the fast pre-commit pass.
        /// Structure checks still run repo-wide; CI should keep running the full scan.
        #[arg(long)]
        changed: bool,
    },
    /// Explain standard drift: what changes for this repo if the pinned standard moves (read-only).
    Drift {
        /// Version to diff: `<to>` or `<from>..<to>`. Default: the pinned version → the embedded one.
        spec: Option<String>,
        /// Diff against a local conventions.json as the `from` side (unreleased/WIP registry).
        #[arg(long, value_name = "PATH")]
        from_file: Option<PathBuf>,
        /// Diff against a local conventions.json as the `to` side (unreleased/WIP registry).
        #[arg(long, value_name = "PATH")]
        to_file: Option<PathBuf>,
    },
    /// Write/update the version-stamped midas managed block in agent docs.
    Sync {
        /// Report drift without writing (exit 2 if a block is missing/stale).
        #[arg(long)]
        check: bool,
    },
    /// Explain one convention: what it requires, how it's enforced, its escape policy, its doc.
    Explain {
        /// Convention id, e.g. BE-0010
        id: String,
    },
    /// List the embedded convention catalog (the standard this binary enforces).
    Conventions {
        /// Filter by enforcement tier: check | review
        #[arg(long)]
        tier: Option<String>,
        /// Filter by escape policy: hard | ledgered | advisory
        #[arg(long)]
        escape: Option<String>,
        /// Filter by layer, e.g. backend | frontend | cli | process | agent | stack
        #[arg(long)]
        layer: Option<String>,
    },
    /// Ledger a deviation in midas.toml [deviations] (refused for `hard`-escape conventions).
    Deviate {
        /// Convention id, e.g. FE-0004
        id: Option<String>,
        /// Why this project deviates (recorded in the ledger)
        #[arg(long)]
        reason: Option<String>,
        /// Drop ledger entries whose conventions now pass (the `drift` ledger-cleanup worklist).
        #[arg(long)]
        prune: bool,
    },
    /// Scaffold a conformant project or piece (project · module · state · migration · component).
    Touch {
        #[command(subcommand)]
        cmd: TouchCmd,
    },
    /// Adopt the standard in an existing repo: pinned midas.toml + synced agent docs + first check.
    Adopt {
        /// Project profile (prompted when omitted; default: app)
        #[arg(long, value_enum)]
        profile: Option<Profile>,
    },
    /// Diagnose the local dev environment.
    Doctor {
        /// Remediate the fixable subset (today: re-sync a missing/stale agent-docs block).
        #[arg(long)]
        fix: bool,
    },
    /// Generate shell completions to stdout (bash · zsh · fish · elvish · powershell).
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Deprecated alias for `midas touch project`.
    #[command(hide = true)]
    New {
        /// Project name
        name: Option<String>,
        /// Project profile
        #[arg(long, value_enum, default_value_t = Profile::App)]
        profile: Profile,
        /// Parent directory to create the project in (default: cwd)
        #[arg(long)]
        dir: Option<String>,
        #[arg(long)]
        force: bool,
    },
}

fn main() {
    let cli = Cli::parse();
    let ctx = Ctx::new(cli.global);
    crate::core::log::init(&ctx.global);

    let result: CliResult = (|| match cli.command {
        Commands::Dev { only, no_watch } => cmd::dev::run(&ctx, only, no_watch),
        Commands::Flow { cmd } => {
            let start = manifest::resolve_root(&ctx.global)?;
            let manifest = Manifest::find(&start)?.map(|(m, _)| m).unwrap_or_default();
            cmd::flow::run(&ctx, &manifest, cmd)
        }
        Commands::Migrate { cmd } => cmd::migrate::run(&ctx, cmd.unwrap_or(MigrateCmd::Apply)),
        Commands::Check { changed } => cmd::check::run(&ctx, changed),
        Commands::Drift {
            spec,
            from_file,
            to_file,
        } => cmd::drift::run(&ctx, spec, from_file, to_file),
        Commands::Sync { check } => cmd::sync::run(&ctx, check),
        Commands::Explain { id } => cmd::explain::explain(&ctx, &id),
        Commands::Conventions {
            tier,
            escape,
            layer,
        } => cmd::explain::list(&ctx, tier, escape, layer),
        Commands::Deviate { id, reason, prune } => cmd::deviate::run(&ctx, id, reason, prune),
        Commands::Touch { cmd } => cmd::touch::run(&ctx, cmd),
        Commands::Adopt { profile } => cmd::adopt::run(&ctx, profile),
        Commands::Doctor { fix } => cmd::doctor::run(&ctx, true, fix),
        Commands::Completions { shell } => {
            // Completions are the command's data — stdout is correct here (CLI-0003).
            clap_complete::generate(shell, &mut Cli::command(), "midas", &mut std::io::stdout());
            Ok(())
        }
        Commands::New {
            name,
            profile,
            dir,
            force,
        } => cmd::new::run(&ctx, name, profile, dir, force),
    })();

    std::process::exit(finish(&ctx.out, result));
}
