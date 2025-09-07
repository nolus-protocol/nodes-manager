// File: manager/src/web/handlers.rs
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, Json},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{error, info};

use crate::snapshot::{SnapshotInfo, SnapshotStats};
use crate::web::{AppState, HermesInstance, NodeHealthSummary, MaintenanceInfo, EtlServiceSummary};
use crate::operation_tracker::OperationStatus;

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

fn default_max_hours() -> i64 { 12 }

// CHANGED: Enhanced health status conversion with better catching up detection
async fn convert_health_to_summary(health: &crate::health::monitor::HealthStatus, config: &crate::config::Config) -> NodeHealthSummary {
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
        "Catching Up".to_string()  // NEW: Clear catching up status
    } else {
        "Synced".to_string()  // CHANGED: More precise "Synced" instead of "Healthy"
    };

    NodeHealthSummary {
        node_name: health.node_name.clone(),
        status,
        latest_block_height: health.block_height.map(|h| h as u64),
        catching_up: health.is_syncing,
        last_check: health.last_check.to_rfc3339(),
        error_message: health.error_message.clone(),
        server_host: health.server_host.clone(),
        maintenance_info,
        snapshot_enabled: node_config.map(|c| c.snapshots_enabled.unwrap_or(false)).unwrap_or(false),
        auto_restore_enabled: node_config.map(|c| c.auto_restore_enabled.unwrap_or(false)).unwrap_or(false),
        scheduled_snapshots_enabled: node_config.map(|c| c.snapshot_schedule.is_some()).unwrap_or(false),
        snapshot_retention_count: node_config.and_then(|c| c.snapshot_retention_count.map(|cnt| cnt as u32)),
    }
}

// NEW: Convert ETL health status to summary
async fn convert_etl_health_to_summary(health: &crate::health::monitor::EtlHealthStatus, config: &crate::config::Config) -> EtlServiceSummary {
    let etl_config = config.etl.get(&health.service_name);

    let status = if health.is_healthy {
        "Healthy".to_string()
    } else {
        "Unhealthy".to_string()
    };

    EtlServiceSummary {
        service_name: health.service_name.clone(),
        status,
        service_url: health.service_url.clone(),
        response_time_ms: health.response_time_ms,
        status_code: health.status_code,
        last_check: health.last_check.to_rfc3339(),
        error_message: health.error_message.clone(),
        server_host: health.server_host.clone(),
        enabled: health.enabled,
        description: etl_config.and_then(|c| c.description.clone()),
    }
}

