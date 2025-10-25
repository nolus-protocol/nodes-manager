// File: manager/src/web/mod.rs
pub mod handlers;
pub mod server;

pub use server::start_web_server;

use serde::Serialize;
use std::sync::Arc;

use crate::config::{Config, ConfigManager};
use crate::database::Database;
use crate::health::HealthMonitor;
use crate::http::HttpAgentManager;
use crate::operation_tracker::SimpleOperationTracker;
use crate::services::{HermesService, OperationExecutor, SnapshotService, StateSyncService};
use crate::snapshot::SnapshotManager;

// Application state shared across all handlers
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    // Business logic services with AlertService integration
    pub operation_executor: Arc<OperationExecutor>,
    pub hermes_service: Arc<HermesService>,
    // Note: MaintenanceService removed from AppState - only used by scheduler, not web handlers
    pub snapshot_service: Arc<SnapshotService>,
    pub state_sync_service: Arc<StateSyncService>,
    // Low-level infrastructure services (kept for background tasks)
    pub health_monitor: Arc<HealthMonitor>,
    pub http_agent_manager: Arc<HttpAgentManager>,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: Arc<Config>,
        _database: Arc<Database>,
        health_monitor: Arc<HealthMonitor>,
        http_manager: Arc<HttpAgentManager>,
        _config_manager: Arc<ConfigManager>,
        _snapshot_manager: Arc<SnapshotManager>,
        _operation_tracker: Arc<SimpleOperationTracker>,
        operation_executor: Arc<OperationExecutor>,
        hermes_service: Arc<HermesService>,
        _maintenance_service: Arc<crate::services::MaintenanceService>, // Only used by scheduler
        snapshot_service: Arc<SnapshotService>,
        state_sync_service: Arc<StateSyncService>,
    ) -> Self {
        Self {
            config,
            operation_executor,
            hermes_service,
            snapshot_service,
            state_sync_service,
            health_monitor,
            http_agent_manager: http_manager,
        }
    }
}

// API response types for compatibility with existing UI
#[derive(Debug, Clone, Serialize)]
pub struct NodeHealthSummary {
    pub node_name: String,
    pub status: String, // "Healthy", "Unhealthy", "Maintenance", "Unknown"
    pub latest_block_height: Option<u64>,
    pub catching_up: Option<bool>,
    pub last_check: String,
    pub error_message: Option<String>,
    pub server_host: String,
    pub maintenance_info: Option<MaintenanceInfo>,
    pub snapshot_enabled: bool,
    pub auto_restore_enabled: bool,
    pub scheduled_snapshots_enabled: bool,
    pub snapshot_retention_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaintenanceInfo {
    pub operation_type: String,
    pub started_at: String,
    pub estimated_duration_minutes: u32,
    pub elapsed_minutes: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HermesInstance {
    pub name: String,
    pub server_host: String,
    pub service_name: String,
    pub status: String, // "Running", "Stopped", "Failed", "Unknown"
    pub uptime_formatted: Option<String>,
    pub dependent_nodes: Vec<String>,
    pub in_maintenance: bool,
}

// NEW: ETL service summary for API responses
#[derive(Debug, Clone, Serialize)]
pub struct EtlServiceSummary {
    pub service_name: String,
    pub status: String, // "Healthy", "Unhealthy", "Unknown"
    pub service_url: String,
    pub response_time_ms: Option<u64>,
    pub status_code: Option<u16>,
    pub last_check: String,
    pub error_message: Option<String>,
    pub server_host: String,
    pub enabled: bool,
    pub description: Option<String>,
}
