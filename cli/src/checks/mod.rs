//! Mechanical check kinds. A `Scanner` walks the repo once (respecting `.gitignore`) and caches
//! file contents so each convention's banned-call scan is cheap.

use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Whether a drift finding was introduced by this branch or inherited from the PR base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Branch,
    Trunk,
}

/// One mechanical hit: a file:line that violates a convention.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub file: String,
    pub line: u32,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
}

impl Finding {
    pub fn at(file: impl Into<String>, line: u32, text: impl AsRef<str>) -> Self {
        Finding {
            file: file.into(),
            line,
            text: text.as_ref().to_string(),
            origin: None,
        }
    }
}

/// Cap findings per convention so a pervasive violation doesn't flood output.
const MAX_FINDINGS: usize = 50;

pub struct Scanner {
    root: PathBuf,
    files: Vec<PathBuf>, // relative to root; may be narrowed by `--changed`
    /// Unfiltered walk — source-drift is a history check, so `--changed` must not hide a
    /// doc whose *sources* moved. Same idea as "structure checks still run repo-wide".
    all_files: Vec<PathBuf>,
    cache: HashMap<PathBuf, Option<String>>,
    /// Memoised `git rev-parse --is-shallow-repository`. `None` until first probed.
    shallow: Option<bool>,
    /// Memoised `git log -1 --format='%cs %H'` per pathspec.
    last_change: HashMap<String, Option<(String, String)>>,
    /// Merge-base (or `--base`) used to attribute drift. `None` means no attribution —
    /// findings stay absolute, matching a check with no resolvable PR base.
    baseline: Option<String>,
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
            all_files: files.clone(),
            files,
            cache: HashMap::new(),
            shallow: None,
            last_change: HashMap::new(),
            baseline: None,
        })
    }

    /// The ref drift findings are attributed against. Unset → absolute (every stale doc fails).
    pub fn set_baseline(&mut self, baseline: Option<String>) {
        self.baseline = baseline;
    }

    pub fn baseline(&self) -> Option<&str> {
        self.baseline.as_deref()
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Narrow the scan to `keep` (root-relative, forward-slashed) — `check --changed`. Only the
    /// content scans (banned-call / banned-file) consult the file list; file-structure,
    /// managed-block, and source-drift checks probe the whole tree (a doc is stale when its
    /// *sources* moved, not when the doc itself is in the diff).
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
        self.filter_files(&self.files, globs, allow)
    }

    /// Same as [`matching_files`] over the unfiltered walk — used by source-drift so a
    /// `--changed` retain cannot hide a doc that did not itself change.
    fn matching_all_files(&self, globs: &GlobSet, allow: &GlobSet) -> Vec<PathBuf> {
        self.filter_files(&self.all_files, globs, allow)
    }

    fn filter_files(&self, files: &[PathBuf], globs: &GlobSet, allow: &GlobSet) -> Vec<PathBuf> {
        files
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
                    findings.push(Finding::at(
                        rel_str.clone(),
                        (i + 1) as u32,
                        line.trim().chars().take(160).collect::<String>(),
                    ));
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
            .map(|rel| {
                Finding::at(
                    rel_slash(rel),
                    0,
                    message.unwrap_or("file must be gitignored, never committed"),
                )
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
                    findings.push(Finding::at(
                        rel_str.clone(),
                        0,
                        format!("missing '{key}' in frontmatter"),
                    ));
                }
            }
            if canon_true_set.is_match(&rel_str)
                && fm.get("canon").map(String::as_str) != Some("true")
            {
                findings.push(Finding::at(
                    rel_str.clone(),
                    0,
                    "missing 'canon: true' in frontmatter",
                ));
            }
            let is_nested = rel_str.contains('/');
            if is_nested && capped_set.as_ref().is_some_and(|s| s.is_match(&rel_str)) {
                let lines = content.lines().count() as u32;
                if lines > max_lines {
                    findings.push(Finding::at(
                        rel_str.clone(),
                        0,
                        format!("{lines} lines exceeds the {max_lines}-line cap for nested docs"),
                    ));
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
                findings.push(Finding::at(p.clone(), 0, "required path is missing"));
            }
        }
        for p in must_not_exist {
            if self.root.join(p).exists() {
                findings.push(Finding::at(p.clone(), 0, "forbidden path exists"));
            }
        }
        findings
    }

    /// The DOC family (`DOC-0001`..`DOC-0004`). One kind, four `rule`s, because the four ids carry
    /// different escapes but share the parse: a doc's identity is its filename, its state is its
    /// frontmatter, and the two must agree.
    ///
    /// `kind` and the per-kind contract are fixed by the standard — lifecycle means the same thing
    /// in every repo. `scopes` is the repo's own subsystem vocabulary, from `[docs] scopes`.
    pub fn doc_lifecycle(
        &mut self,
        rule: &str,
        root: &str,
        scopes: &[String],
        exclude: &[String],
        code_globs: &[String],
    ) -> Result<Vec<Finding>> {
        if rule == "citations" {
            return self.doc_citations(root, code_globs, exclude);
        }
        // A repo that declares no scopes has not opted in; DOC has nothing to say about it.
        if scopes.is_empty() {
            return Ok(Vec::new());
        }

        let docs_glob = build_globset(&[format!("{root}/**/*.md"), format!("{root}/**/*.html")])?;
        let exclude_set = build_globset(exclude)?;
        let mut findings = Vec::new();
        let mut drift_items: Vec<(String, String)> = Vec::new();
        let candidates = if rule == "drift" {
            self.matching_all_files(&docs_glob, &exclude_set)
        } else {
            self.matching_files(&docs_glob, &exclude_set)
        };

        for rel in candidates {
            let rel_str = rel_slash(&rel);
            let base = rel
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let dir = rel
                .parent()
                .map(rel_slash)
                .unwrap_or_default()
                .trim_start_matches(root)
                .trim_matches('/')
                .to_string();

            let Some(name) = DocName::parse(&base, scopes) else {
                if rule == "encoding" {
                    findings.push(Finding::at(rel_str, 0, format!(
                            "name must be <kind>.<scope>.<slug>[.YYYY-MM-DD].md — kind ∈ {}, scope ∈ {}",
                            KINDS.join("|"),
                            scopes.join("|")
                        )));
                }
                continue;
            };
            let Some(content) = self.content(&rel).map(str::to_string) else {
                continue;
            };
            let fm = frontmatter_map(&content);

            match rule {
                "encoding" => {
                    let want_dir = dir_for_kind(name.kind);
                    if dir != want_dir {
                        findings.push(Finding::at(
                            rel_str.clone(),
                            0,
                            format!(
                                "a '{}' belongs in {root}/{want_dir} (found in {root}/{dir})",
                                name.kind
                            ),
                        ));
                    }
                    let dated = matches!(name.kind, "adr" | "note");
                    if dated && name.date.is_none() {
                        findings.push(Finding::at(
                            rel_str.clone(),
                            0,
                            format!(
                                "a '{}' is point-in-time — its name needs a .YYYY-MM-DD. date",
                                name.kind
                            ),
                        ));
                    }
                    if !dated && name.date.is_some() {
                        findings.push(Finding::at(rel_str.clone(), 0, format!(
                                "a '{}' is a living doc — no date in the name (state lives in frontmatter)",
                                name.kind
                            )));
                    }
                    if base.ends_with(".md") {
                        for (key, want) in [("kind", name.kind), ("scope", name.scope.as_str())] {
                            match fm.get(key) {
                                Some(got) if got == want => {}
                                Some(got) => findings.push(Finding::at(rel_str.clone(), 0, format!("frontmatter {key} '{got}' disagrees with the filename '{want}'"))),
                                None => findings.push(Finding::at(rel_str.clone(), 0, format!("missing '{key}' in frontmatter"))),
                            }
                        }
                    }
                }
                "frontmatter" => {
                    if !base.ends_with(".md") {
                        continue;
                    }
                    for key in required_keys(name.kind) {
                        if !fm.contains_key(*key) {
                            findings.push(Finding::at(
                                rel_str.clone(),
                                0,
                                format!("missing '{key}' in frontmatter"),
                            ));
                        }
                    }
                    match fm.get("status") {
                        Some(s) if statuses(name.kind).contains(&s.as_str()) => {}
                        Some(s) => findings.push(Finding::at(
                            rel_str.clone(),
                            0,
                            format!(
                                "status '{s}' is not legal for a '{}' — expected {}",
                                name.kind,
                                statuses(name.kind).join("|")
                            ),
                        )),
                        None => {}
                    }
                    if fm.get("canon").map(String::as_str) == Some("true")
                        && frontmatter_list(&content, "sources").is_empty()
                    {
                        findings.push(Finding::at(
                            rel_str.clone(),
                            0,
                            "a canon doc must declare 'sources:' — what it describes, so drift is checkable",
                        ));
                    }
                }
                "drift" => drift_items.push((rel_str, content)),
                _ => {}
            }
        }
        if rule == "drift" {
            findings.extend(self.sources_drift_batch(&drift_items, false, 0));
        }
        Ok(findings)
    }

    /// `DOC-0003` — source files may cite `ref`/`adr` docs, never a plan or an archived note. Those
    /// two move by design, so a comment pointing at one is a dangling reference waiting to happen.
    fn doc_citations(
        &mut self,
        root: &str,
        code_globs: &[String],
        exclude: &[String],
    ) -> Result<Vec<Finding>> {
        let globs = build_globset(code_globs)?;
        let empty = build_globset(exclude)?;
        let needles = [format!("{root}/plans/"), format!("{root}/archive/")];
        let mut findings = Vec::new();
        for rel in self.matching_files(&globs, &empty) {
            let rel_str = rel_slash(&rel);
            let Some(content) = self.content(&rel) else {
                continue;
            };
            for (i, line) in content.lines().enumerate() {
                if let Some(hit) = needles.iter().find(|n| line.contains(n.as_str())) {
                    findings.push(Finding::at(
                        rel_str.clone(),
                        i as u32 + 1,
                        format!(
                            "cites {hit}… — code may only cite {root}/ref.* or {root}/decisions/"
                        ),
                    ));
                    break;
                }
            }
        }
        Ok(findings)
    }

    /// A canon doc is stale when something it claims to describe moved after it was last read —
    /// or when another governed doc it lists in `sources:` is already stale (fixing that one
    /// will rewrite it, which would fail the next check).
    ///
    /// Shared by `DOC-0004` (the docs corpus) and `AGT-0010` (agent instruction files) — the two
    /// differ only in which files they glob and how long they wait after the change (`grace_days`).
    /// `require_sources` also flags a `canon: true` file that never declared what it describes,
    /// since an undeclared doc is silently exempt from the check that matters most.
    ///
    /// Deliberately keyed on *change*, not the calendar: a doc about untouched code is not stale,
    /// and a date bumped without reading is the one failure no check can see. `grace_days` is a
    /// delay on enforcement, not a second trigger. Transitive hops do not wait again — they fire
    /// the moment the upstream doc is due, so one `midas check` names the whole cascade.
    fn sources_drift_batch(
        &mut self,
        items: &[(String, String)],
        require_sources: bool,
        grace_days: u32,
    ) -> Vec<Finding> {
        let mut findings = Vec::new();
        let mut docs: Vec<DriftDoc> = Vec::new();
        for (rel_str, content) in items {
            match self.governed_drift_doc(rel_str, content, require_sources) {
                DriftParse::Skip => {}
                DriftParse::MissingSources => findings.push(Finding::at(
                    rel_str.clone(),
                    0,
                    "declare 'sources:' — the globs this describes, so staleness is checkable",
                )),
                DriftParse::Doc(doc) => docs.push(doc),
            }
        }
        // A shallow clone has only the head commit, so every path would date to today and every
        // doc reviewed earlier would "drift". Report nothing rather than something false — CI
        // wanting this check must fetch full history (`fetch-depth: 0`). Missing `sources:` is
        // structural and does not need history, so those findings still return.
        if self.is_shallow() {
            return findings;
        }
        findings.extend(self.close_source_drift(&docs, grace_days));
        findings
    }

    fn governed_drift_doc(
        &self,
        rel_str: &str,
        content: &str,
        require_sources: bool,
    ) -> DriftParse {
        let fm = frontmatter_map(content);
        // DOC-0004 governs docs marked `canon: true`. AGT-0010 governs every file AGT-0009 already
        // covers — which is what carries `last_reviewed` — because `SKILL.md` is not required to
        // carry `canon: true`, and skills go stale exactly like anything else.
        let governed = fm.get("canon").map(String::as_str) == Some("true")
            || (require_sources && fm.contains_key("last_reviewed"));
        if !governed {
            return DriftParse::Skip;
        }
        let sources = frontmatter_list(content, "sources");
        if sources.is_empty() {
            // The root `AGENTS.md` is the index, not a description of a subsystem: any glob it
            // could name is either everything or an arbitrary slice pretending to be everything.
            // Same carve-out AGT-0009 makes for the line cap.
            let is_root_index = rel_str == "AGENTS.md";
            // An explicit empty `sources:` is an answer, not an omission — a doc about a practice
            // rather than about code has nothing here that can go stale.
            let declared_empty = frontmatter_declares(content, "sources");
            return if require_sources && !is_root_index && !declared_empty {
                DriftParse::MissingSources
            } else {
                DriftParse::Skip
            };
        }
        let Some(reviewed) = fm.get("last_reviewed").cloned() else {
            return DriftParse::Skip;
        };
        DriftParse::Doc(DriftDoc {
            path: rel_str.to_string(),
            sources,
            reviewed,
        })
    }

    /// Direct drift, then walk `sources:` to a fixpoint so a bump on A cannot surprise the next
    /// check with B (B listed A). Transitive hops use today's date and no extra grace: the rewrite
    /// of A happens the day someone acts on the finding.
    fn close_source_drift(&mut self, docs: &[DriftDoc], grace_days: u32) -> Vec<Finding> {
        let today = crate::date::today_ymd();
        // path → (reason, is_direct)
        let mut stale: HashMap<String, DriftReason> = HashMap::new();
        for doc in docs {
            for src in &doc.sources {
                if let Some((changed, commit)) = self.last_change(src) {
                    if crate::date::drift_is_due(&changed, &doc.reviewed, &today, grace_days) {
                        let origin = self.origin_of(&commit);
                        stale.insert(
                            doc.path.clone(),
                            DriftReason::Direct {
                                src: src.clone(),
                                changed,
                                reviewed: doc.reviewed.clone(),
                                origin,
                            },
                        );
                        break;
                    }
                }
            }
        }

        let mut queue: Vec<String> = stale.keys().cloned().collect();
        while let Some(upstream) = queue.pop() {
            for doc in docs {
                if stale.contains_key(&doc.path) || doc.path == upstream {
                    continue;
                }
                if !doc
                    .sources
                    .iter()
                    .any(|g| source_glob_matches(g, &upstream))
                {
                    continue;
                }
                // Fixing `upstream` rewrites it today. Same-day `last_reviewed` is not drift.
                if crate::date::drift_is_due(&today, &doc.reviewed, &today, 0) {
                    let origin = stale.get(&upstream).and_then(|r| r.origin());
                    stale.insert(
                        doc.path.clone(),
                        DriftReason::Via {
                            via: upstream.clone(),
                            origin,
                        },
                    );
                    queue.push(doc.path.clone());
                }
            }
        }

        let grace = if grace_days == 0 {
            String::new()
        } else {
            format!("{grace_days}-day grace elapsed; ")
        };
        docs.iter()
            .filter_map(|doc| {
                let reason = stale.get(&doc.path)?;
                let (text, origin) = match reason {
                    DriftReason::Direct {
                        src,
                        changed,
                        reviewed,
                        origin,
                    } => (
                        format!(
                            "'{src}' changed {changed}, after last_reviewed {reviewed} — {grace}re-read it, then bump the date"
                        ),
                        *origin,
                    ),
                    DriftReason::Via { via, origin } => (
                        format!(
                            "'{via}' is stale and listed in sources: — re-read it when that date moves, then bump the date"
                        ),
                        *origin,
                    ),
                };
                let mut finding = Finding::at(doc.path.clone(), 0, text);
                finding.origin = origin;
                Some(finding)
            })
            .collect()
    }

    /// `AGT-0010` — the same staleness contract over any glob set, so agent instruction files get
    /// the guarantee the docs corpus already has.
    pub fn source_drift(
        &mut self,
        globs: &[String],
        exclude: &[String],
        require_sources: bool,
        grace_days: u32,
    ) -> Result<Vec<Finding>> {
        let glob_set = build_globset(globs)?;
        let exclude_set = build_globset(exclude)?;
        let mut items = Vec::new();
        for rel in self.matching_all_files(&glob_set, &exclude_set) {
            let rel_str = rel_slash(&rel);
            let Some(content) = self.content(&rel).map(str::to_string) else {
                continue;
            };
            items.push((rel_str, content));
        }
        Ok(self.sources_drift_batch(&items, require_sources, grace_days))
    }

    /// Whether this working copy has truncated history. Drift is uncomputable if so.
    fn is_shallow(&mut self) -> bool {
        if let Some(known) = self.shallow {
            return known;
        }
        let shallow = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["rev-parse", "--is-shallow-repository"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
            .unwrap_or(false);
        self.shallow = Some(shallow);
        shallow
    }

    /// Last commit date + hash touching a pathspec, or `None` outside a git repo / for a
    /// path with no history. Glob magic is explicit so `**` means what the frontmatter says.
    fn last_change(&mut self, pathspec: &str) -> Option<(String, String)> {
        if let Some(cached) = self.last_change.get(pathspec) {
            return cached.clone();
        }
        let hit = (|| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&self.root)
                .args(["log", "-1", "--format=%cs %H", "--"])
                .arg(format!(":(glob){pathspec}"))
                .output()
                .ok()?;
            if !out.status.success() {
                return None;
            }
            let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let (date, commit) = line.split_once(' ')?;
            (date.len() == 10 && (commit.len() == 40 || commit.len() == 64))
                .then(|| (date.to_string(), commit.to_string()))
        })();
        self.last_change.insert(pathspec.to_string(), hit.clone());
        hit
    }

    /// Attribute a causing commit: ancestor of the baseline is trunk debt; otherwise this branch.
    /// No baseline (or baseline == HEAD) means no attribution — the finding stays absolute.
    fn origin_of(&self, commit: &str) -> Option<Origin> {
        let baseline = self.baseline.as_deref()?;
        if self.rev_parse("HEAD").as_deref() == Some(baseline) {
            return None;
        }
        Some(if self.is_ancestor(commit, baseline) {
            Origin::Trunk
        } else {
            Origin::Branch
        })
    }

    fn rev_parse(&self, spec: &str) -> Option<String> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["rev-parse", spec])
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
    }

    fn is_ancestor(&self, commit: &str, of: &str) -> bool {
        std::process::Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["merge-base", "--is-ancestor", commit, of])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

