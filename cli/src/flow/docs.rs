//! Opt-in resolution of `last_reviewed`-only rebase conflicts in canon refs.
//!
//! A conflict qualifies only when both sides are a canon ref and stripping the
//! `last_reviewed` scalar leaves identical documents. Trunk wins — we never
//! synthesize a date. Substantive content next to the date stays unresolved.

use crate::checks::frontmatter_map;
use serde::Serialize;

/// One automatically resolved path, for text and `--json` output.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResolvedDocDate {
    pub path: String,
    /// Always `"trunk"` — the branch date is discarded, not max()'d.
    pub kept: &'static str,
}

/// Filename of a living canon ref: `ref.<scope>.<slug>.md` (no date segment).
pub fn is_canon_ref_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let Some(stem) = name.strip_suffix(".md") else {
        return false;
    };
    let parts: Vec<&str> = stem.split('.').collect();
    parts.len() >= 3 && parts[0] == "ref"
}

/// True when both sides are canon refs and differ only in the `last_reviewed` scalar.
pub fn date_only_conflict(ours: &str, theirs: &str) -> bool {
    if !is_canon_doc(ours) || !is_canon_doc(theirs) {
        return false;
    }
    let a = strip_last_reviewed(ours);
    let b = strip_last_reviewed(theirs);
    !a.is_empty() && a == b
}

fn is_canon_doc(content: &str) -> bool {
    let fm = frontmatter_map(content);
    fm.get("canon").map(String::as_str) == Some("true") && fm.contains_key("last_reviewed")
}

/// Drop the `last_reviewed:` line from the leading frontmatter block and normalize newlines
/// so a date-only bump compares equal regardless of CRLF or a trailing newline.
fn strip_last_reviewed(content: &str) -> String {
    let content = content.replace("\r\n", "\n");
    let mut out = String::new();
    let mut lines = content.lines();
    match lines.next() {
        Some("---") => out.push_str("---\n"),
        Some(first) => {
            out.push_str(first);
            out.push('\n');
            for line in lines {
                out.push_str(line);
                out.push('\n');
            }
            return normalize_trailing(&out);
        }
        None => return String::new(),
    }
    let mut in_fm = true;
    for line in lines {
        if in_fm && line.trim() == "---" {
            out.push_str("---\n");
            in_fm = false;
            continue;
        }
        if in_fm && line.trim_start().starts_with("last_reviewed:") {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    normalize_trailing(&out)
}

fn normalize_trailing(s: &str) -> String {
    format!("{}\n", s.trim_end_matches('\n'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canon(reviewed: &str, body: &str) -> String {
        format!(
            "---\nkind: ref\nscope: api\nstatus: current\nowner: x\nlast_reviewed: {reviewed}\ncanon: true\nsources:\n  - app/api/src/**\n---\n\n{body}"
        )
    }

    #[test]
    fn path_accepts_living_refs_only() {
        assert!(is_canon_ref_path("docs/ref.api.thing.md"));
        assert!(is_canon_ref_path("ref.app.analytics.md"));
        assert!(!is_canon_ref_path("docs/adr.api.thing.2026-08-01.md"));
        assert!(!is_canon_ref_path("AGENTS.md"));
        assert!(!is_canon_ref_path("docs/plan.api.thing.md"));
    }

    #[test]
    fn date_only_sides_qualify() {
        let trunk = canon("2026-08-01", "# thing\n");
        let branch = canon("2026-08-20", "# thing\n");
        assert!(date_only_conflict(&trunk, &branch));
    }

    #[test]
    fn quoted_dates_still_qualify() {
        let a = "---\ncanon: true\nlast_reviewed: \"2026-08-01\"\n---\n\n# x\n";
        let b = "---\ncanon: true\nlast_reviewed: 2026-08-20\n---\n\n# x\n";
        assert!(date_only_conflict(a, b));
    }

    #[test]
    fn body_change_does_not_qualify() {
        let trunk = canon("2026-08-01", "# thing\n\ntrunk paragraph\n");
        let branch = canon("2026-08-20", "# thing\n\nbranch paragraph\n");
        assert!(!date_only_conflict(&trunk, &branch));
    }

    #[test]
    fn missing_canon_does_not_qualify() {
        let a = "---\nlast_reviewed: 2026-08-01\n---\n\n# x\n";
        let b = "---\nlast_reviewed: 2026-08-20\n---\n\n# x\n";
        assert!(!date_only_conflict(a, b));
    }

    #[test]
    fn adjacent_frontmatter_change_does_not_qualify() {
        let a = "---\ncanon: true\nowner: a\nlast_reviewed: 2026-08-01\n---\n\n# x\n";
        let b = "---\ncanon: true\nowner: b\nlast_reviewed: 2026-08-20\n---\n\n# x\n";
        assert!(!date_only_conflict(a, b));
    }
}
