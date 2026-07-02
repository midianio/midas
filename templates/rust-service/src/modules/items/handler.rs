use super::model::Item;
use super::service;
use crate::auth::RequireAuth;
use crate::error::{AppError, ErrorBody};
use crate::response::{self, ApiResponse};
use crate::routes::AppState;
use axum::extract::State;
use axum::response::Response;

// Thin handler (BE-0001): extract → call service → one envelope (BE-0002). Auth via the RequireAuth
// extractor (BE-0004); authz, when needed, goes through a central access seam (not scattered checks).
#[utoipa::path(
    get,
    path = "/items",
    tag = "items",
    operation_id = "listItems",
    security(("clerk_jwt" = [])),
    responses(
        (status = 200, description = "List items for the authenticated user", body = inline(ApiResponse<Vec<Item>>)),
        (status = 401, description = "Missing or invalid auth", body = ErrorBody),
    ),
)]
pub async fn list(State(_st): State<AppState>, auth: RequireAuth) -> Result<Response, AppError> {
    // A DB-backed module would use `let pool = _st.db()?;` and `service::list(pool, &auth.user_id).await?`.
    let items = service::list(&auth.user_id);
    Ok(response::ok_list(items))
}
