// File: src/web/mod.rs

pub mod handlers;
pub mod server;

pub use server::start_web_server;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use std::sync::Arc;

use crate::config::ConfigManager;
use crate::database::Database;
use crate::health::HealthMonitor;
use crate::scheduler::MaintenanceScheduler;
use crate::ssh::SshManager;
use crate::Config;

// Shared application state
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub database: Arc<Database>,
    pub health_monitor: Arc<HealthMonitor>,
    pub ssh_manager: Arc<SshManager>,
    pub scheduler: Arc<MaintenanceScheduler>,
    pub config_manager: Arc<ConfigManager>,
}

impl AppState {
    pub fn new(
        config: Arc<Config>,
        database: Arc<Database>,
        health_monitor: Arc<HealthMonitor>,
        ssh_manager: Arc<SshManager>,
        scheduler: Arc<MaintenanceScheduler>,
        config_manager: Arc<ConfigManager>,
    ) -> Self {
        Self {
            config,
            database,
            health_monitor,
            ssh_manager,
            scheduler,
            config_manager,
        }
    }
}

// Custom error type for web API
#[derive(Debug)]
pub struct ApiError {
    pub status_code: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn new(status_code: StatusCode, message: String) -> Self {
        Self {
            status_code,
            message,
        }
    }

    pub fn internal_server_error(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message.into())
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message.into())
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(json!({
            "error": self.message,
            "status_code": self.status_code.as_u16()
        }));

        (self.status_code, body).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        Self::internal_server_error(err.to_string())
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        Self::internal_server_error(format!("Database error: {}", err))
    }
}

// Result type for API handlers
pub type ApiResult<T> = Result<T, ApiError>;

// Common response structures
#[derive(serde::Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: None,
        }
    }

    pub fn success_with_message(data: T, message: String) -> Self {
        Self {
            success: true,
            data: Some(data),
            message: Some(message),
        }
    }
}

// Request/Response types
#[derive(serde::Deserialize)]
pub struct PruningRequest {
    pub node_names: Vec<String>,
}

#[derive(serde::Deserialize)]
pub struct HermesRestartRequest {
    pub hermes_names: Vec<String>,
}

#[derive(serde::Deserialize)]
pub struct ScheduleOperationRequest {
    pub operation_type: String,
    pub target_name: String,
    pub schedule: String,
}

#[derive(serde::Deserialize)]
pub struct NodeConfigUpdateRequest {
    pub enabled: Option<bool>,
    pub pruning_enabled: Option<bool>,
    pub pruning_schedule: Option<String>,
    pub pruning_keep_blocks: Option<u64>,
    pub pruning_keep_versions: Option<u64>,
}

#[derive(serde::Serialize)]
pub struct SystemStatusResponse {
    pub server_count: usize,
    pub node_count: usize,
    pub hermes_count: usize,
    pub healthy_nodes: usize,
    pub unhealthy_nodes: usize,
    pub active_ssh_connections: usize,
    pub running_operations: usize,
    pub scheduled_operations: usize,
    pub uptime_seconds: u64,
}

#[derive(serde::Serialize)]
pub struct NodeHealthSummary {
    pub node_name: String,
    pub status: String,
    pub latest_block_height: Option<u64>,
    pub catching_up: Option<bool>,
    pub last_check: String,
    pub error_message: Option<String>,
    pub server_host: String,
}

#[derive(serde::Serialize)]
pub struct ServerStatusSummary {
    pub server_name: String,
    pub host: String,
    pub connected: bool,
    pub node_count: usize,
    pub hermes_count: usize,
    pub last_activity: Option<String>,
}

// Health check endpoints response
#[derive(serde::Serialize)]
pub struct HealthCheckResponse {
    pub status: String,
    pub timestamp: String,
    pub version: String,
    pub database_connected: bool,
    pub monitoring_active: bool,
    pub scheduler_active: bool,
}

// Utility functions
pub fn format_duration_seconds(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

pub fn parse_boolean_query(value: Option<&str>) -> bool {
    match value {
        Some(v) => matches!(v.to_lowercase().as_str(), "true" | "1" | "yes" | "on"),
        None => false,
    }
}

pub fn validate_node_name(node_name: &str, config: &Config) -> ApiResult<()> {
    if !config.nodes.contains_key(node_name) {
        return Err(ApiError::not_found(format!("Node '{}' not found", node_name)));
    }
    Ok(())
}

pub fn validate_hermes_name(hermes_name: &str, config: &Config) -> ApiResult<()> {
    if !config.hermes.contains_key(hermes_name) {
        return Err(ApiError::not_found(format!("Hermes instance '{}' not found", hermes_name)));
    }
    Ok(())
}

// Response transformation utilities
pub fn transform_health_to_summary(
    health: &crate::NodeHealth,
    config: &Config,
) -> NodeHealthSummary {
    let server_host = config
        .nodes
        .get(&health.node_name)
        .map(|node| node.server_host.clone())
        .unwrap_or_else(|| "unknown".to_string());

    NodeHealthSummary {
        node_name: health.node_name.clone(),
        status: format!("{:?}", health.status),
        latest_block_height: health.latest_block_height,
        catching_up: health.catching_up,
        last_check: health.last_check.to_rfc3339(),
        error_message: health.error_message.clone(),
        server_host,
    }
}

// Constants
pub const MAX_QUERY_LIMIT: i32 = 1000;
pub const DEFAULT_QUERY_LIMIT: i32 = 50;

// Request validation
pub fn validate_limit(limit: Option<i32>) -> i32 {
    match limit {
        Some(l) if l > 0 && l <= MAX_QUERY_LIMIT => l,
        Some(_) => DEFAULT_QUERY_LIMIT,
        None => DEFAULT_QUERY_LIMIT,
    }
}

pub fn validate_operation_type(operation_type: &str) -> ApiResult<()> {
    match operation_type.to_lowercase().as_str() {
        "pruning" | "hermes_restart" | "system_maintenance" => Ok(()),
        _ => Err(ApiError::bad_request(format!(
            "Invalid operation type: {}. Valid types: pruning, hermes_restart, system_maintenance",
            operation_type
        ))),
    }
}
