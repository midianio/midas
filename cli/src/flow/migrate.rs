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
    /// Pending files recorded without running, because the schema already carries them. For
    /// `status` these are the files the next `apply` would adopt.
    pub adopted: Vec<String>,
    /// Full ordered list with applied flag (after the pass).
    pub states: Vec<MigrationState>,
}

/// Typed runner failure so the command layer can choose the right exit code.
#[derive(Debug)]
pub enum MigrateError {
    /// An applied migration changed — a clean "no" (exit 2).
    Drift(String),
    /// A `stamp` named a file that isn't a migration on disk — a clean "no" (exit 2).
    Unknown(String),
    /// A pending migration is only partly in the schema — a clean "no" (exit 2).
    Partial(String),
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
    let mut adopted = Vec::new();
    let mut schema = None;
    for m in &migs {
        if done.contains(&m.version) {
            continue;
        }
        if schema.is_none() {
            schema = Some(load_schema(&mut conn).await?);
        }
        match classify(&m.sql, schema.as_ref().expect("loaded above")) {
            Verdict::Adopt => {
                record(&mut conn, m).await?;
                done.insert(m.version.clone());
                adopted.push(m.version.clone());
                continue;
            }
            Verdict::Partial(missing) => return Err(partial_error(&m.version, &missing)),
            Verdict::Run => {}
        }
        // No transaction wrapper — Vitess forbids DDL-in-transaction (OPS-0008).
        sqlx::raw_sql(&m.sql)
            .execute(&mut conn)
            .await
            .map_err(|e| MigrateError::Failed(anyhow::anyhow!("applying {}: {e}", m.version)))?;
        record(&mut conn, m).await?;
        done.insert(m.version.clone());
        newly_applied.push(m.version.clone());
        schema = None; // This file changed the schema; the next pending one must see that.
    }

    let states = states(&migs, &done);
    let _ = conn.close().await;
    Ok(Report {
        newly_applied,
        adopted,
        states,
    })
}

async fn record(conn: &mut MySqlConnection, m: &Migration) -> Result<()> {
    sqlx::query("INSERT INTO _migrations (version, applied_at, checksum) VALUES (?, ?, ?)")
        .bind(&m.version)
        .bind(now_millis())
        .bind(&m.checksum)
        .execute(&mut *conn)
        .await
        .map_err(|e| MigrateError::Failed(anyhow::anyhow!("recording {}: {e}", m.version)))?;
    Ok(())
}

fn partial_error(version: &str, missing: &[String]) -> MigrateError {
    MigrateError::Partial(format!(
        "{version} is partly in the schema (missing: {}) — it failed mid-file or the branch \
         drifted. Fix forward, or `midas migrate stamp {version}` if the schema is right",
        missing.join(", ")
    ))
}

