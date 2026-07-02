//! `midas sync` — materialize the version-stamped managed block into each repo's agent docs
//! (`CLAUDE.md`, `AGENTS.md`). Only the delimited region is touched; project content is untouched.
//! `--check` reports drift (missing/stale block) without writing — exit 2 on drift.

use crate::core::exit::{CliError, CliResult};
use crate::core::Ctx;
use crate::registry::Registry;
use serde::Serialize;
use serde_json::json;

pub(crate) const TARGETS: &[&str] = &["CLAUDE.md", "AGENTS.md"];
const BEGIN_PREFIX: &str = "<!-- midas:";
const END: &str = "<!-- /midas -->";

#[derive(Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BlockStatus {
    Missing,
    Stale,
    Current,
}

#[derive(Serialize)]
struct Target {
    name: String,
    status: BlockStatus,
}

/// Write the managed block into every agent doc under `root` (no output). Returns
/// `(version, changed targets)` — the quiet core `run` and `midas adopt` share.
pub(crate) fn write_blocks(root: &std::path::Path) -> Result<(String, Vec<String>), CliError> {
    let version = Registry::embedded()
        .map(|r| r.version)
        .unwrap_or_else(|_| "0.0.0".into());
    let block = managed_block(&version);
    let mut changed: Vec<String> = Vec::new();
    for name in TARGETS {
        let path = root.join(name);
        let existing = std::fs::read_to_string(&path).ok();
        if let Some(next) = next_content(existing.as_deref(), &block) {
            std::fs::write(&path, next)?;
            changed.push(name.to_string());
        }
    }
    Ok((version, changed))
}

pub fn run(ctx: &Ctx, check_only: bool) -> CliResult {
    let root = crate::manifest::resolve_root(&ctx.global)?;
    let version = Registry::embedded()
        .map(|r| r.version)
        .unwrap_or_else(|_| "0.0.0".into());
    let block = managed_block(&version);

    let mut targets: Vec<Target> = Vec::new();
    let mut changed: Vec<String> = Vec::new();

    for name in TARGETS {
        let path = root.join(name);
        let existing = std::fs::read_to_string(&path).ok();
        let status = status_of(existing.as_deref(), &version);

        if check_only {
            targets.push(Target {
                name: name.to_string(),
                status,
            });
            continue;
        }

        if let Some(next) = next_content(existing.as_deref(), &block) {
            std::fs::write(&path, next)?;
            changed.push(name.to_string());
        }
        targets.push(Target {
            name: name.to_string(),
            status: BlockStatus::Current,
        });
    }

    if check_only {
        let drift: Vec<&Target> = targets
            .iter()
            .filter(|t| t.status != BlockStatus::Current)
            .collect();
        let drifted = !drift.is_empty();
        ctx.out.data(
            &json!({ "version": version, "targets": &targets, "drift": drifted }),
            |s| {
                let mut o = String::new();
                for t in &targets {
                    let m = match t.status {
                        BlockStatus::Current => s.green("✓"),
                        BlockStatus::Stale => s.yellow("⚑"),
                        BlockStatus::Missing => s.red("✗"),
                    };
                    o.push_str(&format!("  {m} {}\n", t.name));
                }
                o
            },
        );
        if drifted {
            return Err(CliError::expected(format!(
                "{} doc(s) missing/stale midas block — run `midas sync`",
                drift.len()
            )));
        }
        return Ok(());
    }

    if changed.is_empty() {
        ctx.out
            .success(format!("midas block already current ({version})"));
    } else {
        ctx.out
            .success(format!("synced {} ({})", changed.join(", "), version));
    }
    ctx.out
        .data(&json!({ "version": version, "changed": changed }), |_| {
            if changed.is_empty() {
                "already current".into()
            } else {
                changed.join(", ")
            }
        });
    Ok(())
}

pub(crate) fn status_of(existing: Option<&str>, version: &str) -> BlockStatus {
    let Some(text) = existing else {
        return BlockStatus::Missing;
    };
    match find_block(text) {
        None => BlockStatus::Missing,
        Some((start, _)) => {
            let header = &text[start..];
            let want = format!("{BEGIN_PREFIX}{version} -->");
            if header.starts_with(&want) {
                BlockStatus::Current
            } else {
                BlockStatus::Stale
            }
        }
    }
}

/// Return the new file content if a write is needed, else `None` (already identical).
fn next_content(existing: Option<&str>, block: &str) -> Option<String> {
    let next = match existing {
        None => format!("{block}\n"),
        Some(text) => match find_block(text) {
            Some((start, end)) => format!("{}{}{}", &text[..start], block, &text[end..]),
            None => {
                let trimmed = text.trim_end_matches(['\n', ' ', '\t', '\r']);
                if trimmed.is_empty() {
                    format!("{block}\n")
                } else {
                    format!("{trimmed}\n\n{block}\n")
                }
            }
        },
    };
    if existing == Some(next.as_str()) {
        None
    } else {
        Some(next)
    }
}

/// Byte span of the managed block (begin marker .. end marker inclusive).
fn find_block(text: &str) -> Option<(usize, usize)> {
    let start = text.find(BEGIN_PREFIX)?;
    let end_marker = text[start..].find(END)? + start;
    Some((start, end_marker + END.len()))
}

pub(crate) fn managed_block(version: &str) -> String {
    format!(
        "{BEGIN_PREFIX}{version} -->\n\
<!-- Generated by `midas sync` — do not edit inside this block. -->\n\
\n\
## Engineering standard (midas {version})\n\
\n\
This project follows the midian engineering standard. Conventions are IDed (`BE-0010`, `FE-0001`, …)\n\
and live in the `midas` repo under `standards/`. Before opening a PR:\n\
\n\
- **Gate:** `midas check` must be clean — or each failure ledgered in `midas.toml [deviations]`.\n\
- **Scaffold** a conformant project or piece with `midas touch`, never hand-rolled.\n\
- **Use the seams** the conventions name; don't reach around them.\n\
- On conflict between a stale local doc and the pinned standard, the standard wins.\n\
\n\
Review agents: run `midas check --json` and read `standards/` for the full set (review-tier\n\
conventions are semantic and not machine-checked).\n\
{END}"
    )
}
