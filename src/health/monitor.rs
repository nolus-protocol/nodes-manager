// File: src/health/monitor.rs

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
use crate::health::{parse_rpc_response, parse_block_height, parse_block_time, HealthThresholds};
use crate::maintenance_tracker::MaintenanceTracker;
use crate::snapshot::SnapshotManager;
use crate::{AlarmPayload, Config, HealthStatus, NodeConfig, NodeHealth};

// NEW: Track block progression for health determination
#[derive(Debug, Clone)]
struct BlockProgressionInfo {
    node_name: String,
    current_height: u64,
    previous_height: Option<u64>,
    last_progression_time: chrono::DateTime<Utc>,
    stuck_check_count: u32,
}

pub struct HealthMonitor {
    config: Arc<Config>,
    database: Arc<Database>,
    http_client: Client,
    thresholds: HealthThresholds,
    failure_counts: Arc<tokio::sync::RwLock<HashMap<String, u32>>>,
    last_alarm_times: Arc<tokio::sync::RwLock<HashMap<String, chrono::DateTime<Utc>>>>,
    alarm_counts: Arc<tokio::sync::RwLock<HashMap<String, u32>>>,
    maintenance_tracker: Arc<MaintenanceTracker>,
    snapshot_manager: Arc<SnapshotManager>,
    auto_restore_attempts: Arc<tokio::sync::RwLock<HashMap<String, chrono::DateTime<Utc>>>>,
    // NEW: Track block progression
    block_progression: Arc<tokio::sync::RwLock<HashMap<String, BlockProgressionInfo>>>,
    // Log monitoring state
    last_log_check_times: Arc<tokio::sync::RwLock<HashMap<String, chrono::DateTime<Utc>>>>,
    log_alarm_counts: Arc<tokio::sync::RwLock<HashMap<String, u32>>>,
}

