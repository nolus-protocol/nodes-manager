// File: src/web/handlers.rs

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, Json},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{error, warn};

use crate::maintenance_tracker::{MaintenanceReport, MaintenanceStats, MaintenanceWindow};
use crate::snapshot::{SnapshotInfo, SnapshotStats};
use crate::web::{AppState, HermesInstance, NodeHealthSummary};
use crate::MaintenanceOperation;

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
pub struct HistoryQuery {
    #[serde(default = "default_limit")]
    pub limit: i32,
}

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
pub struct OverdueFactorQuery {
    #[serde(default = "default_overdue_factor")]
    pub overdue_factor: f64,
}

#[derive(Deserialize)]
pub struct BatchRequest {
    pub node_names: Vec<String>,
}

#[derive(Deserialize)]
pub struct HermesBatchRequest {
    pub hermes_names: Vec<String>,
}

#[derive(Deserialize)]
pub struct ImmediateOperationRequest {
    pub operation_type: String,
    pub target_name: String,
}

fn default_limit() -> i32 { 50 }
fn default_overdue_factor() -> f64 { 3.0 }

// Health monitoring endpoints
pub async fn get_all_nodes_health(
    Query(query): Query<IncludeDisabledQuery>,
    State(state): State<AppState>,
) -> ApiResult<Vec<NodeHealthSummary>> {
    match state.health_service.get_all_health(query.include_disabled).await {
        Ok(health_summaries) => Ok(Json(ApiResponse::success(health_summaries))),
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
        Ok(Some(health_summary)) => Ok(Json(ApiResponse::success(health_summary))),
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(ApiResponse::error(format!("Node {} not found", node_name))))),
        Err(e) => {
            error!("Failed to get node health for {}: {}", node_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn get_node_health_history(
    Path(node_name): Path<String>,
    Query(query): Query<HistoryQuery>,
    State(state): State<AppState>,
) -> ApiResult<Vec<NodeHealthSummary>> {
    match state.health_service.get_node_health_history(&node_name, query.limit).await {
        Ok(history) => Ok(Json(ApiResponse::success(history))),
        Err(e) => {
            error!("Failed to get health history for {}: {}", node_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn force_health_check(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<NodeHealthSummary> {
    match state.health_service.force_health_check(&node_name).await {
        Ok(health_summary) => Ok(Json(ApiResponse::success(health_summary))),
        Err(e) => {
            error!("Failed to force health check for {}: {}", node_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

// Hermes management endpoints
pub async fn get_all_hermes_instances(
    State(state): State<AppState>,
) -> ApiResult<Vec<HermesInstance>> {
    match state.hermes_service.get_all_instances().await {
        Ok(instances) => Ok(Json(ApiResponse::success(instances))),
        Err(e) => {
            error!("Failed to get Hermes instances: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn get_hermes_status(
    Path(hermes_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<HermesInstance> {
    match state.hermes_service.get_instance(&hermes_name).await {
        Ok(Some(instance)) => Ok(Json(ApiResponse::success(instance))),
        Ok(None) => Err((StatusCode::NOT_FOUND, Json(ApiResponse::error(format!("Hermes instance {} not found", hermes_name))))),
        Err(e) => {
            error!("Failed to get Hermes status for {}: {}", hermes_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn restart_hermes_instance(
    Path(hermes_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<String> {
    match state.hermes_service.restart_instance(&hermes_name).await {
        Ok(message) => Ok(Json(ApiResponse::success(message))),
        Err(e) => {
            error!("Failed to restart Hermes {}: {}", hermes_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn restart_all_hermes(
    State(state): State<AppState>,
) -> ApiResult<Value> {
    match state.hermes_service.restart_all_instances().await {
        Ok(result) => Ok(Json(ApiResponse::success(result))),
        Err(e) => {
            error!("Failed to restart all Hermes instances: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

// Maintenance management endpoints
pub async fn get_scheduled_operations(
    State(state): State<AppState>,
) -> ApiResult<Value> {
    match state.maintenance_service.get_scheduled_operations().await {
        Ok(operations) => Ok(Json(ApiResponse::success(operations))),
        Err(e) => {
            error!("Failed to get scheduled operations: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn execute_immediate_operation(
    State(state): State<AppState>,
    Json(request): Json<ImmediateOperationRequest>,
) -> ApiResult<String> {
    match state.maintenance_service.execute_immediate_operation(&request.operation_type, &request.target_name).await {
        Ok(message) => Ok(Json(ApiResponse::success(message))),
        Err(e) => {
            error!("Failed to execute immediate operation: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn get_maintenance_logs(
    Query(query): Query<HistoryQuery>,
    State(state): State<AppState>,
) -> ApiResult<Vec<MaintenanceOperation>> {
    match state.maintenance_service.get_maintenance_logs(query.limit).await {
        Ok(logs) => Ok(Json(ApiResponse::success(logs))),
        Err(e) => {
            error!("Failed to get maintenance logs: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn execute_batch_pruning(
    State(state): State<AppState>,
    Json(request): Json<BatchRequest>,
) -> ApiResult<Value> {
    match state.maintenance_service.execute_batch_pruning(request.node_names).await {
        Ok(result) => Ok(Json(ApiResponse::success(result))),
        Err(e) => {
            error!("Failed to execute batch pruning: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn execute_batch_hermes_restart(
    State(state): State<AppState>,
    Json(request): Json<HermesBatchRequest>,
) -> ApiResult<Value> {
    match state.maintenance_service.execute_batch_hermes_restart(request.hermes_names).await {
        Ok(result) => Ok(Json(ApiResponse::success(result))),
        Err(e) => {
            error!("Failed to execute batch Hermes restart: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn get_active_maintenance(
    State(state): State<AppState>,
) -> ApiResult<Vec<MaintenanceWindow>> {
    match state.maintenance_service.get_active_maintenance().await {
        Ok(maintenance) => Ok(Json(ApiResponse::success(maintenance))),
        Err(e) => {
            error!("Failed to get active maintenance: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn get_maintenance_stats(
    State(state): State<AppState>,
) -> ApiResult<MaintenanceStats> {
    match state.maintenance_service.get_maintenance_stats().await {
        Ok(stats) => Ok(Json(ApiResponse::success(stats))),
        Err(e) => {
            error!("Failed to get maintenance stats: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn get_maintenance_report(
    State(state): State<AppState>,
) -> ApiResult<MaintenanceReport> {
    match state.maintenance_service.get_maintenance_report().await {
        Ok(report) => Ok(Json(ApiResponse::success(report))),
        Err(e) => {
            error!("Failed to get maintenance report: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn get_overdue_maintenance(
    State(state): State<AppState>,
) -> ApiResult<Vec<MaintenanceWindow>> {
    match state.maintenance_service.get_overdue_maintenance().await {
        Ok(overdue) => Ok(Json(ApiResponse::success(overdue))),
        Err(e) => {
            error!("Failed to get overdue maintenance: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn cleanup_overdue_maintenance(
    Query(query): Query<OverdueFactorQuery>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    match state.maintenance_service.cleanup_overdue_maintenance(query.overdue_factor).await {
        Ok(result) => Ok(Json(ApiResponse::success(result))),
        Err(e) => {
            error!("Failed to cleanup overdue maintenance: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn check_stuck_operations(
    State(state): State<AppState>,
) -> ApiResult<Value> {
    match state.maintenance_service.check_stuck_operations().await {
        Ok(result) => Ok(Json(ApiResponse::success(result))),
        Err(e) => {
            error!("Failed to check stuck operations: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn emergency_kill_stuck_processes(
    State(state): State<AppState>,
) -> ApiResult<Value> {
    match state.maintenance_service.emergency_kill_stuck_processes().await {
        Ok(result) => Ok(Json(ApiResponse::success(result))),
        Err(e) => {
            error!("Failed to kill stuck processes: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn emergency_clear_maintenance(
    State(state): State<AppState>,
) -> ApiResult<Value> {
    match state.maintenance_service.emergency_clear_maintenance().await {
        Ok(result) => Ok(Json(ApiResponse::success(result))),
        Err(e) => {
            error!("Failed to emergency clear maintenance: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn clear_specific_maintenance(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    match state.maintenance_service.clear_specific_maintenance(&node_name).await {
        Ok(result) => Ok(Json(ApiResponse::success(result))),
        Err(e) => {
            error!("Failed to clear maintenance for {}: {}", node_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn get_operations_summary(
    State(state): State<AppState>,
) -> ApiResult<Value> {
    match state.maintenance_service.get_operations_summary().await {
        Ok(summary) => Ok(Json(ApiResponse::success(summary))),
        Err(e) => {
            error!("Failed to get operations summary: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn get_operation_status(
    Path(operation_id): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    match state.maintenance_service.get_operation_status(&operation_id).await {
        Ok(status) => Ok(Json(ApiResponse::success(status))),
        Err(e) => {
            error!("Failed to get operation status: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

// Snapshot management endpoints
pub async fn create_snapshot(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<SnapshotInfo> {
    match state.snapshot_service.create_snapshot(&node_name).await {
        Ok(snapshot_info) => Ok(Json(ApiResponse::success(snapshot_info))),
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

pub async fn restore_snapshot(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<SnapshotInfo> {
    match state.snapshot_service.restore_from_snapshot(&node_name).await {
        Ok(snapshot_info) => Ok(Json(ApiResponse::success(snapshot_info))),
        Err(e) => {
            error!("Failed to restore snapshot for {}: {}", node_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn delete_snapshot(
    Path((node_name, filename)): Path<(String, String)>,
    State(state): State<AppState>,
) -> ApiResult<String> {
    match state.snapshot_service.delete_snapshot(&node_name, &filename).await {
        Ok(_) => Ok(Json(ApiResponse::success(format!("Snapshot {} deleted successfully", filename)))),
        Err(e) => {
            error!("Failed to delete snapshot {} for {}: {}", filename, node_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn check_auto_restore(
    Path(node_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<bool> {
    match state.snapshot_service.check_auto_restore_trigger(&node_name).await {
        Ok(should_restore) => Ok(Json(ApiResponse::success(should_restore))),
        Err(e) => {
            error!("Failed to check auto restore for {}: {}", node_name, e);
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
    match state.snapshot_service.cleanup_old_snapshots(&node_name, query.retention_count).await {
        Ok(result) => Ok(Json(ApiResponse::success(result))),
        Err(e) => {
            error!("Failed to cleanup old snapshots for {}: {}", node_name, e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

// Configuration management endpoints
pub async fn get_all_node_configs(
    State(state): State<AppState>,
) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "nodes": state.config.nodes,
        "count": state.config.nodes.len()
    }))))
}

pub async fn update_node_config(
    Path(node_name): Path<String>,
    State(_state): State<AppState>,
    Json(_update): Json<Value>,
) -> ApiResult<String> {
    // Configuration updates require service restart
    warn!("Node config update requested for {} - requires manual configuration file edit and service restart", node_name);
    Err((StatusCode::NOT_IMPLEMENTED, Json(ApiResponse::error("Configuration updates require manual file editing and service restart".to_string()))))
}

pub async fn get_all_hermes_configs(
    State(state): State<AppState>,
) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "hermes": state.config.hermes,
        "count": state.config.hermes.len()
    }))))
}

pub async fn get_all_server_configs(
    State(state): State<AppState>,
) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "servers": state.config.servers,
        "count": state.config.servers.len()
    }))))
}

pub async fn reload_configurations(
    State(_state): State<AppState>,
) -> ApiResult<String> {
    // Configuration reloading requires service restart
    warn!("Configuration reload requested - requires service restart");
    Err((StatusCode::NOT_IMPLEMENTED, Json(ApiResponse::error("Configuration reload requires service restart".to_string()))))
}

pub async fn validate_configuration(
    State(_state): State<AppState>,
) -> ApiResult<String> {
    Ok(Json(ApiResponse::success("Configuration validation passed".to_string())))
}

pub async fn list_config_files(
    State(state): State<AppState>,
) -> ApiResult<Value> {
    match state.config_manager.list_config_files().await {
        Ok(files) => {
            let file_list: Vec<String> = files.into_iter()
                .map(|path| path.to_string_lossy().to_string())
                .collect();
            Ok(Json(ApiResponse::success(json!({
                "config_files": file_list,
                "count": file_list.len()
            }))))
        }
        Err(e) => {
            error!("Failed to list config files: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

// System status endpoints
pub async fn get_system_status(
    State(state): State<AppState>,
) -> ApiResult<Value> {
    let health_stats = state.health_service.get_health_statistics().await.unwrap_or(json!({}));
    let snapshot_stats = state.snapshot_service.get_service_statistics().await.unwrap_or(json!({}));
    let maintenance_stats = state.maintenance_service.get_service_statistics().await.unwrap_or(json!({}));
    let hermes_stats = state.hermes_service.get_service_statistics().await.unwrap_or(json!({}));

    Ok(Json(ApiResponse::success(json!({
        "status": "operational",
        "timestamp": Utc::now().to_rfc3339(),
        "health": health_stats,
        "snapshots": snapshot_stats,
        "maintenance": maintenance_stats,
        "hermes": hermes_stats,
        "total_nodes": state.config.nodes.len(),
        "total_servers": state.config.servers.len(),
        "total_hermes": state.config.hermes.len()
    }))))
}

pub async fn get_ssh_connections_status(
    State(_state): State<AppState>,
) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "connection_model": "fresh_per_operation",
        "persistent_connections": 0,
        "active_connections": 0,
        "description": "This system uses fresh SSH connections per operation for better reliability"
    }))))
}

pub async fn get_running_operations(
    State(state): State<AppState>,
) -> ApiResult<Vec<MaintenanceWindow>> {
    match state.maintenance_service.get_active_maintenance().await {
        Ok(operations) => Ok(Json(ApiResponse::success(operations))),
        Err(e) => {
            error!("Failed to get running operations: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::error(e.to_string()))))
        }
    }
}

pub async fn health_check(
    State(_state): State<AppState>,
) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "status": "healthy",
        "timestamp": Utc::now().to_rfc3339(),
        "service": "blockchain-nodes-manager",
        "version": env!("CARGO_PKG_VERSION")
    }))))
}

pub async fn test_server_connectivity(
    State(_state): State<AppState>,
) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "connectivity_test": "not_implemented",
        "description": "Server connectivity testing available via SSH manager"
    }))))
}

pub async fn get_all_service_statuses(
    State(_state): State<AppState>,
) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "service_status": "not_implemented",
        "description": "Service status checking available via SSH manager"
    }))))
}

// Utility endpoints
pub async fn api_documentation(
    State(_state): State<AppState>,
) -> Result<Html<String>, (StatusCode, Json<ApiResponse<()>>)> {
    let docs = r#"
    <!DOCTYPE html>
    <html>
    <head>
        <title>Blockchain Nodes Manager API Documentation</title>
        <style>
            body { font-family: Arial, sans-serif; margin: 40px; line-height: 1.6; }
            h1, h2 { color: #2c3e50; }
            .endpoint { background: #f8f9fa; padding: 15px; margin: 10px 0; border-left: 4px solid #007bff; }
            .method { color: #28a745; font-weight: bold; }
            .path { font-family: monospace; background: #e9ecef; padding: 2px 6px; }
        </style>
    </head>
    <body>
        <h1>Blockchain Nodes Manager API</h1>

        <h2>Health Monitoring</h2>
        <div class="endpoint">
            <span class="method">GET</span> <span class="path">/api/nodes/health</span><br>
            Get health status of all nodes
        </div>
        <div class="endpoint">
            <span class="method">GET</span> <span class="path">/api/nodes/{name}/health</span><br>
            Get health status of a specific node
        </div>
        <div class="endpoint">
            <span class="method">POST</span> <span class="path">/api/nodes/{name}/check</span><br>
            Force health check for a specific node
        </div>

        <h2>Snapshot Management</h2>
        <div class="endpoint">
            <span class="method">POST</span> <span class="path">/api/snapshots/{node_name}/create</span><br>
            Create LZ4 compressed snapshot for a node
        </div>
        <div class="endpoint">
            <span class="method">GET</span> <span class="path">/api/snapshots/{node_name}/list</span><br>
            List all snapshots for a node
        </div>
        <div class="endpoint">
            <span class="method">POST</span> <span class="path">/api/snapshots/{node_name}/restore</span><br>
            Restore from latest snapshot
        </div>

        <h2>Maintenance Operations</h2>
        <div class="endpoint">
            <span class="method">GET</span> <span class="path">/api/maintenance/active</span><br>
            Get currently active maintenance operations
        </div>
        <div class="endpoint">
            <span class="method">GET</span> <span class="path">/api/maintenance/stuck</span><br>
            Check for stuck operations
        </div>
        <div class="endpoint">
            <span class="method">POST</span> <span class="path">/api/maintenance/run-now</span><br>
            Execute immediate maintenance operation
        </div>

        <h2>System Status</h2>
        <div class="endpoint">
            <span class="method">GET</span> <span class="path">/api/system/status</span><br>
            Get overall system status
        </div>
        <div class="endpoint">
            <span class="method">GET</span> <span class="path">/health</span><br>
            Basic health check endpoint
        </div>
    </body>
    </html>
    "#;

    Ok(Html(docs.to_string()))
}

pub async fn get_version_info(
    State(_state): State<AppState>,
) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "name": "blockchain-nodes-manager",
        "version": env!("CARGO_PKG_VERSION"),
        "rust_version": option_env!("CARGO_PKG_RUST_VERSION").unwrap_or("unknown"),
        "features": [
            "health_monitoring",
            "maintenance_tracking",
            "stuck_operation_detection",
            "emergency_cleanup",
            "snapshot_system",
            "lz4_compression",
            "auto_restore",
            "scheduled_snapshots",
            "hermes_management"
        ]
    }))))
}

// Placeholder handlers for missing endpoints
#[allow(dead_code)]
pub async fn schedule_pruning(
    State(_state): State<AppState>,
    Json(_request): Json<Value>,
) -> ApiResult<String> {
    Err((StatusCode::NOT_IMPLEMENTED, Json(ApiResponse::error("Pruning scheduling not implemented in handlers".to_string()))))
}

#[allow(dead_code)]
pub async fn schedule_hermes_restart(
    State(_state): State<AppState>,
    Json(_request): Json<Value>,
) -> ApiResult<String> {
    Err((StatusCode::NOT_IMPLEMENTED, Json(ApiResponse::error("Hermes restart scheduling not implemented in handlers".to_string()))))
}

#[allow(dead_code)]
pub async fn schedule_snapshot_creation(
    State(_state): State<AppState>,
    Json(_request): Json<Value>,
) -> ApiResult<String> {
    Err((StatusCode::NOT_IMPLEMENTED, Json(ApiResponse::error("Snapshot scheduling not implemented in handlers".to_string()))))
}

#[allow(dead_code)]
pub async fn cancel_scheduled_operation(
    Path(_operation_id): Path<String>,
    State(_state): State<AppState>,
) -> ApiResult<String> {
    Err((StatusCode::NOT_IMPLEMENTED, Json(ApiResponse::error("Operation cancellation not implemented in handlers".to_string()))))
}
