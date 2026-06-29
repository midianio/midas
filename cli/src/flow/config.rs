use crate::manifest::Manifest;

/// Branch types, in display order. `feat`/`fix` default to a seeded paired pscale branch;
/// `chore`/`docs`/`spike` are git-only by default.
pub const BRANCH_TYPES: &[&str] = &["feat", "fix", "chore", "docs", "spike"];

pub fn valid_branch_type(t: &str) -> bool {
    BRANCH_TYPES.contains(&t)
}

/// feat/fix touch behavior or schema → seeded paired branch by default; the rest are git-only.
pub fn seed_by_default(branch_type: &str) -> bool {
    matches!(branch_type, "feat" | "fix")
}

/// Resolved flow configuration. Defaults match midian's `midflow` (and are wire-compatible with the
/// existing `.midflow/active.json` + `# >>> midflow >>>` env markers, so `midas flow` is a drop-in).
#[derive(Debug, Clone)]
pub struct FlowConfig {
    pub trunk: String,
    pub org: String,
    pub db: String,
    pub parent: String,
    pub region: String,
    pub port: u16,
    pub api_env_local: String,
    pub state_file: String,
    pub env_marker: String,
}

impl Default for FlowConfig {
    fn default() -> Self {
        FlowConfig {
            trunk: "dev".into(),
            org: "midian".into(),
            db: "application".into(),
            parent: "dev".into(),
            region: "us-east".into(),
            port: 3309,
            api_env_local: "app/api/.env.local".into(),
            state_file: ".midflow/active.json".into(),
            env_marker: "midflow".into(),
        }
    }
}

impl FlowConfig {
    pub fn from_manifest(m: &Manifest) -> Self {
        let d = FlowConfig::default();
        let f = &m.flow;
        FlowConfig {
            trunk: f.trunk.clone().unwrap_or(d.trunk),
            org: f.pscale_org.clone().unwrap_or(d.org),
            db: f.pscale_db.clone().unwrap_or(d.db),
            parent: f.pscale_parent.clone().unwrap_or(d.parent),
            region: f.pscale_region.clone().unwrap_or(d.region),
            port: f.tunnel_port.unwrap_or(d.port),
            api_env_local: f.api_env_local.clone().unwrap_or(d.api_env_local),
            state_file: f.state_file.clone().unwrap_or(d.state_file),
            env_marker: f.env_marker.clone().unwrap_or(d.env_marker),
        }
    }

    /// `root@tcp(127.0.0.1:PORT)/DB?...` — the local tunnel DSN written into `.env.local`.
    pub fn local_db_url(&self) -> String {
        format!(
            "root@tcp(127.0.0.1:{})/{}?tls=false&parseTime=true&interpolateParams=true&loc=UTC",
            self.port, self.db
        )
    }
}

/// `feat/notes-pane` → `feat-notes-pane` (so PR-merge automation can recover the pscale branch from
/// the head ref without consulting state).
pub fn pscale_branch_from_git(git_branch: &str) -> String {
    git_branch.replace('/', "-")
}

/// Sanitize a free-form name into a slug usable as both a git suffix and a pscale branch suffix.
pub fn slugify(name: &str) -> String {
    let lower = name.trim().to_lowercase();
    let mut out = String::new();
    let mut prev_dash = false;
    for c in lower.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let mut s = out.trim_matches('-').to_string();
    if s.len() > 60 {
        s.truncate(60);
        s = s.trim_end_matches('-').to_string();
    }
    s
}

pub fn validate_slug(slug: &str) -> anyhow::Result<()> {
    if slug.is_empty() {
        anyhow::bail!("slug is empty after sanitizing — use letters, digits, dashes");
    }
    Ok(())
}
