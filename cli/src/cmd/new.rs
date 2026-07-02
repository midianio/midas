//! `midas touch project` (alias `midas new`) — scaffold a whole conformant project: `midas.toml`
//! (version pinned to this binary),
//! agent docs with the synced managed block, a starter CI, and the standard dir shape. Code profiles
//! also lay down a runnable, conformant skeleton (the `rust-service` template for `service`) that
//! compiles and passes `midas check` out of the box.

use crate::cmd::sync::managed_block;
use crate::cmd::templates::{self, TemplateFile, RUST_SERVICE, SVELTE_APP};
use crate::core::exit::{CliError, CliResult};
use crate::core::{prompt_line, Ctx};
use crate::flow::config::slugify;
use crate::registry::Registry;
use clap::ValueEnum;
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Profile {
    /// Frontend + backend product (svelte + rust).
    App,
    /// Backend service only (rust).
    Service,
    /// A CLI/tooling repo (rust).
    Cli,
    /// A library/shared package.
    Library,
    /// A data pipeline (orca-like; process/agent layers only).
    Pipeline,
}

impl Profile {
    fn as_str(self) -> &'static str {
        match self {
            Profile::App => "app",
            Profile::Service => "service",
            Profile::Cli => "cli",
            Profile::Library => "library",
            Profile::Pipeline => "pipeline",
        }
    }

    /// The runnable code skeleton(s) this profile lays down, token-substituted at write time. App
    /// gets both the backend service and the frontend app; service gets just the backend.
    fn code_template(self) -> Vec<&'static TemplateFile> {
        match self {
            Profile::App => RUST_SERVICE.iter().chain(SVELTE_APP).collect(),
            Profile::Service => RUST_SERVICE.iter().collect(),
            Profile::Cli | Profile::Library | Profile::Pipeline => Vec::new(),
        }
    }

    /// `[stack]` lines for the manifest — only the layers this profile actually has.
    fn stack_toml(self) -> String {
        match self {
            Profile::App => {
                "backend = { current = \"rust\" }\nfrontend = { current = \"svelte\" }\n".into()
            }
            Profile::Service => "backend = { current = \"rust\" }\n".into(),
            Profile::Cli => "cli = { current = \"rust\" }\n".into(),
            Profile::Library | Profile::Pipeline => String::new(),
        }
    }
}

pub fn run(
    ctx: &Ctx,
    name: Option<String>,
    profile: Profile,
    dir: Option<String>,
    force: bool,
) -> CliResult {
    let raw = match name {
        Some(n) => n,
        None => prompt_line(&ctx.out, &ctx.global, "Project name", None)?,
    };
    let name = slugify(&raw);
    if name.is_empty() {
        return Err(CliError::usage("name must contain letters/digits"));
    }

    let base = dir
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));
    let root = base.join(&name);

    if root.exists() && !is_empty_dir(&root) && !force {
        return Err(CliError::expected(format!(
            "{} already exists and is not empty (pass --force)",
            root.display()
        )));
    }

    let version = Registry::embedded()
        .map(|r| r.version)
        .unwrap_or_else(|_| "0.0.0".into());
    let block = managed_block(&version);

    let mut files: Vec<(String, String)> = vec![
        ("midas.toml".into(), manifest_toml(&version, profile)),
        ("README.md".into(), readme(&name, profile)),
        (".gitignore".into(), GITIGNORE.to_string()),
        ("CLAUDE.md".into(), format!("# {name}\n\n{block}\n")),
        ("AGENTS.md".into(), format!("# {name}\n\n{block}\n")),
        (".github/workflows/ci.yml".into(), ci_yml(profile)),
    ];
    // Runnable code skeleton (token-substituted), for profiles that ship one.
    for tf in profile.code_template() {
        files.push((tf.path.to_string(), templates::render(tf.body, &name)));
    }

    let mut created = Vec::new();
    for (rel, body) in &files {
        let path = root.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, body)?;
        created.push(format!("{name}/{rel}"));
    }

    ctx.out
        .success(format!("created project {name} ({})", profile.as_str()));
    ctx.out.info(format!("cd {name} && midas check"));
    match profile {
        Profile::Service => ctx.out.hint("build the service: cd app/api && cargo build"),
        Profile::App => ctx.out.hint(
            "build: (cd app/api && cargo build) · (cd app/web && bun install && bun run build)",
        ),
        _ => {}
    }
    ctx.out
        .hint("scaffold pieces with `midas touch …`; start the flow with `midas flow start`");
    ctx.out.data(
        &json!({ "created": created, "profile": profile.as_str(), "version": version }),
        |_| name.clone(),
    );
    Ok(())
}

