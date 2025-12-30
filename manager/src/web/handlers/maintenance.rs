// Manual maintenance operation endpoints

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use tracing::{error, info};

use super::common::{ApiResponse, ApiResult};
use crate::web::AppState;

/// Manual node restart via OperationExecutor
pub async fn execute_manual_node_restart(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!("Manual node restart requested for: {}", node_name);

    let node_name_clone = node_name.clone();
    let http_manager = state.http_agent_manager.clone();

    match state
        .operation_executor
        .execute_async("node_restart", &node_name, move || {
            let http_manager = http_manager.clone();
            let node_name = node_name_clone.clone();
            async move { http_manager.restart_node(&node_name).await }
        })
        .await
    {
        Ok(operation_id) => {
            info!("Node {} restart started: {}", node_name, operation_id);
            Ok(Json(ApiResponse::success(json!({
                "message": format!("Node {} restart started successfully", node_name),
                "operation_id": operation_id,
                "node_name": node_name,
                "status": "started"
            }))))
        }
        Err(e) => {
            error!("Failed to start node restart for {}: {}", node_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

/// Manual Hermes restart via HermesService
pub async fn execute_manual_hermes_restart(
    Path(hermes_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!("Manual hermes restart requested for: {}", hermes_name);

    match state.hermes_service.restart_instance(&hermes_name).await {
        Ok(operation_id) => {
            info!("Hermes {} restart started: {}", hermes_name, operation_id);
            Ok(Json(ApiResponse::success(json!({
                "message": format!("Hermes {} restart started successfully", hermes_name),
                "operation_id": operation_id,
                "hermes_name": hermes_name,
                "status": "started"
            }))))
        }
        Err(e) => {
            error!("Failed to start Hermes restart for {}: {}", hermes_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

/// Manual node pruning via OperationExecutor
pub async fn execute_manual_node_pruning(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!("Manual node pruning requested for: {}", node_name);

    let node_name_clone = node_name.clone();
    let http_manager = state.http_agent_manager.clone();

    match state
        .operation_executor
        .execute_async("pruning", &node_name, move || {
            let http_manager = http_manager.clone();
            let node_name = node_name_clone.clone();
            async move { http_manager.execute_node_pruning(&node_name).await }
        })
        .await
    {
        Ok(operation_id) => {
            info!("Node {} pruning started: {}", node_name, operation_id);
            Ok(Json(ApiResponse::success(json!({
                "message": format!("Node {} pruning started successfully", node_name),
                "operation_id": operation_id,
                "node_name": node_name,
                "status": "started"
            }))))
        }
        Err(e) => {
            error!("Failed to start pruning for {}: {}", node_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

/// Get maintenance schedule (stub)
pub async fn get_maintenance_schedule(State(_state): State<AppState>) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "scheduled_operations": [],
        "message": "Maintenance schedule - see node configurations for schedule details"
    }))))
}
