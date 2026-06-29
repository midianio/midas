//! Mechanical check kinds. A `Scanner` walks the repo once (respecting `.gitignore`) and caches
//! file contents so each convention's banned-call scan is cheap.

use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// One mechanical hit: a file:line that violates a convention.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub file: String,
    pub line: u32,
    pub text: String,
}

/// Cap findings per convention so a pervasive violation doesn't flood output.
const MAX_FINDINGS: usize = 50;

pub struct Scanner {
    root: PathBuf,
    files: Vec<PathBuf>, // relative to root
    cache: HashMap<PathBuf, Option<String>>,
}

impl Scanner {
    pub fn new(root: &Path) -> Result<Self> {
        let mut files = Vec::new();
        for entry in WalkBuilder::new(root).hidden(true).git_ignore(true).build() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                if let Ok(rel) = entry.path().strip_prefix(root) {
                    files.push(rel.to_path_buf());
                }
            }
        }
        Ok(Scanner {
            root: root.to_path_buf(),
            files,
            cache: HashMap::new(),
        })
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    fn content(&mut self, rel: &Path) -> Option<&str> {
        let abs = self.root.join(rel);
        self.cache
            .entry(rel.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(&abs).ok())
            .as_deref()
    }

    /// Files matching `globs` but not `allow_in` (paths relative to root, forward-slashed).
    fn matching_files(&self, globs: &GlobSet, allow: &GlobSet) -> Vec<PathBuf> {
        self.files
            .iter()
            .filter(|rel| {
                let s = rel.to_string_lossy();
                globs.is_match(s.as_ref()) && !allow.is_match(s.as_ref())
            })
            .cloned()
            .collect()
    }

    /// Scan for a banned regex/substring. Returns findings and whether output was truncated.
    pub fn banned_call(
        &mut self,
        pattern: &str,
        allow_in: &[String],
        globs: &[String],
    ) -> Result<(Vec<Finding>, bool)> {
        let glob_set = build_globset(globs)?;
        let allow_set = build_globset(allow_in)?;
        let re = Regex::new(pattern)
            .or_else(|_| Regex::new(&regex::escape(pattern)))
            .map_err(|e| anyhow::anyhow!("invalid pattern {pattern:?}: {e}"))?;

        let candidates = self.matching_files(&glob_set, &allow_set);
        let mut findings = Vec::new();
        let mut truncated = false;

        for rel in candidates {
            let rel_str = rel.to_string_lossy().to_string();
            let Some(content) = self.content(&rel) else {
                continue;
            };
            for (i, line) in content.lines().enumerate() {
                if re.is_match(line) {
                    if findings.len() >= MAX_FINDINGS {
                        truncated = true;
                        break;
                    }
                    findings.push(Finding {
                        file: rel_str.clone(),
                        line: (i + 1) as u32,
                        text: line.trim().chars().take(160).collect(),
                    });
                }
            }
            if truncated {
                break;
            }
        }
        Ok((findings, truncated))
    }

    /// Check that required paths exist and forbidden paths do not (relative to root).
    pub fn file_structure(&self, must_exist: &[String], must_not_exist: &[String]) -> Vec<Finding> {
        let mut findings = Vec::new();
        for p in must_exist {
            if !self.root.join(p).exists() {
                findings.push(Finding {
                    file: p.clone(),
                    line: 0,
                    text: "required path is missing".into(),
                });
            }
        }
        for p in must_not_exist {
            if self.root.join(p).exists() {
                findings.push(Finding {
                    file: p.clone(),
                    line: 0,
                    text: "forbidden path exists".into(),
                });
            }
        }
        findings
    }
}

fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).map_err(|e| anyhow::anyhow!("invalid glob {p:?}: {e}"))?);
    }
    Ok(b.build()?)
}
