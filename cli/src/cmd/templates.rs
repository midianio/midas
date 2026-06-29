//! Runnable code skeletons, embedded into the binary (single static binary, no repo fetch — same
//! `include_str!` approach as the convention registry). `midas new` lays a profile's template down,
//! substituting the project tokens below.
//!
//! Tokens in a template body:
//! - `{{PKG}}`   → the crate/package name (`<name>-api`)
//! - `{{CRATE}}` → the Rust lib name / `use` path (`<name>_api`)

/// One file in a template: a path relative to the new project root + its (pre-substitution) body.
pub struct TemplateFile {
    pub path: &'static str,
    pub body: &'static str,
}

/// Substitute the project tokens for a concrete project name (a kebab-case slug).
pub fn render(body: &str, name: &str) -> String {
    let pkg = format!("{name}-api");
    let crate_name = format!("{}_api", name.replace('-', "_"));
    body.replace("{{PKG}}", &pkg)
        .replace("{{CRATE}}", &crate_name)
}

/// The `rust-service` skeleton: a minimal conformant axum service under `app/api/` (the canonical
/// backend layout the mechanical checks scope to). Demonstrates the BE-0002/BE-0003/BE-0016 seams.
pub const RUST_SERVICE: &[TemplateFile] = &[
    TemplateFile {
        path: "app/api/Cargo.toml",
        body: include_str!("../../../templates/rust-service/Cargo.toml"),
    },
    TemplateFile {
        path: "app/api/src/main.rs",
        body: include_str!("../../../templates/rust-service/src/main.rs"),
    },
    TemplateFile {
        path: "app/api/src/lib.rs",
        body: include_str!("../../../templates/rust-service/src/lib.rs"),
    },
    TemplateFile {
        path: "app/api/src/response.rs",
        body: include_str!("../../../templates/rust-service/src/response.rs"),
    },
    TemplateFile {
        path: "app/api/src/error.rs",
        body: include_str!("../../../templates/rust-service/src/error.rs"),
    },
    TemplateFile {
        path: "app/api/src/ids.rs",
        body: include_str!("../../../templates/rust-service/src/ids.rs"),
    },
    TemplateFile {
        path: "app/api/src/routes.rs",
        body: include_str!("../../../templates/rust-service/src/routes.rs"),
    },
];
