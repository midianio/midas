//! The migration runner — midas owns this, replacing the external Go `db/cmd/migrate`. It applies the
//! forward-only `db/migrations/*.sql` files in lexical order against the local pscale tunnel, keeping
//! a `_migrations` ledger.
//!
//! The ledger contract is a **drop-in match for the retired Go runner** so the cutover is seamless on
//! branches and prod that already carry an applied ledger: the row key is the migration **filename**
//! (`001_init.sql`), and `applied_at` is unix-millis (BIGINT). We add one purely additive, nullable
//! column — `checksum` — and use it to enforce BE-0007 (editing an applied migration is rejected),
//! something the Go runner never did.
//!
//! Two deliberate choices honor the standard:
//! - **No transaction wrapper.** DDL is applied with `sqlx::raw_sql` and never wrapped in
//!   `BEGIN`/`COMMIT` — Vitess forbids DDL-in-transaction (OPS-0008). A mid-file failure leaves
//!   partial state and the ledger row is *not* written, so the fix is a new forward migration.
//! - **Checksum drift is rejected.** Editing an already-applied migration is caught and refused.

use sqlx::{Connection, MySqlConnection, Row};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Default location of the forward-only migration files (OPS-0008).
pub const MIGRATIONS_DIR: &str = "db/migrations";

/// One migration file discovered on disk.
#[derive(Debug, Clone)]
pub struct Migration {
    /// The ledger key — the file name, e.g. `001_init.sql` (matches the Go runner's `version`).
    pub version: String,
    pub sql: String,
    /// Hex SHA-256 of the file bytes.
    pub checksum: String,
}

/// State of one migration after a runner pass.
#[derive(Debug, Clone, serde::Serialize)]
pub struct MigrationState {
    pub version: String,
    pub applied: bool,
}

/// Outcome of a runner pass.
#[derive(Debug, Default, serde::Serialize)]
pub struct Report {
    /// File names applied during this pass (empty for `status`).
    pub newly_applied: Vec<String>,
    /// Full ordered list with applied flag (after the pass).
    pub states: Vec<MigrationState>,
}

/// Typed runner failure so the command layer can choose the right exit code.
#[derive(Debug)]
pub enum MigrateError {
    /// An applied migration changed — a clean "no" (exit 2).
    Drift(String),
    /// Connection / IO / SQL failure (exit 1).
    Failed(anyhow::Error),
}

impl From<sqlx::Error> for MigrateError {
    fn from(e: sqlx::Error) -> Self {
        MigrateError::Failed(e.into())
    }
}

type Result<T> = std::result::Result<T, MigrateError>;

// Drop-in match for the Go runner's ledger (`version VARCHAR(255) PK, applied_at BIGINT`), plus a
// nullable `checksum` we own. `IF NOT EXISTS` so it survives a fresh DB and an inherited table alike.
const LEDGER_DDL: &str = "CREATE TABLE IF NOT EXISTS _migrations (\
    version    VARCHAR(255) NOT NULL, \
    applied_at BIGINT       NOT NULL, \
    checksum   CHAR(64)     NULL, \
    PRIMARY KEY (version))";

