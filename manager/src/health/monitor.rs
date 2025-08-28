// File: manager/src/health/monitor.rs
use crate::config::{Config, NodeConfig, ServerConfig};
use crate::database::{Database, HealthRecord};
use crate::maintenance_tracker::MaintenanceTracker;
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

pub struct HealthMonitor {
    config: Arc<Config>,
    database: Arc<Database>,
    maintenance_tracker: Arc<MaintenanceTracker>,
    client: HttpClient,
    last_alert_times: Arc<tokio::sync::Mutex<HashMap<String, DateTime<Utc>>>>,
    // NEW: Track previous health states to detect transitions
    previous_health_states: Arc<tokio::sync::Mutex<HashMap<String, bool>>>,
}

impl HealthMonitor {
    pub fn new(
        config: Arc<Config>,
        database: Arc<Database>,
        maintenance_tracker: Arc<MaintenanceTracker>,
    ) -> Self {
        let client = HttpClient::builder()
            .timeout(Duration::from_secs(config.rpc_timeout_seconds))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            database,
            maintenance_tracker,
            client,
            last_alert_times: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
            previous_health_states: Arc::new(tokio::sync::Mutex::new(HashMap::new())),
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

            // FIXED: Check for state transitions instead of just healthy status
            if let Err(e) = self.handle_health_state_change(status).await {
                error!("Failed to handle health state change for {}: {}", status.node_name, e);
            }
        }

        // Log monitoring if enabled
        if self.config.log_monitoring_enabled.unwrap_or(false) {
            if let Err(e) = self.monitor_logs(&health_statuses).await {
                error!("Log monitoring failed: {}", e);
            }
        }

