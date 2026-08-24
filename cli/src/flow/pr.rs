//! PR retargeting and open-PR overlap — pure decisions, tested without `gh`.

use serde::Serialize;

/// An open PR we might retarget or compare files against.
#[derive(Debug, Clone)]
pub struct PrInfo {
    pub number: u64,
    pub url: String,
    pub base: String,
    pub head: String,
    pub draft: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetargetDecision {
    /// Already targeting the intended base.
    Same,
    /// Targeting a configured promotion base — leave it alone.
    Promotion { current: String },
    /// Wrong base; retarget to `to`.
    Retarget { from: String, to: String },
}

/// Decide whether an existing PR's base should move.
///
/// A PR already targeting a `[flow].promotion_bases` entry is an explicit promotion and
/// is not silently moved back to trunk. `--base` / `--promote` change `intended` so a
/// promotion ship keeps that base.
pub fn retarget_decision(
    current: &str,
    intended: &str,
    promotion_bases: &[String],
) -> RetargetDecision {
    if current == intended {
        return RetargetDecision::Same;
    }
    if promotion_bases.iter().any(|b| b == current) {
        return RetargetDecision::Promotion {
            current: current.to_string(),
        };
    }
    RetargetDecision::Retarget {
        from: current.to_string(),
        to: intended.to_string(),
    }
}

/// Intended PR base: `--base` wins, else `--promote` picks the first promotion base,
/// else the configured trunk.
pub fn intended_base(
    trunk: &str,
    promotion_bases: &[String],
    base_flag: Option<&str>,
    promote: bool,
) -> Result<String, String> {
    if let Some(b) = base_flag {
        if b == trunk || promotion_bases.iter().any(|p| p == b) {
            return Ok(b.to_string());
        }
        return Err(format!(
            "--base {b:?} is not the trunk ({trunk}) or a [flow].promotion_bases entry"
        ));
    }
    if promote {
        return promotion_bases.first().cloned().ok_or_else(|| {
            "--promote requires [flow].promotion_bases (e.g. promotion_bases = [\"main\"])"
                .to_string()
        });
    }
    Ok(trunk.to_string())
}

/// One overlapping open PR, for text and `--json`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Overlap {
    pub number: u64,
    pub url: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exact: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub nearby: Vec<String>,
}

/// Exact path matches vs same-parent-directory proximity. Does not claim a semantic conflict.
pub fn classify_overlap(ours: &[String], theirs: &[String]) -> (Vec<String>, Vec<String>) {
    let our_set: std::collections::HashSet<&str> = ours.iter().map(String::as_str).collect();
    let mut exact: Vec<String> = theirs
        .iter()
        .filter(|p| our_set.contains(p.as_str()))
        .cloned()
        .collect();
    let our_dirs: std::collections::HashSet<&str> =
        ours.iter().filter_map(|p| parent_dir(p)).collect();
    let mut nearby: Vec<String> = theirs
        .iter()
        .filter(|p| !our_set.contains(p.as_str()))
        .filter(|p| parent_dir(p).is_some_and(|d| our_dirs.contains(d)))
        .cloned()
        .collect();
    sort_overlap_paths(&mut exact);
    sort_overlap_paths(&mut nearby);
    (exact, nearby)
}

fn parent_dir(path: &str) -> Option<&str> {
    path.rsplit_once('/')
        .map(|(d, _)| d)
        .filter(|d| !d.is_empty())
}

/// Canon refs and schema migrations first — those were the Midian hotspots.
fn sort_overlap_paths(paths: &mut [String]) {
    paths.sort_by(|a, b| overlap_rank(a).cmp(&overlap_rank(b)).then(a.cmp(b)));
}

fn overlap_rank(path: &str) -> u8 {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.starts_with("ref.") && name.ends_with(".md") {
        0
    } else if path.contains("db/migrations/") || path.contains("migrations/") {
        1
    } else {
        2
    }
}

pub fn should_warn_overlap(exact: &[String], nearby: &[String]) -> bool {
    !exact.is_empty() || !nearby.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_base_is_a_no_op() {
        assert_eq!(
            retarget_decision("dev", "dev", &["main".into()]),
            RetargetDecision::Same
        );
    }

    #[test]
    fn promotion_base_is_preserved() {
        assert_eq!(
            retarget_decision("main", "dev", &["main".into()]),
            RetargetDecision::Promotion {
                current: "main".into()
            }
        );
    }

    #[test]
    fn wrong_base_retargets_to_trunk() {
        assert_eq!(
            retarget_decision("main", "dev", &[]),
            RetargetDecision::Retarget {
                from: "main".into(),
                to: "dev".into()
            }
        );
    }

    #[test]
    fn intended_base_promote_needs_config() {
        assert!(intended_base("dev", &[], None, true).is_err());
        assert_eq!(
            intended_base("dev", &["main".into()], None, true).unwrap(),
            "main"
        );
        assert_eq!(
            intended_base("dev", &["main".into()], Some("main"), false).unwrap(),
            "main"
        );
        assert!(intended_base("dev", &["main".into()], Some("staging"), false).is_err());
    }

    #[test]
    fn overlap_distinguishes_exact_from_nearby() {
        let ours = vec![
            "docs/ref.app.analytics.md".into(),
            "app/web/src/lib/foo.ts".into(),
        ];
        let theirs = vec![
            "docs/ref.app.analytics.md".into(),
            "app/web/src/lib/bar.ts".into(),
            "README.md".into(),
        ];
        let (exact, nearby) = classify_overlap(&ours, &theirs);
        assert_eq!(exact, vec!["docs/ref.app.analytics.md"]);
        assert_eq!(nearby, vec!["app/web/src/lib/bar.ts"]);
    }

    #[test]
    fn overlap_ranks_canon_refs_first() {
        let ours = vec![
            "docs/ref.app.analytics.md".into(),
            "db/migrations/002_x.sql".into(),
            "app/web/src/a.ts".into(),
        ];
        let theirs = ours.clone();
        let (exact, _) = classify_overlap(&ours, &theirs);
        assert_eq!(
            exact,
            vec![
                "docs/ref.app.analytics.md",
                "db/migrations/002_x.sql",
                "app/web/src/a.ts"
            ]
        );
    }
}
