//! Lockstep version bump for the midas repo itself (SPEC §7 / AGENTS.md). `midas flow tag` calls
//! into this when the requested tag doesn't match the on-disk versions, or when a prior tag push
//! left a broken release behind.

use crate::cmd::sync;
use crate::core::config::load_toml;
use crate::core::exit::CliError;
use crate::core::output::Output;
use crate::manifest::Manifest;
use crate::registry::Registry;
use regex::Regex;
use std::path::{Path, PathBuf};

/// On-disk versions that must move together before `cargo-dist` can release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseState {
    pub cargo: String,
    pub registry: String,
    pub midas_toml: String,
}

impl ReleaseState {
    /// True when every tracked version equals `target` (no leading `v`).
    pub fn matches(&self, target: &str) -> bool {
        self.cargo == target && self.registry == target && self.midas_toml == target
    }

    /// Human-readable summary of what is out of sync.
    pub fn drift_summary(&self, target: &str) -> String {
        let mut parts = Vec::new();
        if self.cargo != target {
            parts.push(format!("cli/Cargo.toml is {}", self.cargo));
        }
        if self.registry != target {
            parts.push(format!("registry/conventions.json is {}", self.registry));
        }
        if self.midas_toml != target {
            parts.push(format!("midas.toml [standard] is {}", self.midas_toml));
        }
        parts.join("; ")
    }
}

/// True when `root` looks like the midas repo (not a consumer project).
pub fn is_midas_repo(root: &Path) -> bool {
    root.join("cli/Cargo.toml").is_file()
        && root.join("registry/conventions.json").is_file()
        && root.join("cli/src/registry.rs").is_file()
}

/// Read the three lockstep version pins from disk.
pub fn read_state(root: &Path) -> Result<ReleaseState, CliError> {
    let cargo = read_cargo_version(&root.join("cli/Cargo.toml"))?;
    let registry = Registry::from_file(&root.join("registry/conventions.json"))
        .map_err(CliError::tool)?
        .version;
    let manifest: Manifest = load_toml(&root.join("midas.toml")).map_err(CliError::tool)?;
    let midas_toml = manifest.standard.version.clone();
    Ok(ReleaseState {
        cargo,
        registry,
        midas_toml,
    })
}

/// Bump every lockstep pin to `target`, freeze history, and refresh agent-doc blocks.
pub fn bump(root: &Path, target: &str, out: &Output) -> Result<Vec<PathBuf>, CliError> {
    let state = read_state(root)?;
    if state.matches(target) {
        return Ok(Vec::new());
    }

    out.info(format!(
        "versions out of sync for v{target} — {}",
        state.drift_summary(target)
    ));

    let mut touched = Vec::new();

    let cargo_path = root.join("cli/Cargo.toml");
    write_cargo_version(&cargo_path, target)?;
    touched.push(cargo_path);

    let registry_path = root.join("registry/conventions.json");
    write_registry_version(&registry_path, target)?;
    touched.push(registry_path.clone());

    let history_path = root.join(format!("registry/history/{target}.json"));
    if !history_path.is_file() {
        std::fs::copy(&registry_path, &history_path).map_err(CliError::tool)?;
        touched.push(history_path);
    }

    let registry_rs = root.join("cli/src/registry.rs");
    write_history_entry(&registry_rs, target)?;
    touched.push(registry_rs);

    let midas_toml = root.join("midas.toml");
    write_midas_toml_version(&midas_toml, target)?;
    touched.push(midas_toml);

    let (_, changed) = sync::write_blocks_at_version(root, target)?;
    for name in changed {
        touched.push(root.join(name));
    }

    out.success(format!("bumped lockstep versions to {target}"));
    Ok(touched)
}

