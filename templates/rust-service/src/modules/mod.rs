//! Feature modules (BE-0001). Each `modules/<name>/` owns its `model` (wire DTOs), `service`
//! (data access / business logic), and `handler` (thin axum handlers). `midas touch module <name>`
//! scaffolds a new one and wires its `pub mod` here.

pub mod items;
