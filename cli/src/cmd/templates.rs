//! Runnable code skeletons, embedded into the binary (single static binary, no repo fetch — same
//! `include_str!` approach as the convention registry). `midas new` lays a profile's template down,
//! substituting the project tokens below.
//!
//! Tokens in a template body:
//! - `{{NAME}}`  → the project name (the kebab-case slug, used as-is)
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
    body.replace("{{NAME}}", name)
        .replace("{{PKG}}", &pkg)
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

/// The `svelte-app` skeleton: a minimal conformant SvelteKit app under `app/web/` (the canonical
/// frontend layout the mechanical checks scope to). Demonstrates the FE-0001 state singleton,
/// FE-0005 `api<T>()` wrapper, FE-0010 `generateId`, and FE-0012 platform detection.
pub const SVELTE_APP: &[TemplateFile] = &[
    TemplateFile {
        path: "app/web/package.json",
        body: include_str!("../../../templates/svelte-app/package.json"),
    },
    TemplateFile {
        path: "app/web/svelte.config.js",
        body: include_str!("../../../templates/svelte-app/svelte.config.js"),
    },
    TemplateFile {
        path: "app/web/vite.config.ts",
        body: include_str!("../../../templates/svelte-app/vite.config.ts"),
    },
    TemplateFile {
        path: "app/web/tsconfig.json",
        body: include_str!("../../../templates/svelte-app/tsconfig.json"),
    },
    TemplateFile {
        path: "app/web/src/app.html",
        body: include_str!("../../../templates/svelte-app/src/app.html"),
    },
    TemplateFile {
        path: "app/web/src/app.d.ts",
        body: include_str!("../../../templates/svelte-app/src/app.d.ts"),
    },
    TemplateFile {
        path: "app/web/src/lib/utils.ts",
        body: include_str!("../../../templates/svelte-app/src/lib/utils.ts"),
    },
    TemplateFile {
        path: "app/web/src/lib/api.ts",
        body: include_str!("../../../templates/svelte-app/src/lib/api.ts"),
    },
    TemplateFile {
        path: "app/web/src/lib/state/app.svelte.ts",
        body: include_str!("../../../templates/svelte-app/src/lib/state/app.svelte.ts"),
    },
    TemplateFile {
        path: "app/web/src/routes/+layout.ts",
        body: include_str!("../../../templates/svelte-app/src/routes/+layout.ts"),
    },
    TemplateFile {
        path: "app/web/src/routes/+page.svelte",
        body: include_str!("../../../templates/svelte-app/src/routes/+page.svelte"),
    },
];