fn is_empty_dir(p: &Path) -> bool {
    std::fs::read_dir(p)
        .map(|mut rd| rd.next().is_none())
        .unwrap_or(true)
}

/// The `[dev]` block for `midas dev` — the concurrent process set per profile. Only code profiles
/// (app/service) get one; the tunnel is off by default (enable it + set `[flow]` pscale_* when the
/// project uses a PlanetScale DB).
fn dev_block(profile: Profile) -> String {
    let processes = match profile {
        Profile::App => {
            "  { name = \"api\", cwd = \"app/api\", cmd = \"cargo run\" },\n  \
{ name = \"web\", cwd = \"app/web\", cmd = \"bun run dev\" },\n"
        }
        Profile::Service => "  { name = \"api\", cwd = \"app/api\", cmd = \"cargo run\" },\n",
        Profile::Cli | Profile::Library | Profile::Pipeline => return String::new(),
    };
    format!(
        "\n# `midas dev` runs these concurrently (prefixed output, one Ctrl-C tears all down). Set\n\
# tunnel = true (and configure [flow] pscale_*) to raise a PlanetScale tunnel before they start.\n\
[dev]\n\
tunnel = false\n\
processes = [\n{processes}]\n"
    )
}

fn manifest_toml(version: &str, profile: Profile) -> String {
    let stack = profile.stack_toml();
    let stack_block = if stack.is_empty() {
        String::new()
    } else {
        format!("\n[stack]\n{stack}")
    };
    format!(
        "# Pins this project to a midas version (governs the CLI + embedded rules). See SPEC §7.\n\
[standard]\n\
version = \"{version}\"\n\
profile = \"{}\"\n\
{stack_block}\n\
[flow]\n\
trunk = \"main\"\n\
{}\n\
# Ledgered escape hatches: convention id → reason. `midas check` treats these as expected.\n\
[deviations]\n",
        profile.as_str(),
        dev_block(profile),
    )
}

fn readme(name: &str, profile: Profile) -> String {
    format!(
        "# {name}\n\n\
A midian project ({} profile). It follows the **midian engineering standard** (`midas`).\n\n\
- `midas check` — the conformance gate (must be clean, or ledgered in `midas.toml`).\n\
- `midas touch …` — scaffold a conformant project or piece.\n\
- `midas flow …` — the release/branch flow.\n\n\
Conventions live in the `midas` repo under `standards/`; this repo pins its version in `midas.toml`.\n",
        profile.as_str()
    )
}

fn ci_yml(profile: Profile) -> String {
    // Stack-specific gate hints, commented so the starter is valid as-is.
    let stack_gates = match profile {
        Profile::App => "      # - run: cargo clippy --workspace -- -D warnings\n      # - run: bun run check && bun run build\n",
        Profile::Service | Profile::Cli => "      # - run: cargo clippy --workspace -- -D warnings\n      # - run: cargo test\n",
        Profile::Library => "      # - run: cargo test\n",
        Profile::Pipeline => "      # - run: <your pipeline lint/test>\n",
    };
    format!(
        "name: ci\n\
on:\n\
  push: {{ branches: [main] }}\n\
  pull_request:\n\
\n\
jobs:\n\
  midas:\n\
    runs-on: ubuntu-latest\n\
    steps:\n\
      - uses: actions/checkout@v4\n\
      # TODO: install midas (pin once published), then gate on it:\n\
      # - run: midas check\n\
      # - run: midas sync --check   # agent docs block is current\n\
{stack_gates}"
    )
}

const GITIGNORE: &str = "# Rust\n/target\n**/target\n\n# JS\nnode_modules\ndist\n.turbo\n.svelte-kit\nbuild\n\n# local env / os\n.env\n.env.local\n.DS_Store\n";