/// Record `requested` migrations as applied **without executing them** — for a branch whose schema
/// already carries their effect (a PlanetScale deploy request just landed it, or a branch copy).
/// Issues no DDL, so it works on safe-migrations branches; the ledger must already exist.
pub async fn stamp(url: &str, root: &Path, requested: &[String]) -> Result<Report> {
    let migs = discover(root)?;
    let mut done = BTreeSet::new();
    plan_stamp(&migs, &done, requested)?;
    let mut conn = connect(url).await?;
    require_ledger(&mut conn).await?;
    let applied = load_applied(&mut conn).await?;
    reconcile(&mut conn, &migs, &applied).await?;

    done.extend(applied.keys().cloned());
    let mut newly_applied = Vec::new();
    for m in plan_stamp(&migs, &done, requested)? {
        record(&mut conn, m).await?;
        newly_applied.push(m.version.clone());
    }
    done.extend(newly_applied.iter().cloned());

    let states = states(&migs, &done);
    let _ = conn.close().await;
    Ok(Report {
        newly_applied,
        adopted: Vec::new(),
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
    let schema = load_schema(&mut conn).await?;
    let adopted = migs
        .iter()
        .filter(|m| !done.contains(&m.version))
        .filter(|m| matches!(classify(&m.sql, &schema), Verdict::Adopt))
        .map(|m| m.version.clone())
        .collect();
    let states = states(&migs, &done);
    let _ = conn.close().await;
    Ok(Report {
        newly_applied: Vec::new(),
        adopted,
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

/// `stamp`'s DDL-free stand-in for [`ensure_ledger`]: shared branches reject direct DDL.
async fn require_ledger(conn: &mut MySqlConnection) -> Result<()> {
    let has_checksum: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns \
         WHERE table_schema = DATABASE() AND table_name = '_migrations' AND column_name = 'checksum'",
    )
    .fetch_one(&mut *conn)
    .await?;
    if has_checksum == 0 {
        return Err(MigrateError::Failed(anyhow::anyhow!(
            "no `_migrations` ledger with a checksum column on the target — `stamp` never runs \
             DDL, so create it with `midas migrate status` on a branch that allows DDL"
        )));
    }
    Ok(())
}

/// Which `requested` files to record: each must exist on disk (a bare name or a
/// `db/migrations/…` path); ones already in the ledger are skipped so re-runs are no-ops.
fn plan_stamp<'a>(
    migs: &'a [Migration],
    done: &BTreeSet<String>,
    requested: &[String],
) -> Result<Vec<&'a Migration>> {
    let mut wanted = BTreeSet::new();
    for r in requested {
        let name = Path::new(r)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(r.as_str());
        if !migs.iter().any(|m| m.version == name) {
            return Err(MigrateError::Unknown(format!(
                "{name} is not a migration under {MIGRATIONS_DIR} — nothing stamped"
            )));
        }
        wanted.insert(name.to_string());
    }
    Ok(migs
        .iter()
        .filter(|m| wanted.contains(&m.version) && !done.contains(&m.version))
        .collect())
}

/// Every table, column and index on the target, for [`classify`]. Names are lowercased; `CAST`
/// because Vitess can hand `information_schema` strings back as binary.
async fn load_schema(conn: &mut MySqlConnection) -> Result<Schema> {
    let mut schema = Schema::default();
    let rows = sqlx::query(
        "SELECT CAST(table_name AS CHAR), CAST(column_name AS CHAR) \
         FROM information_schema.columns WHERE table_schema = DATABASE()",
    )
    .fetch_all(&mut *conn)
    .await?;
    for r in rows {
        let t: String = r.try_get(0)?;
        let c: String = r.try_get(1)?;
        let t = t.to_lowercase();
        schema.columns.insert((t.clone(), c.to_lowercase()));
        schema.tables.insert(t);
    }
    let rows = sqlx::query(
        "SELECT CAST(table_name AS CHAR), CAST(index_name AS CHAR) \
         FROM information_schema.statistics WHERE table_schema = DATABASE()",
    )
    .fetch_all(&mut *conn)
    .await?;
    for r in rows {
        let t: String = r.try_get(0)?;
        let i: String = r.try_get(1)?;
        schema.indexes.insert((t.to_lowercase(), i.to_lowercase()));
    }
    Ok(schema)
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

// --- Adoption -------------------------------------------------------------------------------
//
// A deploy request ships schema but never `_migrations` rows, and a seeded branch copies whatever
// its parent's ledger says, so a pending file can already be live. Replaying it dies on 1050/1060.
// `classify` compares what a file would do with what the target has, and only records it when
// every statement is provably done. Presence only: it doesn't compare column types.

/// The target's structure, lowercased.
#[derive(Debug, Default)]
struct Schema {
    tables: BTreeSet<String>,
    columns: BTreeSet<(String, String)>,
    indexes: BTreeSet<(String, String)>,
}

/// One checkable consequence of a statement.
#[derive(Debug, PartialEq)]
enum Effect {
    Table(String),
    NoTable(String),
    Column(String, String),
    NoColumn(String, String),
    Index(String, String),
    NoIndex(String, String),
}

impl Effect {
    fn holds(&self, s: &Schema) -> bool {
        let pair = |a: &String, b: &String| (a.clone(), b.clone());
        match self {
            Effect::Table(t) => s.tables.contains(t),
            Effect::NoTable(t) => !s.tables.contains(t),
            Effect::Column(t, c) => s.columns.contains(&pair(t, c)),
            Effect::NoColumn(t, c) => !s.columns.contains(&pair(t, c)),
            Effect::Index(t, i) => s.indexes.contains(&pair(t, i)),
            Effect::NoIndex(t, i) => !s.indexes.contains(&pair(t, i)),
        }
    }

    fn describe(&self) -> String {
        match self {
            Effect::Table(t) => format!("table {t}"),
            Effect::NoTable(t) => format!("drop of table {t}"),
            Effect::Column(t, c) => format!("column {t}.{c}"),
            Effect::NoColumn(t, c) => format!("drop of column {t}.{c}"),
            Effect::Index(t, i) => format!("index {t}.{i}"),
            Effect::NoIndex(t, i) => format!("drop of index {t}.{i}"),
        }
    }
}

/// What the runner should do with a pending file.
#[derive(Debug, PartialEq)]
enum Verdict {
    /// Nothing of it is live yet (or it can't be judged): execute it.
    Run,
    /// All of it is live: record it without executing.
    Adopt,
    /// Some is live and some isn't: replaying would fail, recording would lie.
    Partial(Vec<String>),
}

/// Judge a pending file against `schema`. `IF [NOT] EXISTS` statements can't conflict, so they
/// never count as evidence that the file already ran, and a file with statements we can't check
/// (data, `MODIFY`) is only ever `Partial` or `Run`: never adopted on a guess.
fn classify(sql: &str, schema: &Schema) -> Verdict {
    let mut held = 0;
    let mut missing = Vec::new();
    let mut opaque = false;
    for stmt in statements(sql) {
        let parsed = parse_statement(&stmt);
        opaque |= parsed.opaque;
        for e in parsed.effects {
            if !e.holds(schema) {
                missing.push(e.describe());
            } else if !parsed.idempotent {
                held += 1;
            }
        }
    }
    match (held, missing.is_empty(), opaque) {
        (0, _, _) => Verdict::Run,
        (_, true, false) => Verdict::Adopt,
        (_, true, true) => Verdict::Partial(vec!["statements midas can't check".to_string()]),
        (_, false, _) => Verdict::Partial(missing),
    }
}

/// Split a file into statements: comments stripped, split on `;` outside quotes.
fn statements(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut chars = sql.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                cur.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '\'' | '"' | '`' => {
                    quote = Some(c);
                    cur.push(c);
                }
                '-' if chars.peek() == Some(&'-') => {
                    for n in chars.by_ref() {
                        if n == '\n' {
                            break;
                        }
                    }
                    cur.push(' ');
                }
                '#' => {
                    for n in chars.by_ref() {
                        if n == '\n' {
                            break;
                        }
                    }
                    cur.push(' ');
                }
                '/' if chars.peek() == Some(&'*') => {
                    chars.next();
                    let mut prev = ' ';
                    for n in chars.by_ref() {
                        if prev == '*' && n == '/' {
                            break;
                        }
                        prev = n;
                    }
                    cur.push(' ');
                }
                ';' => {
                    out.push(std::mem::take(&mut cur));
                }
                _ => cur.push(c),
            },
        }
    }
    out.push(cur);
    out.retain(|s| !s.trim().is_empty());
    out
}

