use super::model::Item;
use crate::ids;

/// List items for a user. In-memory demo data — a real module queries the DB:
///
/// ```ignore
/// pub async fn list(pool: &sqlx::MySqlPool, user_id: &str) -> Result<Vec<Item>, sqlx::Error> {
///     sqlx::query_as::<_, Item>("SELECT id, user_id, created_at FROM items WHERE user_id = ?")
///         .bind(user_id).fetch_all(pool).await
/// }
/// ```
///
/// (That's what `midas touch module` scaffolds. Switch to `query_as!` + the committed `.sqlx` cache
/// once you standardize on compile-checked queries — BE-0018.)
pub fn list(user_id: &str) -> Vec<Item> {
    vec![Item { id: ids::generate(), user_id: user_id.to_string(), created_at: 0 }]
}
