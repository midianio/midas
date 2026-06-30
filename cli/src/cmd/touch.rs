//! `midas touch` — the golden generator. Everything you touch is born conformant: a whole project
//! (`touch project`) or a conventional piece (`touch module|state|migration|component`), each stamped
//! as deterministic, standard-conformant bytes. A thin front door over `cmd::new` and `cmd::add`.

use crate::cmd::new::Profile;
use crate::core::exit::CliResult;
use crate::core::Ctx;
use clap::Subcommand;

#[derive(Subcommand)]
pub enum TouchCmd {
    /// A whole conformant project (midas.toml, agent docs, CI, dir shape).
    Project {
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
    /// A backend feature module: modules/<name>/{mod,handler,service,model}.rs + `pub mod` wiring.
    Module {
        /// Module name, e.g. `billing`
        name: Option<String>,
        /// Override the modules directory (default: app/api/src/modules)
        #[arg(long)]
        dir: Option<String>,
        /// Don't append `pub mod <name>;` to modules/mod.rs
        #[arg(long)]
        no_wire: bool,
        #[arg(long)]
        force: bool,
    },
    /// A Svelte runes state singleton (FE-0001) in lib/state/<name>.svelte.ts.
    State {
        /// Domain name, e.g. `notes-pane`
        name: Option<String>,
        /// Override the target directory (default: app/web/src/lib/state)
        #[arg(long)]
        dir: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// A forward-only numbered migration (OPS-0008) in db/migrations/NNN_<slug>.sql.
    Migration {
        /// Migration slug, e.g. `add-notes-index`
        slug: Option<String>,
        #[arg(long)]
        dir: Option<String>,
        #[arg(long)]
        force: bool,
    },
    /// A Svelte 5 component (FE-0011) in lib/components/<Name>.svelte (or components/ui with --ui).
    Component {
        /// Component name, e.g. `notes-toolbar`
        name: Option<String>,
        /// Override the target directory
        #[arg(long)]
        dir: Option<String>,
        /// Place under components/ui/ (a reusable UI primitive)
        #[arg(long)]
        ui: bool,
        #[arg(long)]
        force: bool,
    },
}

pub fn run(ctx: &Ctx, cmd: TouchCmd) -> CliResult {
    match cmd {
        TouchCmd::Project {
            name,
            profile,
            dir,
            force,
        } => crate::cmd::new::run(ctx, name, profile, dir, force),
        TouchCmd::Module {
            name,
            dir,
            no_wire,
            force,
        } => crate::cmd::add::module(ctx, name, dir, no_wire, force),
        TouchCmd::State { name, dir, force } => crate::cmd::add::state(ctx, name, dir, force),
        TouchCmd::Migration { slug, dir, force } => {
            crate::cmd::add::migration(ctx, slug, dir, force)
        }
        TouchCmd::Component {
            name,
            dir,
            ui,
            force,
        } => crate::cmd::add::component(ctx, name, dir, ui, force),
    }
}
