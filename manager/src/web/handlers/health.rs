// Health monitoring endpoints

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use tracing::{error, info};

use super::common::{
    convert_health_to_summary, convert_hermes_health_to_instance, ApiResponse, ApiResult,
    IncludeDisabledQuery,
};
use crate::web::{AppState, HermesInstance, NodeHealthSummary};

/// Get cached health status for all nodes
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

/// Trigger fresh health checks for all nodes (for refresh button)
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

/// Get health status for a specific node
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

/// Get cached health status for all Hermes instances
pub async fn get_all_hermes_health(
    State(state): State<AppState>,
) -> ApiResult<Vec<HermesInstance>> {
    match state.health_monitor.get_all_hermes_health_cached().await {
        Ok(health_statuses) => {
            let instances: Vec<HermesInstance> = health_statuses
                .iter()
                .map(convert_hermes_health_to_instance)
                .collect();
            Ok(Json(ApiResponse::success(instances)))
        }
        Err(e) => {
            error!("Failed to get all hermes health (cached): {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

/// Trigger fresh health checks for all Hermes instances (for refresh button)
pub async fn refresh_all_hermes_health(
    State(state): State<AppState>,
) -> ApiResult<Vec<HermesInstance>> {
    info!("Manual refresh requested for all hermes instances");
    match state.health_monitor.check_all_hermes().await {
        Ok(health_statuses) => {
            let instances: Vec<HermesInstance> = health_statuses
                .iter()
                .map(convert_hermes_health_to_instance)
                .collect();
            Ok(Json(ApiResponse::success(instances)))
        }
        Err(e) => {
            error!("Failed to refresh all hermes health: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

/// Get health status for a specific Hermes instance
pub async fn get_hermes_health(
    Path(hermes_name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<HermesInstance> {
    match state.health_monitor.get_hermes_health(&hermes_name).await {
        Ok(Some(health_status)) => {
            let instance = convert_hermes_health_to_instance(&health_status);
            Ok(Json(ApiResponse::success(instance)))
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error(format!(
                "Hermes {} not found",
                hermes_name
            ))),
        )),
        Err(e) => {
            error!("Failed to get hermes health for {}: {}", hermes_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}
