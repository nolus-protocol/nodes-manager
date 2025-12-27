//! Health monitoring orchestration
//!
//! This module coordinates health checks for blockchain nodes.

use super::auto_restore::{clear_auto_restore_checked_state, monitor_auto_restore_triggers};
use super::cosmos::check_cosmos_node_health;
use super::log_monitor::monitor_logs_per_node;
use super::solana::{check_solana_node_health, is_solana_network};
use super::types::{AutoRestoreCooldown, BlockHeightState, HealthStatus, HermesHealthStatus};

use crate::config::{Config, HermesConfig, NodeConfig};
use crate::database::{Database, HealthRecord, HermesHealthRecord};
use crate::http::HttpAgentManager;
use crate::maintenance_tracker::MaintenanceTracker;
use crate::services::alert_service::AlertService;
use crate::snapshot::SnapshotManager;

use anyhow::{anyhow, Result};
use chrono::Utc;
use futures::future::join_all;
use reqwest::Client as HttpClient;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{error, info, instrument};

/// Main health monitoring orchestrator
#[derive(Clone)]
pub struct HealthMonitor {
    config: Arc<Config>,
    database: Arc<Database>,
    maintenance_tracker: Arc<MaintenanceTracker>,
    snapshot_manager: Arc<SnapshotManager>,
    alert_service: Arc<AlertService>,
    http_manager: Arc<HttpAgentManager>,
    client: HttpClient,
    auto_restore_cooldowns: Arc<Mutex<HashMap<String, AutoRestoreCooldown>>>,
    block_height_states: Arc<Mutex<HashMap<String, BlockHeightState>>>,
    auto_restore_checked_states: Arc<Mutex<HashMap<String, bool>>>,
}

