//! `midas deviate` — manage the `midas.toml [deviations]` ledger without hand-editing TOML. The
//! ledger is a first-class seam (`check` classifies against it, `drift` reports its cleanup), so
//! writing an entry gets the same discipline as the gate: the id must exist in the embedded
//! registry, `hard`-escape rules are refused outright (an entry against them is itself a check
//! failure), and `advisory` rules are refused as pointless (they never block). `--prune` drops
//! entries whose conventions now pass — exactly the `ledger_cleanup` work `drift` identifies.
//!
//! Edits are textual (comments and formatting outside the touched lines survive) and verified: the
//! rewritten file must still parse as a manifest or the original is restored.

use crate::checks::Scanner;
use crate::cmd::check::{outcome_of, Outcome};
use crate::core::config::load_toml;
use crate::core::exit::{CliError, CliResult};
use crate::core::{prompt_line, Ctx};
use crate::manifest::{Manifest, MANIFEST_NAME};
use crate::registry::{Escape, Registry, Tier};
use serde_json::json;
use std::path::Path;

pub fn run(ctx: &Ctx, id: Option<String>, reason: Option<String>, prune: bool) -> CliResult {
    let root = crate::manifest::resolve_root(&ctx.global)?;
    let path = root.join(MANIFEST_NAME);
    if !path.is_file() {
        return Err(CliError::usage(format!(
            "no midas.toml at {} — adopt the standard first (`midas adopt`)",
            root.display()
        )));
    }

    if prune {
        if id.is_some() {
            return Err(CliError::usage(
                "--prune takes no convention id — it sweeps the whole ledger",
            ));
        }
        return prune_ledger(ctx, &root, &path);
    }

    let id = match id {
        Some(i) => i.to_uppercase(),
        None => {
            return Err(CliError::usage(
                "convention id required, e.g. `midas deviate FE-0004 --reason \"web-only\"`",
            ))
        }
    };
    let registry = Registry::embedded().map_err(CliError::tool)?;
    let Some(conv) = registry.by_id(&id) else {
        return Err(CliError::usage(format!(
            "unknown convention id {id:?} — see `midas conventions` for the catalog"
        )));
    };
    match conv.escape {
        Escape::Hard => {
            return Err(CliError::usage(format!(
                "{id} is a `hard` rule — no deviation allowed; fix the finding (see `midas explain {id}`)"
            )))
        }
        Escape::Advisory => {
            return Err(CliError::usage(format!(
                "{id} is advisory — it never blocks, so a ledger entry would be dead weight"
            )))
        }
        Escape::Ledgered => {}
    }

    let reason = match reason {
        Some(r) => r,
        None => prompt_line(&ctx.out, &ctx.global, "Reason (--reason)", None)?,
    };
    if reason.trim().is_empty() {
        return Err(CliError::usage("--reason must not be empty"));
    }

    let raw = std::fs::read_to_string(&path)?;
    let (next, updated) = upsert(&raw, &id, &reason);
    write_verified(&path, &raw, &next)?;

    ctx.out.success(format!(
        "{} deviation {id}: {reason}",
        if updated { "updated" } else { "ledgered" }
    ));
    ctx.out
        .hint("`midas check` now classifies its findings as ledgered, not failing");
    ctx.out.data(
        &json!({ "id": id, "reason": reason, "updated": updated }),
        |_| format!("{id} ledgered"),
    );
    Ok(())
}

