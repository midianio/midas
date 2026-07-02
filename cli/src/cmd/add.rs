//! Deterministic scaffolding for conventional pieces — stamps identical bytes so a human and an
//! agent produce the same file (SPEC §5). These are the piece-level kinds behind `midas touch`
//! (`touch module|state|migration|component`); `cmd::touch` is the front door.

use crate::core::exit::{CliError, CliResult};
use crate::core::{prompt_line, Ctx};
use crate::flow::config::slugify;
use serde_json::json;
use std::path::PathBuf;

/// `touch state` — a Svelte runes state singleton (FE-0001) in lib/state/<name>.svelte.ts.
pub fn state(ctx: &Ctx, name: Option<String>, dir: Option<String>, force: bool) -> CliResult {
    add_state(ctx, &repo_root(), name, dir, force)
}

/// `touch migration` — a forward-only numbered migration (OPS-0008) in db/migrations/NNN_<slug>.sql.
pub fn migration(ctx: &Ctx, slug: Option<String>, dir: Option<String>, force: bool) -> CliResult {
    add_migration(ctx, &repo_root(), slug, dir, force)
}

/// `touch component` — a Svelte 5 component (FE-0011) in lib/components/<Name>.svelte (--ui → components/ui).
pub fn component(
    ctx: &Ctx,
    name: Option<String>,
    dir: Option<String>,
    ui: bool,
    force: bool,
) -> CliResult {
    add_component(ctx, &repo_root(), name, dir, ui, force)
}

/// `touch module` — a backend feature module (BE-0001/0002/0004/0008): modules/<name>/{mod,handler,
/// service,model}.rs + a `pub mod <name>;` registration. Conformant-by-construction skeleton.
pub fn module(
    ctx: &Ctx,
    name: Option<String>,
    dir: Option<String>,
    no_wire: bool,
    force: bool,
) -> CliResult {
    add_module(ctx, &repo_root(), name, dir, no_wire, force)
}

