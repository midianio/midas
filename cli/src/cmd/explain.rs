//! `midas explain <ID>` + `midas conventions` — the embedded registry, self-served. `check` says
//! *that* BE-0010 failed; `explain` answers the next question — what the convention requires, how
//! it's enforced, what the escape policy is, and where the full doc lives — without leaving the
//! terminal. `conventions` lists the whole catalog (filterable), so the standard is discoverable
//! from the binary itself, not just by reading the `midas` repo.

use crate::core::exit::{CliError, CliResult};
use crate::core::Ctx;
use crate::registry::{CheckSpec, Convention, Escape, Registry, Tier};
use serde_json::json;

pub fn explain(ctx: &Ctx, id: &str) -> CliResult {
    let registry = Registry::embedded().map_err(CliError::tool)?;
    let wanted = id.to_uppercase();
    let Some(conv) = registry.by_id(&wanted) else {
        let mut msg = format!("unknown convention id {wanted:?}");
        let near: Vec<&str> = registry
            .conventions
            .iter()
            .map(|c| c.id.as_str())
            .filter(|c| c.split('-').next() == wanted.split('-').next())
            .collect();
        if near.is_empty() {
            msg.push_str(" — see `midas conventions` for the catalog");
        } else {
            msg.push_str(&format!(" — nearby ids: {}", near.join(", ")));
        }
        return Err(CliError::usage(msg));
    };

    ctx.out.data(
        &json!({ "version": registry.version, "convention": conv }),
        |s| {
            let mut o = String::new();
            o.push_str(&format!("{} · {}\n", s.bold(&conv.id), conv.title));
            let stack = conv
                .stack
                .as_deref()
                .map(|st| format!(" ({st})"))
                .unwrap_or_default();
            o.push_str(&format!("  {}    {}{stack}\n", s.dim("layer"), conv.layer));
            o.push_str(&format!(
                "  {}     {}\n",
                s.dim("tier"),
                tier_line(conv.tier)
            ));
            o.push_str(&format!(
                "  {}   {}\n",
                s.dim("escape"),
                escape_line(conv.escape)
            ));
            if let Some(spec) = &conv.check {
                o.push_str(&format!("  {}    {}\n", s.dim("check"), spec_line(spec)));
                if let Some(m) = spec_message(spec) {
                    o.push_str(&format!("           {}\n", s.dim(&format!("→ {m}"))));
                }
            }
            if let Some(doc) = &conv.doc {
                o.push_str(&format!("  {}      standards/{doc}\n", s.dim("doc")));
            }
            o
        },
    );
    Ok(())
}

pub fn list(
    ctx: &Ctx,
    tier: Option<String>,
    escape: Option<String>,
    layer: Option<String>,
) -> CliResult {
    let registry = Registry::embedded().map_err(CliError::tool)?;

    let tier = tier.as_deref().map(parse_tier).transpose()?;
    let escape = escape.as_deref().map(parse_escape).transpose()?;

    let matches: Vec<&Convention> = registry
        .conventions
        .iter()
        .filter(|c| tier.is_none_or(|t| c.tier == t))
        .filter(|c| escape.is_none_or(|e| c.escape == e))
        .filter(|c| {
            layer
                .as_deref()
                .is_none_or(|l| c.layer.eq_ignore_ascii_case(l))
        })
        .collect();

    ctx.out.data(
        &json!({
            "version": registry.version,
            "count": matches.len(),
            "conventions": &matches,
        }),
        |s| {
            let mut o = String::new();
            o.push_str(&format!(
                "{} v{} · {} convention(s)\n",
                s.bold("midas standard"),
                registry.version,
                matches.len()
            ));
            for c in &matches {
                o.push_str(&format!(
                    "  {}  {:<8} {:<9} {}\n",
                    s.dim(&format!("{:<9}", c.id)),
                    tier_str(c.tier),
                    escape_str(c.escape),
                    c.title
                ));
            }
            o.push_str(&s.dim("\n  details: midas explain <ID>\n"));
            o
        },
    );
    Ok(())
}

fn parse_tier(s: &str) -> Result<Tier, CliError> {
    match s.to_lowercase().as_str() {
        "check" => Ok(Tier::Check),
        "review" => Ok(Tier::Review),
        other => Err(CliError::usage(format!(
            "invalid --tier {other:?} (must be: check, review)"
        ))),
    }
}

fn parse_escape(s: &str) -> Result<Escape, CliError> {
    match s.to_lowercase().as_str() {
        "hard" => Ok(Escape::Hard),
        "ledgered" => Ok(Escape::Ledgered),
        "advisory" => Ok(Escape::Advisory),
        other => Err(CliError::usage(format!(
            "invalid --escape {other:?} (must be: hard, ledgered, advisory)"
        ))),
    }
}

fn tier_str(t: Tier) -> &'static str {
    match t {
        Tier::Check => "check",
        Tier::Review => "review",
    }
}

fn escape_str(e: Escape) -> &'static str {
    match e {
        Escape::Hard => "hard",
        Escape::Ledgered => "ledgered",
        Escape::Advisory => "advisory",
    }
}

fn tier_line(t: Tier) -> &'static str {
    match t {
        Tier::Check => "check — verified mechanically by `midas check`",
        Tier::Review => "review — semantic; delegated to your review agent (not machine-checked)",
    }
}

fn escape_line(e: Escape) -> &'static str {
    match e {
        Escape::Hard => "hard — no deviation allowed (a ledger entry is itself a check failure)",
        Escape::Ledgered => {
            "ledgered — deviation allowed if recorded: `midas deviate <ID> --reason …`"
        }
        Escape::Advisory => "advisory — recommended; a violation never blocks",
    }
}

/// One-line summary of the mechanical spec.
fn spec_line(spec: &CheckSpec) -> String {
    match spec {
        CheckSpec::BannedCall {
            pattern,
            allow_in,
            globs,
            ..
        } => {
            let allow = if allow_in.is_empty() {
                String::new()
            } else {
                format!(" (allowed in: {})", allow_in.join(", "))
            };
            format!("banned-call: `{pattern}` in {}{allow}", globs.join(", "))
        }
        CheckSpec::FileStructure {
            must_exist,
            must_not_exist,
        } => {
            let mut parts = Vec::new();
            if !must_exist.is_empty() {
                parts.push(format!("must exist: {}", must_exist.join(", ")));
            }
            if !must_not_exist.is_empty() {
                parts.push(format!("must not exist: {}", must_not_exist.join(", ")));
            }
            format!("file-structure: {}", parts.join(" · "))
        }
        CheckSpec::BannedFile { globs, .. } => {
            format!("banned-file: {} must be gitignored", globs.join(", "))
        }
        CheckSpec::ManagedBlock {} => {
            "managed-block: agent docs carry the current `midas sync` block".into()
        }
        CheckSpec::ArtifactHash { .. } => "artifact-hash (deferred — reported as skipped)".into(),
        CheckSpec::ProvenanceDrift {} => "provenance-drift (deferred — reported as skipped)".into(),
        CheckSpec::Clippy { .. } => {
            "clippy passthrough (deferred — CI runs clippy directly)".into()
        }
    }
}

fn spec_message(spec: &CheckSpec) -> Option<&str> {
    match spec {
        CheckSpec::BannedCall { message, .. } | CheckSpec::BannedFile { message, .. } => {
            message.as_deref()
        }
        _ => None,
    }
}
