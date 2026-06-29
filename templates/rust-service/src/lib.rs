//! A midian service (axum). These seams mirror the standard's canonical backend conventions:
//! `response` (BE-0002 envelope), `error` (BE-0003 `AppError`), `ids` (BE-0016), `auth`
//! (BE-0004 `RequireAuth`), `http` (BE-0010 pooled client), `tasks` (BE-0011 tracked work), and
//! `openapi` (BE-0014 generated contract). Feature code lives in `modules/<name>/` (BE-0001).
//!
//! Grow it with `midas add module <name>` (which scaffolds `modules/<name>/{mod,model,service,
//! handler}.rs` and wires `pub mod`); then register the handler in `openapi::router`. When the DB
//! is in use, switch a module's `query_as::<_, T>("…")` to the compile-checked `query_as!` and
//! commit the `.sqlx` offline cache (BE-0018). Auth/billing is Clerk (STK-0005) — see `auth`.

pub mod auth;
pub mod db;
pub mod error;
pub mod http;
pub mod ids;
pub mod modules;
pub mod openapi;
pub mod response;
pub mod routes;
pub mod tasks;
