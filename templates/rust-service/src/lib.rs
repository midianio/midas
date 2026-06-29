//! A midian service (axum). These seams mirror the standard's canonical backend conventions:
//! `response` (BE-0002 envelope), `error` (BE-0003 `AppError`), `ids` (BE-0016 id generation).
//!
//! Grow it with `midas add module <name>` (each module owns its routes/service/model) and mount the
//! module router in `routes::build`. When the service gains a database, add `sqlx` with the offline
//! query cache (BE-0018); when it exposes a contract, add `utoipa` OpenAPI generation (BE-0014).

pub mod error;
pub mod ids;
pub mod response;
pub mod routes;
