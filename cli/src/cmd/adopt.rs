//! `midas adopt` — brownfield onboarding. `touch project` covers greenfield; this is the other
//! door: bring an *existing* repo under the standard in one command. It writes a pinned
//! `midas.toml` (if absent), syncs the agent-docs managed block, then runs the mechanical gate —
//! offering to ledger standing `ledgered`-escape violations as dated-for-triage deviations so the
//! repo is immediately `check`-clean where the standard allows it. `hard` violations can't be
//! ledgered; they surface as the adoption worklist. Idempotent: re-running re-syncs and re-checks.

use crate::checks::Scanner;
use crate::cmd::check::{outcome_of, Outcome};
use crate::cmd::deviate;
use crate::cmd::new::Profile;
use crate::core::exit::{CliError, CliResult};
use crate::core::{prompt_line, Ctx};
use crate::manifest::{Manifest, MANIFEST_NAME};
use crate::registry::{Escape, Registry, Tier};
use clap::ValueEnum;

pub fn run(ctx: &Ctx, profile: Option<Profile>) -> CliResult {
    let root = crate::manifest::resolve_root(&ctx.global)?;
    let registry = Registry::embedded().map_err(CliError::tool)?;
    let manifest_path = root.join(MANIFEST_NAME);

    ctx.out.banner(format!(
        "Adopting midas {} in {}",
        registry.version,
        root.display()
    ));

    // 1. The pinned manifest (skipped when one exists — adopt never overwrites).
    if manifest_path.is_file() {
        ctx.out.info("midas.toml already present — keeping it");
    } else {
        let profile = match profile {
            Some(p) => p,
            None => {
                let names: Vec<String> = Profile::value_variants()
                    .iter()
                    .filter_map(|p| p.to_possible_value().map(|v| v.get_name().to_string()))
                    .collect();
                let raw = prompt_line(
                    &ctx.out,
                    &ctx.global,
                    &format!("Profile [{}]", names.join("/")),
                    Some("app"),
                )?;
                Profile::from_str(&raw, true)
                    .map_err(|_| CliError::usage(format!("invalid profile {raw:?}")))?
            }
        };
        std::fs::write(&manifest_path, manifest_toml(&registry.version, profile))?;
        ctx.out.success(format!(
            "wrote midas.toml (version {}, profile {})",
            registry.version,
            profile
                .to_possible_value()
                .map(|v| v.get_name().to_string())
                .unwrap_or_default()
        ));
    }

    // 2. The version-stamped agent-docs block (creates CLAUDE.md/AGENTS.md when missing).
    let (version, changed) = crate::cmd::sync::write_blocks(&root)?;
    if changed.is_empty() {
        ctx.out
            .info(format!("agent docs already current ({version})"));
    } else {
        ctx.out
            .success(format!("synced {} ({version})", changed.join(", ")));
    }

    // 3. Standing violations: offer to ledger the ledgerable ones so the gate reflects intent.
    let (manifest, _) = Manifest::find(&root)?
        .ok_or_else(|| CliError::tool(anyhow::anyhow!("midas.toml vanished after write")))?;
    let mut scanner = Scanner::new(&root).map_err(CliError::tool)?;
    let mut ledgerable: Vec<String> = Vec::new();
    for conv in &registry.conventions {
        if conv.tier != Tier::Check || conv.escape != Escape::Ledgered {
            continue;
        }
        let e = outcome_of(conv, &manifest, true, &mut scanner, &registry.version);
        if e.outcome == Outcome::Fail {
            ledgerable.push(conv.id.clone());
        }
    }
    if !ledgerable.is_empty() {
        for id in &ledgerable {
            ctx.out
                .info(format!("standing violation (ledgerable): {id}"));
        }
        if ctx.confirm(
            &format!(
                "Ledger {} standing violation(s) in [deviations] for later triage?",
                ledgerable.len()
            ),
            true,
        )? {
            let raw = std::fs::read_to_string(&manifest_path)?;
            let mut next = raw.clone();
            for id in &ledgerable {
                next = deviate::upsert(&next, id, "standing violation at adoption — triage").0;
            }
            deviate::write_verified(&manifest_path, &raw, &next)?;
            ctx.out
                .success(format!("ledgered {} deviation(s)", ledgerable.len()));
            ctx.out
                .hint("review them with `midas conventions` + `midas deviate --prune` as you fix");
        }
    }

    // 4. Finish on the gate itself — what's left is the real adoption worklist.
    crate::cmd::check::run(ctx, false)
}

/// A minimal brownfield manifest: the pin, the profile's stack layers, and an empty ledger. No
/// `[dev]` processes — an existing repo already has its own way to run; wire `[dev]` up by hand.
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
# Ledgered escape hatches: convention id → reason. `midas check` treats these as expected.\n\
[deviations]\n",
        profile.as_str(),
    )
}
