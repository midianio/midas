//! OpenAPI contract seam (BE-0014). The spec is generated from each handler's `#[utoipa::path]`
//! annotation and the `ToSchema` derives on the DTOs — `utoipa-axum`'s `OpenApiRouter` auto-collects
//! every registered route's path *and* its referenced schemas, so the contract can't drift from the
//! routes. Adding a documented route to `router()` is the only step; downstream TypeScript types are
//! generated from `/openapi.json` (e.g. `openapi-typescript`), closing the type-drift gap.

use crate::routes::AppState;
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

/// Top-level API metadata + the shared bearer security scheme. Paths/schemas are NOT listed here —
/// they're collected from the handler annotations by `router()` below.
#[derive(OpenApi)]
#[openapi(
    info(title = "{{NAME}} API", version = "0.1.0"),
    modifiers(&SecurityAddon),
    tags((name = "items", description = "Sample feature module")),
)]
pub struct ApiDoc;

struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "clerk_jwt",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }
    }
}

/// The documented router. Register each module's handlers here via `routes!(…)`; their paths,
/// params, and response schemas come from the `#[utoipa::path]` annotations.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(crate::modules::items::handler::list))
}
