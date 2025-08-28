// File: manager/src/health/monitor.rs
use crate::config::{Config, NodeConfig, ServerConfig};
use crate::database::{Database, HealthRecord};
use crate::maintenance_tracker::MaintenanceTracker;
use crate::snapshot::SnapshotManager;
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use futures::future::join_all;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub node_name: String,
    pub rpc_url: String,
    pub is_healthy: bool,
    pub error_message: Option<String>,
    pub last_check: DateTime<Utc>,
    pub block_height: Option<i64>,
    pub is_syncing: Option<bool>,
    pub is_catching_up: bool,
    pub validator_address: Option<String>,
    pub network: String,
    pub server_host: String,
    pub enabled: bool,
    pub in_maintenance: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: String,
    pub result: Option<StatusResult>,
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResult {
    pub node_info: NodeInfo,
    pub sync_info: SyncInfo,
    pub validator_info: ValidatorInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub network: String,
    pub moniker: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncInfo {
    pub latest_block_height: String,
    pub catching_up: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    pub address: String,
    pub voting_power: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Clone)]
struct AlertState {
    first_unhealthy: DateTime<Utc>,
    last_alert_sent: DateTime<Utc>,
    alert_count: u32,
}

// Auto-restore cooldown tracking
#[derive(Debug, Clone)]
struct AutoRestoreCooldown {
    last_restore_attempt: DateTime<Utc>,
    restore_count: u32,
}

// Block height tracking for progression detection
#[derive(Debug, Clone)]
struct BlockHeightState {
    last_height: i64,
    last_updated: DateTime<Utc>,
    consecutive_no_progress: u32,
}

pub struct HealthMonitor {
    config: Arc<Config>,
    database: Arc<Database>,
    maintenance_tracker: Arc<MaintenanceTracker>,
    snapshot_manager: Arc<SnapshotManager>,
    client: HttpClient,
    alert_states: Arc<tokio::sync::Mutex<HashMap<String, AlertState>>>,
    previous_health_states: Arc<tokio::sync::Mutex<HashMap<String, bool>>>,
    // Auto-restore cooldown tracking (2 hour cooldown)
    auto_restore_cooldowns: Arc<tokio::sync::Mutex<HashMap<String, AutoRestoreCooldown>>>,
    // Block height progression tracking
    block_height_states: Arc<tokio::sync::Mutex<HashMap<String, BlockHeightState>>>,
    // NEW: Track which nodes have already been checked for auto-restore triggers in their current unhealthy state
    auto_restore_checked_states: Arc<tokio::sync::Mutex<HashMap<String, bool>>>,
}

