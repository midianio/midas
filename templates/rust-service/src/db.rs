//! Database pool (STK-0004: PlanetScale/Vitess MySQL). The pool lives in `AppState` as an
//! `Option` — the server starts even when the DB is unconfigured/unreachable, so liveness probes
//! pass while it warms up; a handler that needs it calls `AppState::db()` and gets a clean 500 if
//! it's absent.

use sqlx::mysql::MySqlPoolOptions;
use sqlx::MySqlPool;

/// Connect a bounded MySQL pool. Forward-only migrations (STK-0004) live in `db/migrations/`
/// (`midas add migration <slug>`).
pub async fn connect(url: &str) -> Result<MySqlPool, sqlx::Error> {
    MySqlPoolOptions::new().max_connections(5).connect(url).await
}
