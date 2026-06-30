//! `midas check` — the mechanical conformance gate. Deterministic only: it owns the blocking exit
//! code (2). Semantic/`review`-tier conventions are delegated to an external agent reviewer that
//! invokes this with `--json` and reads `standards/` (see `SPEC.md §8`); the binary does not run them.

use crate::checks::{Finding, Scanner};
use crate::core::exit::{CliError, CliResult};
use crate::core::Ctx;
use crate::manifest::Manifest;
use crate::registry::{CheckSpec, Convention, Escape, Registry, Tier};
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// No violations.
    Pass,
    /// Blocking violation.
    Fail,
    /// Violation, but ledgered in `[deviations]` (allowed).
    Ledgered,
    /// Violation of an advisory-escape rule (non-blocking).
    Advisory,
    /// Not run: stack n/a, no mechanical spec, or a deferred check kind.
    Skipped,
}

#[derive(Serialize)]
struct Result1 {
    id: String,
    title: String,
    tier: &'static str,
    escape: &'static str,
    outcome: Outcome,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    findings: Vec<Finding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    doc: Option<String>,
}

pub fn run(ctx: &Ctx, root_arg: Option<PathBuf>) -> CliResult {
    // Resolve the project root: --root, else the git toplevel, else cwd.
    let root = match root_arg {
        Some(r) => r,
        None => crate::proc::capture("git", &["rev-parse", "--show-toplevel"])
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))),
    };
    if !root.is_dir() {
        return Err(CliError::usage(format!(
            "root {} is not a directory",
            root.display()
        )));
    }

    let (manifest, has_manifest) = match Manifest::find(&root)? {
        Some((m, _)) => (m, true),
        None => {
            ctx.out.warn(
                "no midas.toml found — using midian defaults (backend=rust, frontend=svelte)",
            );
            (Manifest::default(), false)
        }
    };

    let registry = Registry::embedded().map_err(CliError::tool)?;

    // Drift direction #1: the manifest pins a different standard version than this binary embeds.
    let pinned = manifest.standard.version.clone();
    if !pinned.is_empty() && pinned != registry.version {
        ctx.out.warn(format!(
            "midas.toml pins {pinned} but this binary embeds {} — `midas upgrade` (or re-pin)",
            registry.version
        ));
    }

    let mut scanner = Scanner::new(&root).map_err(CliError::tool)?;
    ctx.out.step(format!(
        "scanning {} ({} files)",
        root.display(),
        scanner.file_count()
    ));

    let mut results: Vec<Result1> = Vec::new();
    let mut review_count = 0usize;

    for conv in &registry.conventions {
        if conv.tier == Tier::Review {
            review_count += 1;
            continue;
        }
        results.push(evaluate(conv, &manifest, has_manifest, &mut scanner));
    }

    let failed = results
        .iter()
        .filter(|r| r.outcome == Outcome::Fail)
        .count();
    let passed = results
        .iter()
        .filter(|r| r.outcome == Outcome::Pass)
        .count();
    let ledgered = results
        .iter()
        .filter(|r| r.outcome == Outcome::Ledgered)
        .count();
    let advisory = results
        .iter()
        .filter(|r| r.outcome == Outcome::Advisory)
        .count();
    let skipped = results
        .iter()
        .filter(|r| r.outcome == Outcome::Skipped)
        .count();

    let payload = json!({
        "version": registry.version,
        "root": root.display().to_string(),
        "mechanical": {
            "checked": results.len(),
            "passed": passed, "failed": failed, "ledgered": ledgered,
            "advisory": advisory, "skipped": skipped,
            "results": &results,
        },
        "semantic": {
            "delegated": review_count,
            "semantic_strict": manifest.check.semantic_strict,
            "note": "review-tier conventions are delegated to an external agent reviewer (midas check is mechanical-only)"
        }
    });

    let verbose = ctx.global.verbose > 0;
    ctx.out.data(&payload, |s| {
        let mut o = String::new();
        o.push_str(&s.bold("MECHANICAL"));
        o.push_str(&s.dim("  (deterministic · gates CI)\n"));
        for r in &results {
            match r.outcome {
                Outcome::Pass if !verbose => continue,
                Outcome::Skipped if !verbose => continue,
                _ => {}
            }
            let marker = match r.outcome {
                Outcome::Pass => s.green("✓"),
                Outcome::Fail => s.red("✗"),
                Outcome::Ledgered => s.yellow("⚑"),
                Outcome::Advisory => s.yellow("⚠"),
                Outcome::Skipped => s.dim("·"),
            };
            o.push_str(&format!("  {marker} {}  {}\n", s.dim(&r.id), r.title));
            if let Some(note) = &r.note {
                o.push_str(&format!("      {}\n", s.dim(note)));
            }
            if matches!(r.outcome, Outcome::Fail | Outcome::Ledgered | Outcome::Advisory) {
                if let Some(doc) = &r.doc {
                    o.push_str(&format!("      {}\n", s.dim(&format!("standards/{doc}"))));
                }
            }
            for f in r.findings.iter().take(8) {
                let loc = if f.line > 0 {
                    format!("{}:{}", f.file, f.line)
                } else {
                    f.file.clone()
                };
                o.push_str(&format!("      {}  {}\n", s.dim(&loc), f.text));
            }
            if r.findings.len() > 8 {
                o.push_str(&format!("      {}\n", s.dim(&format!("… +{} more", r.findings.len() - 8))));
            }
        }
        o.push_str(&format!(
            "\n  {} passed · {} failed · {} ledgered · {} advisory · {} skipped\n",
            s.green(&passed.to_string()),
            if failed > 0 { s.red(&failed.to_string()) } else { failed.to_string() },
            ledgered,
            advisory,
            skipped,
        ));
        o.push('\n');
        o.push_str(&s.bold("SEMANTIC"));
        o.push_str(&s.dim(&format!(
            "  (delegated — not run here)\n  {review_count} review-tier conventions — run your agent reviewer with standards/ as context\n"
        )));
        o
    });

    if failed > 0 {
        Err(CliError::expected(format!(
            "{failed} mechanical violation(s)"
        )))
    } else {
        Ok(())
    }
}

