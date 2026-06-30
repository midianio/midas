//! `midas drift` — the standard-drift briefing. A **read-only**, model-facing report (never a gate,
//! never mutates) that answers "what does moving the standard mean for *this* repo?". It fuses two
//! views: (A) standard-vs-standard — what changed in the conventions between two embedded versions —
//! and (B) project-vs-standard — this repo's standing drift from the version it's on. The headline
//! is a **deep outcome diff**: it runs the same classifier `check` uses against the current tree
//! under each version's rules and reports the *transitions*, so the output is an upgrade worklist
//! ("BE-0010 went advisory→hard, you have 3 findings, must fix") rather than a registry changelog.
//!
//! Holds the repo + `[deviations]` ledger constant and varies only the registry version, so every
//! transition is attributable purely to the standard moving. Always exits 0 — the gate stays
//! `check`. See `SPEC.md §7`.

use crate::checks::{Finding, Scanner};
use crate::cmd::check::{outcome_of, Outcome};
use crate::core::exit::{CliError, CliResult};
use crate::core::Ctx;
use crate::manifest::Manifest;
use crate::registry::{semver_key, Convention, Escape, Registry, Tier};
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::path::PathBuf;

/// An outcome over `check`'s set plus `absent` — the convention didn't exist at that version.
#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum State {
    Pass,
    Fail,
    Ledgered,
    Advisory,
    Skipped,
    Absent,
}

impl State {
    fn of(o: Outcome) -> Self {
        match o {
            Outcome::Pass => State::Pass,
            Outcome::Fail => State::Fail,
            Outcome::Ledgered => State::Ledgered,
            Outcome::Advisory => State::Advisory,
            Outcome::Skipped => State::Skipped,
        }
    }
    fn label(self) -> &'static str {
        match self {
            State::Pass => "pass",
            State::Fail => "fail",
            State::Ledgered => "ledgered",
            State::Advisory => "advisory",
            State::Skipped => "skipped",
            State::Absent => "absent",
        }
    }
    fn real(self) -> bool {
        !matches!(self, State::Skipped | State::Absent)
    }
}

/// The severity ladder a model reads top-down.
#[derive(Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Class {
    /// Newly fails and can't be ledgered (`hard`) — must fix to upgrade.
    Blocking,
    /// Newly fails but is ledgerable, or a new convention you violate — fix or ledger.
    ActionNeeded,
    /// A `[deviations]` entry that's now dead or unnecessary — pure cleanup.
    LedgerCleanup,
    /// Metadata/text change with no actionable outcome shift for this repo.
    Informational,
}

impl Class {
    fn rank(self) -> u8 {
        match self {
            Class::Blocking => 0,
            Class::ActionNeeded => 1,
            Class::LedgerCleanup => 2,
            Class::Informational => 3,
        }
    }
}

/// The machine-readable directive attached to a transition or standing item.
#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
enum Action {
    FixRequired,
    FixOrLedger,
    RemoveDeadDeviation,
    DeleteUnneededDeviation,
    Review,
    None,
}

/// The standard-side "why" behind a transition.
#[derive(Serialize)]
struct Change {
    /// `added` | `removed` | `modified` | `unchanged`.
    kind: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    details: Vec<String>,
}

/// One convention's before→after under a version move.
#[derive(Serialize)]
struct Transition {
    id: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    doc: Option<String>,
    layer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stack: Option<String>,
    /// Whether the convention is relevant to this repo's stack (false → informational only).
    applies: bool,
    old_outcome: State,
    new_outcome: State,
    change: Change,
    class: Class,
    action: Action,
    rationale: String,
    /// The actionable `file:line` worklist at the target version (only for things you must act on).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    findings: Vec<Finding>,
}

/// (B) project-vs-standard drift at the target version, independent of any version move.
#[derive(Serialize)]
struct Standing {
    /// Convention id, or the stack layer name for `stack_target_drift`.
    id: String,
    title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    doc: Option<String>,
    /// `stale_deviation` | `advisory_finding` | `stack_target_drift`.
    kind: &'static str,
    action: Action,
    rationale: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    findings: Vec<Finding>,
}

#[derive(Serialize)]
struct Summary {
    blocking: usize,
    action_needed: usize,
    ledger_cleanup: usize,
    informational: usize,
    standing: usize,
}

#[derive(Serialize)]
struct Report {
    from_version: String,
    to_version: String,
    /// `upgrade` | `downgrade` | `same`.
    direction: &'static str,
    summary: Summary,
    transitions: Vec<Transition>,
    standing: Vec<Standing>,
}

