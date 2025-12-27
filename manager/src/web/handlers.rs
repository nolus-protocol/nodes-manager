// File: manager/src/web/handlers.rs
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::timeout;
use tracing::{error, info};

use crate::operation_tracker::OperationStatus;
use crate::snapshot::{SnapshotInfo, SnapshotStats};
use crate::web::{AppState, HermesInstance, MaintenanceInfo, NodeHealthSummary};

// Helper type for API responses
pub type ApiResult<T> = Result<Json<ApiResponse<T>>, (StatusCode, Json<ApiResponse<()>>)>;

#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
    pub timestamp: String,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
            timestamp: Utc::now().to_rfc3339(),
        }
    }
}

impl ApiResponse<()> {
    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            message: Some(message),
            timestamp: Utc::now().to_rfc3339(),
        }
    }
}

// Query parameters
#[derive(Deserialize)]
pub struct IncludeDisabledQuery {
    #[serde(default)]
    pub include_disabled: bool,
}

#[derive(Deserialize)]
pub struct RetentionQuery {
    pub retention_count: u32,
}

#[derive(Deserialize)]
pub struct EmergencyCleanupQuery {
    #[serde(default = "default_max_hours")]
    pub max_hours: i64,
}

fn default_max_hours() -> i64 {
    12
}

// CHANGED: Enhanced health status conversion with better catching up detection
async fn convert_health_to_summary(
    health: &crate::health::HealthStatus,
    config: &crate::config::Config,
) -> NodeHealthSummary {
    let node_config = config.nodes.get(&health.node_name);

    let maintenance_info = if health.in_maintenance {
        Some(MaintenanceInfo {
            operation_type: "maintenance".to_string(),
            started_at: Utc::now().to_rfc3339(),
            estimated_duration_minutes: 60,
            elapsed_minutes: 5,
        })
    } else {
        None
    };

    // CHANGED: Enhanced status determination with clear catching up vs synced distinction
    let status = if health.in_maintenance {
        "Maintenance".to_string()
    } else if !health.is_healthy {
        "Unhealthy".to_string()
    } else if health.is_catching_up {
        "Catching Up".to_string() // NEW: Clear catching up status
    } else {
        "Synced".to_string() // CHANGED: More precise "Synced" instead of "Healthy"
    };

    NodeHealthSummary {
        node_name: health.node_name.clone(),
        status,
        latest_block_height: health.block_height.map(|h| h as u64),
        catching_up: health.is_syncing,
        last_check: health.last_check.to_rfc3339(),
        error_message: health.error_message.clone(),
        server_host: health.server_host.clone(),
        network: node_config.map(|c| c.network.clone()).unwrap_or_default(),
        maintenance_info,
        snapshot_enabled: node_config
            .map(|c| c.snapshots_enabled.unwrap_or(false))
            .unwrap_or(false),
        auto_restore_enabled: node_config
            .map(|c| c.auto_restore_enabled.unwrap_or(false))
            .unwrap_or(false),
        scheduled_snapshots_enabled: node_config
            .map(|c| c.snapshot_schedule.is_some())
            .unwrap_or(false),
        snapshot_retention_count: node_config
            .and_then(|c| c.snapshot_retention_count.map(|cnt| cnt as u32)),
    }
}

