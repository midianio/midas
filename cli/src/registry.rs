//! The embedded convention registry — the machine-readable mirror of `standards/`. Built into the
//! binary via `include_str!` so `midas check` is self-contained (no repo fetch, no checker/rules
//! skew — see `SPEC.md §7`).

use anyhow::{Context, Result};
use serde::Deserialize;

/// `registry/conventions.json`, embedded at build time.
const EMBEDDED: &str = include_str!("../../registry/conventions.json");

#[derive(Debug, Deserialize)]
pub struct Registry {
    pub version: String,
    pub conventions: Vec<Convention>,
}

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Mechanically verifiable → `midas check`.
    Check,
    /// Semantic → delegated to an external agent reviewer (not run by the binary).
    Review,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Deserialize, Clone)]
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
    /// Parse the embedded registry.
    pub fn embedded() -> Result<Registry> {
        serde_json::from_str(EMBEDDED).context("parse embedded registry/conventions.json")
    }
}
