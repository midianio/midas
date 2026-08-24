//! The release/branch flow — a faithful Rust port of the Go `midflow` CLI, with midian-specific
//! config lifted into `[flow]` in `midas.toml` (defaults reproduce midflow exactly).

pub mod config;
pub mod docs;
pub mod env;
pub mod gh;
pub mod git;
pub mod migrate;
pub mod pr;
pub mod pscale;
pub mod release;

pub use config::FlowConfig;
