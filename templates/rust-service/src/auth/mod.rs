//! Auth extractor (BE-0004). Add `auth: RequireAuth` to a handler to require an authenticated
//! caller; it yields `{ user_id, sid }`. There is no fallback identity — any failure is a 401.
//!
//! TODO(auth): this is a DEV STUB — it trusts the bearer token's presence WITHOUT verifying the
//! signature. The midian standard is **Clerk** for auth and billing (STK-0005). Before production,
//! verify the JWT locally: RS256 against Clerk's JWKS (cache the keyset in `AppState`, refetch on a
//! `kid` miss), validate the issuer by `https://clerk.` / `.clerk.accounts` prefix, and disable
//! `aud` validation. See `standards/backend/authorization.md`. Fetch JWKS through the `Http` seam.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;

use crate::error::AppError;

/// The authenticated caller. `user_id` is the verified subject; `sid` is the session id when present.
pub struct RequireAuth {
    pub user_id: String,
    pub sid: Option<String>,
}

impl<S> FromRequestParts<S> for RequireAuth
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let token = extract_token(&parts.headers).ok_or(AppError::Unauthorized)?;
        // DEV STUB: accept any non-empty token and treat it as the user id. Replace with real
        // verification (see the module TODO) before this is exposed to untrusted callers.
        Ok(RequireAuth { user_id: token, sid: None })
    }
}

/// Pull the session token from `Authorization: Bearer …`, falling back to the `__session` cookie
/// (Clerk's browser cookie).
fn extract_token(headers: &axum::http::HeaderMap) -> Option<String> {
    if let Some(auth) = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
    {
        let token = auth.strip_prefix("Bearer ").unwrap_or(auth).trim();
        if !token.is_empty() {
            return Some(token.to_string());
        }
    }
    let cookie = headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())?;
    for part in cookie.split(';') {
        if let Some(val) = part.trim().strip_prefix("__session=") {
            return Some(val.to_string());
        }
    }
    None
}
