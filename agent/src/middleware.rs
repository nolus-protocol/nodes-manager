//! Middleware for the agent server
//!
//! Provides authentication and other cross-cutting concerns.

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
};
use std::sync::Arc;

use crate::AppState;

/// Extractor that validates the API key from the Authorization header.
/// Use this instead of manually calling validate_api_key in each handler.
///
/// # Example
/// ```ignore
/// async fn my_handler(
///     _auth: ApiKeyAuth,  // This validates the API key
///     State(state): State<Arc<AppState>>,
///     Json(request): Json<MyRequest>,
/// ) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
///     // Handler logic here - API key is already validated
/// }
/// ```
pub struct ApiKeyAuth;

impl FromRequestParts<Arc<AppState>> for ApiKeyAuth {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "));

        match auth_header {
            Some(token) if token == state.api_key => Ok(ApiKeyAuth),
            _ => Err(StatusCode::UNAUTHORIZED),
        }
    }
}
