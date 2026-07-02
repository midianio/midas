//! Runnable code skeletons, embedded into the binary (single static binary, no repo fetch — same
//! `include_str!` approach as the convention registry). `midas touch project` lays a profile's template down,
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

/// Terse constructor for the template tables below.
const fn tf(path: &'static str, body: &'static str) -> TemplateFile {
    TemplateFile { path, body }
}

/// Substitute the project tokens for a concrete project name (a kebab-case slug).
pub fn render(body: &str, name: &str) -> String {
    let pkg = format!("{name}-api");
    let crate_name = format!("{}_api", name.replace('-', "_"));
    body.replace("{{NAME}}", name)
        .replace("{{PKG}}", &pkg)
        .replace("{{CRATE}}", &crate_name)
}

/// The `rust-service` skeleton: a conformant axum service under `app/api/` (the canonical backend
/// layout the mechanical checks scope to). Demonstrates the seams — `response` (BE-0002), `error`
/// (BE-0003), `ids` (BE-0016), `auth`/`RequireAuth` (BE-0004), `http` (BE-0010), `tasks` (BE-0011),
/// `openapi` (BE-0014) — plus a sample `modules/items` feature module (BE-0001).
pub const RUST_SERVICE: &[TemplateFile] = &[
    tf(
        "app/api/Cargo.toml",
        include_str!("../../../templates/rust-service/Cargo.toml"),
    ),
    tf(
        "app/api/src/main.rs",
        include_str!("../../../templates/rust-service/src/main.rs"),
    ),
    tf(
        "app/api/src/lib.rs",
        include_str!("../../../templates/rust-service/src/lib.rs"),
    ),
    tf(
        "app/api/src/response.rs",
        include_str!("../../../templates/rust-service/src/response.rs"),
    ),
    tf(
        "app/api/src/error.rs",
        include_str!("../../../templates/rust-service/src/error.rs"),
    ),
    tf(
        "app/api/src/ids.rs",
        include_str!("../../../templates/rust-service/src/ids.rs"),
    ),
    tf(
        "app/api/src/http.rs",
        include_str!("../../../templates/rust-service/src/http.rs"),
    ),
    tf(
        "app/api/src/tasks.rs",
        include_str!("../../../templates/rust-service/src/tasks.rs"),
    ),
    tf(
        "app/api/src/db.rs",
        include_str!("../../../templates/rust-service/src/db.rs"),
    ),
    tf(
        "app/api/src/auth/mod.rs",
        include_str!("../../../templates/rust-service/src/auth/mod.rs"),
    ),
    tf(
        "app/api/src/openapi.rs",
        include_str!("../../../templates/rust-service/src/openapi.rs"),
    ),
    tf(
        "app/api/src/routes.rs",
        include_str!("../../../templates/rust-service/src/routes.rs"),
    ),
    tf(
        "app/api/src/modules/mod.rs",
        include_str!("../../../templates/rust-service/src/modules/mod.rs"),
    ),
    tf(
        "app/api/src/modules/items/mod.rs",
        include_str!("../../../templates/rust-service/src/modules/items/mod.rs"),
    ),
    tf(
        "app/api/src/modules/items/model.rs",
        include_str!("../../../templates/rust-service/src/modules/items/model.rs"),
    ),
    tf(
        "app/api/src/modules/items/service.rs",
        include_str!("../../../templates/rust-service/src/modules/items/service.rs"),
    ),
    tf(
        "app/api/src/modules/items/handler.rs",
        include_str!("../../../templates/rust-service/src/modules/items/handler.rs"),
    ),
    // Forward-only migrations live at the project root (OPS-0008), applied by `midas migrate`.
    tf(
        "db/migrations/001_init.sql",
        include_str!("../../../templates/rust-service/db/migrations/001_init.sql"),
    ),
];