// NOTE: Hermes health is not persisted to database, so we use a timeout-based approach
// CHANGED: Now runs checks in parallel for better performance
async fn get_hermes_instances(
    state: &AppState,
    use_timeout: bool,
) -> Result<Vec<HermesInstance>, anyhow::Error> {
    let timeout_duration = if use_timeout {
        Duration::from_secs(2) // Fast timeout for cached mode
    } else {
        Duration::from_secs(10) // Normal timeout for refresh mode
    };

    // CHANGED: Spawn parallel tasks instead of sequential loop
    let mut tasks = Vec::new();

    for (hermes_name, hermes_config) in &state.config.hermes {
        let hermes_name = hermes_name.clone();
        let hermes_config = hermes_config.clone();
        let http_manager = state.http_agent_manager.clone();

        let task = tokio::spawn(async move {
            let status = match timeout(
                timeout_duration,
                http_manager
                    .check_service_status(&hermes_config.server_host, &hermes_config.service_name),
            )
            .await
            {
                Ok(Ok(service_status)) => format!("{:?}", service_status),
                Ok(Err(_)) => "Unknown".to_string(),
                Err(_) => "Timeout".to_string(),
            };

            let uptime_formatted = match timeout(
                timeout_duration,
                http_manager
                    .get_service_uptime(&hermes_config.server_host, &hermes_config.service_name),
            )
            .await
            {
                Ok(Ok(Some(uptime))) => {
                    let total_seconds = uptime.as_secs();
                    let hours = total_seconds / 3600;
                    let minutes = (total_seconds % 3600) / 60;
                    let seconds = total_seconds % 60;

                    if hours > 0 {
                        Some(format!("{}h {}m {}s", hours, minutes, seconds))
                    } else if minutes > 0 {
                        Some(format!("{}m {}s", minutes, seconds))
                    } else {
                        Some(format!("{}s", seconds))
                    }
                }
                _ => Some("Unknown".to_string()),
            };

            HermesInstance {
                name: hermes_name,
                server_host: hermes_config.server_host,
                service_name: hermes_config.service_name,
                status,
                uptime_formatted,
                dependent_nodes: hermes_config.dependent_nodes.unwrap_or_default(),
                in_maintenance: false,
            }
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete in parallel
    let mut instances = Vec::new();
    for task in tasks {
        match task.await {
            Ok(instance) => instances.push(instance),
            Err(e) => error!("Hermes health check task failed: {}", e),
        }
    }

    Ok(instances)
}

// === HEALTH MONITORING ENDPOINTS ===

// CHANGED: Now returns cached data from database for instant response
pub async fn get_all_nodes_health(
    Query(query): Query<IncludeDisabledQuery>,
    State(state): State<AppState>,
) -> ApiResult<Vec<NodeHealthSummary>> {
    match state.health_monitor.get_all_nodes_health_cached().await {
        Ok(health_statuses) => {
            let mut summaries = Vec::new();

            for health in health_statuses {
                if query.include_disabled || health.enabled {
                    let summary = convert_health_to_summary(&health, &state.config).await;
                    summaries.push(summary);
                }
            }

            Ok(Json(ApiResponse::success(summaries)))
        }
        Err(e) => {
            error!("Failed to get all nodes health (cached): {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

// NEW: Triggers fresh health checks for all nodes (for refresh button)
pub async fn refresh_all_nodes_health(
    Query(query): Query<IncludeDisabledQuery>,
    State(state): State<AppState>,
) -> ApiResult<Vec<NodeHealthSummary>> {
    info!("Manual refresh requested for all nodes health");
    match state.health_monitor.check_all_nodes().await {
        Ok(health_statuses) => {
            let mut summaries = Vec::new();

            for health in health_statuses {
                if query.include_disabled || health.enabled {
                    let summary = convert_health_to_summary(&health, &state.config).await;
                    summaries.push(summary);
                }
            }

            Ok(Json(ApiResponse::success(summaries)))
        }
        Err(e) => {
            error!("Failed to refresh all nodes health: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn get_node_health(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<NodeHealthSummary> {
    match state.health_monitor.get_node_health(&node_name).await {
        Ok(Some(health_status)) => {
            let summary = convert_health_to_summary(&health_status, &state.config).await;
            Ok(Json(ApiResponse::success(summary)))
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error(format!("Node {} not found", node_name))),
        )),
        Err(e) => {
            error!("Failed to get node health for {}: {}", node_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

// CHANGED: Now uses fast timeout for cached-like behavior
pub async fn get_all_hermes_health(
    State(state): State<AppState>,
) -> ApiResult<Vec<HermesInstance>> {
    match get_hermes_instances(&state, true).await {
        Ok(hermes_instances) => Ok(Json(ApiResponse::success(hermes_instances))),
        Err(e) => {
            error!("Failed to get all hermes health: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

// NEW: Triggers fresh hermes checks with longer timeout (for refresh button)
pub async fn refresh_all_hermes_health(
    State(state): State<AppState>,
) -> ApiResult<Vec<HermesInstance>> {
    info!("Manual refresh requested for all hermes instances");
    match get_hermes_instances(&state, false).await {
        Ok(hermes_instances) => Ok(Json(ApiResponse::success(hermes_instances))),
        Err(e) => {
            error!("Failed to refresh all hermes health: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn get_hermes_health(
    Path(hermes_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<HermesInstance> {
    match get_hermes_instances(&state, true).await {
        Ok(instances) => {
            if let Some(instance) = instances.into_iter().find(|i| i.name == hermes_name) {
                Ok(Json(ApiResponse::success(instance)))
            } else {
                Err((
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::error(format!(
                        "Hermes {} not found",
                        hermes_name
                    ))),
                ))
            }
        }
        Err(e) => {
            error!("Failed to get hermes health for {}: {}", hermes_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

// === CONFIGURATION ENDPOINTS ===

pub async fn get_all_node_configs(State(state): State<AppState>) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "nodes": state.config.nodes
    }))))
}

pub async fn get_all_hermes_configs(State(state): State<AppState>) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "hermes": state.config.hermes
    }))))
}

// === OPERATION ENDPOINTS WITH OPERATION TRACKING ===

// Manual node restart via OperationExecutor
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

// Manual Hermes restart via HermesService
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

// Manual node pruning via OperationExecutor
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

// === SNAPSHOT MANAGEMENT ENDPOINTS ===

// Manual snapshot creation via OperationExecutor
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

// Delete snapshot via SnapshotService
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

// Cleanup old snapshots via SnapshotService
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

// === NEW: MANUAL RESTORE ENDPOINTS ===

// Manual restore via OperationExecutor
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

// === STATE SYNC ENDPOINT ===
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

pub async fn get_auto_restore_status(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    // Check if auto-restore is enabled for this node
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

// === OPERATION MANAGEMENT ENDPOINTS ===

pub async fn get_active_operations(State(state): State<AppState>) -> ApiResult<OperationStatus> {
    let operations = state.http_agent_manager.get_active_operations().await;
    Ok(Json(ApiResponse::success(operations)))
}

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

// === MAINTENANCE SCHEDULE ENDPOINTS ===

pub async fn get_maintenance_schedule(State(_state): State<AppState>) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "scheduled": [],
        "active": []
    }))))
}