/// Drop ledger entries whose conventions now pass mechanically (or no longer exist in the
/// standard). Review-tier and still-violated entries are kept and reported.
fn prune_ledger(ctx: &Ctx, root: &Path, path: &Path) -> CliResult {
    let manifest: Manifest = load_toml(path).map_err(CliError::tool)?;
    if manifest.deviations.is_empty() {
        ctx.out.info("ledger is empty — nothing to prune");
        ctx.out.data(&json!({ "pruned": [], "kept": [] }), |_| {
            "ledger empty".into()
        });
        return Ok(());
    }
    let registry = Registry::embedded().map_err(CliError::tool)?;
    let mut scanner = Scanner::new(root).map_err(CliError::tool)?;

    let mut pruned: Vec<String> = Vec::new();
    let mut kept: Vec<(String, &'static str)> = Vec::new();
    for id in manifest.deviations.keys() {
        match registry.by_id(id) {
            None => pruned.push(id.clone()), // gone from the standard — dead entry
            Some(c) if c.tier == Tier::Review => {
                kept.push((id.clone(), "review-tier (not mechanically verifiable)"))
            }
            Some(c) => {
                let e = outcome_of(c, &manifest, true, &mut scanner, &registry.version);
                match e.outcome {
                    Outcome::Pass => pruned.push(id.clone()),
                    Outcome::Skipped => {
                        kept.push((id.clone(), "check skipped (stack n/a or deferred)"))
                    }
                    _ => kept.push((id.clone(), "still violated — entry still needed")),
                }
            }
        }
    }

    if pruned.is_empty() {
        ctx.out
            .info("nothing to prune — every entry is still load-bearing");
    } else {
        let raw = std::fs::read_to_string(path)?;
        let next = remove_entries(&raw, &pruned);
        write_verified(path, &raw, &next)?;
        for id in &pruned {
            ctx.out
                .success(format!("pruned {id} — passes under the current standard"));
        }
    }
    for (id, why) in &kept {
        ctx.out.info(format!("kept {id} — {why}"));
    }
    let kept_ids: Vec<&String> = kept.iter().map(|(id, _)| id).collect();
    ctx.out
        .data(&json!({ "pruned": &pruned, "kept": kept_ids }), |_| {
            format!("pruned {} · kept {}", pruned.len(), kept_ids.len())
        });
    Ok(())
}

/// Write `next`, then prove it still parses as a manifest; on parse failure restore `original`.
pub(crate) fn write_verified(path: &Path, original: &str, next: &str) -> CliResult {
    std::fs::write(path, next)?;
    if let Err(e) = load_toml::<Manifest>(path) {
        std::fs::write(path, original)?;
        return Err(CliError::tool(anyhow::anyhow!(
            "rewritten midas.toml failed to parse — restored the original: {e}"
        )));
    }
    Ok(())
}

/// Insert or replace `"<id>" = "<reason>"` inside `[deviations]`, creating the section if absent.
/// Returns (new content, whether an existing entry was replaced). Shared with `midas adopt`, which
/// ledgers standing violations at adoption time.
pub(crate) fn upsert(raw: &str, id: &str, reason: &str) -> (String, bool) {
    let entry = format!("\"{id}\" = \"{}\"", toml_escape(reason));
    let mut lines: Vec<String> = raw.lines().map(str::to_string).collect();

    let Some(section_start) = lines.iter().position(|l| l.trim() == "[deviations]") else {
        let mut out = raw.trim_end_matches('\n').to_string();
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str("# Ledgered escape hatches: convention id → reason. `midas check` treats these as expected.\n");
        out.push_str(&format!("[deviations]\n{entry}\n"));
        return (out, false);
    };

    let section_end = lines[section_start + 1..]
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .map(|i| section_start + 1 + i)
        .unwrap_or(lines.len());

    for line in &mut lines[section_start + 1..section_end] {
        if entry_key(line).is_some_and(|k| k == id) {
            *line = entry.clone();
            return (lines.join("\n") + "\n", true);
        }
    }

    // Insert at the end of the section, before any trailing blank lines that pad the next header.
    let mut insert_at = section_end;
    while insert_at > section_start + 1 && lines[insert_at - 1].trim().is_empty() {
        insert_at -= 1;
    }
    lines.insert(insert_at, entry);
    (lines.join("\n") + "\n", false)
}

/// Remove the given ids' entry lines from the `[deviations]` section.
fn remove_entries(raw: &str, ids: &[String]) -> String {
    let mut in_section = false;
    let kept: Vec<&str> = raw
        .lines()
        .filter(|l| {
            let t = l.trim();
            if t.starts_with('[') {
                in_section = t == "[deviations]";
                return true;
            }
            if in_section {
                if let Some(k) = entry_key(l) {
                    return !ids.iter().any(|id| id == &k);
                }
            }
            true
        })
        .collect();
    kept.join("\n") + "\n"
}

/// The key of a `key = value` line, unquoted; `None` for comments/blank lines.
fn entry_key(line: &str) -> Option<String> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') {
        return None;
    }
    let (key, _) = t.split_once('=')?;
    Some(key.trim().trim_matches('"').to_string())
}

fn toml_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_creates_section_when_absent() {
        let (next, updated) = upsert("[standard]\nversion = \"0.2.0\"\n", "FE-0004", "web-only");
        assert!(!updated);
        assert!(next.contains("[deviations]\n\"FE-0004\" = \"web-only\"\n"));
        assert!(next.starts_with("[standard]"));
    }

    #[test]
    fn upsert_appends_within_existing_section() {
        let raw = "[deviations]\n\"FE-0004\" = \"web-only\"\n\n[flow]\ntrunk = \"main\"\n";
        let (next, updated) = upsert(raw, "BE-0014", "no ts client yet");
        assert!(!updated);
        let dev_idx = next.find("\"BE-0014\"").unwrap();
        let flow_idx = next.find("[flow]").unwrap();
        assert!(dev_idx < flow_idx, "entry lands inside [deviations]");
    }

    #[test]
    fn upsert_replaces_existing_entry() {
        let raw = "[deviations]\n\"FE-0004\" = \"old reason\"\n";
        let (next, updated) = upsert(raw, "FE-0004", "new reason");
        assert!(updated);
        assert!(next.contains("\"FE-0004\" = \"new reason\""));
        assert!(!next.contains("old reason"));
    }

    #[test]
    fn remove_entries_only_touches_the_section() {
        let raw = "[check]\n\"FE-0004\" = \"not a deviation\"\n[deviations]\n\"FE-0004\" = \"x\"\n\"BE-0014\" = \"y\"\n";
        let next = remove_entries(raw, &["FE-0004".into()]);
        assert!(next.contains("[check]\n\"FE-0004\" = \"not a deviation\""));
        assert!(next.contains("\"BE-0014\" = \"y\""));
        assert!(!next.contains("\"FE-0004\" = \"x\""));
    }

    #[test]
    fn escaped_reason_survives() {
        let (next, _) = upsert("", "FE-0004", "say \"hi\" \\ bye");
        assert!(next.contains(r#""FE-0004" = "say \"hi\" \\ bye""#));
    }
}