/// One governed doc that declared `sources:` and a `last_reviewed` date.
struct DriftDoc {
    path: String,
    sources: Vec<String>,
    reviewed: String,
}

enum DriftParse {
    Skip,
    MissingSources,
    Doc(DriftDoc),
}

enum DriftReason {
    Direct {
        src: String,
        changed: String,
        reviewed: String,
        origin: Option<Origin>,
    },
    Via {
        via: String,
        origin: Option<Origin>,
    },
}

impl DriftReason {
    fn origin(&self) -> Option<Origin> {
        match self {
            DriftReason::Direct { origin, .. } | DriftReason::Via { origin, .. } => *origin,
        }
    }
}

/// Whether a `sources:` glob matches a repo-relative path. Used to walk the doc graph
/// (`B` lists `A`, `A` is stale ⇒ `B` will be stale the day `A`'s date moves).
fn source_glob_matches(glob: &str, path: &str) -> bool {
    Glob::new(glob)
        .ok()
        .map(|g| g.compile_matcher().is_match(path))
        .unwrap_or(false)
}

/// The fixed lifecycle vocabulary. Repos vary in what they *have* (`scope`); they do not vary in
/// what a document can *be*.
const KINDS: [&str; 4] = ["ref", "adr", "plan", "note"];

fn dir_for_kind(kind: &str) -> &'static str {
    match kind {
        "adr" => "decisions",
        "plan" => "plans",
        "note" => "archive",
        _ => "",
    }
}