        Ok(health_statuses)
    }

    // NEW: Handle health state changes with proper transition detection
    async fn handle_health_state_change(&self, status: &HealthStatus) -> Result<()> {
        let mut previous_states = self.previous_health_states.lock().await;
        let previous_health = previous_states.get(&status.node_name).copied();

        // Update the current state
        previous_states.insert(status.node_name.clone(), status.is_healthy);

        // Detect state transitions
        match (previous_health, status.is_healthy, status.in_maintenance) {
            // Node became unhealthy (and not in maintenance) - send alert
            (Some(true), false, false) | (None, false, false) => {
                info!("Node {} became unhealthy - sending alert", status.node_name);
                if let Err(e) = self.send_alert_if_needed(status).await {
                    error!("Failed to send alert for {}: {}", status.node_name, e);
                }
            }

            // Node recovered (was unhealthy, now healthy) - send recovery notification
            (Some(false), true, _) => {
                info!("Node {} recovered - sending recovery notification", status.node_name);
                if let Err(e) = self.send_recovery_notification(status).await {
                    error!("Failed to send recovery notification for {}: {}", status.node_name, e);
                }
            }

            // Node is still unhealthy - check if we need to send repeated alerts
            (Some(false), false, false) => {
                if let Err(e) = self.send_alert_if_needed(status).await {
                    error!("Failed to send alert for {}: {}", status.node_name, e);
                }
            }

            // All other cases: no notification needed
            // (Some(true), true, _) - Still healthy, no action
            // (_, _, true) - In maintenance, handled above
            _ => {
                debug!("No health state change notification needed for {}", status.node_name);
            }
        }

        Ok(())
    }

    // NEW: Separate recovery notification method
    async fn send_recovery_notification(&self, status: &HealthStatus) -> Result<()> {
        // Clear the alert timer since node recovered
        let mut last_alerts = self.last_alert_times.lock().await;
        last_alerts.remove(&status.node_name);
        drop(last_alerts); // Release lock early

        // Send recovery notification
        let payload = serde_json::json!({
            "node_name": status.node_name,
            "network": status.network,
            "server_host": status.server_host,
            "message": "Node has recovered and is now healthy",
            "rpc_url": status.rpc_url,
            "timestamp": status.last_check,
            "is_healthy": true,
            "block_height": status.block_height,
            "recovery": true
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
                    status.is_healthy = true;
                    if let Ok(height) = result.sync_info.latest_block_height.parse::<i64>() {
                        status.block_height = Some(height);
                    }
                    status.is_catching_up = result.sync_info.catching_up;
                    status.is_syncing = Some(result.sync_info.catching_up);
                    status.validator_address = Some(result.validator_info.address);
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

    // NEW: Method to force check a single node (useful for API)
    pub async fn force_check_node(&self, node_name: &str) -> Result<HealthStatus> {
        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow!("Node {} not found in configuration", node_name))?;

        let status = self.check_node_health(node_name, node_config).await?;

        // Store the result
        if let Err(e) = self.store_health_record(&status).await {
            error!("Failed to store health record for {}: {}", status.node_name, e);
        }

        // Handle state transitions
        if let Err(e) = self.handle_health_state_change(&status).await {
            error!("Failed to handle health state change for {}: {}", status.node_name, e);
        }

        Ok(status)
    }

    // NEW: Get health history for a node
    pub async fn get_health_history(&self, node_name: &str, limit: Option<i32>) -> Result<Vec<HealthRecord>> {
        // This would require a new database method - for now return empty
        // You'll need to implement get_health_history in database.rs if needed
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

    async fn send_alert_if_needed(&self, status: &HealthStatus) -> Result<()> {
        let mut last_alerts = self.last_alert_times.lock().await;
        let now = Utc::now();

        let should_alert = match last_alerts.get(&status.node_name) {
            None => true, // First alert
            Some(last_alert) => {
                let hours_since_last = (now - *last_alert).num_hours();

                // Progressive delay: immediate, 6h, 12h, 24h, 48h, then every 48h
                hours_since_last >= match hours_since_last {
                    0..=5 => 0,      // Immediate
                    6..=11 => 6,     // After 6 hours
                    12..=23 => 12,   // After 12 hours
                    24..=47 => 24,   // After 24 hours
                    _ => 48,         // Every 48 hours after that
                }
            }
        };

        if should_alert {
            self.send_webhook_alert(status).await?;
            last_alerts.insert(status.node_name.clone(), now);
        }

        Ok(())
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
            "is_catching_up": status.is_catching_up
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
            _ => return Ok(()), // No patterns configured
        };

        let _interval = self.config.log_monitoring_interval_minutes.unwrap_or(5);
        // Extract the i32 value before the closure to avoid Option type issues
        let context_lines_value: i32 = self.config.log_monitoring_context_lines.unwrap_or(2);

        let mut tasks = Vec::new();

        for status in health_statuses {
            // Only monitor logs for healthy nodes
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

            // Extract all variables before the async closure
            let node_name = status.node_name.clone();
            let server_host = status.server_host.clone();
            let log_path = log_path.clone();
            let patterns = patterns.clone();
            let monitor = self.clone();
            let context_lines = context_lines_value; // Use the extracted i32 value

            let task = tokio::spawn(async move {
                monitor.check_node_logs(&node_name, &server_host, &log_path, &patterns, context_lines).await
            });

            tasks.push(task);
        }

        // Wait for all log monitoring tasks
        let results = join_all(tasks).await;
        for result in results {
            if let Err(e) = result {
                error!("Log monitoring task failed: {}", e);
            }
        }

        Ok(())
    }

    async fn check_node_logs(&self, node_name: &str, server_host: &str, log_path: &str, patterns: &[String], context_lines: i32) -> Result<()> {
        // Get server config for HTTP connection
        let server_config = self.config.servers.get(server_host)
            .ok_or_else(|| anyhow!("Server {} not found", server_host))?;

        // Create HTTP request for log monitoring
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
            client: self.client.clone(),
            last_alert_times: self.last_alert_times.clone(),
            previous_health_states: self.previous_health_states.clone(),
        }
    }
}
