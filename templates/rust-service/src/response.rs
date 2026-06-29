//! The one success envelope every JSON endpoint returns through (BE-0002).
//!
//! Wire shape: `{ "data": …, "code": 200, "timestamp": <RFC3339>, "count": N }` — `count` is the
//! list length for arrays, else 1. Returning through `ok` / `ok_list` keeps the shape identical
//! across every handler, so it cannot drift endpoint-by-endpoint.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// The success envelope wrapping every JSON payload.
#[derive(Serialize)]
pub struct ApiResponse<T> {
    /// The payload (an object for detail endpoints, an array for list endpoints).
    pub data: T,
    /// HTTP status code, echoed in the body.
    pub code: u16,
    /// RFC3339 server timestamp (seconds precision, local offset).
    pub timestamp: String,
    /// Item count: list length for arrays, else 1.
    pub count: usize,
}

fn now_rfc3339() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

/// Single-object response — `count` = 1.
pub fn ok<T: Serialize>(data: T) -> Response {
    (
        StatusCode::OK,
        Json(ApiResponse { data, code: 200, timestamp: now_rfc3339(), count: 1 }),
    )
        .into_response()
}

/// List response — `count` = items.len().
pub fn ok_list<T: Serialize>(items: Vec<T>) -> Response {
    let count = items.len();
    (
        StatusCode::OK,
        Json(ApiResponse { data: items, code: 200, timestamp: now_rfc3339(), count }),
    )
        .into_response()
}
