// File: src/health/monitor.rs (simple escalating alarms)

use anyhow::Result;
use chrono::Utc;
use reqwest::Client;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::database::Database;
use crate::health::{parse_rpc_response, parse_block_height, parse_block_time, HealthMetrics, HealthThresholds};
use crate::{AlarmPayload, Config, HealthStatus, NodeConfig, NodeHealth};

pub struct HealthMonitor {
    config: Arc<Config>,
    database: Arc<Database>,
    http_client: Client,
    thresholds: HealthThresholds,
    failure_counts: Arc<tokio::sync::RwLock<HashMap<String, u32>>>,
    last_alarm_times: Arc<tokio::sync::RwLock<HashMap<String, chrono::DateTime<Utc>>>>,
    alarm_counts: Arc<tokio::sync::RwLock<HashMap<String, u32>>>,
}

impl HealthMonitor {
    pub fn new(config: Arc<Config>, database: Arc<Database>) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(config.rpc_timeout_seconds))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            database,
            http_client,
            thresholds: HealthThresholds::default(),
            failure_counts: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            last_alarm_times: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            alarm_counts: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    pub async fn start_monitoring(&self) -> Result<()> {
        info!("Starting health monitoring service");

        let check_interval = Duration::from_secs(self.config.check_interval_seconds);
        info!("Health check interval: {}s", check_interval.as_secs());

        loop {
            let start_time = Instant::now();

            // Get all enabled nodes
            let enabled_nodes: Vec<(&String, &NodeConfig)> = self
                .config
                .nodes
                .iter()
                .filter(|(_, node)| node.enabled)
                .collect();

            info!("Checking health for {} enabled nodes", enabled_nodes.len());

            // Check all nodes in parallel
            let mut tasks = Vec::new();
            for (node_name, node_config) in enabled_nodes {
                let monitor = self.clone();
                let name = node_name.clone();
                let config = node_config.clone();

                let task = tokio::spawn(async move {
                    monitor.check_and_process_node_health(&name, &config).await
                });
                tasks.push(task);
            }

            // Wait for all health checks to complete
            let results = futures::future::join_all(tasks).await;
            let mut successful_checks = 0;
            let mut failed_checks = 0;

            for task_result in results {
                match task_result {
                    Ok(check_result) => {
                        match check_result {
                            Ok(_) => successful_checks += 1,
                            Err(e) => {
                                failed_checks += 1;
                                error!("Health check failed: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        failed_checks += 1;
                        error!("Health check task panicked: {}", e);
                    }
                }
            }

            let check_duration = start_time.elapsed();
            info!(
                "Health check cycle completed in {:.2}s: {} successful, {} failed",
                check_duration.as_secs_f64(),
                successful_checks,
                failed_checks
            );

            // Cleanup old health records periodically (once per hour)
            if Utc::now().timestamp() % 3600 < self.config.check_interval_seconds as i64 {
                if let Err(e) = self.database.cleanup_old_health_records(7).await {
                    warn!("Failed to cleanup old health records: {}", e);
                } else {
                    debug!("Cleaned up old health records");
                }
            }

            // Wait for next check interval
            sleep(check_interval).await;
        }
    }

    async fn check_and_process_node_health(&self, node_name: &str, node_config: &NodeConfig) -> Result<()> {
        debug!("Checking health for node: {}", node_name);

        let health = self.check_node_health(node_name, node_config).await;

        // Handle recovery notification
        if matches!(health.status, HealthStatus::Healthy) {
            // Check if node was previously unhealthy and send recovery notification
            let had_alarms = {
                let alarm_counts = self.alarm_counts.read().await;
                alarm_counts.get(node_name).copied().unwrap_or(0) > 0
            };

            if had_alarms {
                self.send_recovery_notification(node_name).await.ok();
                // Reset alarm state
                {
                    let mut alarm_counts = self.alarm_counts.write().await;
                    let mut last_alarms = self.last_alarm_times.write().await;
                    alarm_counts.remove(node_name);
                    last_alarms.remove(node_name);
                }
            }
        }

        // Save to database
        if let Err(e) = self.database.save_node_health(&health).await {
            error!("Failed to save health data for node {}: {}", node_name, e);
        }

        // Check if we need to send an alarm
        if self.should_send_alarm(node_name, &health).await {
            if let Err(e) = self.send_health_alarm(node_name, &health).await {
                error!("Failed to send alarm for node {}: {}", node_name, e);
            }
        }

        Ok(())
    }

    pub async fn check_node_health(&self, node_name: &str, node: &NodeConfig) -> NodeHealth {
        let start_time = Instant::now();
        let check_time = Utc::now();

        debug!("Making RPC call to {}", node.rpc_url);

        // Prepare RPC request
        let rpc_request = json!({
            "jsonrpc": "2.0",
            "method": "status",
            "params": [],
            "id": 1
        });

        // Make RPC call
        let response_result = self
            .http_client
            .post(&node.rpc_url)
            .json(&rpc_request)
            .send()
            .await;

        let response_time = start_time.elapsed();

        match response_result {
            Ok(response) => {
                if response.status().is_success() {
                    match response.text().await {
                        Ok(response_text) => {
                            match parse_rpc_response(&response_text) {
                                Ok(rpc_status) => {
                                    let block_height = parse_block_height(&rpc_status.result.sync_info.latest_block_height);
                                    let block_time = parse_block_time(&rpc_status.result.sync_info.latest_block_time);
                                    let catching_up = rpc_status.result.sync_info.catching_up;

                                    // Reset failure count when RPC succeeds
                                    {
                                        let mut failure_counts = self.failure_counts.write().await;
                                        failure_counts.insert(node_name.to_string(), 0);
                                    }

                                    // Create health metrics
                                    let metrics = HealthMetrics {
                                        node_name: node_name.to_string(),
                                        network: node.network.clone(),
                                        block_height,
                                        block_time,
                                        catching_up,
                                        response_time_ms: response_time.as_millis() as u64,
                                        peers_count: None,
                                        last_check: check_time,
                                        consecutive_failures: 0,
                                    };

                                    let status = if metrics.is_healthy(&self.thresholds) {
                                        HealthStatus::Healthy
                                    } else if catching_up {
                                        HealthStatus::Unknown
                                    } else {
                                        HealthStatus::Unhealthy
                                    };

                                    NodeHealth {
                                        node_name: node_name.to_string(),
                                        status,
                                        latest_block_height: block_height,
                                        latest_block_time: block_time.map(|dt| dt.to_rfc3339()),
                                        catching_up: Some(catching_up),
                                        last_check: check_time,
                                        error_message: None,
                                    }
                                }
                                Err(e) => {
                                    self.increment_failure_count(node_name).await;
                                    NodeHealth {
                                        node_name: node_name.to_string(),
                                        status: HealthStatus::Unhealthy,
                                        latest_block_height: None,
                                        latest_block_time: None,
                                        catching_up: None,
                                        last_check: check_time,
                                        error_message: Some(format!("RPC parse error: {}", e)),
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            self.increment_failure_count(node_name).await;
                            NodeHealth {
                                node_name: node_name.to_string(),
                                status: HealthStatus::Unhealthy,
                                latest_block_height: None,
                                latest_block_time: None,
                                catching_up: None,
                                last_check: check_time,
                                error_message: Some(format!("Response read error: {}", e)),
                            }
                        }
                    }
                } else {
                    self.increment_failure_count(node_name).await;
                    NodeHealth {
                        node_name: node_name.to_string(),
                        status: HealthStatus::Unhealthy,
                        latest_block_height: None,
                        latest_block_time: None,
                        catching_up: None,
                        last_check: check_time,
                        error_message: Some(format!("HTTP error: {}", response.status())),
                    }
                }
            }
            Err(e) => {
                self.increment_failure_count(node_name).await;
                NodeHealth {
                    node_name: node_name.to_string(),
                    status: HealthStatus::Unhealthy,
                    latest_block_height: None,
                    latest_block_time: None,
                    catching_up: None,
                    last_check: check_time,
                    error_message: Some(format!("Network error: {}", e)),
                }
            }
        }
    }

    async fn increment_failure_count(&self, node_name: &str) {
        let mut failure_counts = self.failure_counts.write().await;
        let count = failure_counts.get(node_name).unwrap_or(&0) + 1;
        failure_counts.insert(node_name.to_string(), count);
    }

    fn get_alarm_interval_hours(&self, alarm_count: u32) -> u64 {
        match alarm_count {
            0 => 0,   // 1st alarm: immediate
            1 => 6,   // 2nd alarm: 6 hours later
            2 => 12,  // 3rd alarm: 12 hours later
            3 => 24,  // 4th alarm: 24 hours later
            _ => 48,  // 5th+ alarms: 48 hours later
        }
    }

    async fn should_send_alarm(&self, node_name: &str, health: &NodeHealth) -> bool {
        if matches!(health.status, HealthStatus::Healthy) {
            return false;
        }

        let failure_count = self.get_failure_count(node_name).await;
        if failure_count < self.thresholds.max_consecutive_failures {
            return false;
        }

        // Check alarm interval
        let alarm_count = {
            let alarm_counts = self.alarm_counts.read().await;
            alarm_counts.get(node_name).copied().unwrap_or(0)
        };

        let required_hours = self.get_alarm_interval_hours(alarm_count);
        if required_hours == 0 {
            return true; // First alarm - send immediately
        }

        let last_alarms = self.last_alarm_times.read().await;
        if let Some(last_alarm) = last_alarms.get(node_name) {
            let hours_since = Utc::now().signed_duration_since(*last_alarm).num_hours();
            hours_since >= required_hours as i64
        } else {
            true // No previous alarm
        }
    }

    async fn send_health_alarm(&self, node_name: &str, health: &NodeHealth) -> Result<()> {
        if self.config.alarm_webhook_url.is_empty() {
            return Ok(());
        }

        let alarm = AlarmPayload {
            timestamp: Utc::now(),
            alarm_type: "node_health".to_string(),
            severity: match health.status {
                HealthStatus::Unhealthy => "high".to_string(),
                HealthStatus::Unknown => "medium".to_string(),
                _ => "low".to_string(),
            },
            node_name: node_name.to_string(),
            message: health.error_message
                .clone()
                .unwrap_or_else(|| format!("Node {} is {:?}", node_name, health.status)),
            details: json!({
                "status": health.status,
                "latest_block_height": health.latest_block_height,
                "latest_block_time": health.latest_block_time,
                "catching_up": health.catching_up,
                "consecutive_failures": self.get_failure_count(node_name).await,
                "server_host": self.get_server_for_node(node_name).await,
            }),
        };

        let response = self
            .http_client
            .post(&self.config.alarm_webhook_url)
            .json(&alarm)
            .send()
            .await?;

        if response.status().is_success() {
            // Update alarm tracking
            {
                let mut last_alarms = self.last_alarm_times.write().await;
                let mut alarm_counts = self.alarm_counts.write().await;
                last_alarms.insert(node_name.to_string(), Utc::now());
                let count = alarm_counts.get(node_name).copied().unwrap_or(0);
                alarm_counts.insert(node_name.to_string(), count + 1);
            }
        }

        Ok(())
    }

    async fn send_recovery_notification(&self, node_name: &str) -> Result<()> {
        if self.config.alarm_webhook_url.is_empty() {
            return Ok(());
        }

        let alarm = AlarmPayload {
            timestamp: Utc::now(),
            alarm_type: "node_recovery".to_string(),
            severity: "info".to_string(),
            node_name: node_name.to_string(),
            message: format!("Node {} has recovered and is now healthy", node_name),
            details: json!({
                "status": "Healthy",
                "server_host": self.get_server_for_node(node_name).await,
            }),
        };

        self.http_client
            .post(&self.config.alarm_webhook_url)
            .json(&alarm)
            .send()
            .await?;

        Ok(())
    }

    async fn get_failure_count(&self, node_name: &str) -> u32 {
        let failure_counts = self.failure_counts.read().await;
        failure_counts.get(node_name).copied().unwrap_or(0)
    }

    async fn get_server_for_node(&self, node_name: &str) -> Option<String> {
        self.config
            .nodes
            .get(node_name)
            .map(|node| node.server_host.clone())
    }

    pub async fn get_all_health_status(&self) -> Result<Vec<NodeHealth>> {
        self.database.get_all_latest_health().await
    }

    pub async fn get_node_health_history(&self, node_name: &str, limit: i32) -> Result<Vec<NodeHealth>> {
        self.database.get_node_health_history(node_name, limit).await
    }

    pub async fn force_health_check(&self, node_name: &str) -> Result<NodeHealth> {
        let node_config = self
            .config
            .nodes
            .get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        let health = self.check_node_health(node_name, node_config).await;
        self.database.save_node_health(&health).await?;
        Ok(health)
    }
}

impl Clone for HealthMonitor {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            database: self.database.clone(),
            http_client: self.http_client.clone(),
            thresholds: self.thresholds.clone(),
            failure_counts: self.failure_counts.clone(),
            last_alarm_times: self.last_alarm_times.clone(),
            alarm_counts: self.alarm_counts.clone(),
        }
    }
}