impl HealthMonitor {
    pub fn new(
        config: Arc<Config>,
        database: Arc<Database>,
        maintenance_tracker: Arc<MaintenanceTracker>,
        snapshot_manager: Arc<SnapshotManager>,
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
            client,
            alert_states: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            previous_health_states: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            auto_restore_cooldowns: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            block_height_states: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            auto_restore_checked_states: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
        }
    }

    pub async fn check_all_nodes(&self) -> Result<Vec<HealthStatus>> {
        let mut health_statuses = Vec::new();
        let mut tasks = Vec::new();

        for (node_name, node_config) in &self.config.nodes {
            if !node_config.enabled {
                continue;
            }

            let task = {
                let node_name = node_name.clone();
                let node_config = node_config.clone();
                let monitor = self.clone();
                tokio::spawn(async move { monitor.check_node_health(&node_name, &node_config).await })
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

        // Store results in database and handle state transitions
        for status in &health_statuses {
            if let Err(e) = self.store_health_record(status).await {
                error!("Failed to store health record for {}: {}", status.node_name, e);
            }

            if let Err(e) = self.handle_health_state_change(status).await {
                error!("Failed to handle health state change for {}: {}", status.node_name, e);
            }
        }

        // Log monitoring if enabled (only for healthy nodes)
        if self.config.log_monitoring_enabled.unwrap_or(false) {
            if let Err(e) = self.monitor_logs(&health_statuses).await {
                error!("Log monitoring failed: {}", e);
            }
        }

        // FIXED: Auto-restore monitoring - only check UNHEALTHY nodes ONCE per unhealthy period
        if let Err(e) = self.monitor_auto_restore_triggers(&health_statuses).await {
            error!("Auto-restore monitoring failed: {}", e);
        }

        Ok(health_statuses)
    }

    // FIXED: Only monitor auto-restore triggers for UNHEALTHY nodes and only ONCE per unhealthy period
    async fn monitor_auto_restore_triggers(&self, health_statuses: &[HealthStatus]) -> Result<()> {
        let trigger_words = match &self.config.auto_restore_trigger_words {
            Some(words) if !words.is_empty() => words,
            _ => return Ok(()), // No trigger words configured
        };

        // Only check UNHEALTHY nodes for auto-restore triggers
        let unhealthy_nodes: Vec<_> = health_statuses.iter()
            .filter(|status| !status.is_healthy && status.enabled && !status.in_maintenance)
            .collect();

        if unhealthy_nodes.is_empty() {
            debug!("No unhealthy nodes to check for auto-restore triggers");
            return Ok(());
        }

        info!("Checking auto-restore triggers for {} unhealthy nodes", unhealthy_nodes.len());

        let mut tasks = Vec::new();

        for status in unhealthy_nodes {
            // FIXED: Check if we've already checked this node during its current unhealthy state
            if self.has_already_checked_auto_restore(&status.node_name).await {
                debug!("Auto-restore triggers already checked for {} during current unhealthy state", status.node_name);
                continue;
            }

            let node_config = match self.config.nodes.get(&status.node_name) {
                Some(config) => config,
                None => continue,
            };

            // Only check nodes with auto-restore enabled
            if !node_config.auto_restore_enabled.unwrap_or(false) || !node_config.snapshots_enabled.unwrap_or(false) {
                continue;
            }

            let log_path = match &node_config.log_path {
                Some(path) => path,
                None => continue,
            };

            let node_name = status.node_name.clone();
            let server_host = status.server_host.clone();
            let log_path = log_path.clone();
            let trigger_words = trigger_words.clone();
            let monitor = self.clone();

            let task = tokio::spawn(async move {
                monitor.check_auto_restore_triggers_for_node(&node_name, &server_host, &log_path, &trigger_words).await
            });

            tasks.push(task);
        }

        // Wait for all auto-restore trigger checks
        let results = join_all(tasks).await;
        for result in results {
            if let Err(e) = result {
                error!("Auto-restore trigger check task failed: {}", e);
            }
        }

        Ok(())
    }

    // NEW: Check if we've already checked auto-restore triggers for this node during its current unhealthy state
    async fn has_already_checked_auto_restore(&self, node_name: &str) -> bool {
        let checked_states = self.auto_restore_checked_states.lock().await;
        checked_states.get(node_name).copied().unwrap_or(false)
    }

    // NEW: Mark a node as checked for auto-restore triggers during its current unhealthy state
    async fn mark_auto_restore_checked(&self, node_name: &str) {
        let mut checked_states = self.auto_restore_checked_states.lock().await;
        checked_states.insert(node_name.to_string(), true);
        debug!("Marked {} as checked for auto-restore triggers", node_name);
    }

    // NEW: Clear auto-restore checked flag when node becomes healthy (so it can be checked again if it becomes unhealthy later)
    async fn clear_auto_restore_checked(&self, node_name: &str) {
        let mut checked_states = self.auto_restore_checked_states.lock().await;
        if checked_states.remove(node_name).is_some() {
            debug!("Cleared auto-restore checked flag for {} (node recovered)", node_name);
        }
    }

    async fn check_auto_restore_triggers_for_node(
        &self,
        node_name: &str,
        server_host: &str,
        log_path: &str,
        trigger_words: &[String],
    ) -> Result<()> {
        // Check cooldown first
        if !self.is_auto_restore_allowed(node_name).await {
            debug!("Auto-restore for {} is in cooldown period", node_name);
            // FIXED: Still mark as checked even if in cooldown, so we don't keep trying
            self.mark_auto_restore_checked(node_name).await;
            return Ok(());
        }

        // Get server config for HTTP connection
        let server_config = self.config.servers.get(server_host)
            .ok_or_else(|| anyhow!("Server {} not found", server_host))?;

        // Check only latest 500 lines instead of 1000
        let log_file = format!("{}/out1.log", log_path);
        let command = format!(
            "tail -n 500 '{}' | grep -q -E '{}'",
            log_file,
            trigger_words.join("|")
        );

        // FIXED: Mark as checked BEFORE performing the check to prevent duplicate checks
        self.mark_auto_restore_checked(node_name).await;

        match self.execute_log_command(server_config, &command).await {
            Ok(_) => {
                // Trigger words found - execute auto-restore
                warn!("Auto-restore trigger words found in {} log file: {}", node_name, log_file);
                if let Err(e) = self.execute_auto_restore(node_name, trigger_words).await {
                    error!("Auto-restore failed for {}: {}", node_name, e);
                    self.send_auto_restore_failed_alert(node_name, &e.to_string()).await?;
                } else {
                    info!("Auto-restore completed successfully for {}", node_name);
                }
            }
            Err(_) => {
                // No trigger words found - this is normal
                debug!("No auto-restore trigger words found for {}", node_name);
            }
        }

        Ok(())
    }

    // Check if auto-restore is allowed (cooldown mechanism)
    async fn is_auto_restore_allowed(&self, node_name: &str) -> bool {
        let cooldowns = self.auto_restore_cooldowns.lock().await;
        let now = Utc::now();

        match cooldowns.get(node_name) {
            Some(cooldown) => {
                let hours_since_last = (now - cooldown.last_restore_attempt).num_hours();
                // 2 hour cooldown between restore attempts
                if hours_since_last >= 2 {
                    true
                } else {
                    debug!("Auto-restore for {} is in cooldown ({}h remaining)",
                          node_name, 2 - hours_since_last);
                    false
                }
            }
            None => true, // No previous restore attempts
        }
    }

    // Execute auto-restore with cooldown tracking
    async fn execute_auto_restore(&self, node_name: &str, trigger_words: &[String]) -> Result<()> {
        info!("Executing auto-restore for {} due to trigger words: {:?}", node_name, trigger_words);

        // Update cooldown tracking
        {
            let mut cooldowns = self.auto_restore_cooldowns.lock().await;
            let now = Utc::now();
            let cooldown = cooldowns.entry(node_name.to_string()).or_insert(AutoRestoreCooldown {
                last_restore_attempt: now,
                restore_count: 0,
            });
            cooldown.last_restore_attempt = now;
            cooldown.restore_count += 1;
        }

        // Send notification that auto-restore is starting
        self.send_auto_restore_alert(node_name, "starting", trigger_words).await?;

        // Execute the restore using SnapshotManager
        match self.snapshot_manager.restore_from_snapshot(node_name).await {
            Ok(snapshot_info) => {
                info!("Auto-restore completed for {} using snapshot: {}", node_name, snapshot_info.filename);
                self.send_auto_restore_alert(node_name, "completed", trigger_words).await?;
                Ok(())
            }
            Err(e) => {
                error!("Auto-restore failed for {}: {}", node_name, e);
                self.send_auto_restore_alert(node_name, "failed", trigger_words).await?;
                Err(e)
            }
        }
    }

    // Send auto-restore notifications
    async fn send_auto_restore_alert(&self, node_name: &str, status: &str, trigger_words: &[String]) -> Result<()> {
        if self.config.alarm_webhook_url.is_empty() {
            return Ok(());
        }

        let severity = match status {
            "failed" => "critical",
            "starting" => "warning",
            "completed" => "info",
            _ => "info",
        };

        let message = match status {
            "starting" => format!("Auto-restore STARTED for {} due to corruption indicators", node_name),
            "completed" => format!("Auto-restore COMPLETED for {} - node should be syncing from restored state", node_name),
            "failed" => format!("Auto-restore FAILED for {} - manual intervention required", node_name),
            _ => format!("Auto-restore {} for {}", status, node_name),
        };

        let payload = serde_json::json!({
            "node_name": node_name,
            "message": message,
            "trigger_words": trigger_words,
            "status": status,
            "timestamp": Utc::now(),
            "alert_type": "auto_restore",
            "severity": severity
        });

        match timeout(
            Duration::from_secs(10),
            self.client.post(&self.config.alarm_webhook_url)
                .json(&payload)
                .send(),
        )
        .await
        {
            Ok(Ok(response)) => {
                if response.status().is_success() {
                    info!("Auto-restore alert sent for {}: {}", node_name, status);
                } else {
                    warn!("Auto-restore alert webhook returned status: {} for {}", response.status(), node_name);
                }
            }
            Ok(Err(e)) => {
                warn!("Failed to send auto-restore alert for {}: {}", node_name, e);
            }
            Err(_) => {
                warn!("Auto-restore alert timeout for {}", node_name);
            }
        }

        Ok(())
    }

    async fn send_auto_restore_failed_alert(&self, node_name: &str, error_message: &str) -> Result<()> {
        if self.config.alarm_webhook_url.is_empty() {
            return Ok(());
        }

        let payload = serde_json::json!({
            "node_name": node_name,
            "message": format!("CRITICAL: Auto-restore failed for {} - manual intervention required", node_name),
            "error_message": error_message,
            "timestamp": Utc::now(),
            "alert_type": "auto_restore_failure",
            "severity": "critical"
        });

        let _ = timeout(
            Duration::from_secs(10),
            self.client.post(&self.config.alarm_webhook_url)
                .json(&payload)
                .send(),
        ).await;

        Ok(())
    }

    async fn handle_health_state_change(&self, status: &HealthStatus) -> Result<()> {
        let mut previous_states = self.previous_health_states.lock().await;
        let previous_health = previous_states.get(&status.node_name).copied();

        previous_states.insert(status.node_name.clone(), status.is_healthy);
        drop(previous_states);

        match (previous_health, status.is_healthy, status.in_maintenance) {
            (Some(true), false, false) | (None, false, false) => {
                info!("Node {} became unhealthy - sending alert", status.node_name);
                if let Err(e) = self.send_progressive_alert(status).await {
                    error!("Failed to send alert for {}: {}", status.node_name, e);
                }
            }

            (Some(false), true, _) => {
                info!("Node {} recovered - sending recovery notification", status.node_name);
                // FIXED: Clear auto-restore checked flag when node recovers
                self.clear_auto_restore_checked(&status.node_name).await;

                if let Err(e) = self.send_recovery_notification(status).await {
                    error!("Failed to send recovery notification for {}: {}", status.node_name, e);
                }
            }

            (Some(false), false, false) => {
                if let Err(e) = self.send_progressive_alert(status).await {
                    error!("Failed to send alert for {}: {}", status.node_name, e);
                }
            }

            _ => {
                debug!("No health state change notification needed for {}", status.node_name);
            }
        }

        Ok(())
    }

    async fn send_progressive_alert(&self, status: &HealthStatus) -> Result<()> {
        let mut alert_states = self.alert_states.lock().await;
        let now = Utc::now();

        let should_send_alert = match alert_states.get_mut(&status.node_name) {
            None => {
                let alert_state = AlertState {
                    first_unhealthy: now,
                    last_alert_sent: now,
                    alert_count: 1,
                };
                alert_states.insert(status.node_name.clone(), alert_state);
                true
            }

            Some(alert_state) => {
                let hours_since_first = (now - alert_state.first_unhealthy).num_hours();
                let hours_since_last = (now - alert_state.last_alert_sent).num_hours();

                let should_alert = match alert_state.alert_count {
                    1 => hours_since_first >= 3,
                    2 => hours_since_first >= 6,
                    3 => hours_since_first >= 12,
                    _ => hours_since_last >= 24,
                };

                if should_alert {
                    alert_state.last_alert_sent = now;
                    alert_state.alert_count += 1;
                    true
                } else {
                    false
                }
            }
        };

        drop(alert_states);

        if should_send_alert {
            self.send_webhook_alert(status).await?;
        }

        Ok(())
    }

    async fn send_recovery_notification(&self, status: &HealthStatus) -> Result<()> {
        let mut alert_states = self.alert_states.lock().await;
        alert_states.remove(&status.node_name);
        drop(alert_states);

        let payload = serde_json::json!({
            "node_name": status.node_name,
            "network": status.network,
            "server_host": status.server_host,
            "message": "Node has recovered and is now healthy",
            "rpc_url": status.rpc_url,
            "timestamp": status.last_check,
            "is_healthy": true,
            "block_height": status.block_height,
            "recovery": true,
            "alert_type": "node_recovery"
        });

        match timeout(
            Duration::from_secs(10),
            self.client.post(&self.config.alarm_webhook_url)
                .json(&payload)
                .send(),
        )
        .await
        {
            Ok(Ok(response)) => {
                if response.status().is_success() {
                    info!("Recovery notification sent for node: {}", status.node_name);
                } else {
                    warn!("Recovery notification webhook returned status: {} for {}",
                          response.status(), status.node_name);
                }
            }
            Ok(Err(e)) => {
                warn!("Failed to send recovery notification for {}: {}", status.node_name, e);
            }
            Err(_) => {
                warn!("Recovery notification timeout for {}", status.node_name);
            }
        }

        Ok(())
    }

    // Enhanced health check with block progression tracking
    pub async fn check_node_health(&self, node_name: &str, node_config: &NodeConfig) -> Result<HealthStatus> {
        let is_in_maintenance = self.maintenance_tracker
            .is_in_maintenance(node_name)
            .await;

        let mut status = HealthStatus {
            node_name: node_name.to_string(),
            rpc_url: node_config.rpc_url.clone(),
            is_healthy: false,
            error_message: None,
            last_check: Utc::now(),
            block_height: None,
            is_syncing: None,
            is_catching_up: false,
            validator_address: None,
            network: node_config.network.clone(),
            server_host: node_config.server_host.clone(),
            enabled: node_config.enabled,
            in_maintenance: is_in_maintenance,
        };

        if is_in_maintenance {
            status.error_message = Some("Node is in maintenance mode".to_string());
            return Ok(status);
        }

        match self.fetch_node_status(&node_config.rpc_url).await {
            Ok(rpc_response) => {
                if let Some(result) = rpc_response.result {
                    let current_height = result.sync_info.latest_block_height.parse::<i64>().unwrap_or(0);
                    let is_catching_up = result.sync_info.catching_up;

                    status.block_height = Some(current_height);
                    status.is_catching_up = is_catching_up;
                    status.is_syncing = Some(is_catching_up);
                    status.validator_address = Some(result.validator_info.address);

                    // Check block progression for health determination
                    let block_progression_healthy = self.check_block_progression(node_name, current_height).await;

                    // Node is healthy if:
                    // 1. RPC responds successfully
                    // 2. Block height is progressing (or node is catching up, which is normal)
                    status.is_healthy = block_progression_healthy || is_catching_up;

                    // Set appropriate error message if not healthy
                    if !status.is_healthy && !is_catching_up {
                        status.error_message = Some("Block height not progressing".to_string());
                    }

                } else if let Some(error) = rpc_response.error {
                    status.error_message = Some(format!("RPC Error: {}", error.message));
                } else {
                    status.error_message = Some("Unknown RPC response format".to_string());
                }
            }
            Err(e) => {
                status.error_message = Some(e.to_string());
            }
        }

        Ok(status)
    }

    // Check if block height is progressing
    async fn check_block_progression(&self, node_name: &str, current_height: i64) -> bool {
        let mut block_states = self.block_height_states.lock().await;
        let now = Utc::now();

        match block_states.get_mut(node_name) {
            None => {
                // First check for this node
                block_states.insert(node_name.to_string(), BlockHeightState {
                    last_height: current_height,
                    last_updated: now,
                    consecutive_no_progress: 0,
                });
                true // Assume healthy on first check
            }
            Some(state) => {
                let minutes_since_update = (now - state.last_updated).num_minutes();

                // Only check progression if enough time has passed (at least 2 minutes)
                if minutes_since_update < 2 {
                    return true; // Too soon to judge, assume healthy
                }

                if current_height > state.last_height {
                    // Block height increased - healthy
                    state.last_height = current_height;
                    state.last_updated = now;
                    state.consecutive_no_progress = 0;
                    true
                } else if current_height == state.last_height {
                    // Block height stayed the same
                    state.consecutive_no_progress += 1;
                    state.last_updated = now;

                    // Allow up to 3 consecutive checks without progress (about 6 minutes)
                    // before marking as unhealthy
                    state.consecutive_no_progress < 3
                } else {
                    // Block height decreased - this shouldn't happen, mark as unhealthy
                    state.last_height = current_height;
                    state.last_updated = now;
                    state.consecutive_no_progress += 1;
                    false
                }
            }
        }
    }

    async fn fetch_node_status(&self, rpc_url: &str) -> Result<RpcResponse> {
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "status",
            "params": [],
            "id": Uuid::new_v4().to_string()
        });

        let response = timeout(
            Duration::from_secs(self.config.rpc_timeout_seconds),
            self.client.post(rpc_url).json(&request_body).send(),
        )
        .await
        .map_err(|_| anyhow!("RPC request timeout"))?
        .map_err(|e| anyhow!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP error {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            ));
        }

        let rpc_response: RpcResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse JSON response: {}", e))?;

        Ok(rpc_response)
    }

    pub async fn get_node_health(&self, node_name: &str) -> Result<Option<HealthStatus>> {
        let record = self.database.get_latest_health_record(node_name).await?;

        match record {
            Some(record) => {
                let node_config = self.config.nodes.get(node_name)
                    .ok_or_else(|| anyhow!("Node {} not found in configuration", node_name))?;

                let is_in_maintenance = self.maintenance_tracker
                    .is_in_maintenance(node_name)
                    .await;

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

    pub async fn force_check_node(&self, node_name: &str) -> Result<HealthStatus> {
        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow!("Node {} not found in configuration", node_name))?;

        let status = self.check_node_health(node_name, node_config).await?;

        if let Err(e) = self.store_health_record(&status).await {
            error!("Failed to store health record for {}: {}", status.node_name, e);
        }

        if let Err(e) = self.handle_health_state_change(&status).await {
            error!("Failed to handle health state change for {}: {}", status.node_name, e);
        }

        Ok(status)
    }

    pub async fn get_health_history(&self, node_name: &str, limit: Option<i32>) -> Result<Vec<HealthRecord>> {
        let _ = (node_name, limit);
        Ok(Vec::new())
    }

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

    async fn send_webhook_alert(&self, status: &HealthStatus) -> Result<()> {
        let payload = serde_json::json!({
            "node_name": status.node_name,
            "network": status.network,
            "server_host": status.server_host,
            "error_message": status.error_message,
            "rpc_url": status.rpc_url,
            "timestamp": status.last_check,
            "is_healthy": status.is_healthy,
            "block_height": status.block_height,
            "is_catching_up": status.is_catching_up,
            "alert_type": "node_unhealthy"
        });

        let response = timeout(
            Duration::from_secs(10),
            self.client.post(&self.config.alarm_webhook_url)
                .json(&payload)
                .send(),
        )
        .await
        .map_err(|_| anyhow!("Webhook timeout"))?
        .map_err(|e| anyhow!("Webhook request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("Webhook returned status: {}", response.status()));
        }

        debug!("Alert sent for node: {}", status.node_name);
        Ok(())
    }

    async fn monitor_logs(&self, health_statuses: &[HealthStatus]) -> Result<()> {
        let patterns = match &self.config.log_monitoring_patterns {
            Some(patterns) if !patterns.is_empty() => patterns,
            _ => return Ok(()),
        };

        let context_lines_value: i32 = self.config.log_monitoring_context_lines.unwrap_or(2);
        let mut tasks = Vec::new();

        for status in health_statuses {
            if !status.is_healthy {
                continue;
            }

            let node_config = match self.config.nodes.get(&status.node_name) {
                Some(config) => config,
                None => continue,
            };

            let log_path = match &node_config.log_path {
                Some(path) => path,
                None => continue,
            };

            let node_name = status.node_name.clone();
            let server_host = status.server_host.clone();
            let log_path = log_path.clone();
            let patterns = patterns.clone();
            let monitor = self.clone();
            let context_lines = context_lines_value;

            let task = tokio::spawn(async move {
                monitor.check_node_logs(&node_name, &server_host, &log_path, &patterns, context_lines).await
            });

            tasks.push(task);
        }

        let results = join_all(tasks).await;
        for result in results {
            if let Err(e) = result {
                error!("Log monitoring task failed: {}", e);
            }
        }

        Ok(())
    }

    async fn check_node_logs(&self, node_name: &str, server_host: &str, log_path: &str, patterns: &[String], context_lines: i32) -> Result<()> {
        let server_config = self.config.servers.get(server_host)
            .ok_or_else(|| anyhow!("Server {} not found", server_host))?;

        let command = format!(
            "tail -n 1000 {}/out1.log | grep -n -A {} -B {} -E '{}'",
            log_path,
            context_lines,
            context_lines,
            patterns.join("|")
        );

        match self.execute_log_command(server_config, &command).await {
            Ok(output) => {
                if !output.trim().is_empty() {
                    self.send_log_alert(node_name, server_host, log_path, &output).await?;
                }
            }
            Err(e) => {
                debug!("Log monitoring for {} failed: {}", node_name, e);
            }
        }

        Ok(())
    }

    async fn execute_log_command(&self, server_config: &ServerConfig, command: &str) -> Result<String> {
        let agent_url = format!("http://{}:{}/command/execute", server_config.host, server_config.agent_port);

        let payload = serde_json::json!({
            "command": command,
            "api_key": server_config.api_key
        });

        let response = timeout(
            Duration::from_secs(server_config.request_timeout_seconds),
            self.client.post(&agent_url).json(&payload).send(),
        )
        .await
        .map_err(|_| anyhow!("Log command timeout"))?
        .map_err(|e| anyhow!("Log command request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("Log command returned status: {}", response.status()));
        }

        let result: serde_json::Value = response.json().await?;
        let output = result.get("output")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(output)
    }

    async fn send_log_alert(&self, node_name: &str, server_host: &str, log_path: &str, log_output: &str) -> Result<()> {
        let payload = serde_json::json!({
            "node_name": node_name,
            "server_host": server_host,
            "log_path": log_path,
            "log_output": log_output,
            "timestamp": Utc::now(),
            "alert_type": "log_pattern_match"
        });

        let response = timeout(
            Duration::from_secs(10),
            self.client.post(&self.config.alarm_webhook_url)
                .json(&payload)
                .send(),
        )
        .await
        .map_err(|_| anyhow!("Log alert webhook timeout"))?
        .map_err(|e| anyhow!("Log alert webhook request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("Log alert webhook returned status: {}", response.status()));
        }

        debug!("Log alert sent for node: {}", node_name);
        Ok(())
    }
}

impl Clone for HealthMonitor {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            database: self.database.clone(),
            maintenance_tracker: self.maintenance_tracker.clone(),
            snapshot_manager: self.snapshot_manager.clone(),
            client: self.client.clone(),
            alert_states: self.alert_states.clone(),
            previous_health_states: self.previous_health_states.clone(),
            auto_restore_cooldowns: self.auto_restore_cooldowns.clone(),
            block_height_states: self.block_height_states.clone(),
            auto_restore_checked_states: self.auto_restore_checked_states.clone(),
        }
    }
}
