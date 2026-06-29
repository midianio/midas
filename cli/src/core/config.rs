use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};

/// Walk up from `start` (inclusive) looking for `filename`; return its path if found. Used to find
/// `midas.toml` at the repo root from anywhere inside the tree.
pub fn find_up(start: &Path, filename: &str) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let candidate = d.join(filename);
        if candidate.is_file() {
            return Some(candidate);
        }
        dir = d.parent();
    }
    None
}

/// Read + parse a TOML file into `T`.
pub fn load_toml<T: DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("read {}: {e}", path.display()))?;
    toml::from_str(&raw).map_err(|e| anyhow::anyhow!("parse {}: {e}", path.display()))
}
