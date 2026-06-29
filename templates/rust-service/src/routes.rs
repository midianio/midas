//! Router assembly. Routes return through the `response` envelope (BE-0002) and fail with `AppError`
//! (BE-0003). Grow the service with `midas add module <name>` and mount each module's router here.

use axum::routing::get;
use axum::response::Response;
use axum::Router;

use crate::error::AppError;
use crate::{ids, response};

/// Shared application state handed to every handler. Add your pool, config, and seams as fields.
#[derive(Clone, Default)]
pub struct AppState {}

pub fn build(state: AppState) -> Router {
    Router::new()
        .route("/ping", get(ping))
        .route("/hello", get(hello))
        .with_state(state)
}

/// Liveness probe — passes immediately, before any dependency warms up.
async fn ping() -> &'static str {
    "pong"
}

/// Demo endpoint: a fresh id returned through the standard envelope. Replace with real modules.
async fn hello() -> Result<Response, AppError> {
    Ok(response::ok(serde_json::json!({ "id": ids::generate() })))
}