fn statuses(kind: &str) -> &'static [&'static str] {
    match kind {
        "ref" => &["current", "needs-review"],
        "adr" => &["accepted", "superseded"],
        "plan" => &["draft", "in-flight", "shipped", "abandoned"],
        _ => &["historical"],
    }
}

fn required_keys(kind: &str) -> &'static [&'static str] {
    match kind {
        "ref" => &["kind", "scope", "status", "owner", "last_reviewed"],
        "adr" => &["kind", "scope", "status", "owner", "decided"],
        "plan" => &["kind", "scope", "status", "owner"],
        _ => &["kind", "scope", "status", "owner", "recorded"],
    }
}

/// A parsed `<kind>.<scope>.<slug>[.<YYYY-MM-DD>].<ext>` filename.
struct DocName {
    kind: &'static str,
    scope: String,
    date: Option<String>,
}

impl DocName {
    fn parse(base: &str, scopes: &[String]) -> Option<DocName> {
        let stem = base
            .strip_suffix(".md")
            .or_else(|| base.strip_suffix(".html"))?;
        let parts: Vec<&str> = stem.split('.').collect();
        if parts.len() < 3 {
            return None;
        }
        let kind = KINDS.iter().find(|k| **k == parts[0])?;
        let scope = scopes.iter().find(|s| *s == parts[1])?.clone();
        let last = parts[parts.len() - 1];
        let date = crate::date::is_iso_date(last).then(|| last.to_string());
        // slug must be non-empty once kind, scope and any date are removed
        let slug_parts = parts.len() - 2 - usize::from(date.is_some());
        if slug_parts == 0 {
            return None;
        }
        Some(DocName { kind, scope, date })
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

/// Whether a frontmatter key is *present at all*, regardless of value. `sources: []` and a
/// `sources:` block with no entries both mean "nothing in this repo to drift against" — a
/// deliberate statement — which is not the same as never having considered the question.
fn frontmatter_declares(content: &str, key: &str) -> bool {
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return false;
    }
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if line.starts_with(&format!("{key}:")) {
            return true;
        }
    }
    false
}

/// Values of a frontmatter list key, in either YAML shape a human actually writes:
/// a block list (`sources:` then `  - glob`) or an inline array (`sources: [a, b]`).
/// [`frontmatter_map`] is single-line only and drops both, so DOC-0004 needs its own reader.
fn frontmatter_list(content: &str, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return out;
    }
    let mut in_key = false;
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        if let Some(rest) = line.strip_prefix(&format!("{key}:")) {
            let rest = rest.trim();
            if let Some(inner) = rest.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                out.extend(
                    inner
                        .split(',')
                        .map(|v| v.trim().trim_matches(['"', '\'']).to_string())
                        .filter(|v| !v.is_empty()),
                );
                return out;
            }
            in_key = rest.is_empty();
            continue;
        }
        if in_key {
            let t = line.trim();
            match t.strip_prefix("- ") {
                Some(v) => out.push(v.trim().trim_matches(['"', '\'']).to_string()),
                None => in_key = false,
            }
        }
    }
    out
}

fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).map_err(|e| anyhow::anyhow!("invalid glob {p:?}: {e}"))?);
    }
    Ok(b.build()?)
}
