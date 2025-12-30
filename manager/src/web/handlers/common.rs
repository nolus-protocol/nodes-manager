// Common types and utilities for API handlers

use axum::{http::StatusCode, response::Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::web::{HermesInstance, MaintenanceInfo, NodeHealthSummary};

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

// Health status conversion helpers
pub async fn convert_health_to_summary(
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

    let status = if health.in_maintenance {
        "Maintenance".to_string()
    } else if !health.is_healthy {
        "Unhealthy".to_string()
    } else if health.is_catching_up {
        "Catching Up".to_string()
    } else {
        "Synced".to_string()
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

pub fn convert_hermes_health_to_instance(
    health: &crate::health::HermesHealthStatus,
) -> HermesInstance {
    HermesInstance {
        name: health.hermes_name.clone(),
        server_host: health.server_host.clone(),
        service_name: health.service_name.clone(),
        status: health.status.clone(),
        uptime_formatted: health.uptime_formatted.clone(),
        dependent_nodes: health.dependent_nodes.clone(),
        in_maintenance: health.in_maintenance,
    }
}
