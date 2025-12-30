// Read-only configuration endpoints

use axum::{extract::State, response::Json};
use serde_json::{json, Value};

use super::common::{ApiResponse, ApiResult};
use crate::web::AppState;

/// Get all node configurations (read-only view)
pub async fn get_all_node_configs(State(state): State<AppState>) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "nodes": state.config.nodes
    }))))
}

/// Get all Hermes configurations (read-only view)
pub async fn get_all_hermes_configs(State(state): State<AppState>) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "hermes": state.config.hermes
    }))))
}
