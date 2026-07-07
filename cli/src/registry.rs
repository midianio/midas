//! The embedded convention registry — the machine-readable mirror of `standards/`. Built into the
//! binary via `include_str!` so `midas check` is self-contained (no repo fetch, no checker/rules
//! skew — see `SPEC.md §7`).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// `registry/conventions.json`, embedded at build time — the *live* standard this binary speaks.
const EMBEDDED: &str = include_str!("../../registry/conventions.json");

/// Frozen snapshots of every *released* standard version, embedded so `midas drift` can diff any two
/// versions fully offline (same self-contained principle as the live registry — no repo fetch, no
/// skew). The release flow appends a `registry/history/<version>.json` when the standard bumps; the
/// `history_snapshot_matches_live` test below keeps the snapshot for the current version honest.
const HISTORY: &[(&str, &str)] = &[
    ("0.1.0", include_str!("../../registry/history/0.1.0.json")),
    ("0.2.0", include_str!("../../registry/history/0.2.0.json")),
    ("0.3.0", include_str!("../../registry/history/0.3.0.json")),
    ("0.4.0", include_str!("../../registry/history/0.4.0.json")),
    ("0.4.1", include_str!("../../registry/history/0.4.1.json")),
];

#[derive(Debug, Deserialize)]
pub struct Registry {
    pub version: String,
    pub conventions: Vec<Convention>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Convention {
    pub id: String,
    pub title: String,
    pub layer: String,
    #[serde(default)]
    pub stack: Option<String>,
    /// proposed | adopted | deprecated. Part of the mirror; not yet consumed by `check`.
    #[serde(default)]
    #[allow(dead_code)]
    pub status: Option<String>,
    pub tier: Tier,
    pub escape: Escape,
    #[serde(default)]
    pub check: Option<CheckSpec>,
    #[serde(default)]
    pub doc: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Mechanically verifiable → `midas check`.
    Check,
    /// Semantic → delegated to an external agent reviewer (not run by the binary).
    Review,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Escape {
    /// No deviation allowed.
    Hard,
    /// Allowed if recorded in `midas.toml [deviations]`.
    Ledgered,
    /// Recommended; a violation never blocks.
    Advisory,
}

/// A mechanical check spec. `kind` is the discriminant.
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum CheckSpec {
    /// A regex/substring that must not appear (outside `allow_in`) in files matching `globs`.
    BannedCall {
        pattern: String,
        #[serde(default)]
        allow_in: Vec<String>,
        globs: Vec<String>,
        #[serde(default)]
        message: Option<String>,
    },
    /// Paths that must / must not exist (relative to the repo root).
    FileStructure {
        #[serde(default)]
        must_exist: Vec<String>,
        #[serde(default)]
        must_not_exist: Vec<String>,
    },
    /// Files matching `globs` must be gitignored (or absent) — e.g. `.env.local` must never be
    /// committable. Any match visible to the gitignore-respecting scan is a violation.
    BannedFile {
        globs: Vec<String>,
        #[serde(default)]
        message: Option<String>,
    },
    /// The version-stamped `midas sync` managed block must be present and current in every agent
    /// doc (`CLAUDE.md`, `AGENTS.md`), stamped with the version of the standard being evaluated.
    ManagedBlock {},
    /// A generated artifact must be in sync with its source (deferred — reported as skipped).
    ArtifactHash {
        #[serde(default)]
        #[allow(dead_code)]
        source: Option<String>,
        #[serde(default)]
        #[allow(dead_code)]
        artifact: Option<String>,
    },
    /// A vendored `// midas:provenance` file vs its canonical version (deferred — skipped).
    ProvenanceDrift {},
    /// Passthrough a clippy lint (deferred — skipped; CI runs clippy directly).
    Clippy {
        #[serde(default)]
        #[allow(dead_code)]
        lint: Option<String>,
    },
}

impl Registry {
    /// Parse the embedded (live) registry.
    pub fn embedded() -> Result<Registry> {
        serde_json::from_str(EMBEDDED).context("parse embedded registry/conventions.json")
    }

    /// Look up a convention by id.
    pub fn by_id(&self, id: &str) -> Option<&Convention> {
        self.conventions.iter().find(|c| c.id == id)
    }

    /// Every standard version this binary can resolve: the live one plus all frozen snapshots,
    /// sorted ascending by semver. This is the set `drift` accepts and the list it prints when a
    /// caller asks for an unknown version.
    pub fn available_versions() -> Vec<String> {
        let mut versions: Vec<String> = HISTORY.iter().map(|(v, _)| v.to_string()).collect();
        if let Ok(live) = Registry::embedded() {
            if !versions.contains(&live.version) {
                versions.push(live.version);
            }
        }
        versions.sort_by_key(|v| semver_key(v));
        versions.dedup();
        versions
    }

    /// Resolve a specific standard version to its registry: the live registry if it matches, else a
    /// frozen snapshot. `Ok(None)` when the version isn't embedded (the caller turns that into a
    /// usage error listing [`available_versions`]).
    pub fn at_version(version: &str) -> Result<Option<Registry>> {
        let live = Registry::embedded()?;
        if live.version == version {
            return Ok(Some(live));
        }
        for (ver, json) in HISTORY {
            if *ver == version {
                let r: Registry = serde_json::from_str(json)
                    .with_context(|| format!("parse embedded history registry {ver}"))?;
                return Ok(Some(r));
            }
        }
        Ok(None)
    }

    /// Load a registry from a local `conventions.json` — the `--from-file/--to-file` escape hatch
    /// for unreleased or work-in-progress registries that aren't embedded.
    pub fn from_file(path: &Path) -> Result<Registry> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read registry {}", path.display()))?;
        serde_json::from_str(&raw).with_context(|| format!("parse registry {}", path.display()))
    }
}

/// A sortable/comparable key for a `MAJOR.MINOR.PATCH` version. Non-numeric or short versions sort
/// before well-formed ones; this is an ordering aid, not a strict semver parser.
pub fn semver_key(v: &str) -> (u64, u64, u64) {
    let mut it = v.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Honesty guard for the embedded history: the snapshot for the *live* version must describe the
    /// same standard as the live registry (same version + same conventions). If the live registry
    /// evolves without its snapshot being re-frozen at release, `drift` would silently lie about the
    /// current version — this fails the build first.
    #[test]
    fn history_snapshot_matches_live() {
        let live = Registry::embedded().expect("live registry parses");
        let snapshot = Registry::at_version(&live.version)
            .expect("resolve live version")
            .expect("a frozen snapshot exists for the live standard version");
        assert_eq!(snapshot.version, live.version);

        let key = |r: &Registry| {
            let mut rows: Vec<(String, String, String, Option<String>)> = r
                .conventions
                .iter()
                .map(|c| {
                    (
                        c.id.clone(),
                        format!("{:?}", c.tier),
                        format!("{:?}", c.escape),
                        c.doc.clone(),
                    )
                })
                .collect();
            rows.sort();
            rows
        };
        assert_eq!(
            key(&snapshot),
            key(&live),
            "history/{}.json has drifted from the live registry — re-freeze it",
            live.version
        );
    }

    #[test]
    fn available_versions_includes_live() {
        let live = Registry::embedded().unwrap();
        assert!(Registry::available_versions().contains(&live.version));
    }

    /// The one-tag invariant (SPEC §7): the binary's crate version IS the standard version it
    /// embeds. A release bumps `cli/Cargo.toml`, `registry/conventions.json`, and freezes
    /// `registry/history/<ver>.json` together — this fails the build when they diverge.
    #[test]
    fn binary_version_equals_embedded_standard_version() {
        let live = Registry::embedded().unwrap();
        assert_eq!(
            env!("CARGO_PKG_VERSION"),
            live.version,
            "cli/Cargo.toml version and registry/conventions.json version must move together \
             (one git tag governs both)"
        );
    }
}