impl HealthMonitor {
    pub fn new(
        config: Arc<Config>,
        database: Arc<Database>,
        maintenance_tracker: Arc<MaintenanceTracker>,
        snapshot_manager: Arc<SnapshotManager>,
        alert_service: Arc<AlertService>,
        http_manager: Arc<HttpAgentManager>,
    ) -> Self {
        let client = HttpClient::builder()
            .timeout(Duration::from_secs(config.rpc_timeout_seconds))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            database,
            maintenance_tracker,
            snapshot_manager,
            alert_service,
            http_manager,
            client,
            auto_restore_cooldowns: Arc::new(Mutex::new(HashMap::new())),
            block_height_states: Arc::new(Mutex::new(HashMap::new())),
            auto_restore_checked_states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Check health of all configured nodes
    #[instrument(skip(self))]
    pub async fn check_all_nodes(&self) -> Result<Vec<HealthStatus>> {
        let mut health_statuses = Vec::new();
        let mut tasks = Vec::new();

        for (node_name, node_config) in &self.config.nodes {
            if !node_config.enabled {
                continue;
            }

            // Skip health checks entirely for nodes in maintenance
            if self.maintenance_tracker.is_in_maintenance(node_name).await {
                info!(
                    "Skipping health check for {} - node in maintenance mode",
                    node_name
                );

                let maintenance_status = HealthStatus {
                    node_name: node_name.clone(),
                    rpc_url: node_config.rpc_url.clone(),
                    is_healthy: false,
                    error_message: Some(
                        "Node is in maintenance mode - health checks suspended".to_string(),
                    ),
                    last_check: Utc::now(),
                    block_height: None,
                    is_syncing: None,
                    is_catching_up: false,
                    validator_address: None,
                    network: node_config.network.clone(),
                    server_host: node_config.server_host.clone(),
                    enabled: node_config.enabled,
                    in_maintenance: true,
                };

                health_statuses.push(maintenance_status);
                continue;
            }

            let task = {
                let node_name = node_name.clone();
                let node_config = node_config.clone();
                let monitor = self.clone();
                tokio::spawn(
                    async move { monitor.check_node_health(&node_name, &node_config).await },
                )
            };
            tasks.push(task);
        }

        let results = join_all(tasks).await;
        for result in results {
            match result {
                Ok(Ok(status)) => health_statuses.push(status),
                Ok(Err(e)) => error!("Health check failed: {}", e),
                Err(e) => error!("Health check task panicked: {}", e),
            }
        }

        // Store results in database and handle alerts
        for status in &health_statuses {
            if let Err(e) = self.store_health_record(status).await {
                error!(
                    "Failed to store health record for {}: {}",
                    status.node_name, e
                );
            }

            if let Err(e) = self.handle_health_alerts(status).await {
                error!(
                    "Failed to handle health alerts for {}: {}",
                    status.node_name, e
                );
            }
        }

        // Per-node log monitoring - only for healthy nodes NOT in maintenance
        let non_maintenance_statuses: Vec<_> = health_statuses
            .iter()
            .filter(|status| !status.in_maintenance)
            .collect();

        if !non_maintenance_statuses.is_empty() {
            if let Err(e) = monitor_logs_per_node(
                &self.config,
                &self.client,
                &non_maintenance_statuses,
                &self.alert_service,
            )
            .await
            {
                error!("Log monitoring failed: {}", e);
            }
        }

        // Auto-restore monitoring - only check UNHEALTHY nodes ONCE per unhealthy period
        let non_maintenance_statuses: Vec<_> = health_statuses
            .iter()
            .filter(|status| !status.in_maintenance)
            .cloned()
            .collect();

        if !non_maintenance_statuses.is_empty() {
            if let Err(e) = monitor_auto_restore_triggers(
                &self.config,
                &self.client,
                &non_maintenance_statuses,
                &self.auto_restore_cooldowns,
                &self.auto_restore_checked_states,
                &self.snapshot_manager,
                &self.alert_service,
            )
            .await
            {
                error!("Auto-restore monitoring failed: {}", e);
            }
        }

        Ok(health_statuses)
    }

    /// Check individual node health (routes to Cosmos or Solana handler)
    pub async fn check_node_health(
        &self,
        node_name: &str,
        node_config: &NodeConfig,
    ) -> Result<HealthStatus> {
        if is_solana_network(&node_config.network) {
            check_solana_node_health(
                &self.client,
                node_name,
                node_config,
                self.config.rpc_timeout_seconds,
                &self.block_height_states,
            )
            .await
        } else {
            check_cosmos_node_health(
                &self.client,
                node_name,
                node_config,
                self.config.rpc_timeout_seconds,
                &self.block_height_states,
            )
            .await
        }
    }

    /// Handle health alerts using centralized AlertService
    async fn handle_health_alerts(&self, status: &HealthStatus) -> Result<()> {
        // Skip alerts for maintenance nodes
        if status.in_maintenance {
            return Ok(());
        }

        // Reset auto-restore checked state when node becomes healthy
        if status.is_healthy {
            clear_auto_restore_checked_state(&status.node_name, &self.auto_restore_checked_states)
                .await;
        }

        let details = Some(serde_json::json!({
            "rpc_url": status.rpc_url,
            "block_height": status.block_height,
            "is_catching_up": status.is_catching_up,
            "network": status.network,
            "last_check": status.last_check.to_rfc3339()
        }));

        self.alert_service
            .send_progressive_alert(
                &status.node_name,
                &status.server_host,
                status.is_healthy,
                status.error_message.clone(),
                details,
            )
            .await
    }

    /// Store health record in database
    async fn store_health_record(&self, status: &HealthStatus) -> Result<()> {
        let record = HealthRecord {
            node_name: status.node_name.clone(),
            is_healthy: status.is_healthy,
            error_message: status.error_message.clone(),
            timestamp: status.last_check,
            block_height: status.block_height,
            is_syncing: status.is_syncing.map(|s| if s { 1 } else { 0 }),
            is_catching_up: Some(if status.is_catching_up { 1 } else { 0 }),
            validator_address: status.validator_address.clone(),
        };

        self.database.store_health_record(&record).await
    }

    /// Get cached health status for a single node
    pub async fn get_node_health(&self, node_name: &str) -> Result<Option<HealthStatus>> {
        let record = self.database.get_latest_health_record(node_name).await?;

        match record {
            Some(record) => {
                let node_config = self
                    .config
                    .nodes
                    .get(node_name)
                    .ok_or_else(|| anyhow!("Node {} not found in configuration", node_name))?;

                let is_in_maintenance = self.maintenance_tracker.is_in_maintenance(node_name).await;

                let status = HealthStatus {
                    node_name: record.node_name,
                    rpc_url: node_config.rpc_url.clone(),
                    is_healthy: record.is_healthy,
                    error_message: record.error_message,
                    last_check: record.timestamp,
                    block_height: record.block_height,
                    is_syncing: record.is_syncing.map(|s| s != 0),
                    is_catching_up: record.is_catching_up.unwrap_or(0) != 0,
                    validator_address: record.validator_address,
                    network: node_config.network.clone(),
                    server_host: node_config.server_host.clone(),
                    enabled: node_config.enabled,
                    in_maintenance: is_in_maintenance,
                };

                Ok(Some(status))
            }
            None => Ok(None),
        }
    }

    /// Get cached health status for all nodes (parallel database reads)
    pub async fn get_all_nodes_health_cached(&self) -> Result<Vec<HealthStatus>> {
        let mut tasks = Vec::new();

        for (node_name, node_config) in &self.config.nodes {
            if !node_config.enabled {
                continue;
            }

            let node_name = node_name.clone();
            let monitor = self.clone();

            let task = tokio::spawn(async move { monitor.get_node_health(&node_name).await });

            tasks.push(task);
        }

        let mut health_statuses = Vec::new();
        for task in tasks {
            match task.await {
                Ok(Ok(Some(status))) => health_statuses.push(status),
                Ok(Ok(None)) => {}
                Ok(Err(e)) => error!("Failed to get cached health: {}", e),
                Err(e) => error!("Health check task panicked: {}", e),
            }
        }

        Ok(health_statuses)
    }

    // === HERMES HEALTH MONITORING ===

    /// Check health of all configured Hermes instances
    #[instrument(skip(self))]
    pub async fn check_all_hermes(&self) -> Result<Vec<HermesHealthStatus>> {
        let mut health_statuses = Vec::new();
        let mut tasks = Vec::new();

        for (hermes_name, hermes_config) in &self.config.hermes {
            let hermes_name = hermes_name.clone();
            let hermes_config = hermes_config.clone();
            let monitor = self.clone();

            let task = tokio::spawn(async move {
                monitor
                    .check_hermes_health(&hermes_name, &hermes_config)
                    .await
            });
            tasks.push(task);
        }

        let results = join_all(tasks).await;
        for result in results {
            match result {
                Ok(Ok(status)) => health_statuses.push(status),
                Ok(Err(e)) => error!("Hermes health check failed: {}", e),
                Err(e) => error!("Hermes health check task panicked: {}", e),
            }
        }

        // Store results in database
        for status in &health_statuses {
            if let Err(e) = self.store_hermes_health_record(status).await {
                error!(
                    "Failed to store hermes health record for {}: {}",
                    status.hermes_name, e
                );
            }
        }

        Ok(health_statuses)
    }

    /// Check individual Hermes instance health
    async fn check_hermes_health(
        &self,
        hermes_name: &str,
        hermes_config: &HermesConfig,
    ) -> Result<HermesHealthStatus> {
        let timeout_duration = Duration::from_secs(10);

        // Check service status
        let (status, error_message) = match tokio::time::timeout(
            timeout_duration,
            self.http_manager
                .check_service_status(&hermes_config.server_host, &hermes_config.service_name),
        )
        .await
        {
            Ok(Ok(service_status)) => (format!("{:?}", service_status), None),
            Ok(Err(e)) => ("Unknown".to_string(), Some(e.to_string())),
            Err(_) => (
                "Timeout".to_string(),
                Some("Health check timed out".to_string()),
            ),
        };

        // Check uptime
        let uptime_seconds = match tokio::time::timeout(
            timeout_duration,
            self.http_manager
                .get_service_uptime(&hermes_config.server_host, &hermes_config.service_name),
        )
        .await
        {
            Ok(Ok(Some(uptime))) => Some(uptime.as_secs()),
            _ => None,
        };

        let uptime_formatted = uptime_seconds.map(|secs| {
            let hours = secs / 3600;
            let minutes = (secs % 3600) / 60;
            let seconds = secs % 60;
            if hours > 0 {
                format!("{}h {}m {}s", hours, minutes, seconds)
            } else if minutes > 0 {
                format!("{}m {}s", minutes, seconds)
            } else {
                format!("{}s", seconds)
            }
        });

        let is_healthy = status == "Running";

        Ok(HermesHealthStatus {
            hermes_name: hermes_name.to_string(),
            server_host: hermes_config.server_host.clone(),
            service_name: hermes_config.service_name.clone(),
            is_healthy,
            status,
            uptime_seconds,
            uptime_formatted,
            error_message,
            last_check: Utc::now(),
            dependent_nodes: hermes_config.dependent_nodes.clone().unwrap_or_default(),
            in_maintenance: false,
        })
    }

    /// Store Hermes health record in database
    async fn store_hermes_health_record(&self, status: &HermesHealthStatus) -> Result<()> {
        let record = HermesHealthRecord {
            hermes_name: status.hermes_name.clone(),
            is_healthy: status.is_healthy,
            status: status.status.clone(),
            uptime_seconds: status.uptime_seconds.map(|s| s as i64),
            error_message: status.error_message.clone(),
            timestamp: status.last_check,
            server_host: status.server_host.clone(),
            service_name: status.service_name.clone(),
        };

        self.database.store_hermes_health_record(&record).await
    }

    /// Get cached health status for a single Hermes instance
    pub async fn get_hermes_health(&self, hermes_name: &str) -> Result<Option<HermesHealthStatus>> {
        let record = self
            .database
            .get_latest_hermes_health_record(hermes_name)
            .await?;

        match record {
            Some(record) => {
                let hermes_config = self
                    .config
                    .hermes
                    .get(hermes_name)
                    .ok_or_else(|| anyhow!("Hermes {} not found in configuration", hermes_name))?;

                let uptime_formatted = record.uptime_seconds.map(|secs| {
                    let secs = secs as u64;
                    let hours = secs / 3600;
                    let minutes = (secs % 3600) / 60;
                    let seconds = secs % 60;
                    if hours > 0 {
                        format!("{}h {}m {}s", hours, minutes, seconds)
                    } else if minutes > 0 {
                        format!("{}m {}s", minutes, seconds)
                    } else {
                        format!("{}s", seconds)
                    }
                });

                let status = HermesHealthStatus {
                    hermes_name: record.hermes_name,
                    server_host: record.server_host,
                    service_name: record.service_name,
                    is_healthy: record.is_healthy,
                    status: record.status,
                    uptime_seconds: record.uptime_seconds.map(|s| s as u64),
                    uptime_formatted,
                    error_message: record.error_message,
                    last_check: record.timestamp,
                    dependent_nodes: hermes_config.dependent_nodes.clone().unwrap_or_default(),
                    in_maintenance: false,
                };

                Ok(Some(status))
            }
            None => Ok(None),
        }
    }

    /// Get cached health status for all Hermes instances (parallel database reads)
    pub async fn get_all_hermes_health_cached(&self) -> Result<Vec<HermesHealthStatus>> {
        let mut tasks = Vec::new();

        for hermes_name in self.config.hermes.keys() {
            let hermes_name = hermes_name.clone();
            let monitor = self.clone();

            let task = tokio::spawn(async move { monitor.get_hermes_health(&hermes_name).await });

            tasks.push(task);
        }

        let mut health_statuses = Vec::new();
        for task in tasks {
            match task.await {
                Ok(Ok(Some(status))) => health_statuses.push(status),
                Ok(Ok(None)) => {}
                Ok(Err(e)) => error!("Failed to get cached hermes health: {}", e),
                Err(e) => error!("Hermes health check task panicked: {}", e),
            }
        }

        Ok(health_statuses)
    }
}