#[derive(Debug, Default)]
struct Parsed {
    effects: Vec<Effect>,
    idempotent: bool,
    opaque: bool,
}

/// Whitespace-separated tokens with `(`, `)` and `,` as their own tokens; backticks removed.
fn tokens(stmt: &str) -> Vec<String> {
    stmt.replace('`', "")
        .replace('(', " ( ")
        .replace(')', " ) ")
        .replace(',', " , ")
        .split_whitespace()
        .map(str::to_lowercase)
        .collect()
}

fn parse_statement(stmt: &str) -> Parsed {
    let t = tokens(stmt);
    let w: Vec<&str> = t.iter().map(String::as_str).collect();
    let opaque = Parsed {
        opaque: true,
        ..Parsed::default()
    };
    match w.as_slice() {
        ["create", "table", "if", "not", "exists", name, ..] => Parsed {
            effects: vec![Effect::Table(bare(name))],
            idempotent: true,
            opaque: false,
        },
        ["create", "table", name, ..] => Parsed {
            effects: vec![Effect::Table(bare(name))],
            ..Parsed::default()
        },
        ["drop", "table", "if", "exists", names @ ..] => Parsed {
            effects: table_list(names).into_iter().map(Effect::NoTable).collect(),
            idempotent: true,
            opaque: false,
        },
        ["drop", "table", names @ ..] => Parsed {
            effects: table_list(names).into_iter().map(Effect::NoTable).collect(),
            ..Parsed::default()
        },
        ["create", "unique", "index", i, "on", t, ..] | ["create", "index", i, "on", t, ..] => {
            Parsed {
                effects: vec![Effect::Index(bare(t), bare(i))],
                ..Parsed::default()
            }
        }
        ["drop", "index", i, "on", t, ..] => Parsed {
            effects: vec![Effect::NoIndex(bare(t), bare(i))],
            ..Parsed::default()
        },
        ["alter", "table", t, rest @ ..] => parse_alter(&bare(t), rest).unwrap_or(opaque),
        _ => opaque,
    }
}

