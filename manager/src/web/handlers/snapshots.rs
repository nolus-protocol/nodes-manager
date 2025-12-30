// Snapshot management endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use chrono::Utc;
use serde_json::{json, Value};
use tracing::{error, info};

use super::common::{ApiResponse, ApiResult, RetentionQuery};
use crate::snapshot::{SnapshotInfo, SnapshotStats};
use crate::web::AppState;

/// Create a snapshot for a node
pub async fn create_snapshot(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!("Snapshot creation requested for: {}", node_name);

    let node_name_clone = node_name.clone();
    let http_manager = state.http_agent_manager.clone();

    match state
        .operation_executor
        .execute_async("snapshot_creation", &node_name, move || {
            let http_manager = http_manager.clone();
            let node_name = node_name_clone.clone();
            async move {
                http_manager
                    .create_node_snapshot(&node_name)
                    .await
                    .map(|_| ())
            }
        })
        .await
    {
        Ok(operation_id) => {
            info!(
                "Snapshot creation started for {}: {}",
                node_name, operation_id
            );
            Ok(Json(ApiResponse::success(json!({
                "message": format!("Snapshot creation started for node {}", node_name),
                "operation_id": operation_id,
                "node_name": node_name,
                "status": "started"
            }))))
        }
        Err(e) => {
            error!("Failed to start snapshot creation for {}: {}", node_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

/// List all snapshots for a node
pub async fn list_snapshots(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Vec<SnapshotInfo>> {
    match state.snapshot_service.list_snapshots(&node_name).await {
        Ok(snapshots) => Ok(Json(ApiResponse::success(snapshots))),
        Err(e) => {
            error!("Failed to list snapshots for {}: {}", node_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

/// Delete a specific snapshot
pub async fn delete_snapshot(
    Path((node_name, filename)): Path<(String, String)>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!(
        "Snapshot deletion requested for {}: {}",
        node_name, filename
    );

    match state
        .snapshot_service
        .delete_snapshot(&node_name, &filename)
        .await
    {
        Ok(_) => {
            info!(
                "Snapshot {} deleted successfully for {}",
                filename, node_name
            );
            Ok(Json(ApiResponse::success(json!({
                "message": format!("Snapshot {} deleted successfully", filename),
                "node_name": node_name,
                "filename": filename,
                "status": "completed"
            }))))
        }
        Err(e) => {
            error!(
                "Failed to delete snapshot {} for {}: {}",
                filename, node_name, e
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

/// Get snapshot statistics for a node
pub async fn get_snapshot_stats(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<SnapshotStats> {
    match state.snapshot_service.get_snapshot_stats(&node_name).await {
        Ok(stats) => Ok(Json(ApiResponse::success(stats))),
        Err(e) => {
            error!("Failed to get snapshot stats for {}: {}", node_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

/// Cleanup old snapshots based on retention count
pub async fn cleanup_old_snapshots(
    Path(node_name): Path<String>,
    Query(query): Query<RetentionQuery>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!(
        "Snapshot cleanup requested for {} (retention: {})",
        node_name, query.retention_count
    );

    match state
        .snapshot_service
        .cleanup_old_snapshots(&node_name, query.retention_count)
        .await
    {
        Ok(result) => {
            info!("Snapshot cleanup completed for {}", node_name);
            Ok(Json(ApiResponse::success(result)))
        }
        Err(e) => {
            error!("Failed to cleanup old snapshots for {}: {}", node_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

/// Restore from latest snapshot
pub async fn execute_manual_restore_from_latest(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!(
        "Manual restore from latest snapshot requested for: {}",
        node_name
    );

    let node_name_clone = node_name.clone();
    let snapshot_service = state.snapshot_service.clone();

    match state
        .operation_executor
        .execute_async("snapshot_restore", &node_name, move || {
            let snapshot_service = snapshot_service.clone();
            let node_name = node_name_clone.clone();
            async move {
                snapshot_service
                    .restore_from_snapshot(&node_name)
                    .await
                    .map(|_| ())
            }
        })
        .await
    {
        Ok(operation_id) => {
            info!(
                "Snapshot restore started for {}: {}",
                node_name, operation_id
            );
            Ok(Json(ApiResponse::success(json!({
                "message": format!("Restore operation started for node {}", node_name),
                "operation_id": operation_id,
                "node_name": node_name,
                "status": "started"
            }))))
        }
        Err(e) => {
            error!("Failed to start restore for {}: {}", node_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

/// Check auto-restore triggers for a node
pub async fn check_auto_restore_triggers(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!("Checking auto-restore triggers for: {}", node_name);

    match state
        .snapshot_service
        .check_auto_restore_trigger(&node_name)
        .await
    {
        Ok(triggers_found) => {
            info!(
                "Auto-restore trigger check completed for {}: triggers_found={}",
                node_name, triggers_found
            );
            Ok(Json(ApiResponse::success(json!({
                "node_name": node_name,
                "triggers_found": triggers_found,
                "timestamp": Utc::now().to_rfc3339()
            }))))
        }
        Err(e) => {
            error!(
                "Failed to check auto-restore triggers for {}: {}",
                node_name, e
            );
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

/// Get auto-restore status for a node
pub async fn get_auto_restore_status(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    let node_config = state.config.nodes.get(&node_name);

    let auto_restore_enabled = node_config
        .map(|c| c.auto_restore_enabled.unwrap_or(false) && c.snapshots_enabled.unwrap_or(false))
        .unwrap_or(false);

    let trigger_words = state
        .config
        .auto_restore_trigger_words
        .clone()
        .unwrap_or_default();

    let status = json!({
        "node_name": node_name,
        "auto_restore_enabled": auto_restore_enabled,
        "trigger_words": trigger_words,
        "snapshots_enabled": node_config.map(|c| c.snapshots_enabled.unwrap_or(false)).unwrap_or(false),
        "log_path": node_config.and_then(|c| c.log_path.as_ref()),
        "timestamp": Utc::now().to_rfc3339()
    });

    Ok(Json(ApiResponse::success(status)))
}

/// Execute manual state sync
pub async fn execute_manual_state_sync(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!("Manual state sync requested for: {}", node_name);

    let node_name_clone = node_name.clone();
    let state_sync_service = state.state_sync_service.clone();

    match state
        .operation_executor
        .execute_async("state_sync", &node_name, move || {
            let state_sync_service = state_sync_service.clone();
            let node_name = node_name_clone.clone();
            async move { state_sync_service.execute_state_sync(&node_name).await }
        })
        .await
    {
        Ok(operation_id) => {
            info!("State sync started for {}: {}", node_name, operation_id);
            Ok(Json(ApiResponse::success(json!({
                "message": format!("State sync operation started for node {}", node_name),
                "operation_id": operation_id,
                "node_name": node_name,
                "status": "started"
            }))))
        }
        Err(e) => {
            error!("Failed to start state sync for {}: {}", node_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}