async fn get_hermes_instances(state: &AppState) -> Result<Vec<HermesInstance>, anyhow::Error> {
    let mut instances = Vec::new();

    for (hermes_name, hermes_config) in &state.config.hermes {
        let status = match state.agent_manager.check_service_status(&hermes_config.server_host, &hermes_config.service_name).await {
            Ok(service_status) => format!("{:?}", service_status),
            Err(_) => "Unknown".to_string(),
        };

        let uptime_formatted = match state.agent_manager.get_service_uptime(&hermes_config.server_host, &hermes_config.service_name).await {
            Ok(Some(uptime)) => {
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

        instances.push(HermesInstance {
            name: hermes_name.clone(),
            server_host: hermes_config.server_host.clone(),
            service_name: hermes_config.service_name.clone(),
            status,
            uptime_formatted,
            dependent_nodes: hermes_config.dependent_nodes.clone().unwrap_or_default(),
            in_maintenance: false,
        });
    }

    Ok(instances)
}

// === HEALTH MONITORING ENDPOINTS ===

pub async fn get_all_nodes_health(
    Query(query): Query<IncludeDisabledQuery>,
    State(state): State<AppState>,
) -> ApiResult<Vec<NodeHealthSummary>> {
    match state.health_service.check_all_nodes().await {
        Ok(health_statuses) => {
            let mut summaries = Vec::new();

            for health in health_statuses {
                if query.include_disabled || health.enabled {
                    let summary = convert_health_to_summary(&health, &state.config).await;
                    summaries.push(summary);
                }
            }

            Ok(Json(ApiResponse::success(summaries)))
        },
        Err(e) => {
            error!("Failed to get all nodes health: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn get_node_health(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<NodeHealthSummary> {
    match state.health_service.get_node_health(&node_name).await {
        Ok(Some(health_status)) => {
            let summary = convert_health_to_summary(&health_status, &state.config).await;
            Ok(Json(ApiResponse::success(summary)))
        },
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(ApiResponse::error(format!("Node {} not found", node_name))))),
        Err(e) => {
            error!("Failed to get node health for {}: {}", node_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn get_all_hermes_health(
    State(state): State<AppState>,
) -> ApiResult<Vec<HermesInstance>> {
    match get_hermes_instances(&state).await {
        Ok(hermes_instances) => Ok(Json(ApiResponse::success(hermes_instances))),
        Err(e) => {
            error!("Failed to get all hermes health: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn get_hermes_health(
    Path(hermes_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<HermesInstance> {
    match get_hermes_instances(&state).await {
        Ok(instances) => {
            if let Some(instance) = instances.into_iter().find(|i| i.name == hermes_name) {
                Ok(Json(ApiResponse::success(instance)))
            } else {
                Err((StatusCode::NOT_FOUND, Json(ApiResponse::error(format!("Hermes {} not found", hermes_name)))))
            }
        },
        Err(e) => {
            error!("Failed to get hermes health for {}: {}", hermes_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

// === NEW: ETL SERVICE HEALTH ENDPOINTS ===

pub async fn get_all_etl_health(
    Query(query): Query<IncludeDisabledQuery>,
    State(state): State<AppState>,
) -> ApiResult<Vec<EtlServiceSummary>> {
    match state.health_service.check_all_etl_services().await {
        Ok(etl_statuses) => {
            let mut summaries = Vec::new();

            for etl_status in etl_statuses {
                if query.include_disabled || etl_status.enabled {
                    let summary = convert_etl_health_to_summary(&etl_status, &state.config).await;
                    summaries.push(summary);
                }
            }

            Ok(Json(ApiResponse::success(summaries)))
        },
        Err(e) => {
            error!("Failed to get all ETL services health: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn get_etl_health(
    Path(service_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<EtlServiceSummary> {
    match state.health_service.get_etl_service_health(&service_name).await {
        Ok(Some(etl_status)) => {
            let summary = convert_etl_health_to_summary(&etl_status, &state.config).await;
            Ok(Json(ApiResponse::success(summary)))
        },
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(ApiResponse::error(format!("ETL service {} not found", service_name))))),
        Err(e) => {
            error!("Failed to get ETL service health for {}: {}", service_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn refresh_etl_health(
    State(state): State<AppState>,
) -> ApiResult<Vec<EtlServiceSummary>> {
    info!("Manual ETL health refresh requested");
    match state.health_service.check_all_etl_services().await {
        Ok(etl_statuses) => {
            let mut summaries = Vec::new();
            for etl_status in etl_statuses {
                let summary = convert_etl_health_to_summary(&etl_status, &state.config).await;
                summaries.push(summary);
            }
            Ok(Json(ApiResponse::success(summaries)))
        },
        Err(e) => {
            error!("Failed to refresh ETL services health: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

// === CONFIGURATION ENDPOINTS ===

pub async fn get_all_node_configs(
    State(state): State<AppState>,
) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "nodes": state.config.nodes
    }))))
}

pub async fn get_all_hermes_configs(
    State(state): State<AppState>,
) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "hermes": state.config.hermes
    }))))
}

// NEW: ETL configuration endpoint
pub async fn get_all_etl_configs(
    State(state): State<AppState>,
) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "etl": state.config.etl
    }))))
}

// === OPERATION ENDPOINTS WITH OPERATION TRACKING ===

pub async fn execute_manual_node_restart(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!("Manual node restart requested for: {}", node_name);

    match state.agent_manager.restart_node(&node_name).await {
        Ok(_) => {
            info!("Node {} restarted successfully", node_name);
            Ok(Json(ApiResponse::success(json!({
                "message": format!("Node {} restart completed successfully", node_name)
            }))))
        }
        Err(e) => {
            error!("Failed to restart node {}: {}", node_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn execute_manual_hermes_restart(
    Path(hermes_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!("Manual hermes restart requested for: {}", hermes_name);

    if let Some(hermes_config) = state.config.hermes.get(&hermes_name) {
        match state.agent_manager.restart_hermes(hermes_config).await {
            Ok(_) => {
                info!("Hermes {} restarted successfully", hermes_name);
                Ok(Json(ApiResponse::success(json!({
                    "message": format!("Hermes {} restart completed successfully", hermes_name)
                }))))
            }
            Err(e) => {
                error!("Failed to restart hermes {}: {}", hermes_name, e);
                Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
            }
        }
    } else {
        Err((StatusCode::NOT_FOUND, Json(ApiResponse::error(format!("Hermes {} not found", hermes_name)))))
    }
}

pub async fn execute_manual_node_pruning(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!("Manual node pruning requested for: {}", node_name);

    match state.agent_manager.execute_node_pruning(&node_name).await {
        Ok(_) => {
            info!("Node {} pruning completed successfully", node_name);
            Ok(Json(ApiResponse::success(json!({
                "message": format!("Node {} pruning completed successfully", node_name)
            }))))
        }
        Err(e) => {
            error!("Failed to start pruning for {}: {}", node_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

// === SNAPSHOT MANAGEMENT ENDPOINTS ===

pub async fn create_snapshot(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<SnapshotInfo> {
    info!("Snapshot creation requested for: {}", node_name);

    match state.agent_manager.create_node_snapshot(&node_name).await {
        Ok(snapshot_info) => {
            info!("Snapshot created successfully for {}: {}", node_name, snapshot_info.filename);
            Ok(Json(ApiResponse::success(snapshot_info)))
        }
        Err(e) => {
            error!("Failed to create snapshot for {}: {}", node_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
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
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn delete_snapshot(
    Path((node_name, filename)): Path<(String, String)>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!("Snapshot deletion requested for {}: {}", node_name, filename);

    match state.snapshot_service.delete_snapshot(&node_name, &filename).await {
        Ok(_) => {
            info!("Snapshot {} deleted successfully for {}", filename, node_name);
            Ok(Json(ApiResponse::success(json!({
                "message": format!("Snapshot {} deleted successfully", filename)
            }))))
        }
        Err(e) => {
            error!("Failed to delete snapshot {} for {}: {}", filename, node_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
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
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn cleanup_old_snapshots(
    Path(node_name): Path<String>,
    Query(query): Query<RetentionQuery>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!("Snapshot cleanup requested for {} (retention: {})", node_name, query.retention_count);

    match state.snapshot_service.cleanup_old_snapshots(&node_name, query.retention_count).await {
        Ok(deleted_count) => {
            info!("Snapshot cleanup completed for {}", node_name);
            Ok(Json(ApiResponse::success(json!({
                "deleted_count": deleted_count,
                "node_name": node_name,
                "retention_count": query.retention_count
            }))))
        }
        Err(e) => {
            error!("Failed to cleanup old snapshots for {}: {}", node_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

// === NEW: MANUAL RESTORE ENDPOINTS ===

pub async fn execute_manual_restore_from_latest(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<SnapshotInfo> {
    info!("Manual restore from latest snapshot requested for: {}", node_name);

    match state.snapshot_service.restore_from_snapshot(&node_name).await {
        Ok(snapshot_info) => {
            info!("Manual restore completed successfully for {}: {}", node_name, snapshot_info.filename);
            Ok(Json(ApiResponse::success(snapshot_info)))
        }
        Err(e) => {
            error!("Failed to restore from snapshot for {}: {}", node_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn check_auto_restore_triggers(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!("Checking auto-restore triggers for: {}", node_name);

    match state.snapshot_service.check_auto_restore_trigger(&node_name).await {
        Ok(triggers_found) => {
            info!("Auto-restore trigger check completed for {}: triggers_found={}", node_name, triggers_found);
            Ok(Json(ApiResponse::success(json!({
                "node_name": node_name,
                "triggers_found": triggers_found,
                "timestamp": Utc::now().to_rfc3339()
            }))))
        }
        Err(e) => {
            error!("Failed to check auto-restore triggers for {}: {}", node_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
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

    let trigger_words = state.config.auto_restore_trigger_words.clone().unwrap_or_default();

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

pub async fn get_active_operations(
    State(state): State<AppState>,
) -> ApiResult<OperationStatus> {
    let operations = state.agent_manager.get_active_operations().await;
    Ok(Json(ApiResponse::success(operations)))
}

pub async fn cancel_operation(
    Path(target_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!("Operation cancellation requested for: {}", target_name);

    match state.agent_manager.cancel_operation(&target_name).await {
        Ok(_) => {
            info!("Operation cancelled successfully for {}", target_name);
            Ok(Json(ApiResponse::success(json!({
                "message": format!("Operation cancelled for {}", target_name)
            }))))
        }
        Err(e) => {
            error!("Failed to cancel operation for {}: {}", target_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn emergency_cleanup_operations(
    Query(query): Query<EmergencyCleanupQuery>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    info!("Emergency cleanup requested for operations older than {} hours", query.max_hours);

    let cleaned_count = state.agent_manager.emergency_cleanup_operations(query.max_hours).await;

    Ok(Json(ApiResponse::success(json!({
        "message": format!("Emergency cleanup completed: {} operations removed", cleaned_count),
        "cleaned_count": cleaned_count
    }))))
}

pub async fn check_target_status(
    Path(target_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    let is_busy = state.agent_manager.is_target_busy(&target_name).await;
    let active_operation = if is_busy {
        state.agent_manager.operation_tracker.get_active_operation(&target_name).await
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

pub async fn get_maintenance_schedule(
    State(_state): State<AppState>,
) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "scheduled": [],
        "active": []
    }))))
}

// === STATIC FILE HANDLER ===

pub async fn serve_index() -> Html<&'static str> {
    Html(include_str!("../../../static/index.html"))
}