pub fn run(
    ctx: &Ctx,
    spec: Option<String>,
    from_file: Option<PathBuf>,
    to_file: Option<PathBuf>,
    root_arg: Option<PathBuf>,
) -> CliResult {
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
        None => (Manifest::default(), false),
    };

    let live = Registry::embedded().map_err(CliError::tool)?;

    // Resolve the (from, to) versions. Default: pinned → embedded — the "Drift direction #1" case
    // `check` already warns about. With no pin we fall back to the live version (so from == to and
    // `drift` degrades into the (B) standing-drift pass — never a no-op).
    let pinned = manifest.standard.version.clone();
    let default_from = if pinned.is_empty() {
        live.version.clone()
    } else {
        pinned
    };
    let default_to = live.version.clone();
    let (from_req, to_req) = match spec.as_deref() {
        None => (default_from, default_to),
        Some(s) if s.contains("..") => {
            let (a, b) = s.split_once("..").unwrap();
            let a = a.trim();
            let b = b.trim();
            (
                if a.is_empty() {
                    default_from
                } else {
                    a.to_string()
                },
                if b.is_empty() {
                    default_to
                } else {
                    b.to_string()
                },
            )
        }
        Some(s) => (default_from, s.trim().to_string()),
    };

    let (from_version, from_reg) = resolve(&from_req, from_file)?;
    let (to_version, to_reg) = resolve(&to_req, to_file)?;

    let direction = match semver_key(&from_version).cmp(&semver_key(&to_version)) {
        Ordering::Less => "upgrade",
        Ordering::Greater => "downgrade",
        Ordering::Equal => "same",
    };

    let verbose = ctx.global.verbose > 0;
    let mut scanner = Scanner::new(&root).map_err(CliError::tool)?;
    ctx.out.step(format!(
        "diffing standard {from_version} → {to_version} against {} ({} files)",
        root.display(),
        scanner.file_count()
    ));

    // ---- (A) version-driven transitions ----
    // Target order first (so the report reads in convention order), then any from-only ids (removed).
    let mut ids: Vec<String> = to_reg.conventions.iter().map(|c| c.id.clone()).collect();
    for c in &from_reg.conventions {
        if to_reg.by_id(&c.id).is_none() {
            ids.push(c.id.clone());
        }
    }

    let mut transitions: Vec<Transition> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    for id in &ids {
        let cf = from_reg.by_id(id);
        let ct = to_reg.by_id(id);

        let old_state = match cf {
            Some(c) => State::of(outcome_of(c, &manifest, has_manifest, &mut scanner).outcome),
            None => State::Absent,
        };
        let (new_state, new_findings) = match ct {
            Some(c) => {
                let e = outcome_of(c, &manifest, has_manifest, &mut scanner);
                (State::of(e.outcome), e.findings)
            }
            None => (State::Absent, vec![]),
        };

        let details = match (cf, ct) {
            (Some(a), Some(b)) => describe_changes(a, b),
            _ => vec![],
        };
        let change_kind = match (cf.is_some(), ct.is_some()) {
            (false, true) => "added",
            (true, false) => "removed",
            _ if details.is_empty() => "unchanged",
            _ => "modified",
        };

        let deviated = manifest.deviations.contains_key(id);
        let to_escape = ct.map(|c| c.escape);
        let Some(decision) = classify(
            &to_version,
            cf.is_some(),
            ct.is_some(),
            old_state,
            new_state,
            deviated,
            to_escape,
            &details,
        ) else {
            continue; // nothing changed for this convention
        };
        if decision.class == Class::Informational && !verbose {
            continue;
        }

        let desc = ct.or(cf).unwrap();
        let findings = if matches!(decision.class, Class::Blocking | Class::ActionNeeded) {
            new_findings
        } else {
            vec![]
        };
        seen.insert(id.clone());
        transitions.push(Transition {
            id: id.clone(),
            title: desc.title.clone(),
            doc: desc.doc.clone(),
            layer: desc.layer.clone(),
            stack: desc.stack.clone(),
            applies: old_state.real() || new_state.real(),
            old_outcome: old_state,
            new_outcome: new_state,
            change: Change {
                kind: change_kind,
                details,
            },
            class: decision.class,
            action: decision.action,
            rationale: decision.rationale,
            findings,
        });
    }

    transitions.sort_by(|a, b| a.class.rank().cmp(&b.class.rank()).then(a.id.cmp(&b.id)));

    // ---- (B) standing drift at the target version (deduped against version transitions) ----
    let mut standing: Vec<Standing> = Vec::new();
    for c in &to_reg.conventions {
        if seen.contains(&c.id) {
            continue;
        }
        let e = outcome_of(c, &manifest, has_manifest, &mut scanner);
        let deviated = manifest.deviations.contains_key(&c.id);
        if deviated && e.outcome == Outcome::Pass {
            standing.push(Standing {
                id: c.id.clone(),
                title: c.title.clone(),
                doc: c.doc.clone(),
                kind: "stale_deviation",
                action: Action::DeleteUnneededDeviation,
                rationale: "you ledger a deviation for a convention that currently passes — remove the dead entry".into(),
                findings: vec![],
            });
        } else if e.outcome == Outcome::Advisory {
            standing.push(Standing {
                id: c.id.clone(),
                title: c.title.clone(),
                doc: c.doc.clone(),
                kind: "advisory_finding",
                action: Action::Review,
                rationale: "advisory findings — never block, but real drift from the standard"
                    .into(),
                findings: e.findings,
            });
        }
    }
    // Stack-target drift is informational → verbose only.
    if verbose {
        for (layer, l) in &manifest.stack {
            if let Some(target) = &l.target {
                if l.current.as_ref() != Some(target) {
                    standing.push(Standing {
                        id: layer.clone(),
                        title: format!(
                            "stack {layer}: {} → {target}",
                            l.current.as_deref().unwrap_or("?")
                        ),
                        doc: None,
                        kind: "stack_target_drift",
                        action: Action::Review,
                        rationale: "a stack layer declares a migration target it hasn't reached"
                            .into(),
                        findings: vec![],
                    });
                }
            }
        }
    }

    let summary = Summary {
        blocking: count(&transitions, Class::Blocking),
        action_needed: count(&transitions, Class::ActionNeeded),
        ledger_cleanup: count(&transitions, Class::LedgerCleanup),
        informational: count(&transitions, Class::Informational),
        standing: standing.len(),
    };

    let report = Report {
        from_version: from_version.clone(),
        to_version: to_version.clone(),
        direction,
        summary,
        transitions,
        standing,
    };

    ctx.out.data(&report, |s| render(s, &report, verbose));
    Ok(())
}