impl HealthMonitor {
    pub fn new(
        config: Arc<Config>,
        database: Arc<Database>,
        maintenance_tracker: Arc<MaintenanceTracker>,
        snapshot_manager: Arc<SnapshotManager>,
    ) -> Self {
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
            maintenance_tracker,
            snapshot_manager,
            auto_restore_attempts: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            block_progression: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            last_log_check_times: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            log_alarm_counts: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    pub async fn start_monitoring(&self) -> Result<()> {
        info!("Starting health monitoring service with block progression-based health checks and log monitoring");

        let check_interval = Duration::from_secs(self.config.check_interval_seconds);
        info!("Health check interval: {}s", check_interval.as_secs());

        // Log monitoring configuration
        if self.config.log_monitoring_enabled {
            info!("Log monitoring enabled with {} patterns, checking every {} minutes",
                  self.config.log_monitoring_patterns.len(),
                  self.config.log_monitoring_interval_minutes);

            // Start log monitoring task
            self.start_log_monitoring_task().await;
        } else {
            info!("Log monitoring disabled");
        }

        // Start maintenance cleanup task
        self.start_maintenance_cleanup_task().await;

        loop {
            let start_time = Instant::now();

            // Get all enabled nodes
            let enabled_nodes: Vec<(&String, &NodeConfig)> = self
                .config
                .nodes
                .iter()
                .filter(|(_, node)| node.enabled)
                .collect();

            info!("Checking health for {} enabled nodes using block progression logic", enabled_nodes.len());

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
            let mut maintenance_checks = 0;

            for task_result in results {
                match task_result {
                    Ok(check_result) => {
                        match check_result {
                            Ok(health) => {
                                successful_checks += 1;
                                if matches!(health.status, HealthStatus::Maintenance) {
                                    maintenance_checks += 1;
                                }
                            }
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
                "Health check cycle completed in {:.2}s: {} successful, {} failed, {} in maintenance",
                check_duration.as_secs_f64(),
                successful_checks,
                failed_checks,
                maintenance_checks
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

    // Log monitoring task
    async fn start_log_monitoring_task(&self) {
        let monitor = self.clone();

        tokio::spawn(async move {
            let log_check_interval = Duration::from_secs(monitor.config.log_monitoring_interval_minutes * 60);
            info!("Starting log monitoring task - checking every {} minutes", monitor.config.log_monitoring_interval_minutes);

            loop {
                sleep(log_check_interval).await;

                let start_time = Instant::now();
                let enabled_nodes: Vec<(&String, &NodeConfig)> = monitor
                    .config
                    .nodes
                    .iter()
                    .filter(|(_, node)| node.enabled)
                    .collect();

                debug!("Log monitoring: checking {} enabled nodes", enabled_nodes.len());
                let mut checked_nodes = 0;
                let mut pattern_matches = 0;

                for (node_name, node_config) in enabled_nodes {
                    // Only check logs for healthy nodes (not in maintenance)
                    let (in_maintenance, _) = monitor.maintenance_tracker.get_maintenance_status_atomic(node_name).await;
                    if in_maintenance {
                        debug!("Log monitoring: skipping {} - in maintenance", node_name);
                        continue;
                    }

                    // Check if node has health status as healthy
                    if let Ok(Some(health)) = monitor.database.get_latest_node_health(node_name).await {
                        if !matches!(health.status, HealthStatus::Healthy) {
                            debug!("Log monitoring: skipping {} - not healthy", node_name);
                            continue;
                        }
                    } else {
                        debug!("Log monitoring: skipping {} - no health data", node_name);
                        continue;
                    }

                    checked_nodes += 1;

                    match monitor.check_log_patterns(node_name, node_config).await {
                        Ok(found_patterns) => {
                            if found_patterns > 0 {
                                pattern_matches += found_patterns;
                                info!("Log monitoring: found {} pattern matches for node {}", found_patterns, node_name);
                            } else {
                                debug!("Log monitoring: no patterns found for node {}", node_name);
                            }
                        }
                        Err(e) => {
                            warn!("Log monitoring: failed to check patterns for node {}: {}", node_name, e);
                        }
                    }
                }

                let check_duration = start_time.elapsed();
                info!("Log monitoring cycle completed in {:.2}s: {} nodes checked, {} pattern matches found",
                      check_duration.as_secs_f64(), checked_nodes, pattern_matches);
            }
        });
    }

    // Check log patterns for a specific node
    async fn check_log_patterns(&self, node_name: &str, node_config: &NodeConfig) -> Result<u32> {
        if self.config.log_monitoring_patterns.is_empty() {
            return Ok(0);
        }

        // Get log path - use node's log_path and append "/out1.log"
        let log_file_path = if let Some(log_path) = &node_config.log_path {
            format!("{}/out1.log", log_path)
        } else {
            debug!("Log monitoring: no log_path configured for node {}", node_name);
            return Ok(0);
        };

        // Check logs for patterns (last 500 lines)
        let check_logs_cmd = format!(
            "tail -n 500 '{}' 2>/dev/null || echo ''",
            log_file_path
        );

        // Use SSH to read logs
        let log_output = match self.execute_ssh_command(&node_config.server_host, &check_logs_cmd).await {
            Ok(output) => output,
            Err(e) => {
                warn!("Log monitoring: failed to read logs for {}: {}", node_name, e);
                return Err(e);
            }
        };

        if log_output.trim().is_empty() {
            debug!("Log monitoring: no log content found for node {}", node_name);
            return Ok(0);
        }

        let mut found_patterns = 0;

        // Check for any patterns in logs
        for pattern in &self.config.log_monitoring_patterns {
            if log_output.to_lowercase().contains(&pattern.to_lowercase()) {
                found_patterns += 1;
                info!("Log monitoring: pattern '{}' found for node {}", pattern, node_name);

                // Get context lines around the match
                let context_lines = self.get_log_context(&log_output, pattern, self.config.log_monitoring_context_lines);

                // Check if we should send alarm for this pattern
                if self.should_send_log_alarm(node_name, pattern).await {
                    if let Err(e) = self.send_log_pattern_alarm(node_name, pattern, &context_lines).await {
                        error!("Log monitoring: failed to send alarm for pattern '{}' on node {}: {}", pattern, node_name, e);
                    }
                }
            }
        }

        Ok(found_patterns)
    }

    // Get context lines around pattern match
    fn get_log_context(&self, log_output: &str, pattern: &str, context_lines: u32) -> Vec<String> {
        let lines: Vec<&str> = log_output.lines().collect();
        let mut context = Vec::new();

        for (i, line) in lines.iter().enumerate() {
            if line.to_lowercase().contains(&pattern.to_lowercase()) {
                // Add context before
                let start = i.saturating_sub(context_lines as usize);
                let end = std::cmp::min(i + context_lines as usize + 1, lines.len());

                for j in start..end {
                    if j < lines.len() {
                        context.push(lines[j].to_string());
                    }
                }
                break; // Only get context for first match
            }
        }

        // If no context found, return the pattern line itself
        if context.is_empty() {
            for line in lines {
                if line.to_lowercase().contains(&pattern.to_lowercase()) {
                    context.push(line.to_string());
                    break;
                }
            }
        }

        context
    }

    // Check if we should send alarm for log pattern (reuse existing rate limiting logic)
    async fn should_send_log_alarm(&self, node_name: &str, pattern: &str) -> bool {
        let alarm_key = format!("{}:{}", node_name, pattern);

        // Check alarm interval using same logic as health alarms
        let log_alarm_counts = self.log_alarm_counts.read().await;
        let alarm_count = log_alarm_counts.get(&alarm_key).copied().unwrap_or(0);

        let required_hours = self.get_alarm_interval_hours(alarm_count);
        if required_hours == 0 {
            return true; // First alarm - send immediately
        }

        let last_log_checks = self.last_log_check_times.read().await;
        if let Some(last_check) = last_log_checks.get(&alarm_key) {
            let hours_since = Utc::now().signed_duration_since(*last_check).num_hours();
            hours_since >= required_hours as i64
        } else {
            true // No previous alarm
        }
    }

    // Send log pattern alarm
    async fn send_log_pattern_alarm(&self, node_name: &str, pattern: &str, context_lines: &[String]) -> Result<()> {
        if self.config.alarm_webhook_url.is_empty() {
            return Ok(());
        }

        let alarm_key = format!("{}:{}", node_name, pattern);
        let server_host = self.get_server_for_node(node_name).await.unwrap_or_else(|| "unknown".to_string());

        let context_text = if context_lines.is_empty() {
            "No context available".to_string()
        } else {
            context_lines.join("\n")
        };

        let alarm = AlarmPayload {
            timestamp: Utc::now(),
            alarm_type: "log_pattern_match".to_string(),
            severity: "medium".to_string(),
            node_name: node_name.to_string(),
            message: format!("Log pattern '{}' detected for node {}", pattern, node_name),
            details: json!({
                "pattern": pattern,
                "node_name": node_name,
                "server_host": server_host,
                "context_lines_count": context_lines.len(),
                "log_context": context_text,
                "timestamp": Utc::now().to_rfc3339(),
                "monitoring_type": "log_pattern_detection"
            }),
        };

        let response = self
            .http_client
            .post(&self.config.alarm_webhook_url)
            .json(&alarm)
            .send()
            .await?;

        if response.status().is_success() {
            // Update alarm tracking for log patterns
            {
                let mut last_checks = self.last_log_check_times.write().await;
                let mut alarm_counts = self.log_alarm_counts.write().await;
                last_checks.insert(alarm_key.clone(), Utc::now());
                let count = alarm_counts.get(&alarm_key).copied().unwrap_or(0);
                alarm_counts.insert(alarm_key, count + 1);
            }

            info!("Sent log pattern alarm for node {}: pattern '{}'", node_name, pattern);
        } else {
            warn!("Failed to send log pattern alarm: HTTP {}", response.status());
        }

        Ok(())
    }

    // Execute SSH command (helper method for log monitoring)
    async fn execute_ssh_command(&self, server_host: &str, command: &str) -> Result<String> {
        // Find server config to use SSH manager pattern
        let server_config = self.config.servers.values()
            .find(|server| &server.host == server_host)
            .ok_or_else(|| anyhow::anyhow!("Server host {} not found in config", server_host))?;

        // Create SSH connection (following existing pattern)
        let mut connection = crate::ssh::SshConnection::new(
            &server_config.host,
            &server_config.ssh_username,
            &server_config.ssh_key_path,
            server_config.ssh_timeout_seconds,
        ).await?;

        // Execute command
        let result = connection.execute_command(command).await?;
        Ok(result)
    }

    async fn start_maintenance_cleanup_task(&self) {
        let maintenance_tracker = self.maintenance_tracker.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600)); // Every hour

            loop {
                interval.tick().await;

                // Cleanup maintenance windows that have been running too long (25 hours max)
                let cleaned = maintenance_tracker.cleanup_expired_maintenance(25).await;
                if cleaned > 0 {
                    warn!("Cleaned up {} expired maintenance windows (25h max)", cleaned);
                }
            }
        });
    }

    /// Check and process node health with race condition elimination
    async fn check_and_process_node_health(&self, node_name: &str, node_config: &NodeConfig) -> Result<NodeHealth> {
        debug!("Checking health for node: {}", node_name);

        // Single atomic operation that gets both maintenance status and details
        let (in_maintenance, maintenance_window) = self.maintenance_tracker.get_maintenance_status_atomic(node_name).await;

        let health = if in_maintenance {
            // Node is in maintenance - return maintenance status using the already-retrieved details
            self.create_maintenance_health_status(node_name, maintenance_window).await
        } else {
            // Node is not in maintenance - do regular health check
            self.check_node_health(node_name, node_config).await
        };

        // Auto-restore logic for unhealthy nodes
        if matches!(health.status, HealthStatus::Unhealthy) {
            let current_failures = self.get_failure_count(node_name).await;

            // After 3 consecutive failures, check if auto-restore should trigger
            if current_failures >= 3 && node_config.auto_restore_enabled.unwrap_or(false) {
                info!("Node {} has {} consecutive failures, checking auto-restore conditions", node_name, current_failures);

                // Check if we've already attempted auto-restore recently (prevent loops)
                if self.should_attempt_auto_restore(node_name).await {
                    match self.snapshot_manager.check_auto_restore_trigger(node_name).await {
                        Ok(true) => {
                            info!("Auto-restore triggers detected for node {}, initiating restore", node_name);

                            // Record auto-restore attempt
                            self.record_auto_restore_attempt(node_name).await;

                            // Execute auto-restore in background task to not block health checking
                            let snapshot_manager = self.snapshot_manager.clone();
                            let node_name_clone = node_name.to_string();

                            tokio::spawn(async move {
                                match snapshot_manager.execute_auto_restore(&node_name_clone).await {
                                    Ok(_) => {
                                        info!("Auto-restore completed successfully for node {}", node_name_clone);
                                    }
                                    Err(e) => {
                                        error!("Auto-restore failed for node {}: {}", node_name_clone, e);
                                    }
                                }
                            });
                        }
                        Ok(false) => {
                            debug!("Auto-restore triggers not found for node {}", node_name);
                        }
                        Err(e) => {
                            warn!("Failed to check auto-restore triggers for node {}: {}", node_name, e);
                        }
                    }
                } else {
                    debug!("Auto-restore recently attempted for node {}, skipping", node_name);
                }
            }
        }

        // Handle recovery notification (only for non-maintenance status)
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

                // Clear auto-restore attempt tracking on recovery
                {
                    let mut auto_restore_attempts = self.auto_restore_attempts.write().await;
                    auto_restore_attempts.remove(node_name);
                }
            }
        }

        // Save to database - this is the single point of truth for health status
        if let Err(e) = self.database.save_node_health(&health).await {
            error!("Failed to save health data for node {}: {}", node_name, e);
        }

        // Check if we need to send an alarm (but NOT for maintenance status)
        if !matches!(health.status, HealthStatus::Maintenance) && self.should_send_alarm(node_name, &health).await {
            if let Err(e) = self.send_health_alarm(node_name, &health).await {
                error!("Failed to send alarm for node {}: {}", node_name, e);
            }
        }

        Ok(health)
    }

    /// Check if we should attempt auto-restore (prevent infinite loops)
    async fn should_attempt_auto_restore(&self, node_name: &str) -> bool {
        let auto_restore_attempts = self.auto_restore_attempts.read().await;

        if let Some(last_attempt) = auto_restore_attempts.get(node_name) {
            let time_since_attempt = Utc::now().signed_duration_since(*last_attempt);
            // Only attempt auto-restore if it's been more than 2 hours since last attempt
            time_since_attempt.num_hours() >= 2
        } else {
            // No previous attempt recorded
            true
        }
    }

    /// Record auto-restore attempt
    async fn record_auto_restore_attempt(&self, node_name: &str) {
        let mut auto_restore_attempts = self.auto_restore_attempts.write().await;
        auto_restore_attempts.insert(node_name.to_string(), Utc::now());
    }

    /// Create maintenance health status using already-retrieved maintenance window
    async fn create_maintenance_health_status(&self, node_name: &str, maintenance_window: Option<crate::maintenance_tracker::MaintenanceWindow>) -> NodeHealth {
        if let Some(maintenance) = maintenance_window {
            let duration = Utc::now().signed_duration_since(maintenance.started_at);

            NodeHealth {
                node_name: node_name.to_string(),
                status: HealthStatus::Maintenance,
                latest_block_height: None,
                latest_block_time: None,
                catching_up: None,
                last_check: Utc::now(),
                error_message: Some(format!(
                    "Node is undergoing {} ({}m elapsed, estimated {}m total)",
                    maintenance.operation_type,
                    duration.num_minutes(),
                    maintenance.estimated_duration_minutes
                )),
            }
        } else {
            // Fallback case - maintenance tracker says it's in maintenance but no details available
            NodeHealth {
                node_name: node_name.to_string(),
                status: HealthStatus::Maintenance,
                latest_block_height: None,
                latest_block_time: None,
                catching_up: None,
                last_check: Utc::now(),
                error_message: Some("Node is undergoing scheduled maintenance".to_string()),
            }
        }
    }

    /// NEW: Block progression-based health check
    async fn check_node_health(&self, node_name: &str, node: &NodeConfig) -> NodeHealth {
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

                                    // NEW: Determine health based on block progression
                                    let status = self.determine_health_from_block_progression(
                                        node_name,
                                        block_height,
                                        catching_up,
                                        response_time.as_millis() as u64
                                    ).await;

                                    // Reset failure count when RPC succeeds and node is healthy
                                    if matches!(status, HealthStatus::Healthy) {
                                        let mut failure_counts = self.failure_counts.write().await;
                                        failure_counts.insert(node_name.to_string(), 0);
                                    }

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

    /// NEW: Determine health based on block progression logic
    async fn determine_health_from_block_progression(
        &self,
        node_name: &str,
        current_block_height: Option<u64>,
        catching_up: bool,
        response_time_ms: u64,
    ) -> HealthStatus {
        // Check response time first
        if response_time_ms > self.thresholds.max_response_time_ms {
            debug!("Node {} unhealthy: slow response ({}ms)", node_name, response_time_ms);
            return HealthStatus::Unhealthy;
        }

        // If we don't have current block height, it's unhealthy
        let current_height = match current_block_height {
            Some(h) => h,
            None => {
                debug!("Node {} unhealthy: no block height from RPC", node_name);
                return HealthStatus::Unhealthy;
            }
        };

        // If node is catching up, it's not unhealthy, just syncing
        if catching_up {
            debug!("Node {} is catching up (height: {})", node_name, current_height);
            // Update progression info even when catching up
            self.update_block_progression(node_name, current_height).await;
            return HealthStatus::Unknown;
        }

        let now = Utc::now();

        // Read previous progression info first
        let previous_info = {
            let progression_map = self.block_progression.read().await;
            progression_map.get(node_name).cloned()
        };

        // Determine health status based on previous info
        match previous_info {
            Some(prev_info) => {
                let prev_height = prev_info.previous_height.unwrap_or(prev_info.current_height);

                if current_height > prev_height {
                    // Blocks are progressing - healthy!
                    debug!("Node {} healthy: blocks progressing ({} -> {})",
                           node_name, prev_height, current_height);

                    // Update progression info
                    let mut progression_map = self.block_progression.write().await;
                    progression_map.insert(node_name.to_string(), BlockProgressionInfo {
                        node_name: node_name.to_string(),
                        current_height,
                        previous_height: Some(prev_height),
                        last_progression_time: now,
                        stuck_check_count: 0,
                    });

                    HealthStatus::Healthy
                } else if current_height == prev_height {
                    // No block progression - could be stuck or just fast checks
                    let time_since_progression = now.signed_duration_since(prev_info.last_progression_time);
                    let stuck_check_count = prev_info.stuck_check_count + 1;

                    // Update stuck check count
                    let mut progression_map = self.block_progression.write().await;
                    progression_map.insert(node_name.to_string(), BlockProgressionInfo {
                        node_name: node_name.to_string(),
                        current_height,
                        previous_height: Some(prev_height),
                        last_progression_time: prev_info.last_progression_time,
                        stuck_check_count,
                    });

                    // Consider stuck if no progression for configured time AND multiple checks
                    if time_since_progression.num_minutes() > self.thresholds.block_stuck_threshold_minutes as i64
                       && stuck_check_count >= self.thresholds.min_block_progression_checks {
                        warn!("Node {} appears stuck: no block progression for {}m ({} checks at height {})",
                              node_name, time_since_progression.num_minutes(), stuck_check_count, current_height);
                        HealthStatus::Unhealthy
                    } else {
                        debug!("Node {} same height: {} ({}m since progression, {} checks)",
                               node_name, current_height, time_since_progression.num_minutes(), stuck_check_count);
                        HealthStatus::Healthy // Still healthy, just no new blocks yet
                    }
                } else {
                    // Block height went backwards - very bad!
                    error!("Node {} critical: block height went backwards ({} -> {})",
                           node_name, prev_height, current_height);
                    HealthStatus::Unhealthy
                }
            }
            None => {
                // First check for this node - assume healthy
                debug!("Node {} first check: height={}, catching_up={}", node_name, current_height, catching_up);

                let mut progression_map = self.block_progression.write().await;
                progression_map.insert(node_name.to_string(), BlockProgressionInfo {
                    node_name: node_name.to_string(),
                    current_height,
                    previous_height: None,
                    last_progression_time: now,
                    stuck_check_count: 0,
                });

                HealthStatus::Healthy
            }
        }
    }

    /// Helper to update block progression info
    async fn update_block_progression(&self, node_name: &str, current_height: u64) {
        let now = Utc::now();

        // Read previous info first
        let previous_info = {
            let progression_map = self.block_progression.read().await;
            progression_map.get(node_name).cloned()
        };

        // Update with new info
        let mut progression_map = self.block_progression.write().await;

        if let Some(prev_info) = previous_info {
            progression_map.insert(node_name.to_string(), BlockProgressionInfo {
                node_name: node_name.to_string(),
                current_height,
                previous_height: Some(prev_info.current_height),
                last_progression_time: if current_height > prev_info.current_height { now } else { prev_info.last_progression_time },
                stuck_check_count: if current_height > prev_info.current_height { 0 } else { prev_info.stuck_check_count },
            });
        } else {
            progression_map.insert(node_name.to_string(), BlockProgressionInfo {
                node_name: node_name.to_string(),
                current_height,
                previous_height: None,
                last_progression_time: now,
                stuck_check_count: 0,
            });
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
        if matches!(health.status, HealthStatus::Healthy | HealthStatus::Maintenance) {
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
                "health_check_method": "block_progression_based"
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

            info!("Sent health alarm for node: {}", node_name);
        } else {
            warn!("Failed to send health alarm: HTTP {}", response.status());
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
                "health_check_method": "block_progression_based"
            }),
        };

        self.http_client
            .post(&self.config.alarm_webhook_url)
            .json(&alarm)
            .send()
            .await?;

        info!("Sent recovery notification for node: {}", node_name);
        Ok(())
    }

    async fn get_failure_count(&self, node_name: &str) -> u32 {
        let failure_counts = self.failure_counts.read().await;
        failure_counts.get(node_name).copied().unwrap_or(0)
    }

    async fn get_server_for_node(&self, node_name: &str) -> Option<String> {
        // Try to extract server from node name format "server-network"
        if let Some(dash_pos) = node_name.find('-') {
            let server_part = &node_name[..dash_pos];
            if self.config.servers.contains_key(server_part) {
                return Some(server_part.to_string());
            }
        }

        // Fallback: search through nodes config
        for (config_node_name, node_config) in &self.config.nodes {
            if config_node_name == node_name {
                return Some(node_config.server_host.clone());
            }
        }

        None
    }

    pub async fn get_all_health_status(&self) -> Result<Vec<NodeHealth>> {
        self.database.get_all_latest_health().await
    }

    pub async fn get_node_health_history(&self, node_name: &str, limit: i32) -> Result<Vec<NodeHealth>> {
        self.database.get_node_health_history(node_name, limit).await
    }

    /// Force health check now respects maintenance status atomically
    pub async fn force_health_check(&self, node_name: &str) -> Result<NodeHealth> {
        let node_config = self
            .config
            .nodes
            .get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        // Use the same race-condition-free logic as the regular monitoring
        let (in_maintenance, maintenance_window) = self.maintenance_tracker.get_maintenance_status_atomic(node_name).await;

        let health = if in_maintenance {
            self.create_maintenance_health_status(node_name, maintenance_window).await
        } else {
            self.check_node_health(node_name, node_config).await
        };

        self.database.save_node_health(&health).await?;
        Ok(health)
    }

    /// Get maintenance status for all nodes
    #[allow(dead_code)]
    pub async fn get_maintenance_status(&self) -> HashMap<String, crate::maintenance_tracker::MaintenanceWindow> {
        let maintenance_windows = self.maintenance_tracker.get_all_in_maintenance().await;
        let mut status = HashMap::new();

        for window in maintenance_windows {
            status.insert(window.node_name.clone(), window);
        }

        status
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
            maintenance_tracker: self.maintenance_tracker.clone(),
            snapshot_manager: self.snapshot_manager.clone(),
            auto_restore_attempts: self.auto_restore_attempts.clone(),
            block_progression: self.block_progression.clone(),
            last_log_check_times: self.last_log_check_times.clone(),
            log_alarm_counts: self.log_alarm_counts.clone(),
        }
    }
}
