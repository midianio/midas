mod checks;
mod cmd;
mod flow;
mod manifest;
mod proc;
mod registry;

use clap::{Parser, Subcommand};
use cmd::add::AddCmd;
use cmd::flow::FlowCmd;
use cmd::new::Profile;
use manifest::Manifest;
use midian_cli::exit::{finish, CliResult};
use midian_cli::{Ctx, GlobalArgs};
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
    /// Release / branch flow: start · sync · pr · hotfix · tag · db · doctor (the ported midflow).
    Flow {
        #[command(subcommand)]
        cmd: FlowCmd,
    },
    /// Scaffold a conventional piece (state · migration · component · module) as deterministic bytes.
    Add {
        #[command(subcommand)]
        cmd: AddCmd,
    },
    /// Scaffold a whole conformant project (midas.toml, agent docs, CI, dir shape).
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
}

fn main() {
    let cli = Cli::parse();
    let ctx = Ctx::new(cli.global);
    midian_cli::log::init(&ctx.global);

    let result: CliResult = (|| match cli.command {
        Commands::Flow { cmd } => {
            let cwd = std::env::current_dir()?;
            let manifest = Manifest::find(&cwd)?.map(|(m, _)| m).unwrap_or_default();
            cmd::flow::run(&ctx, &manifest, cmd)
        }
        Commands::Add { cmd } => cmd::add::run(&ctx, cmd),
        Commands::New {
            name,
            profile,
            dir,
            force,
        } => cmd::new::run(&ctx, name, profile, dir, force),
        Commands::Check { root } => cmd::check::run(&ctx, root),
        Commands::Sync { check } => cmd::sync::run(&ctx, check),
        Commands::Doctor => cmd::doctor::run(&ctx, false),
    })();

    std::process::exit(finish(&ctx.out, result));
}