/// `DROP TABLE a, b` names, stopping at any trailing `RESTRICT`/`CASCADE`.
fn table_list(names: &[&str]) -> Vec<String> {
    names
        .iter()
        .filter(|n| **n != ",")
        .take_while(|n| !matches!(**n, "restrict" | "cascade"))
        .map(|n| bare(n))
        .collect()
}

/// `db.table` → `table`.
fn bare(name: &str) -> String {
    name.rsplit('.').next().unwrap_or(name).to_string()
}

/// Effects of an `ALTER TABLE`'s comma-separated clauses, or `None` if any clause isn't checkable.
fn parse_alter(table: &str, rest: &[&str]) -> Option<Parsed> {
    let mut effects = Vec::new();
    let mut depth = 0usize;
    let mut clause: Vec<&str> = Vec::new();
    for w in rest.iter().copied().chain(std::iter::once(",")) {
        match w {
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "," if depth == 0 => {
                effects.push(parse_clause(table, &clause)?);
                clause.clear();
                continue;
            }
            _ => {}
        }
        clause.push(w);
    }
    Some(Parsed {
        effects,
        ..Parsed::default()
    })
}

fn parse_clause(t: &str, c: &[&str]) -> Option<Effect> {
    let t = t.to_string();
    match c {
        ["add", "unique" | "fulltext", "index" | "key", i, ..]
        | ["add", "index" | "key" | "unique" | "fulltext", i, ..] => {
            Some(Effect::Index(t, bare(i)))
        }
        ["add", "column", "(", ..] | ["add", "(", ..] | ["add", "constraint", ..] => None,
        ["add", "primary", ..] | ["add", "foreign", ..] | ["add", "check", ..] => None,
        ["add", "column", col, ..] | ["add", col, ..] => Some(Effect::Column(t, bare(col))),
        ["drop", "index" | "key", i] => Some(Effect::NoIndex(t, bare(i))),
        ["drop", "column", col] => Some(Effect::NoColumn(t, bare(col))),
        ["drop", "primary", ..] | ["drop", "foreign", ..] | ["drop", "check", ..] => None,
        ["drop", col] => Some(Effect::NoColumn(t, bare(col))),
        _ => None,
    }
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
    fn plan_stamp_accepts_paths_and_skips_recorded() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "001_init.sql", "SELECT 1;");
        write(tmp.path(), "002_a.sql", "SELECT 2;");
        write(tmp.path(), "003_b.sql", "SELECT 3;");
        let migs = discover(tmp.path()).unwrap();
        let done = BTreeSet::from(["001_init.sql".to_string()]);
        let req = vec![
            "db/migrations/003_b.sql".to_string(),
            "001_init.sql".to_string(),
        ];
        let plan = plan_stamp(&migs, &done, &req).unwrap();
        let names: Vec<_> = plan.iter().map(|m| m.version.as_str()).collect();
        assert_eq!(names, ["003_b.sql"]);
    }

    #[test]
    fn plan_stamp_rejects_unknown_files() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "001_init.sql", "SELECT 1;");
        let migs = discover(tmp.path()).unwrap();
        let err = plan_stamp(&migs, &BTreeSet::new(), &["099_nope.sql".to_string()]);
        assert!(matches!(err, Err(MigrateError::Unknown(_))));
    }

    fn schema(tables: &[&str], columns: &[(&str, &str)], indexes: &[(&str, &str)]) -> Schema {
        let own = |p: &[(&str, &str)]| {
            p.iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect()
        };
        Schema {
            tables: tables.iter().map(|t| t.to_string()).collect(),
            columns: own(columns),
            indexes: own(indexes),
        }
    }

    // The shape of the real 046_slide-decks.sql that kept failing on seeded branches.
    const TABLES_AND_COLUMN: &str = "-- 046 comment; with a semicolon\n\
        CREATE TABLE `slide_decks` (\n  id VARCHAR(64) NOT NULL,\n  PRIMARY KEY (id)\n);\n\
        CREATE TABLE `slide_style_memories` (id VARCHAR(64) NOT NULL);\n\
        ALTER TABLE `users` ADD COLUMN `referral_bonus_slides` INT NOT NULL DEFAULT 0;\n";

    #[test]
    fn adopts_a_file_that_is_fully_live() {
        let s = schema(
            &["slide_decks", "slide_style_memories", "users"],
            &[("users", "referral_bonus_slides")],
            &[],
        );
        assert_eq!(classify(TABLES_AND_COLUMN, &s), Verdict::Adopt);
    }

    #[test]
    fn runs_a_file_that_is_not_live_at_all() {
        let s = schema(&["users"], &[("users", "id")], &[]);
        assert_eq!(classify(TABLES_AND_COLUMN, &s), Verdict::Run);
    }

    #[test]
    fn refuses_a_half_applied_file_and_names_what_is_missing() {
        let s = schema(&["slide_decks", "users"], &[], &[]);
        match classify(TABLES_AND_COLUMN, &s) {
            Verdict::Partial(missing) => assert_eq!(
                missing,
                [
                    "table slide_style_memories",
                    "column users.referral_bonus_slides"
                ]
            ),
            v => panic!("expected Partial, got {v:?}"),
        }
    }

    #[test]
    fn multi_clause_alter_needs_every_clause() {
        let sql = "ALTER TABLE `users`\n  ADD COLUMN `a` text NULL,\n  \
                   ADD COLUMN `b` decimal(10, 2) NULL,\n  ADD INDEX idx_a (a, b);";
        let all = schema(
            &["users"],
            &[("users", "a"), ("users", "b")],
            &[("users", "idx_a")],
        );
        assert_eq!(classify(sql, &all), Verdict::Adopt);
        let some = schema(&["users"], &[("users", "a")], &[]);
        assert!(matches!(classify(sql, &some), Verdict::Partial(_)));
    }

    #[test]
    fn index_spellings_all_name_the_index() {
        let sql = "ALTER TABLE t ADD FULLTEXT INDEX ft (a), ADD UNIQUE KEY uq (b), ADD KEY k (c);";
        let s = schema(&["t"], &[], &[("t", "ft"), ("t", "uq"), ("t", "k")]);
        assert_eq!(classify(sql, &s), Verdict::Adopt);
    }

    #[test]
    fn drops_are_live_once_the_thing_is_gone() {
        let sql = "ALTER TABLE prayers DROP INDEX idx_list, DROP COLUMN list_id;";
        let gone = schema(&["prayers"], &[("prayers", "id")], &[]);
        assert_eq!(classify(sql, &gone), Verdict::Adopt);
        let there = schema(
            &["prayers"],
            &[("prayers", "list_id")],
            &[("prayers", "idx_list")],
        );
        assert_eq!(classify(sql, &there), Verdict::Run);
    }

    #[test]
    fn if_exists_statements_are_not_evidence_the_file_ran() {
        // Vacuously true on a branch that never had the table; 099_drop-dead-tables has this shape.
        let sql = "DROP TABLE IF EXISTS old; CREATE TABLE fresh (id INT);";
        assert_eq!(classify(sql, &schema(&[], &[], &[])), Verdict::Run);
        let only_guarded = "DROP TABLE IF EXISTS old;";
        assert_eq!(classify(only_guarded, &schema(&[], &[], &[])), Verdict::Run);
    }

    #[test]
    fn guarded_statements_still_block_adoption_when_unmet() {
        let sql = "DROP TABLE IF EXISTS old; ALTER TABLE t ADD COLUMN c INT;";
        let s = schema(&["old", "t"], &[("t", "c")], &[]);
        assert!(matches!(classify(sql, &s), Verdict::Partial(_)));
    }

    #[test]
    fn data_statements_are_never_adopted_on_a_guess() {
        let backfill = "UPDATE users SET a = 1;";
        assert_eq!(classify(backfill, &schema(&[], &[], &[])), Verdict::Run);
        let mixed = "ALTER TABLE users ADD COLUMN a INT; UPDATE users SET a = 1;";
        let s = schema(&["users"], &[("users", "a")], &[]);
        assert!(matches!(classify(mixed, &s), Verdict::Partial(_)));
    }

    #[test]
    fn statements_ignores_semicolons_in_comments_and_strings() {
        let got = statements("-- a; b\nINSERT INTO t VALUES ('x;y');\n/* c; d */\nSELECT 1;\n");
        assert_eq!(got.len(), 2);
        assert!(got[0].contains("'x;y'"));
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
