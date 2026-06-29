//! Router assembly + the shared `AppState`. Documented routes come from the OpenAPI seam (so the
//! contract can't drift); liveness and the spec endpoint are added here. Grow the service with
//! `midas add module <name>`, then register its handler in `openapi::router`.

use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use sqlx::MySqlPool;

use crate::error::AppError;
use crate::http::Http;
use crate::tasks::Tasks;

/// Shared application state, handed to every handler. Clone freely — every field is cheap to clone.
#[derive(Clone)]
pub struct AppState {
    /// `None` until a database is configured/reachable (the server still starts — see `db`).
    pub pool: Option<MySqlPool>,
    /// Pooled outbound HTTP (BE-0010).
    pub http: Http,
    /// Tracked background work, drained at shutdown (BE-0011).
    pub tasks: Tasks,
}

impl AppState {
    /// The DB pool, or a clean 500 when no database is configured. DB-backed handlers call this.
    pub fn db(&self) -> Result<&MySqlPool, AppError> {
        self.pool
            .as_ref()
            .ok_or_else(|| AppError::Internal("no database configured".into()))
    }
}

pub fn build(state: AppState) -> Router {
    // The documented router (paths + schemas) comes from the OpenAPI seam; `api` is the assembled
    // spec, served at /openapi.json for downstream type generation.
    let (router, api) = crate::openapi::router().split_for_parts();
    let openapi_json = serde_json::to_string_pretty(&api).expect("serialize openapi spec");

    router
        .route("/ping", get(|| async { "pong" }))
        .route(
            "/openapi.json",
            get(move || {
                let body = openapi_json.clone();
                async move { ([(header::CONTENT_TYPE, "application/json")], body).into_response() }
            }),
        )
        .with_state(state)
}