/// Apply every pending migration, in order. Idempotent: a second run with nothing pending is a
/// clean no-op. Rejects checksum drift before applying anything.
pub async fn apply(url: &str, root: &Path) -> Result<Report> {
    let migs = discover(root)?;
    if migs.is_empty() {
        return Ok(Report::default());
    }
    let mut conn = connect(url).await?;
    ensure_ledger(&mut conn).await?;
    let applied = load_applied(&mut conn).await?;
    reconcile(&mut conn, &migs, &applied).await?;

    let mut done: BTreeSet<String> = applied.keys().cloned().collect();
    let mut newly_applied = Vec::new();
    for m in &migs {
        if done.contains(&m.version) {
            continue;
        }
        // No transaction wrapper — Vitess forbids DDL-in-transaction (OPS-0008).
        sqlx::raw_sql(&m.sql)
            .execute(&mut conn)
            .await
            .map_err(|e| MigrateError::Failed(anyhow::anyhow!("applying {}: {e}", m.version)))?;
        sqlx::query("INSERT INTO _migrations (version, applied_at, checksum) VALUES (?, ?, ?)")
            .bind(&m.version)
            .bind(now_millis())
            .bind(&m.checksum)
            .execute(&mut conn)
            .await
            .map_err(|e| MigrateError::Failed(anyhow::anyhow!("recording {}: {e}", m.version)))?;
        done.insert(m.version.clone());
        newly_applied.push(m.version.clone());
    }

    let states = states(&migs, &done);
    let _ = conn.close().await;
    Ok(Report {
        newly_applied,
        states,
    })
}

/// Read the ledger and report which migrations are applied vs pending. Read-only except for a
/// one-time checksum backfill of pre-existing rows (see [`reconcile`]).
pub async fn status(url: &str, root: &Path) -> Result<Report> {
    let migs = discover(root)?;
    if migs.is_empty() {
        return Ok(Report::default());
    }
    let mut conn = connect(url).await?;
    ensure_ledger(&mut conn).await?;
    let applied = load_applied(&mut conn).await?;
    reconcile(&mut conn, &migs, &applied).await?;
    let done: BTreeSet<String> = applied.keys().cloned().collect();
    let states = states(&migs, &done);
    let _ = conn.close().await;
    Ok(Report {
        newly_applied: Vec::new(),
        states,
    })
}

/// Read & parse `db/migrations/*.sql` under `root`, sorted by file name (matching the Go runner:
/// any `.sql` not starting with `_`). Empty when the dir is absent.
pub fn discover(root: &Path) -> Result<Vec<Migration>> {
    let dir = root.join(MIGRATIONS_DIR);
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(MigrateError::Failed(e.into())),
    };
    for entry in rd.flatten() {
        let version = entry.file_name().to_string_lossy().to_string();
        if !version.ends_with(".sql") || version.starts_with('_') {
            continue;
        }
        let sql =
            std::fs::read_to_string(entry.path()).map_err(|e| MigrateError::Failed(e.into()))?;
        let checksum = sha256_hex(sql.as_bytes());
        out.push(Migration {
            version,
            sql,
            checksum,
        });
    }
    out.sort_by(|a, b| a.version.cmp(&b.version));
    Ok(out)
}

// ---- internals ----------------------------------------------------------------------------------

async fn connect(url: &str) -> Result<MySqlConnection> {
    MySqlConnection::connect(url).await.map_err(|e| {
        MigrateError::Failed(anyhow::anyhow!(
            "connect {}: {e} — is the pscale tunnel up? (try `midas dev` or `pscale connect`)",
            redact(url)
        ))
    })
}

async fn ensure_ledger(conn: &mut MySqlConnection) -> Result<()> {
    sqlx::raw_sql(LEDGER_DDL).execute(&mut *conn).await?;
    // Cutover from the retired Go runner: its `_migrations` predates the checksum column. Add it
    // once, idempotently; `reconcile` then backfills the NULL checksums trust-on-first-sight.
    let has_checksum: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns \
         WHERE table_schema = DATABASE() AND table_name = '_migrations' AND column_name = 'checksum'",
    )
    .fetch_one(&mut *conn)
    .await?;
    if has_checksum == 0 {
        sqlx::raw_sql("ALTER TABLE _migrations ADD COLUMN checksum CHAR(64) NULL")
            .execute(&mut *conn)
            .await?;
    }
    Ok(())
}

