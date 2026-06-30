//! Items — a sample feature module showing the BE-0001 shape (model / service / handler). It's
//! in-memory so the service runs with no database; `midas touch module <name>` scaffolds the
//! DB-backed variant (sqlx `query_as` against the pool).

pub mod handler;
pub mod model;
pub mod service;
