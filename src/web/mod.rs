// File: src/web/mod.rs

pub mod handlers;
pub mod server;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::config::ConfigManager;
use crate::health::HealthMonitor;
use crate::maintenance_tracker::MaintenanceTracker;
use crate::scheduler::MaintenanceScheduler;
use crate::services::{HealthService, MaintenanceService, SnapshotService, HermesService};
use crate::snapshot::SnapshotManager;
use crate::ssh::SshManager;
use crate::Config;

// Public data structures for API responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealthSummary {
    pub node_name: String,
    pub status: String,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesInstance {
    pub name: String,
    pub server_host: String,
    pub service_name: String,
    pub status: String,
    pub uptime_formatted: Option<String>,
    pub dependent_nodes: Vec<String>,
    pub in_maintenance: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceInfo {
    pub operation_type: String,
    pub started_at: String,
    pub estimated_duration_minutes: u32,
    pub elapsed_minutes: i64,
}

// Application state containing all services
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub health_service: HealthService,
    pub maintenance_service: MaintenanceService,
    pub snapshot_service: SnapshotService,
    pub hermes_service: HermesService,
    pub config_manager: Arc<ConfigManager>,
}

impl AppState {
    pub fn new(
        config: Arc<Config>,
        database: Arc<crate::database::Database>,
        health_monitor: Arc<HealthMonitor>,
        ssh_manager: Arc<SshManager>,
        scheduler: Arc<MaintenanceScheduler>,
        config_manager: Arc<ConfigManager>,
        maintenance_tracker: Arc<MaintenanceTracker>,
        snapshot_manager: Arc<SnapshotManager>,
    ) -> Self {
        // Create service instances
        let health_service = HealthService::new(
            config.clone(),
            database.clone(),
            health_monitor,
            maintenance_tracker.clone(),
        );

        let maintenance_service = MaintenanceService::new(
            config.clone(),
            database.clone(),
            maintenance_tracker.clone(),
            scheduler,
            ssh_manager.clone(),
        );

        let snapshot_service = SnapshotService::new(
            config.clone(),
            snapshot_manager,
            maintenance_tracker.clone(),
        );

        let hermes_service = HermesService::new(
            config.clone(),
            ssh_manager,
            maintenance_tracker,
        );

        Self {
            config,
            health_service,
            maintenance_service,
            snapshot_service,
            hermes_service,
            config_manager,
        }
    }
}

// Web server startup function
pub async fn start_web_server(
    config: Arc<Config>,
    database: Arc<crate::database::Database>,
    health_monitor: Arc<HealthMonitor>,
    ssh_manager: Arc<SshManager>,
    scheduler: Arc<MaintenanceScheduler>,
    config_manager: Arc<ConfigManager>,
    maintenance_tracker: Arc<MaintenanceTracker>,
    snapshot_manager: Arc<SnapshotManager>,
) -> Result<()> {
    server::start_web_server(
        config,
        database,
        health_monitor,
        ssh_manager,
        scheduler,
        config_manager,
        maintenance_tracker,
        snapshot_manager,
    ).await
}