/// `filename -> stored checksum` for every applied migration (checksum is `None` for legacy rows
/// the Go runner wrote without one).
async fn load_applied(conn: &mut MySqlConnection) -> Result<BTreeMap<String, Option<String>>> {
    let rows = sqlx::query("SELECT version, checksum FROM _migrations")
        .fetch_all(&mut *conn)
        .await?;
    let mut map = BTreeMap::new();
    for r in rows {
        let v: String = r.try_get("version")?;
        let c: Option<String> = r.try_get("checksum")?;
        map.insert(v, c);
    }
    Ok(map)
}

/// Reject any applied migration whose bytes changed (BE-0007). Backfills NULL checksums on legacy
/// rows (the Go-runner cutover) trust-on-first-sight, so a clean re-run doesn't re-flag them.
async fn reconcile(
    conn: &mut MySqlConnection,
    migs: &[Migration],
    applied: &BTreeMap<String, Option<String>>,
) -> Result<()> {
    for m in migs {
        match applied.get(&m.version) {
            Some(Some(stored)) if stored != &m.checksum => {
                return Err(MigrateError::Drift(format!(
                    "{} was modified after it was applied — fix forward, never edit an applied \
                     migration in place (BE-0007)",
                    m.version
                )));
            }
            Some(None) => {
                sqlx::query("UPDATE _migrations SET checksum = ? WHERE version = ?")
                    .bind(&m.checksum)
                    .bind(&m.version)
                    .execute(&mut *conn)
                    .await?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn states(migs: &[Migration], done: &BTreeSet<String>) -> Vec<MigrationState> {
    migs.iter()
        .map(|m| MigrationState {
            version: m.version.clone(),
            applied: done.contains(&m.version),
        })
        .collect()
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    let digest = Sha256::digest(bytes);
    let mut s = String::with_capacity(64);
    for b in digest {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Strip the password from a `mysql://user:pass@host/db` URL for safe display in errors.
fn redact(url: &str) -> String {
    match (url.find("://"), url.rfind('@')) {
        (Some(s), Some(at)) if at > s + 3 => {
            let user = url[s + 3..at].split(':').next().unwrap_or("");
            format!("{}{}{}", &url[..s + 3], user, &url[at..])
        }
        _ => url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::create_dir_all(dir.join(MIGRATIONS_DIR)).unwrap();
        std::fs::write(dir.join(MIGRATIONS_DIR).join(name), body).unwrap();
    }

    #[test]
    fn discover_orders_by_filename_and_keys_on_it() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "002_add_index.sql", "SELECT 2;");
        write(tmp.path(), "001_init.sql", "SELECT 1;");
        write(tmp.path(), "README.md", "not a migration");
        let migs = discover(tmp.path()).unwrap();
        assert_eq!(migs.len(), 2);
        assert_eq!(migs[0].version, "001_init.sql");
        assert_eq!(migs[1].version, "002_add_index.sql");
    }

    #[test]
    fn discover_skips_underscore_prefixed_like_the_go_runner() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "001_init.sql", "SELECT 1;");
        write(tmp.path(), "_seed.sql", "INSERT ...;");
        let migs = discover(tmp.path()).unwrap();
        assert_eq!(migs.len(), 1);
        assert_eq!(migs[0].version, "001_init.sql");
    }

    #[test]
    fn discover_missing_dir_is_empty_not_error() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(discover(tmp.path()).unwrap().is_empty());
    }

    #[test]
    fn checksum_is_stable_and_byte_sensitive() {
        assert_eq!(sha256_hex(b"abc"), sha256_hex(b"abc"));
        assert_ne!(sha256_hex(b"abc"), sha256_hex(b"abc "));
        assert_eq!(sha256_hex(b"abc").len(), 64);
    }

    #[test]
    fn redact_hides_password() {
        assert_eq!(
            redact("mysql://root:secret@127.0.0.1:3309/app"),
            "mysql://root@127.0.0.1:3309/app"
        );
        assert_eq!(
            redact("mysql://root@127.0.0.1:3309/app"),
            "mysql://root@127.0.0.1:3309/app"
        );
    }
}