struct Decision {
    class: Class,
    action: Action,
    rationale: String,
}

/// Classify one convention's before→after into the severity ladder + a directive. `None` means
/// "not a transition" (present at both versions, identical outcome, no metadata change) → skipped.
#[allow(clippy::too_many_arguments)]
fn classify(
    to_ver: &str,
    has_from: bool,
    has_to: bool,
    old: State,
    new: State,
    deviated: bool,
    to_escape: Option<Escape>,
    details: &[String],
) -> Option<Decision> {
    let detail = || {
        if details.is_empty() {
            String::new()
        } else {
            format!(" ({})", details.join("; "))
        }
    };
    let some = |class, action, rationale: String| {
        Some(Decision {
            class,
            action,
            rationale,
        })
    };

    // Removed at the target.
    if !has_to {
        return if deviated {
            some(
                Class::LedgerCleanup,
                Action::RemoveDeadDeviation,
                format!("removed at {to_ver}; its [deviations] entry is now dead — remove it"),
            )
        } else {
            some(
                Class::Informational,
                Action::None,
                format!("removed at {to_ver}"),
            )
        };
    }

    // Added at the target.
    if !has_from {
        return match new {
            State::Fail if to_escape == Some(Escape::Hard) => some(
                Class::Blocking,
                Action::FixRequired,
                format!("new at {to_ver} and you violate it; `hard` — must fix before upgrading"),
            ),
            State::Fail => some(
                Class::ActionNeeded,
                Action::FixOrLedger,
                format!("new at {to_ver} and you violate it; fix or ledger a deviation"),
            ),
            State::Advisory => some(
                Class::Informational,
                Action::Review,
                format!("new advisory convention at {to_ver}; you have findings (never blocks)"),
            ),
            _ => some(
                Class::Informational,
                Action::None,
                format!("new at {to_ver}; you already conform"),
            ),
        };
    }

    // Present at both versions — classify by the outcome shift, then by metadata.
    if new == State::Fail && old != State::Fail {
        return if to_escape == Some(Escape::Hard) {
            some(
                Class::Blocking,
                Action::FixRequired,
                format!("now fails at {to_ver} and is `hard`{} — must fix", detail()),
            )
        } else {
            some(
                Class::ActionNeeded,
                Action::FixOrLedger,
                format!(
                    "now fails at {to_ver}{} — fix or ledger a deviation",
                    detail()
                ),
            )
        };
    }
    if old == State::Ledgered && new == State::Pass {
        return some(
            Class::LedgerCleanup,
            Action::DeleteUnneededDeviation,
            format!("check loosened at {to_ver}; your deviation is no longer needed — remove it"),
        );
    }
    if old != new {
        let action = if new == State::Advisory {
            Action::Review
        } else {
            Action::None
        };
        return some(
            Class::Informational,
            action,
            format!(
                "outcome shifts {}→{} at {to_ver}{}",
                old.label(),
                new.label(),
                detail()
            ),
        );
    }
    if !details.is_empty() {
        return some(
            Class::Informational,
            Action::Review,
            format!("standard text changed{}", detail()),
        );
    }
    None
}

