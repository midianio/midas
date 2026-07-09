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
        // Hidden files are walked (banned-file checks target dotfiles like `.env.local`), but
        // `.gitignore` rules still apply — even outside a git repo, so fixtures behave like repos.
        for entry in WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(true)
            .require_git(false)
            .filter_entry(|e| e.file_name() != ".git")
            .build()
        {
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

    /// Narrow the scan to `keep` (root-relative, forward-slashed) — `check --changed`. Only the
    /// content scans (banned-call / banned-file) consult the file list; file-structure and
    /// managed-block checks probe the filesystem directly and still see the whole tree.
    pub fn retain(&mut self, keep: &std::collections::HashSet<String>) {
        self.files.retain(|rel| keep.contains(&rel_slash(rel)));
    }

    pub fn root(&self) -> &Path {
        &self.root
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
                let s = rel_slash(rel);
                globs.is_match(&s) && !allow.is_match(&s)
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
            let rel_str = rel_slash(&rel);
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

    /// Files matching `globs` must not be visible to the scan — i.e. they must be gitignored (or
    /// absent). The walk already drops ignored files, so any match here is tracked/committable.
    pub fn banned_file(&self, globs: &[String], message: Option<&str>) -> Result<Vec<Finding>> {
        let glob_set = build_globset(globs)?;
        Ok(self
            .files
            .iter()
            .filter(|rel| glob_set.is_match(rel_slash(rel)))
            .map(|rel| Finding {
                file: rel_slash(rel),
                line: 0,
                text: message
                    .unwrap_or("file must be gitignored, never committed")
                    .into(),
            })
            .collect())
    }

    /// Whether at least one tracked (non-gitignored) file matches `glob` — the presence half of
    /// `artifact-hash`: a glob matching nothing means the file is either absent or gitignored, and
    /// either way there's nothing committed for drift to be checked against.
    pub fn any_match(&self, glob: &str) -> Result<bool> {
        let set = build_globset(std::slice::from_ref(&glob.to_string()))?;
        Ok(self.files.iter().any(|rel| set.is_match(rel_slash(rel))))
    }

    /// AGT-0009: canonical context docs matching `globs` (minus `exclude`) must carry `owner` +
    /// `last_reviewed` frontmatter keys; those additionally matching `canon_true_globs` (root-canon
    /// docs — everything except `SKILL.md`, which has its own `name`/`description` contract) must
    /// also carry `canon: true`; a *nested* (non-root) file additionally matching `capped_glob` is
    /// capped at `max_lines` — the operational-discipline forcing function on per-directory
    /// `AGENTS.md`.
    pub fn canon_context(
        &mut self,
        globs: &[String],
        exclude: &[String],
        canon_true_globs: &[String],
        capped_glob: Option<&str>,
        max_lines: u32,
    ) -> Result<Vec<Finding>> {
        let glob_set = build_globset(globs)?;
        let exclude_set = build_globset(exclude)?;
        let canon_true_set = build_globset(canon_true_globs)?;
        let capped_set = capped_glob
            .map(|g| build_globset(std::slice::from_ref(&g.to_string())))
            .transpose()?;

        let candidates = self.matching_files(&glob_set, &exclude_set);
        let mut findings = Vec::new();
        for rel in candidates {
            let rel_str = rel_slash(&rel);
            let Some(content) = self.content(&rel) else {
                continue;
            };
            let fm = frontmatter_map(content);
            for key in ["owner", "last_reviewed"] {
                if !fm.contains_key(key) {
                    findings.push(Finding {
                        file: rel_str.clone(),
                        line: 0,
                        text: format!("missing '{key}' in frontmatter"),
                    });
                }
            }
            if canon_true_set.is_match(&rel_str)
                && fm.get("canon").map(String::as_str) != Some("true")
            {
                findings.push(Finding {
                    file: rel_str.clone(),
                    line: 0,
                    text: "missing 'canon: true' in frontmatter".into(),
                });
            }
            let is_nested = rel_str.contains('/');
            if is_nested && capped_set.as_ref().is_some_and(|s| s.is_match(&rel_str)) {
                let lines = content.lines().count() as u32;
                if lines > max_lines {
                    findings.push(Finding {
                        file: rel_str.clone(),
                        line: 0,
                        text: format!(
                            "{lines} lines exceeds the {max_lines}-line cap for nested docs"
                        ),
                    });
                }
            }
        }
        Ok(findings)
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

/// A relative path as a forward-slashed string — registry globs use `/`, and findings must render
/// identically across platforms (Windows walks yield `\`-separated paths that `/`-globs never match).
fn rel_slash(rel: &Path) -> String {
    let s = rel.to_string_lossy();
    if cfg!(windows) {
        s.replace('\\', "/")
    } else {
        s.into_owned()
    }
}

/// Present, non-empty `key: value` pairs from a file's leading `---`-delimited frontmatter block
/// (line 1 must be exactly `---`). Minimal single-line scan — matches how these docs are actually
/// authored, not a full YAML parser.
fn frontmatter_map(content: &str) -> std::collections::HashMap<String, String> {
    let mut kv = std::collections::HashMap::new();
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return kv;
    }
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim().trim_matches('"');
            if !v.is_empty() {
                kv.insert(k.trim().to_string(), v.to_string());
            }
        }
    }
    kv
}

fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).map_err(|e| anyhow::anyhow!("invalid glob {p:?}: {e}"))?);
    }
    Ok(b.build()?)
}
