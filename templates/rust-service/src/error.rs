//! One error type for the whole service (BE-0003). Handlers return `Result<_, AppError>` and use
//! `?`; an unhandled error becomes a compile error, not a runtime panic. `IntoResponse` maps each
//! variant to a generic client message + status — internal detail is logged server-side, never put
//! on the wire.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("bad request")]
    BadRequest(String),
    #[error("internal error")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // `detail` is server-side only — logged for 5xx, never sent to the client.
        let (status, text, detail): (StatusCode, &'static str, Option<String>) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", None),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden", None),
            AppError::NotFound => (StatusCode::NOT_FOUND, "not found", None),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, "bad request", Some(m.clone())),
            AppError::Internal(m) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal server error", Some(m.clone()))
            }
        };

        if status.is_server_error() {
            if let Some(d) = &detail {
                tracing::error!("{text}: {d}");
            }
        }

        (status, Json(json!({ "status": text, "code": status.as_u16() }))).into_response()
    }
}