/// The `svelte-app` skeleton: a conformant SvelteKit app under `app/web/` (the canonical frontend
/// layout the mechanical checks scope to). Demonstrates the seams — `state/app` + `state/auth`
/// (FE-0001 runes singletons), `api<T>()` (FE-0005) with the auth token provider, `utils.generateId`
/// (FE-0010) + platform detection (FE-0012), a `ui/Button` component (FE-0011) — plus the
/// `(public)` (SSR'd) / `app` (SPA) route-group split.
pub const SVELTE_APP: &[TemplateFile] = &[
    tf(
        "app/web/package.json",
        include_str!("../../../templates/svelte-app/package.json"),
    ),
    tf(
        "app/web/svelte.config.js",
        include_str!("../../../templates/svelte-app/svelte.config.js"),
    ),
    tf(
        "app/web/vite.config.ts",
        include_str!("../../../templates/svelte-app/vite.config.ts"),
    ),
    tf(
        "app/web/tsconfig.json",
        include_str!("../../../templates/svelte-app/tsconfig.json"),
    ),
    tf(
        "app/web/src/app.html",
        include_str!("../../../templates/svelte-app/src/app.html"),
    ),
    tf(
        "app/web/src/app.d.ts",
        include_str!("../../../templates/svelte-app/src/app.d.ts"),
    ),
    tf(
        "app/web/src/lib/utils.ts",
        include_str!("../../../templates/svelte-app/src/lib/utils.ts"),
    ),
    tf(
        "app/web/src/lib/api.ts",
        include_str!("../../../templates/svelte-app/src/lib/api.ts"),
    ),
    tf(
        "app/web/src/lib/state/app.svelte.ts",
        include_str!("../../../templates/svelte-app/src/lib/state/app.svelte.ts"),
    ),
    tf(
        "app/web/src/lib/state/auth.svelte.ts",
        include_str!("../../../templates/svelte-app/src/lib/state/auth.svelte.ts"),
    ),
    tf(
        "app/web/src/lib/components/ui/Button.svelte",
        include_str!("../../../templates/svelte-app/src/lib/components/ui/Button.svelte"),
    ),
    tf(
        "app/web/src/routes/(public)/+layout.ts",
        include_str!("../../../templates/svelte-app/src/routes/(public)/+layout.ts"),
    ),
    tf(
        "app/web/src/routes/(public)/+page.svelte",
        include_str!("../../../templates/svelte-app/src/routes/(public)/+page.svelte"),
    ),
    tf(
        "app/web/src/routes/app/+layout.ts",
        include_str!("../../../templates/svelte-app/src/routes/app/+layout.ts"),
    ),
    tf(
        "app/web/src/routes/app/+page.svelte",
        include_str!("../../../templates/svelte-app/src/routes/app/+page.svelte"),
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Every file under `templates/` must be embedded in one of the tables above. `include_str!`
    /// already breaks the build when an embedded file is deleted/renamed; this guards the other
    /// direction — a file *added* to a template dir but never wired into the table would silently
    /// not ship in the binary.
    #[test]
    fn every_template_file_on_disk_is_embedded() {
        let templates_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../templates");
        let embedded: Vec<&TemplateFile> = RUST_SERVICE.iter().chain(SVELTE_APP.iter()).collect();

        let mut missing = Vec::new();
        let mut on_disk = 0usize;
        for entry in ignore::WalkBuilder::new(&templates_root)
            .hidden(true)
            .git_ignore(true)
            .build()
            .flatten()
        {
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            // Build artifacts from manually building a template in place are not template files.
            let rel = entry.path().strip_prefix(&templates_root).unwrap();
            let rel_s = rel.to_string_lossy().replace('\\', "/");
            if rel_s.contains("/target/")
                || rel_s.contains("/node_modules/")
                || rel_s.ends_with("Cargo.lock")
                || rel_s.ends_with("bun.lock")
            {
                continue;
            }
            on_disk += 1;
            let body = std::fs::read_to_string(entry.path()).unwrap();
            if !embedded.iter().any(|t| t.body == body) {
                missing.push(rel_s);
            }
        }
        assert!(on_disk > 0, "templates/ not found next to the cli crate");
        assert!(
            missing.is_empty(),
            "template files on disk but not embedded in cmd/templates.rs: {missing:?}"
        );
    }
}
