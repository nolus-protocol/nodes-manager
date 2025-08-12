// File: src/web/handlers.rs

use axum::{
    extract::{Path, Query, State},
    response::Json,
};
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use tracing::info;

use crate::web::{
    ApiError, ApiResponse, ApiResult, AppState, HealthCheckResponse, HermesRestartRequest,
    NodeConfigUpdateRequest, NodeHealthSummary, PruningRequest, ScheduleOperationRequest,
    ServerStatusSummary, SystemStatusResponse,
    format_duration_seconds, parse_boolean_query, transform_health_to_summary,
    validate_hermes_name, validate_limit, validate_node_name, validate_operation_type,
    DEFAULT_QUERY_LIMIT,
};

// Health monitoring handlers
pub async fn get_all_nodes_health(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<ApiResponse<Vec<NodeHealthSummary>>>> {
    let include_disabled = parse_boolean_query(params.get("include_disabled").map(String::as_str));

    let health_records = state.health_monitor.get_all_health_status().await?;

    let mut summaries = Vec::new();
    for health in health_records.iter() {
        if include_disabled || state.config.nodes.get(&health.node_name)
            .map(|node| node.enabled)
            .unwrap_or(false)
        {
            let summary = transform_health_to_summary(health, &state.config, &state.maintenance_tracker).await;
            summaries.push(summary);
        }
    }

    Ok(Json(ApiResponse::success(summaries)))
}

pub async fn get_node_health(
    State(state): State<AppState>,
    Path(node_name): Path<String>,
) -> ApiResult<Json<ApiResponse<NodeHealthSummary>>> {
    validate_node_name(&node_name, &state.config)?;

    let health = state.database.get_latest_node_health(&node_name).await?;

    match health {
        Some(h) => {
            let summary = transform_health_to_summary(&h, &state.config, &state.maintenance_tracker).await;
            Ok(Json(ApiResponse::success(summary)))
        }
        None => Err(ApiError::not_found(format!("No health data found for node {}", node_name))),
    }
}

pub async fn get_node_health_history(
    State(state): State<AppState>,
    Path(node_name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<ApiResponse<Vec<NodeHealthSummary>>>> {
    validate_node_name(&node_name, &state.config)?;

    let limit = params.get("limit")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(DEFAULT_QUERY_LIMIT);
    let validated_limit = validate_limit(Some(limit));

    let history = state.health_monitor.get_node_health_history(&node_name, validated_limit).await?;

    let mut summaries = Vec::new();
    for health in history.iter() {
        let summary = transform_health_to_summary(health, &state.config, &state.maintenance_tracker).await;
        summaries.push(summary);
    }

    Ok(Json(ApiResponse::success(summaries)))
}

pub async fn force_health_check(
    State(state): State<AppState>,
    Path(node_name): Path<String>,
) -> ApiResult<Json<ApiResponse<NodeHealthSummary>>> {
    validate_node_name(&node_name, &state.config)?;

    info!("Forcing health check for node: {}", node_name);

    let health = state.health_monitor.force_health_check(&node_name).await?;
    let summary = transform_health_to_summary(&health, &state.config, &state.maintenance_tracker).await;

    Ok(Json(ApiResponse::success_with_message(
        summary,
        format!("Health check completed for node {}", node_name),
    )))
}

// Maintenance management handlers
pub async fn get_scheduled_operations(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<serde_json::Value>>> {
    let operations = state.scheduler.get_scheduled_operations().await;
    let running_operations = state.scheduler.get_running_operations().await;

    let response = json!({
        "scheduled": operations,
        "running": running_operations,
        "total": operations.len() + running_operations.len()
    });

    Ok(Json(ApiResponse::success(response)))
}

pub async fn schedule_pruning(
    State(state): State<AppState>,
    Json(request): Json<ScheduleOperationRequest>,
) -> ApiResult<Json<ApiResponse<String>>> {
    validate_operation_type(&request.operation_type)?;
    validate_node_name(&request.target_name, &state.config)?;

    if request.operation_type.to_lowercase() != "pruning" {
        return Err(ApiError::bad_request("Operation type must be 'pruning'"));
    }

    let operation_id = state.scheduler
        .schedule_pruning(&request.target_name, &request.schedule)
        .await?;

    Ok(Json(ApiResponse::success_with_message(
        operation_id,
        format!("Pruning scheduled for node {}", request.target_name),
    )))
}

pub async fn schedule_hermes_restart(
    State(state): State<AppState>,
    Json(request): Json<ScheduleOperationRequest>,
) -> ApiResult<Json<ApiResponse<String>>> {
    validate_operation_type(&request.operation_type)?;
    validate_hermes_name(&request.target_name, &state.config)?;

    if request.operation_type.to_lowercase() != "hermes_restart" {
        return Err(ApiError::bad_request("Operation type must be 'hermes_restart'"));
    }

    let operation_id = state.scheduler
        .schedule_hermes_restart(&request.target_name, &request.schedule)
        .await?;

    Ok(Json(ApiResponse::success_with_message(
        operation_id,
        format!("Hermes restart scheduled for {}", request.target_name),
    )))
}

pub async fn cancel_scheduled_operation(
    State(state): State<AppState>,
    Path(operation_id): Path<String>,
) -> ApiResult<Json<ApiResponse<()>>> {
    state.scheduler.cancel_scheduled_operation(&operation_id).await?;

    Ok(Json(ApiResponse::success_with_message(
        (),
        format!("Operation {} cancelled", operation_id),
    )))
}

pub async fn execute_immediate_operation(
    State(state): State<AppState>,
    Json(request): Json<ScheduleOperationRequest>,
) -> ApiResult<Json<ApiResponse<String>>> {
    validate_operation_type(&request.operation_type)?;

    let message = match request.operation_type.to_lowercase().as_str() {
        "pruning" => {
            validate_node_name(&request.target_name, &state.config)?;
            state.scheduler.execute_immediate_pruning(&request.target_name).await?;
            format!("Immediate pruning completed for node {}", request.target_name)
        }
        "hermes_restart" => {
            validate_hermes_name(&request.target_name, &state.config)?;
            state.scheduler.execute_immediate_hermes_restart(&request.target_name).await?;
            format!("Immediate Hermes restart completed for {}", request.target_name)
        }
        _ => return Err(ApiError::bad_request("Invalid operation type")),
    };

    Ok(Json(ApiResponse::success_with_message(
        "completed".to_string(),
        message,
    )))
}

pub async fn get_maintenance_logs(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> ApiResult<Json<ApiResponse<Vec<crate::MaintenanceOperation>>>> {
    let limit = params.get("limit")
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(DEFAULT_QUERY_LIMIT);
    let validated_limit = validate_limit(Some(limit));

    let logs = state.scheduler.get_maintenance_logs(validated_limit).await?;

    Ok(Json(ApiResponse::success(logs)))
}

pub async fn execute_batch_pruning(
    State(state): State<AppState>,
    Json(request): Json<PruningRequest>,
) -> ApiResult<Json<ApiResponse<serde_json::Value>>> {
    // Validate all node names
    for node_name in &request.node_names {
        validate_node_name(node_name, &state.config)?;
    }

    info!("Starting batch pruning for {} nodes", request.node_names.len());

    let result = state.scheduler.execute_batch_pruning(request.node_names).await?;

    let response = json!({
        "operation_id": result.operation_id,
        "total_operations": result.total_operations,
        "successful": result.successful,
        "failed": result.failed,
        "results": result.results
    });

    Ok(Json(ApiResponse::success_with_message(
        response,
        format!("Batch pruning completed: {}/{} successful", result.successful, result.total_operations),
    )))
}

pub async fn execute_batch_hermes_restart(
    State(state): State<AppState>,
    Json(request): Json<HermesRestartRequest>,
) -> ApiResult<Json<ApiResponse<serde_json::Value>>> {
    // Validate all hermes names
    for hermes_name in &request.hermes_names {
        validate_hermes_name(hermes_name, &state.config)?;
    }

    info!("Starting batch Hermes restart for {} instances", request.hermes_names.len());

    let result = state.scheduler.execute_batch_hermes_restart(request.hermes_names).await?;

    let response = json!({
        "operation_id": result.operation_id,
        "total_operations": result.total_operations,
        "successful": result.successful,
        "failed": result.failed,
        "results": result.results
    });

    Ok(Json(ApiResponse::success_with_message(
        response,
        format!("Batch Hermes restart completed: {}/{} successful", result.successful, result.total_operations),
    )))
}

pub async fn get_operation_status(
    State(state): State<AppState>,
    Path(operation_id): Path<String>,
) -> ApiResult<Json<ApiResponse<serde_json::Value>>> {
    let status = state.scheduler.get_operation_status(&operation_id).await;

    let response = json!({
        "operation_id": operation_id,
        "status": status.map(|s| format!("{:?}", s)).unwrap_or_else(|| "NotFound".to_string()),
        "timestamp": Utc::now().to_rfc3339()
    });

    Ok(Json(ApiResponse::success(response)))
}

pub async fn get_operations_summary(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<serde_json::Value>>> {
    let summary = state.scheduler.get_operations_summary().await;
    Ok(Json(ApiResponse::success(summary)))
}

// New maintenance tracking handlers
pub async fn get_active_maintenance(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<Vec<crate::maintenance_tracker::MaintenanceWindow>>>> {
    let active_maintenance = state.maintenance_tracker.get_all_in_maintenance().await;
    let count = active_maintenance.len(); // Get count before moving

    Ok(Json(ApiResponse::success_with_message(
        active_maintenance,
        format!("Found {} active maintenance operations", count),
    )))
}

pub async fn get_maintenance_stats(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<crate::maintenance_tracker::MaintenanceStats>>> {
    let stats = state.maintenance_tracker.get_maintenance_stats().await;

    Ok(Json(ApiResponse::success(stats)))
}

pub async fn emergency_clear_maintenance(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<serde_json::Value>>> {
    info!("Emergency clearing all maintenance windows");

    let cleared_count = state.maintenance_tracker.emergency_clear_all_maintenance().await;

    let response = json!({
        "cleared_count": cleared_count,
        "timestamp": Utc::now().to_rfc3339(),
        "action": "emergency_clear_all"
    });

    Ok(Json(ApiResponse::success_with_message(
        response,
        format!("Emergency cleared {} maintenance windows", cleared_count),
    )))
}

// NEW: Clear specific maintenance handler
pub async fn clear_specific_maintenance(
    State(state): State<AppState>,
    Path(node_name): Path<String>,
) -> ApiResult<Json<ApiResponse<serde_json::Value>>> {
    validate_node_name(&node_name, &state.config)?;

    info!("Clearing maintenance for specific node: {}", node_name);

    // Check if node is actually in maintenance
    if !state.maintenance_tracker.is_in_maintenance(&node_name).await {
        return Err(ApiError::bad_request(format!(
            "Node {} is not currently in maintenance",
            node_name
        )));
    }

    // End maintenance for the specific node
    state.maintenance_tracker.end_maintenance(&node_name).await?;

    let response = json!({
        "node_name": node_name,
        "action": "cleared_maintenance",
        "timestamp": Utc::now().to_rfc3339()
    });

    Ok(Json(ApiResponse::success_with_message(
        response,
        format!("Maintenance cleared for node {}", node_name),
    )))
}

// Hermes management handlers
pub async fn get_all_hermes_instances(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<Vec<serde_json::Value>>>> {
    let mut instances = Vec::new();

    for (hermes_name, hermes_config) in &state.config.hermes {
        let status = state.ssh_manager
            .check_service_status(&hermes_config.server_host, &hermes_config.service_name)
            .await
            .unwrap_or(crate::ssh::ServiceStatus::Unknown("Error".to_string()));

        instances.push(json!({
            "name": hermes_name,
            "server_host": hermes_config.server_host,
            "service_name": hermes_config.service_name,
            "status": format!("{:?}", status),
            "dependent_nodes": hermes_config.dependent_nodes,
            "restart_schedule": hermes_config.restart_schedule
        }));
    }

    Ok(Json(ApiResponse::success(instances)))
}

pub async fn restart_hermes_instance(
    State(state): State<AppState>,
    Path(hermes_name): Path<String>,
) -> ApiResult<Json<ApiResponse<String>>> {
    validate_hermes_name(&hermes_name, &state.config)?;

    let hermes_config = state.config.hermes.get(&hermes_name).unwrap();

    info!("Restarting Hermes instance: {}", hermes_name);
    state.ssh_manager.restart_hermes(hermes_config).await?;

    Ok(Json(ApiResponse::success_with_message(
        "restarted".to_string(),
        format!("Hermes instance {} restarted successfully", hermes_name),
    )))
}

pub async fn get_hermes_status(
    State(state): State<AppState>,
    Path(hermes_name): Path<String>,
) -> ApiResult<Json<ApiResponse<serde_json::Value>>> {
    validate_hermes_name(&hermes_name, &state.config)?;

    let hermes_config = state.config.hermes.get(&hermes_name).unwrap();

    let status = state.ssh_manager
        .check_service_status(&hermes_config.server_host, &hermes_config.service_name)
        .await?;

    let uptime = state.ssh_manager
        .get_service_uptime(&hermes_config.server_host, &hermes_config.service_name)
        .await
        .ok()
        .flatten();

    let response = json!({
        "name": hermes_name,
        "status": format!("{:?}", status),
        "server_host": hermes_config.server_host,
        "service_name": hermes_config.service_name,
        "uptime_seconds": uptime.map(|u| u.as_secs()),
        "uptime_formatted": uptime.map(|u| format_duration_seconds(u.as_secs())),
        "dependent_nodes": hermes_config.dependent_nodes,
        "last_check": Utc::now().to_rfc3339()
    });

    Ok(Json(ApiResponse::success(response)))
}

pub async fn restart_all_hermes(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<serde_json::Value>>> {
    info!("Restarting all Hermes instances");

    let hermes_names: Vec<String> = state.config.hermes.keys().cloned().collect();
    let result = state.scheduler.execute_batch_hermes_restart(hermes_names).await?;

    let response = json!({
        "operation_id": result.operation_id,
        "total_instances": result.total_operations,
        "successful": result.successful,
        "failed": result.failed,
        "results": result.results
    });

    Ok(Json(ApiResponse::success_with_message(
        response,
        format!("Hermes restart completed: {}/{} successful", result.successful, result.total_operations),
    )))
}

// Configuration management handlers
pub async fn get_all_node_configs(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<serde_json::Value>>> {
    let nodes = &state.config.nodes;
    Ok(Json(ApiResponse::success(json!(nodes))))
}

pub async fn update_node_config(
    State(state): State<AppState>,
    Path(node_name): Path<String>,
    Json(request): Json<NodeConfigUpdateRequest>,
) -> ApiResult<Json<ApiResponse<String>>> {
    validate_node_name(&node_name, &state.config)?;

    let mut node_config = state.config.nodes.get(&node_name).unwrap().clone();

    // Update fields if provided
    if let Some(enabled) = request.enabled {
        node_config.enabled = enabled;
    }
    if let Some(pruning_enabled) = request.pruning_enabled {
        node_config.pruning_enabled = Some(pruning_enabled);
    }
    if let Some(schedule) = request.pruning_schedule {
        node_config.pruning_schedule = Some(schedule);
    }
    if let Some(keep_blocks) = request.pruning_keep_blocks {
        node_config.pruning_keep_blocks = Some(keep_blocks);
    }
    if let Some(keep_versions) = request.pruning_keep_versions {
        node_config.pruning_keep_versions = Some(keep_versions);
    }

    state.config_manager.update_node_config(&node_name, &node_config).await?;

    Ok(Json(ApiResponse::success_with_message(
        "updated".to_string(),
        format!("Node configuration updated for {}", node_name),
    )))
}

pub async fn get_all_hermes_configs(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<serde_json::Value>>> {
    let hermes = &state.config.hermes;
    Ok(Json(ApiResponse::success(json!(hermes))))
}

pub async fn get_all_server_configs(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<Vec<ServerStatusSummary>>>> {
    let mut servers = Vec::new();
    let connection_status = state.ssh_manager.get_connection_status().await;

    for (server_name, server_config) in &state.config.servers {
        let node_count = state.config_manager.get_nodes_for_server(&server_config.host).await.len();
        let hermes_count = state.config_manager.get_hermes_for_server(&server_config.host).await.len();

        // Count nodes in maintenance for this server
        let maintenance_windows = state.maintenance_tracker.get_all_in_maintenance().await;
        let nodes_in_maintenance = maintenance_windows.iter()
            .filter(|window| &window.server_host == server_name)
            .count();

        servers.push(ServerStatusSummary {
            server_name: server_name.clone(),
            host: server_config.host.clone(),
            connected: connection_status.get(server_name).copied().unwrap_or(false),
            node_count,
            hermes_count,
            nodes_in_maintenance,
            last_activity: None, // Could be implemented with connection tracking
        });
    }

    Ok(Json(ApiResponse::success(servers)))
}

pub async fn reload_configurations(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<String>>> {
    info!("Reloading configurations");

    let new_config = state.config_manager.reload_configs().await?;

    Ok(Json(ApiResponse::success_with_message(
        "reloaded".to_string(),
        format!("Configuration reloaded: {} servers, {} nodes, {} hermes instances",
                new_config.servers.len(), new_config.nodes.len(), new_config.hermes.len()),
    )))
}

pub async fn validate_configuration(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<serde_json::Value>>> {
    let current_config = state.config_manager.get_current_config().await;

    match state.config_manager.validate_config_content(&current_config) {
        Ok(_) => {
            let response = json!({
                "valid": true,
                "message": "Configuration is valid",
                "servers": current_config.servers.len(),
                "nodes": current_config.nodes.len(),
                "hermes": current_config.hermes.len()
            });
            Ok(Json(ApiResponse::success(response)))
        }
        Err(e) => {
            let response = json!({
                "valid": false,
                "error": e.to_string()
            });
            Ok(Json(ApiResponse::success(response)))
        }
    }
}

pub async fn list_config_files(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<Vec<String>>>> {
    let files = state.config_manager.list_config_files().await?;
    let file_paths: Vec<String> = files.iter()
        .map(|path| path.to_string_lossy().to_string())
        .collect();

    Ok(Json(ApiResponse::success(file_paths)))
}

// System status handlers
pub async fn get_system_status(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<SystemStatusResponse>>> {
    let health_records = state.health_monitor.get_all_health_status().await.unwrap_or_default();
    let healthy_nodes = health_records.iter()
        .filter(|h| matches!(h.status, crate::HealthStatus::Healthy))
        .count();
    let unhealthy_nodes = health_records.iter()
        .filter(|h| matches!(h.status, crate::HealthStatus::Unhealthy | crate::HealthStatus::Unknown))
        .count();
    let maintenance_nodes = health_records.iter()
        .filter(|h| matches!(h.status, crate::HealthStatus::Maintenance))
        .count();

    let active_maintenance_operations = state.maintenance_tracker.get_all_in_maintenance().await.len();

    let response = SystemStatusResponse {
        server_count: state.config.servers.len(),
        node_count: state.config.nodes.len(),
        hermes_count: state.config.hermes.len(),
        healthy_nodes,
        unhealthy_nodes,
        maintenance_nodes,
        active_ssh_connections: state.ssh_manager.get_active_connections().await,
        running_operations: state.scheduler.get_running_operations().await.len(),
        scheduled_operations: state.scheduler.get_scheduled_operations().await.len(),
        active_maintenance_operations,
        uptime_seconds: 0, // Would need to track server start time
    };

    Ok(Json(ApiResponse::success(response)))
}

pub async fn get_ssh_connections_status(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<serde_json::Value>>> {
    let connection_status = state.ssh_manager.get_connection_status().await;
    let active_connections = state.ssh_manager.get_active_connections().await;

    let response = json!({
        "total_servers": state.config.servers.len(),
        "active_connections": active_connections,
        "connection_status": connection_status
    });

    Ok(Json(ApiResponse::success(response)))
}

pub async fn get_running_operations(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<Vec<crate::MaintenanceOperation>>>> {
    let operations = state.scheduler.get_running_operations().await;
    Ok(Json(ApiResponse::success(operations)))
}

pub async fn health_check(
    State(_state): State<AppState>,
) -> ApiResult<Json<HealthCheckResponse>> {
    let response = HealthCheckResponse {
        status: "healthy".to_string(),
        timestamp: Utc::now().to_rfc3339(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        database_connected: true, // Could implement actual DB health check
        monitoring_active: true,  // Could check if monitoring is running
        scheduler_active: true,   // Could check if scheduler is running
        maintenance_tracking_active: true, // Maintenance tracker is always active
    };

    Ok(Json(response))
}

pub async fn test_server_connectivity(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<serde_json::Value>>> {
    info!("Testing connectivity to all servers");

    let connectivity_results = state.ssh_manager.validate_all_servers_connectivity().await;

    let response = json!({
        "total_servers": state.config.servers.len(),
        "connectivity_results": connectivity_results,
        "timestamp": Utc::now().to_rfc3339()
    });

    Ok(Json(ApiResponse::success(response)))
}

pub async fn get_all_service_statuses(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiResponse<serde_json::Value>>> {
    info!("Getting status of all services");

    let service_statuses = state.ssh_manager.get_all_service_statuses().await;

    let response = json!({
        "service_statuses": service_statuses,
        "timestamp": Utc::now().to_rfc3339()
    });

    Ok(Json(ApiResponse::success(response)))
}

// Utility handlers
pub async fn api_documentation() -> Json<serde_json::Value> {
    Json(json!({
        "name": "Blockchain Nodes Manager API",
        "version": env!("CARGO_PKG_VERSION"),
        "description": "REST API for managing blockchain nodes and Hermes relayers with maintenance tracking",
        "endpoints": {
            "health_monitoring": [
                "GET /api/nodes/health",
                "GET /api/nodes/{name}/health",
                "GET /api/nodes/{name}/history",
                "POST /api/nodes/{name}/check"
            ],
            "maintenance": [
                "GET /api/maintenance/schedule",
                "POST /api/maintenance/pruning",
                "POST /api/maintenance/hermes-restart",
                "DELETE /api/maintenance/{id}",
                "POST /api/maintenance/run-now",
                "GET /api/maintenance/logs",
                "POST /api/maintenance/prune-multiple",
                "POST /api/maintenance/restart-multiple",
                "GET /api/maintenance/active",
                "GET /api/maintenance/stats",
                "POST /api/maintenance/emergency-clear",
                "POST /api/maintenance/clear/{node_name}"
            ],
            "hermes": [
                "GET /api/hermes/instances",
                "POST /api/hermes/{name}/restart",
                "GET /api/hermes/{name}/status",
                "POST /api/hermes/restart-all"
            ],
            "configuration": [
                "GET /api/config/nodes",
                "PUT /api/config/nodes/{name}",
                "GET /api/config/hermes",
                "GET /api/config/servers",
                "POST /api/config/reload",
                "POST /api/config/validate"
            ],
            "system": [
                "GET /api/system/status",
                "GET /api/system/ssh-connections",
                "GET /api/system/operations",
                "GET /api/system/health"
            ]
        }
    }))
}

pub async fn get_version_info() -> Json<serde_json::Value> {
    Json(json!({
        "name": env!("CARGO_PKG_NAME"),
        "version": env!("CARGO_PKG_VERSION"),
        "description": env!("CARGO_PKG_DESCRIPTION"),
        "build_timestamp": Utc::now().to_rfc3339(),
        "rust_version": option_env!("CARGO_PKG_RUST_VERSION").unwrap_or("unknown"),
        "features": [
            "health_monitoring",
            "maintenance_tracking",
            "scheduled_operations",
            "ssh_management",
            "hermes_integration"
        ]
    }))
}