fn repo_root() -> PathBuf {
    crate::proc::capture("git", &["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| ".".into()))
}

fn add_state(
    ctx: &Ctx,
    root: &std::path::Path,
    name: Option<String>,
    dir: Option<String>,
    force: bool,
) -> CliResult {
    let raw = match name {
        Some(n) => n,
        None => prompt_line(&ctx.out, &ctx.global, "Domain name", None)?,
    };
    let slug = slugify(&raw);
    if slug.is_empty() {
        return Err(CliError::usage("name must contain letters/digits"));
    }
    let pascal = pascal_case(&slug);
    let camel = camel_case(&slug);
    let dir = dir.unwrap_or_else(|| "app/web/src/lib/state".into());
    let rel = format!("{dir}/{slug}.svelte.ts");
    let path = root.join(&rel);

    if path.exists() && !force {
        return Err(CliError::expected(format!(
            "{rel} already exists (pass --force)"
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, state_template(&pascal, &camel))?;

    ctx.out.success(format!("created {rel}"));
    ctx.out.info(format!(
        "import {{ {camel} }} from \"$lib/state/{slug}.svelte\";"
    ));
    ctx.out.data(
        &json!({ "created": [rel], "class": format!("{pascal}Store"), "instance": camel }),
        |_| rel.clone(),
    );
    Ok(())
}

fn add_migration(
    ctx: &Ctx,
    root: &std::path::Path,
    slug: Option<String>,
    dir: Option<String>,
    force: bool,
) -> CliResult {
    let raw = match slug {
        Some(s) => s,
        None => prompt_line(&ctx.out, &ctx.global, "Migration slug", None)?,
    };
    let slug = slugify(&raw);
    if slug.is_empty() {
        return Err(CliError::usage("slug must contain letters/digits"));
    }
    let dir = dir.unwrap_or_else(|| "db/migrations".into());
    let mig_dir = root.join(&dir);
    let next = next_migration_number(&mig_dir);
    let num = format!("{next:03}");
    let rel = format!("{dir}/{num}_{slug}.sql");
    let path = root.join(&rel);

    if path.exists() && !force {
        return Err(CliError::expected(format!(
            "{rel} already exists (pass --force)"
        )));
    }
    std::fs::create_dir_all(&mig_dir)?;
    std::fs::write(&path, migration_template(&num, &slug))?;

    ctx.out.success(format!("created {rel}"));
    ctx.out
        .hint("forward-only — fix forward, never edit an applied migration in place");
    ctx.out
        .data(&json!({ "created": [rel], "number": next }), |_| {
            rel.clone()
        });
    Ok(())
}

fn add_component(
    ctx: &Ctx,
    root: &std::path::Path,
    name: Option<String>,
    dir: Option<String>,
    ui: bool,
    force: bool,
) -> CliResult {
    let raw = match name {
        Some(n) => n,
        None => prompt_line(&ctx.out, &ctx.global, "Component name", None)?,
    };
    let slug = slugify(&raw);
    if slug.is_empty() {
        return Err(CliError::usage("name must contain letters/digits"));
    }
    let pascal = pascal_case(&slug);
    let dir = dir.unwrap_or_else(|| {
        if ui {
            "app/web/src/lib/components/ui".into()
        } else {
            "app/web/src/lib/components".into()
        }
    });
    let rel = format!("{dir}/{pascal}.svelte");
    let path = root.join(&rel);

    if path.exists() && !force {
        return Err(CliError::expected(format!(
            "{rel} already exists (pass --force)"
        )));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, component_template(&pascal))?;

    ctx.out.success(format!("created {rel}"));
    ctx.out
        .data(&json!({ "created": [rel], "component": pascal }), |_| {
            rel.clone()
        });
    Ok(())
}

/// Highest `NNN_` prefix in the dir, plus one (1 when empty).
fn next_migration_number(dir: &std::path::Path) -> u32 {
    let mut max = 0u32;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(prefix) = name.split('_').next() {
                if prefix.len() == 3 {
                    if let Ok(n) = prefix.parse::<u32>() {
                        max = max.max(n);
                    }
                }
            }
        }
    }
    max + 1
}

fn pascal_case(slug: &str) -> String {
    slug.split(['-', '_'])
        .filter(|s| !s.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

fn camel_case(slug: &str) -> String {
    let p = pascal_case(slug);
    let mut c = p.chars();
    match c.next() {
        Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn snake_case(slug: &str) -> String {
    slug.replace('-', "_")
}

fn add_module(
    ctx: &Ctx,
    root: &std::path::Path,
    name: Option<String>,
    dir: Option<String>,
    no_wire: bool,
    force: bool,
) -> CliResult {
    let raw = match name {
        Some(n) => n,
        None => prompt_line(&ctx.out, &ctx.global, "Module name", None)?,
    };
    let slug = slugify(&raw);
    if slug.is_empty() {
        return Err(CliError::usage("name must contain letters/digits"));
    }
    let snake = snake_case(&slug);
    let pascal = pascal_case(&slug);
    let modules_dir = dir.unwrap_or_else(|| "app/api/src/modules".into());
    let mod_rel = format!("{modules_dir}/{snake}");
    let mod_path = root.join(&mod_rel);

    if mod_path.exists() && !force {
        return Err(CliError::expected(format!(
            "{mod_rel} already exists (pass --force)"
        )));
    }
    std::fs::create_dir_all(&mod_path)?;

    let files = [
        ("mod.rs", module_mod_rs(&pascal)),
        ("model.rs", module_model_rs(&pascal)),
        ("service.rs", module_service_rs(&snake, &pascal)),
        ("handler.rs", module_handler_rs(&snake, &pascal)),
    ];
    let mut created = Vec::new();
    for (file, body) in files {
        std::fs::write(mod_path.join(file), body)?;
        created.push(format!("{mod_rel}/{file}"));
    }

    // Wire `pub mod <snake>;` into modules/mod.rs (idempotent).
    let mut wired = false;
    let registry_rel = format!("{modules_dir}/mod.rs");
    let registry_path = root.join(&registry_rel);
    if !no_wire {
        if let Ok(existing) = std::fs::read_to_string(&registry_path) {
            let decl = format!("pub mod {snake};");
            if existing.contains(&decl) {
                ctx.out
                    .info(format!("{registry_rel} already declares {decl}"));
            } else {
                let next = insert_mod_decl(&existing, &decl);
                std::fs::write(&registry_path, next)?;
                wired = true;
                ctx.out.step(format!("wired {decl} into {registry_rel}"));
            }
        } else {
            ctx.out.warn(format!(
                "{registry_rel} not found — add `pub mod {snake};` yourself"
            ));
        }
    }

    ctx.out
        .success(format!("created module {snake} ({} files)", created.len()));
    ctx.out
        .hint("register routes in routes.rs and replace the TODO table/columns in service.rs");
    ctx.out.data(
        &json!({ "created": created, "wired": wired, "module": pascal }),
        |_| mod_rel.clone(),
    );
    Ok(())
}

/// Insert `decl` after the last `pub mod ` line (keeping the declaration block together), else at EOF.
fn insert_mod_decl(existing: &str, decl: &str) -> String {
    match existing.rmatch_indices("pub mod ").next() {
        Some((idx, _)) => {
            // find end of that line
            let line_end = existing[idx..]
                .find('\n')
                .map(|n| idx + n + 1)
                .unwrap_or(existing.len());
            format!("{}{decl}\n{}", &existing[..line_end], &existing[line_end..])
        }
        None => {
            let trimmed = existing.trim_end_matches('\n');
            format!("{trimmed}\n{decl}\n")
        }
    }
}

fn module_mod_rs(pascal: &str) -> String {
    [
        format!("//! {pascal} feature module."),
        "pub mod handler;".to_string(),
        "pub mod model;".to_string(),
        "pub mod service;".to_string(),
        String::new(),
    ]
    .join("\n")
}

fn module_model_rs(pascal: &str) -> String {
    [
        "use serde::{Deserialize, Serialize};".to_string(),
        String::new(),
        format!("/// API model for {pascal}. camelCase on the wire (BE-0008), snake_case in Rust."),
        "#[derive(Debug, Default, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]"
            .to_string(),
        "#[serde(rename_all = \"camelCase\")]".to_string(),
        format!("pub struct {pascal} {{"),
        "\tpub id: String,".to_string(),
        "\tpub user_id: String,".to_string(),
        "\tpub created_at: i64,".to_string(),
        "}".to_string(),
        String::new(),
    ]
    .join("\n")
}

fn module_service_rs(snake: &str, pascal: &str) -> String {
    [
        format!("use super::model::{pascal};"),
        "use sqlx::MySqlPool;".to_string(),
        String::new(),
        format!("/// List {snake} records for a user. Runtime-checked query (switch to `query_as!` +"),
        "/// committed `.sqlx` cache once standardized — BE-0018). Batch-hydrate related fields in".to_string(),
        "/// one set-based query, never N+1 (BE-0019).".to_string(),
        format!("pub async fn list_{snake}(pool: &MySqlPool, user_id: &str) -> Result<Vec<{pascal}>, sqlx::Error> {{"),
        format!("\tlet rows = sqlx::query_as::<_, {pascal}>("),
        format!("\t\t// TODO: real table + columns\n\t\t\"SELECT id, user_id, created_at FROM {snake} WHERE user_id = ? ORDER BY created_at DESC\","),
        "\t)".to_string(),
        "\t.bind(user_id)".to_string(),
        "\t.fetch_all(pool)".to_string(),
        "\t.await?;".to_string(),
        "\tOk(rows)".to_string(),
        "}".to_string(),
        String::new(),
    ]
    .join("\n")
}

fn module_handler_rs(snake: &str, pascal: &str) -> String {
    [
        format!("use super::model::{pascal};"),
        "use super::service;".to_string(),
        "use crate::auth::RequireAuth;".to_string(),
        "use crate::error::{AppError, ErrorBody};".to_string(),
        "use crate::response::{self, ApiResponse};".to_string(),
        "use crate::routes::AppState;".to_string(),
        "use axum::extract::State;".to_string(),
        "use axum::response::Response;".to_string(),
        String::new(),
        "// Thin handler (BE-0001): extract → call service → one envelope (BE-0002). Auth via the".to_string(),
        "// RequireAuth extractor (BE-0004); authz, when needed, via the central access::require seam.".to_string(),
        "#[utoipa::path(".to_string(),
        "\tget,".to_string(),
        format!("\tpath = \"/{snake}\","),
        format!("\ttag = \"{snake}\","),
        format!("\toperation_id = \"list{pascal}\","),
        "\tsecurity((\"clerk_jwt\" = [])),".to_string(),
        "\tresponses(".to_string(),
        format!("\t\t(status = 200, description = \"List {snake} for the authenticated user\", body = inline(ApiResponse<Vec<{pascal}>>)),"),
        "\t\t(status = 401, description = \"Missing or invalid auth\", body = ErrorBody),".to_string(),
        "\t\t(status = 500, description = \"Internal error\", body = ErrorBody),".to_string(),
        "\t),".to_string(),
        ")]".to_string(),
        "pub async fn list(State(st): State<AppState>, auth: RequireAuth) -> Result<Response, AppError> {".to_string(),
        "\tlet pool = st.db()?;".to_string(),
        format!("\tlet items = service::list_{snake}(pool, &auth.user_id).await?;"),
        "\tOk(response::ok_list(items))".to_string(),
        "}".to_string(),
        String::new(),
    ]
    .join("\n")
}

fn state_template(pascal: &str, camel: &str) -> String {
    let lines = [
        "/**".to_string(),
        format!(" * {pascal} — global state singleton (FE-0001). One class per domain, one exported instance."),
        " * Source-of-truth in `$state`; computed values in `$derived`; fetch/mutation/orchestration".to_string(),
        " * lives here (FE-0009), never in components. Reach other singletons by direct method call (FE-0008).".to_string(),
        " */".to_string(),
        format!("export class {pascal}Store {{"),
        "\t// source-of-truth".to_string(),
        "\topen = $state(false);".to_string(),
        String::new(),
        "\t// derived".to_string(),
        "\t// readonly isReady = $derived(...);".to_string(),
        String::new(),
        "\t// actions".to_string(),
        "\ttoggle() {".to_string(),
        "\t\tthis.open = !this.open;".to_string(),
        "\t}".to_string(),
        "}".to_string(),
        String::new(),
        format!("export const {camel} = new {pascal}Store();"),
        String::new(),
    ];
    lines.join("\n")
}

fn component_template(pascal: &str) -> String {
    let lines = [
        "<script lang=\"ts\">".to_string(),
        format!("\t// {pascal} — UI component (FE-0011). Keep logic in state/ (FE-0009); props in, events/methods out."),
        "\tinterface Props {".to_string(),
        "\t\tclass?: string;".to_string(),
        "\t}".to_string(),
        String::new(),
        "\tlet { class: className = \"\" }: Props = $props();".to_string(),
        "</script>".to_string(),
        String::new(),
        "<div class={className}>".to_string(),
        format!("\t<!-- TODO: {pascal} -->"),
        "</div>".to_string(),
        String::new(),
    ];
    lines.join("\n")
}

fn migration_template(num: &str, slug: &str) -> String {
    let lines = [
        format!("-- {num}_{slug}.sql"),
        "-- Forward-only migration (OPS-0008 / BE-0007). One DDL set per file; fix forward,".to_string(),
        "-- never edit an applied migration in place. Vitess: no FKs — enforce integrity in the access seam.".to_string(),
        String::new(),
        "-- TODO: write the DDL.".to_string(),
        String::new(),
    ];
    lines.join("\n")
}
