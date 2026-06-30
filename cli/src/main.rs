mod checks;
mod cmd;
mod core;
mod flow;
mod manifest;
mod proc;
mod registry;

use crate::core::exit::{finish, CliResult};
use crate::core::{Ctx, GlobalArgs};
use clap::{Parser, Subcommand};
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

#[derive(Subcommand)]
enum Commands {
    /// Release / branch flow: start · sync · pr · tag · end · status.
    Flow {
        #[command(subcommand)]
        cmd: FlowCmd,
    },
    /// Scaffold a conformant project or piece (project · module · state · migration · component).
    Touch {
        #[command(subcommand)]
        cmd: TouchCmd,
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
    /// Lint the repo against the pinned standard — the mechanical conformance gate.
    Check {
        /// Project root to check (defaults to the git toplevel).
        #[arg(long, value_parser = cmd::check::parse_root)]
        root: Option<PathBuf>,
    },
    /// Write/update the version-stamped midas managed block in agent docs.
    Sync {
        /// Report drift without writing (exit 2 if a block is missing/stale).
        #[arg(long)]
        check: bool,
    },
    /// Diagnose the local dev environment.
    Doctor,
    /// Run the project's dev processes concurrently (+ the pscale tunnel) — `[dev]` in midas.toml.
    Dev {
        /// Run only these named processes (the tunnel always runs); default: all.
        only: Vec<String>,
    },
    /// Apply forward-only migrations (`db/migrations/`) to the local tunnel, or show status.
    Migrate {
        /// Defaults to `apply` when omitted.
        #[command(subcommand)]
        cmd: Option<MigrateCmd>,
    },
}

fn main() {
    let cli = Cli::parse();
    let ctx = Ctx::new(cli.global);
    crate::core::log::init(&ctx.global);

    let result: CliResult = (|| match cli.command {
        Commands::Flow { cmd } => {
            let cwd = std::env::current_dir()?;
            let manifest = Manifest::find(&cwd)?.map(|(m, _)| m).unwrap_or_default();
            cmd::flow::run(&ctx, &manifest, cmd)
        }
        Commands::Touch { cmd } => cmd::touch::run(&ctx, cmd),
        Commands::New {
            name,
            profile,
            dir,
            force,
        } => cmd::new::run(&ctx, name, profile, dir, force),
        Commands::Check { root } => cmd::check::run(&ctx, root),
        Commands::Sync { check } => cmd::sync::run(&ctx, check),
        Commands::Doctor => cmd::doctor::run(&ctx, true),
        Commands::Dev { only } => cmd::dev::run(&ctx, only),
        Commands::Migrate { cmd } => cmd::migrate::run(&ctx, cmd.unwrap_or(MigrateCmd::Apply)),
    })();

    std::process::exit(finish(&ctx.out, result));
}