/// The evaluated state of one convention against a tree: the classified [`Outcome`] plus the
/// findings and an optional human note. Shared by `check` (which gates) and `drift` (which diffs
/// two registry versions), so both classify a convention identically — no checker/differ skew.
pub struct Eval {
    pub outcome: Outcome,
    pub findings: Vec<Finding>,
    pub note: Option<String>,
}

/// Classify a single convention against the tree, mirroring `check`'s logic: applicability →
/// mechanical spec → findings → escape/ledger classification. `drift` calls this once per registry
/// version to compute the before/after outcomes for the same working tree + ledger.
pub fn outcome_of(
    conv: &Convention,
    manifest: &Manifest,
    has_manifest: bool,
    scanner: &mut Scanner,
) -> Eval {
    let eval = |outcome: Outcome, findings: Vec<Finding>, note: Option<String>| Eval {
        outcome,
        findings,
        note,
    };

    if !applicable(conv, manifest, has_manifest) {
        return eval(
            Outcome::Skipped,
            vec![],
            Some("stack not applicable".into()),
        );
    }

    let spec = match &conv.check {
        Some(s) => s,
        None => {
            return eval(
                Outcome::Skipped,
                vec![],
                Some("no mechanical check defined".into()),
            )
        }
    };

    let (findings, mut note) = match spec {
        CheckSpec::BannedCall {
            pattern,
            allow_in,
            globs,
            message,
        } => match scanner.banned_call(pattern, allow_in, globs) {
            Ok((f, truncated)) => {
                let mut n = message.clone();
                if truncated {
                    n = Some(format!("{} (truncated)", n.unwrap_or_default()));
                }
                (f, n)
            }
            Err(e) => return eval(Outcome::Skipped, vec![], Some(format!("check error: {e}"))),
        },
        CheckSpec::FileStructure {
            must_exist,
            must_not_exist,
        } => (scanner.file_structure(must_exist, must_not_exist), None),
        CheckSpec::ArtifactHash { .. } => {
            return eval(
                Outcome::Skipped,
                vec![],
                Some("artifact-hash check deferred".into()),
            )
        }
        CheckSpec::ProvenanceDrift {} => {
            return eval(
                Outcome::Skipped,
                vec![],
                Some("provenance-drift check deferred".into()),
            )
        }
        CheckSpec::Clippy { .. } => {
            return eval(
                Outcome::Skipped,
                vec![],
                Some("clippy passthrough deferred (CI runs clippy directly)".into()),
            )
        }
    };

    if findings.is_empty() {
        return eval(Outcome::Pass, vec![], None);
    }

    // Violations present — classify by escape policy + deviation ledger.
    let deviated = manifest.deviations.contains_key(&conv.id);
    match conv.escape {
        Escape::Advisory => eval(Outcome::Advisory, findings, note.take()),
        Escape::Ledgered if deviated => {
            let reason = manifest
                .deviations
                .get(&conv.id)
                .cloned()
                .unwrap_or_default();
            eval(
                Outcome::Ledgered,
                findings,
                Some(format!("ledgered: {reason}")),
            )
        }
        Escape::Hard if deviated => eval(
            Outcome::Fail,
            findings,
            Some("deviation ignored — this rule is `hard` (no escape allowed)".into()),
        ),
        _ => eval(Outcome::Fail, findings, note.take()),
    }
}

