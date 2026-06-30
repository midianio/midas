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

/// Resolved flow configuration. Every field falls back to a midian default, so a fresh checkout runs
/// `midas flow` with no `[flow]` block. Active state is derived from the current git branch (no state
/// file), and the managed `.env.local` block is marked `# >>> midas >>>`.
#[derive(Debug, Clone)]
pub struct FlowConfig {
    pub trunk: String,
    pub org: String,
    pub db: String,
    pub parent: String,
    pub region: String,
    pub port: u16,
    pub api_env_local: String,
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
            env_marker: "midas".into(),
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

    /// The sqlx connection URL the migrate runner should use. Prefers the documented source of
    /// truth — `MYSQL_DATABASE_URL` (the go-sql-driver DSN form, normalized) — and falls back to the
    /// `[flow]` tunnel (`mysql://root@127.0.0.1:PORT/DB`). The runner refuses anything that isn't a
    /// local tunnel (OPS-0009: schema reaches prod only via a reviewed PlanetScale deploy request).
    pub fn migrate_url(&self) -> String {
        match std::env::var("MYSQL_DATABASE_URL") {
            Ok(dsn) if !dsn.trim().is_empty() => normalize_mysql_dsn(&dsn),
            _ => format!("mysql://root@127.0.0.1:{}/{}", self.port, self.db),
        }
    }
}

/// Convert a go-sql-driver DSN (`user[:pass]@tcp(host:port)/db?params`, as written into
/// `.env.local` by `local_db_url`) into the `mysql://user[:pass]@host:port/db` URL sqlx expects.
/// Query params are dropped — sqlx takes its own (TLS handled by the tunnel; DDL needs no tuning).
/// A string that is already a `mysql://` URL is returned unchanged.
pub fn normalize_mysql_dsn(dsn: &str) -> String {
    let dsn = dsn.trim();
    if dsn.starts_with("mysql://") {
        return dsn.split('?').next().unwrap_or(dsn).to_string();
    }
    // Split optional `user[:pass]@` credentials from the `tcp(host:port)/db?…` remainder.
    let (creds, rest) = match dsn.rsplit_once('@') {
        Some((c, r)) => (Some(c), r),
        None => (None, dsn),
    };
    // `tcp(host:port)/db?params` → `host:port` + `/db`.
    let rest = rest.strip_prefix("tcp(").unwrap_or(rest);
    let (host_port, tail) = rest.split_once(')').unwrap_or((rest, ""));
    let path = tail.split('?').next().unwrap_or(tail); // keep leading `/db`
    match creds {
        Some(c) => format!("mysql://{c}@{host_port}{path}"),
        None => format!("mysql://{host_port}{path}"),
    }
}

/// Whether a sqlx mysql URL points at the local loopback tunnel. The migrate runner is
/// dev/preview-only by construction (OPS-0004/OPS-0009); a remote host is refused.
pub fn is_local_mysql_url(url: &str) -> bool {
    let after = url.strip_prefix("mysql://").unwrap_or(url);
    let hostport = after.rsplit_once('@').map(|(_, h)| h).unwrap_or(after);
    let host = hostport
        .split('/')
        .next()
        .unwrap_or(hostport)
        .rsplit_once(':')
        .map(|(h, _)| h)
        .unwrap_or(hostport);
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_go_dsn_to_sqlx_url() {
        assert_eq!(
            normalize_mysql_dsn("root@tcp(127.0.0.1:3309)/application?tls=false&loc=UTC"),
            "mysql://root@127.0.0.1:3309/application"
        );
        assert_eq!(
            normalize_mysql_dsn("u:p@tcp(localhost:3306)/db"),
            "mysql://u:p@localhost:3306/db"
        );
    }

    #[test]
    fn normalize_passes_through_mysql_url_and_drops_params() {
        assert_eq!(
            normalize_mysql_dsn("mysql://root@127.0.0.1:3309/app?ssl-mode=DISABLED"),
            "mysql://root@127.0.0.1:3309/app"
        );
    }

    #[test]
    fn local_guard_accepts_loopback_rejects_remote() {
        assert!(is_local_mysql_url("mysql://root@127.0.0.1:3309/app?ssl-mode=DISABLED"));
        assert!(is_local_mysql_url("mysql://root@localhost:3309/app"));
        assert!(!is_local_mysql_url("mysql://user:pass@aws.connect.psdb.cloud:3306/app"));
    }
}
