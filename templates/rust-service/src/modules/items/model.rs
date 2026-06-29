use serde::{Deserialize, Serialize};

/// API model for an Item. camelCase on the wire (BE-0008), snake_case in Rust; `ToSchema` puts it in
/// the generated OpenAPI contract (BE-0014). A DB-backed module adds `sqlx::FromRow` here.
#[derive(Debug, Default, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub user_id: String,
    pub created_at: i64,
}