fn evaluate(
    conv: &Convention,
    manifest: &Manifest,
    has_manifest: bool,
    scanner: &mut Scanner,
) -> Result1 {
    let e = outcome_of(conv, manifest, has_manifest, scanner);
    Result1 {
        id: conv.id.clone(),
        title: conv.title.clone(),
        tier: "check",
        escape: escape_str(conv.escape),
        outcome: e.outcome,
        findings: e.findings,
        note: e.note,
        doc: conv.doc.clone(),
    }
}

/// A convention applies unless it pins a `stack` that differs from the project's current stack for
/// its layer. Stack-agnostic layers (cli/process/agent/stack) always apply.
///
/// A layer the project doesn't declare in `[stack]` is **not applicable** — a CLI/library repo
/// doesn't get the frontend/backend *app* conventions. The midian defaults (backend=rust,
/// frontend=svelte) are only assumed when there's no manifest at all (the `check --root midian`
/// convenience before midian adopts a `midas.toml`).
fn applicable(conv: &Convention, manifest: &Manifest, has_manifest: bool) -> bool {
    let Some(want) = &conv.stack else {
        return true;
    };
    let layer = conv.layer.as_str();
    if !matches!(layer, "backend" | "frontend") {
        return true;
    }
    let current = match current_stack(manifest, layer) {
        Some(s) => s,
        None if has_manifest => return false, // project declares no such layer → n/a
        None => match layer {
            "backend" => "rust".into(),
            "frontend" => "svelte".into(),
            _ => return true,
        },
    };
    &current == want
}

fn current_stack(manifest: &Manifest, layer: &str) -> Option<String> {
    manifest.stack.get(layer).and_then(|l| l.current.clone())
}

fn escape_str(e: Escape) -> &'static str {
    match e {
        Escape::Hard => "hard",
        Escape::Ledgered => "ledgered",
        Escape::Advisory => "advisory",
    }
}

// Allow `--root` to accept a string path from clap.
pub fn parse_root(s: &str) -> std::result::Result<PathBuf, String> {
    let p = Path::new(s);
    if p.exists() {
        Ok(p.to_path_buf())
    } else {
        Err(format!("path does not exist: {s}"))
    }
}
