// Operation tracking and management endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use tracing::{error, info};

use super::common::{ApiResponse, ApiResult, EmergencyCleanupQuery};
use crate::operation_tracker::OperationStatus;
use crate::web::AppState;

/// Get all active operations
pub async fn get_active_operations(State(state): State<AppState>) -> ApiResult<OperationStatus> {
    let operations = state.http_agent_manager.get_active_operations().await;
    Ok(Json(ApiResponse::success(operations)))
}

/// Cancel an operation for a target
pub async fn cancel_operation(
    Path(target_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!("Operation cancellation requested for: {}", target_name);

    match state
        .http_agent_manager
        .cancel_operation(&target_name)
        .await
    {
        Ok(_) => {
            info!("Operation cancelled successfully for {}", target_name);
            Ok(Json(ApiResponse::success(json!({
                "message": format!("Operation cancelled for {}", target_name)
            }))))
        }
        Err(e) => {
            error!("Failed to cancel operation for {}: {}", target_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

/// Emergency cleanup of stuck operations
pub async fn emergency_cleanup_operations(
    Query(query): Query<EmergencyCleanupQuery>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!(
        "Emergency cleanup requested for operations older than {} hours",
        query.max_hours
    );

    let cleaned_count = state
        .http_agent_manager
        .emergency_cleanup_operations(query.max_hours)
        .await;

    Ok(Json(ApiResponse::success(json!({
        "message": format!("Emergency cleanup completed: {} operations removed", cleaned_count),
        "cleaned_count": cleaned_count
    }))))
}

/// Check status of a specific target
pub async fn check_target_status(
    Path(target_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    let is_busy = state.http_agent_manager.is_target_busy(&target_name).await;
    let active_operation = if is_busy {
        state
            .http_agent_manager
            .operation_tracker
            .get_active_operation(&target_name)
            .await
    } else {
        None
    };

    Ok(Json(ApiResponse::success(json!({
        "target_name": target_name,
        "is_busy": is_busy,
        "active_operation": active_operation
    }))))
}