/// Strip a leading `v` from a tag like `v0.4.1`.
pub fn semver_from_tag(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

fn read_cargo_version(path: &Path) -> Result<String, CliError> {
    let raw = std::fs::read_to_string(path).map_err(CliError::tool)?;
    parse_cargo_version(&raw)
        .ok_or_else(|| CliError::expected(format!("no version in {}", path.display())))
}

fn parse_cargo_version(raw: &str) -> Option<String> {
    let re = Regex::new(r#"(?m)^version\s*=\s*"([^"]+)""#).unwrap();
    re.captures(raw).map(|c| c[1].to_string())
}

fn write_cargo_version(path: &Path, version: &str) -> Result<(), CliError> {
    let raw = std::fs::read_to_string(path).map_err(CliError::tool)?;
    let re = Regex::new(r#"(?m)^(version\s*=\s*)"[^"]+""#).unwrap();
    let next = re.replace(&raw, format!(r#"$1"{version}""#)).into_owned();
    std::fs::write(path, next).map_err(CliError::tool)
}

fn write_registry_version(path: &Path, version: &str) -> Result<(), CliError> {
    let raw = std::fs::read_to_string(path).map_err(CliError::tool)?;
    let re = Regex::new(r#"(?m)^(\s*"version":\s*)"[^"]+""#).unwrap();
    let next = re.replace(&raw, format!(r#"$1"{version}""#)).into_owned();
    std::fs::write(path, next).map_err(CliError::tool)
}

fn write_midas_toml_version(path: &Path, version: &str) -> Result<(), CliError> {
    let raw = std::fs::read_to_string(path).map_err(CliError::tool)?;
    let re = Regex::new(r#"(?m)^(\s*version\s*=\s*)"[^"]+""#).unwrap();
    let next = re.replace(&raw, format!(r#"$1"{version}""#)).into_owned();
    std::fs::write(path, next).map_err(CliError::tool)
}

/// Insert `("X.Y.Z", include_str!(...))` into the embedded `HISTORY` table when missing.
fn write_history_entry(path: &Path, version: &str) -> Result<(), CliError> {
    let raw = std::fs::read_to_string(path).map_err(CliError::tool)?;
    let needle = format!(r#"("{version}", include_str!("../../registry/history/{version}.json"))"#);
    if raw.contains(&needle) {
        return Ok(());
    }

    let entry =
        format!("    (\"{version}\", include_str!(\"../../registry/history/{version}.json\")),\n");
    let re = Regex::new(r"(const HISTORY: &\[.*?\] = &\[)([\s\S]*?)(\];)").unwrap();
    let Some(caps) = re.captures(&raw) else {
        return Err(CliError::expected(format!(
            "could not find HISTORY table in {}",
            path.display()
        )));
    };
    let mut body = caps[2].to_string();
    body.push_str(&entry);
    let next = re
        .replace(&raw, format!("{}{}{}", &caps[1], body, &caps[3]))
        .into_owned();
    std::fs::write(path, next).map_err(CliError::tool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn semver_from_tag_strips_v() {
        assert_eq!(semver_from_tag("v0.4.1"), "0.4.1");
        assert_eq!(semver_from_tag("0.4.1"), "0.4.1");
    }

    #[test]
    fn parse_and_write_cargo_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cargo.toml");
        fs::write(&path, "[package]\nname = \"midas\"\nversion = \"0.4.0\"\n").unwrap();
        assert_eq!(read_cargo_version(&path).unwrap(), "0.4.0");
        write_cargo_version(&path, "0.4.1").unwrap();
        assert_eq!(read_cargo_version(&path).unwrap(), "0.4.1");
    }

    #[test]
    fn write_history_entry_appends_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("registry.rs");
        fs::write(
            &path,
            r#"const HISTORY: &[(&str, &str)] = &[
    ("0.4.0", include_str!("../../registry/history/0.4.0.json")),
];
"#,
        )
        .unwrap();
        write_history_entry(&path, "0.4.1").unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        assert!(raw.contains(r#"("0.4.1", include_str!("../../registry/history/0.4.1.json"))"#));
        write_history_entry(&path, "0.4.1").unwrap();
        assert_eq!(
            raw.matches(r#"("0.4.1", include_str!("../../registry/history/0.4.1.json"))"#)
                .count(),
            1
        );
    }

    #[test]
    fn bump_lockstep_versions_on_mini_repo() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("cli/src")).unwrap();
        fs::create_dir_all(root.join("registry/history")).unwrap();
        fs::write(
            root.join("cli/Cargo.toml"),
            "[package]\nname = \"midas\"\nversion = \"0.4.0\"\n",
        )
        .unwrap();
        fs::write(
            root.join("registry/conventions.json"),
            "{\n  \"version\": \"0.4.0\",\n  \"conventions\": []\n}\n",
        )
        .unwrap();
        fs::write(root.join("midas.toml"), "[standard]\nversion = \"0.4.0\"\n").unwrap();
        fs::write(
            root.join("cli/src/registry.rs"),
            r#"const HISTORY: &[(&str, &str)] = &[
    ("0.4.0", include_str!("../../registry/history/0.4.0.json")),
];
"#,
        )
        .unwrap();
        fs::write(root.join("CLAUDE.md"), "# Project\n").unwrap();

        let out = crate::core::output::Output::new(&crate::core::global::GlobalArgs {
            quiet: true,
            ..Default::default()
        });
        bump(root, "0.4.1", &out).unwrap();
        let state = read_state(root).unwrap();
        assert!(state.matches("0.4.1"));
        assert!(root.join("registry/history/0.4.1.json").is_file());
        let claude = fs::read_to_string(root.join("CLAUDE.md")).unwrap();
        assert!(claude.contains("midas:0.4.1"));
    }
}