/// The field-level diff between the same convention at two versions (drives `change.details`).
fn describe_changes(a: &Convention, b: &Convention) -> Vec<String> {
    let mut d = Vec::new();
    if a.tier != b.tier {
        d.push(format!("tier: {} → {}", tier_str(a.tier), tier_str(b.tier)));
    }
    if a.escape != b.escape {
        d.push(format!(
            "escape: {} → {}",
            escape_str(a.escape),
            escape_str(b.escape)
        ));
    }
    if a.status != b.status {
        d.push(format!(
            "status: {} → {}",
            a.status.as_deref().unwrap_or("—"),
            b.status.as_deref().unwrap_or("—")
        ));
    }
    if format!("{:?}", a.check) != format!("{:?}", b.check) {
        d.push("detection rule changed".into());
    }
    if a.doc != b.doc {
        d.push("doc reference changed".into());
    }
    if a.title != b.title {
        d.push("title reworded".into());
    }
    d
}

/// Resolve a requested version to its registry, or a `--from-file/--to-file` override. An unknown
/// version is a usage error listing what this binary embeds — never a silent nearest-match.
fn resolve(version: &str, file: Option<PathBuf>) -> Result<(String, Registry), CliError> {
    if let Some(path) = file {
        let reg = Registry::from_file(&path).map_err(CliError::tool)?;
        let v = reg.version.clone();
        return Ok((v, reg));
    }
    match Registry::at_version(version).map_err(CliError::tool)? {
        Some(reg) => Ok((version.to_string(), reg)),
        None => Err(CliError::usage(format!(
            "standard {version} not embedded in this binary; available: {}",
            Registry::available_versions().join(", ")
        ))),
    }
}

fn count(t: &[Transition], class: Class) -> usize {
    t.iter().filter(|x| x.class == class).count()
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

fn render(s: &crate::core::style::Style, r: &Report, verbose: bool) -> String {
    let mut o = String::new();
    o.push_str(&s.bold("DRIFT"));
    o.push_str(&s.dim(&format!(
        "  {} → {} ({})\n",
        r.from_version, r.to_version, r.direction
    )));
    o.push_str(&format!(
        "  {} blocking · {} action needed · {} ledger cleanup · {} informational · {} standing\n",
        if r.summary.blocking > 0 {
            s.red(&r.summary.blocking.to_string())
        } else {
            r.summary.blocking.to_string()
        },
        if r.summary.action_needed > 0 {
            s.yellow(&r.summary.action_needed.to_string())
        } else {
            r.summary.action_needed.to_string()
        },
        r.summary.ledger_cleanup,
        r.summary.informational,
        r.summary.standing,
    ));

    if r.transitions.is_empty() && r.standing.is_empty() {
        o.push_str(&s.dim("\n  no drift — this repo is on-standard.\n"));
        return o;
    }

    if !r.transitions.is_empty() {
        o.push('\n');
        for t in &r.transitions {
            let marker = match t.class {
                Class::Blocking => s.red("✗"),
                Class::ActionNeeded => s.yellow("⚠"),
                Class::LedgerCleanup => s.cyan("⚑"),
                Class::Informational => s.dim("·"),
            };
            o.push_str(&format!(
                "  {marker} {}  {}\n",
                s.dim(&t.id),
                trunc(&t.title, 88)
            ));
            o.push_str(&format!(
                "      {} {}\n",
                s.dim(&format!(
                    "{}→{}",
                    t.old_outcome.label(),
                    t.new_outcome.label()
                )),
                s.dim(&t.rationale)
            ));
            if let Some(doc) = &t.doc {
                o.push_str(&format!("      {}\n", s.dim(&format!("standards/{doc}"))));
            }
            for f in t.findings.iter().take(6) {
                let loc = if f.line > 0 {
                    format!("{}:{}", f.file, f.line)
                } else {
                    f.file.clone()
                };
                o.push_str(&format!("      {}  {}\n", s.dim(&loc), trunc(&f.text, 88)));
            }
            if t.findings.len() > 6 {
                o.push_str(&format!(
                    "      {}\n",
                    s.dim(&format!("… +{} more", t.findings.len() - 6))
                ));
            }
        }
    }

    if !r.standing.is_empty() {
        o.push('\n');
        o.push_str(&s.bold("STANDING"));
        o.push_str(&s.dim("  (drift from the version you're on)\n"));
        for st in &r.standing {
            o.push_str(&format!(
                "  {} {}  {}\n",
                s.dim(&format!("[{}]", st.kind)),
                s.dim(&st.id),
                s.dim(&st.rationale)
            ));
        }
    }

    if !verbose {
        o.push_str(&s.dim("\n  (informational changes & stack-target drift hidden — pass -v)\n"));
    }
    o
}

fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n - 1).collect::<String>())
    }
}
