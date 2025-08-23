// File: src/ssh/manager.rs

use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::maintenance_tracker::MaintenanceTracker;
use crate::ssh::{ServiceStatus, SshConnection};
use crate::{AlarmPayload, Config, HermesConfig, NodeConfig};

pub struct SshManager {
    pub config: Arc<Config>,
    pub maintenance_tracker: Arc<MaintenanceTracker>,
}

impl SshManager {
    pub fn new(config: Arc<Config>, maintenance_tracker: Arc<MaintenanceTracker>) -> Self {
        Self {
            config,
            maintenance_tracker,
        }
    }

    /// Create a fresh SSH connection for a single operation
    /// Each operation gets its own connection to avoid conflicts
    pub async fn create_connection(&self, server_name: &str) -> Result<SshConnection> {
        let server_config = self
            .config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server {} not found", server_name))?;

        debug!("Creating fresh SSH connection to {}", server_name);

        SshConnection::new(
            &server_config.host,
            &server_config.ssh_username,
            &server_config.ssh_key_path,
            server_config.ssh_timeout_seconds,
        )
        .await
    }

    /// Execute a single SSH command with a fresh connection
    /// Connection is automatically closed after execution
    pub async fn execute_single_command(&self, server_name: &str, command: &str) -> Result<String> {
        debug!("Executing single SSH command on {}: {}", server_name, command);

        let mut connection = self.create_connection(server_name).await?;
        let result = connection.execute_command(command).await;

        // Connection is automatically dropped here, closing the SSH session
        debug!("SSH command completed on {}, connection closed", server_name);

        result
    }

    /// Execute multiple commands in sequence with a single connection
    /// Use this for operations that need multiple related commands
    pub async fn execute_command_sequence(&self, server_name: &str, commands: Vec<String>) -> Result<Vec<String>> {
        debug!("Executing {} SSH commands in sequence on {}", commands.len(), server_name);

        let mut connection = self.create_connection(server_name).await?;
        let mut results = Vec::new();

        for (i, command) in commands.iter().enumerate() {
            debug!("Executing command {}/{} on {}: {}", i + 1, commands.len(), server_name, command);
            let result = connection.execute_command(command).await?;
            results.push(result);
        }

        // Connection is automatically dropped here, closing the SSH session
        debug!("SSH command sequence completed on {}, connection closed", server_name);

        Ok(results)
    }

    /// Check service status using dedicated SSH connection
    pub async fn check_service_status(&self, server_name: &str, service_name: &str) -> Result<ServiceStatus> {
        let command = format!("systemctl is-active {}", service_name);

        match self.execute_single_command(server_name, &command).await {
            Ok(output) => {
                match output.trim() {
                    "active" => Ok(ServiceStatus::Active),
                    "inactive" => Ok(ServiceStatus::Inactive),
                    "failed" => Ok(ServiceStatus::Failed),
                    "activating" => Ok(ServiceStatus::Activating),
                    "deactivating" => Ok(ServiceStatus::Deactivating),
                    other => Ok(ServiceStatus::Unknown(other.to_string())),
                }
            }
            Err(_) => Ok(ServiceStatus::NotFound),
        }
    }

    /// Get service uptime using dedicated SSH connection
    pub async fn get_service_uptime(&self, server_name: &str, service_name: &str) -> Result<Option<Duration>> {
        let commands = vec![
            format!("systemctl show {} --property=MainPID --value", service_name),
        ];

        let results = self.execute_command_sequence(server_name, commands).await?;
        let pid_output = &results[0];

        if let Ok(pid) = pid_output.trim().parse::<u32>() {
            if pid > 0 {
                let uptime_command = format!("ps -o etime= -p {}", pid);
                match self.execute_single_command(server_name, &uptime_command).await {
                    Ok(uptime_str) => Ok(parse_process_uptime(&uptime_str.trim())),
                    Err(_) => Ok(None),
                }
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Stop service using dedicated SSH connection
    pub async fn stop_service(&self, server_name: &str, service_name: &str) -> Result<()> {
        info!("Stopping service {} on server {}", service_name, server_name);

        let commands = vec![
            format!("sudo systemctl stop {}", service_name),
        ];

        self.execute_command_sequence(server_name, commands).await?;

        // Wait for service to stop
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Verify service stopped (using fresh connection)
        let status = self.check_service_status(server_name, service_name).await?;
        if status.is_running() {
            warn!(
                "Service {} on server {} is still running after stop command",
                service_name, server_name
            );
        } else {
            info!(
                "Service {} stopped successfully on server {}",
                service_name, server_name
            );
        }

        Ok(())
    }

    /// Start service using dedicated SSH connection
    pub async fn start_service(&self, server_name: &str, service_name: &str) -> Result<()> {
        info!("Starting service {} on server {}", service_name, server_name);

        let commands = vec![
            format!("sudo systemctl start {}", service_name),
        ];

        self.execute_command_sequence(server_name, commands).await?;

        // Wait for service to start
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Verify service started (using fresh connection)
        let status = self.check_service_status(server_name, service_name).await?;
        if !status.is_running() {
            return Err(anyhow::anyhow!(
                "Service {} failed to start on server {}: {:?}",
                service_name,
                server_name,
                status
            ));
        }

        info!(
            "Service {} started successfully on server {}",
            service_name, server_name
        );
        Ok(())
    }

    /// Truncate logs using dedicated SSH connection
    pub async fn truncate_logs(&self, server_name: &str, log_path: &str, service_name: &str) -> Result<()> {
        info!("Truncating logs for service {} on server {} at path: {}", service_name, server_name, log_path);

        let cleanup_command = format!(
            "if [ -d '{}' ]; then find '{}' -type f -name '*.log*' -delete 2>/dev/null || true; fi && \
             if [ -f '{}' ]; then rm -f '{}' 2>/dev/null || true; fi && \
             journalctl --vacuum-time=1s --user-unit={} 2>/dev/null || true && \
             journalctl --vacuum-time=1s --system --unit={} 2>/dev/null || true",
            log_path, log_path, log_path, log_path, service_name, service_name
        );

        match self.execute_single_command(server_name, &cleanup_command).await {
            Ok(output) => {
                info!("Log truncation completed for service {} on server {}", service_name, server_name);
                if !output.trim().is_empty() {
                    debug!("Log truncation output: {}", output);
                }
                Ok(())
            }
            Err(e) => {
                warn!("Log truncation failed for service {} on server {}: {}", service_name, server_name, e);
                Ok(())
            }
        }
    }

    /// Run pruning with isolated SSH operations
    /// Each step uses its own connection to avoid interference from health checks
    pub async fn run_pruning(&self, node: &NodeConfig) -> Result<()> {
        let server_name = &node.server_host;
        let service_name = node
            .pruning_service_name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No pruning service name configured for node"))?;
        let deploy_path = node
            .pruning_deploy_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No pruning deploy path configured for node"))?;
        let keep_blocks = node.pruning_keep_blocks.unwrap_or(1000);
        let keep_versions = node.pruning_keep_versions.unwrap_or(1000);

        let node_name = self.find_node_config_key(node).await
            .ok_or_else(|| anyhow::anyhow!("Could not find node config key for pruning"))?;

        info!("Starting pruning for node {} on server {}", node_name, server_name);

        // Start maintenance tracking with extended duration (5 hours for pruning)
        self.maintenance_tracker
            .start_maintenance(&node_name, "pruning", 300, server_name) // 5 hours
            .await?;

        let pruning_result = async {
            // Step 1: Stop service (independent connection)
            info!("Step 1: Stopping service {}", service_name);
            self.stop_service(server_name, service_name).await?;

            // Step 2: Truncate logs if enabled (independent connection)
            if node.truncate_logs_enabled.unwrap_or(false) {
                if let Some(log_path) = &node.log_path {
                    info!("Step 2: Truncating logs for node {}", node_name);
                    self.truncate_logs(server_name, log_path, service_name).await?;
                } else {
                    warn!("Log truncation enabled for node {} but no log_path configured", node_name);
                }
            }

            // Step 3: Execute pruning command (dedicated long-running connection)
            info!("Step 3: Executing pruning command (this may take several hours)");
            let start_time = std::time::Instant::now();

            let prune_command = format!(
                "cosmos-pruner prune {} --blocks={} --versions={}",
                deploy_path, keep_blocks, keep_versions
            );

            info!("Executing long-running pruning command: {}", prune_command);

            // Use dedicated connection for long-running operation
            let output = self.execute_single_command(server_name, &prune_command).await?;

            let duration = start_time.elapsed();

            info!("Cosmos-pruner completed successfully for node {} in {:.2} minutes",
                  node_name, duration.as_secs_f64() / 60.0);

            if !output.trim().is_empty() {
                let preview = output.lines()
                    .take(3)
                    .collect::<Vec<_>>()
                    .join("\n");
                info!("Pruning output preview: {}", preview);
            }

            // Step 4: Start service (independent connection)
            info!("Step 4: Starting service {}", service_name);
            self.start_service(server_name, service_name).await?;

            Ok::<(), anyhow::Error>(())
        }
        .await;

        // End maintenance tracking
        if let Err(e) = self.maintenance_tracker.end_maintenance(&node_name).await {
            error!("Failed to end maintenance mode for {}: {}", node_name, e);
        }

        // Send completion notification
        let completion_status = if pruning_result.is_ok() { "completed" } else { "failed" };
        if let Err(e) = self.send_maintenance_notification(&node_name, completion_status, "pruning").await {
            warn!("Failed to send maintenance completion notification: {}", e);
        }

        match pruning_result {
            Ok(_) => {
                info!("Pruning workflow completed successfully for node {} on server {}", node_name, server_name);
                Ok(())
            }
            Err(e) => {
                error!("Pruning workflow failed for node {} on server {}: {}", node_name, server_name, e);
                Err(e)
            }
        }
    }

    /// Restart Hermes with isolated SSH operations
    pub async fn restart_hermes(&self, hermes: &HermesConfig) -> Result<()> {
        let server_name = &hermes.server_host;
        let service_name = &hermes.service_name;

        info!("Starting Hermes restart: {} on server {}", service_name, server_name);

        // Check dependencies (independent connection for each check)
        if !self.check_node_dependencies(&hermes.dependent_nodes).await? {
            return Err(anyhow::anyhow!(
                "Cannot restart Hermes {}: dependent nodes are not healthy",
                service_name
            ));
        }

        // Check minimum uptime (independent connection)
        if !self.check_hermes_min_uptime(hermes).await? {
            return Err(anyhow::anyhow!(
                "Cannot restart Hermes {}: minimum uptime not reached",
                service_name
            ));
        }

        // Step 1: Stop Hermes service (independent connection)
        info!("Step 1: Stopping Hermes service {}", service_name);
        self.stop_service(server_name, service_name).await?;

        // Step 2: Truncate logs if enabled (independent connection)
        if hermes.truncate_logs_enabled.unwrap_or(false) {
            info!("Step 2: Truncating logs for Hermes {}", service_name);
            self.truncate_logs(server_name, &hermes.log_path, service_name).await?;
        }

        // Step 3: Start Hermes service (independent connection)
        info!("Step 3: Starting Hermes service {}", service_name);
        self.start_service(server_name, service_name).await?;

        // Step 4: Wait and verify startup (independent connection)
        info!("Step 4: Waiting for Hermes {} to fully start...", service_name);
        tokio::time::sleep(Duration::from_secs(15)).await;

        let status = self.check_service_status(server_name, service_name).await?;
        if !status.is_healthy() {
            return Err(anyhow::anyhow!(
                "Hermes failed to start properly: {:?}",
                status
            ));
        }

        // Verify startup (independent connection)
        match self.verify_hermes_startup(server_name, service_name).await {
            Ok(true) => {
                info!("Hermes {} startup verification passed", service_name);
            }
            Ok(false) => {
                warn!("Hermes {} started but verification failed - check logs", service_name);
            }
            Err(e) => {
                warn!("Could not verify Hermes {} startup: {}", service_name, e);
            }
        }

        info!("Hermes {} restarted successfully on server {}", service_name, server_name);
        Ok(())
    }

    /// Check if dependent nodes are healthy (each check uses independent connection)
    pub async fn check_node_dependencies(&self, dependent_nodes: &[String]) -> Result<bool> {
        if dependent_nodes.is_empty() {
            return Ok(true);
        }

        info!("Checking health of {} dependent nodes", dependent_nodes.len());

        for node_name in dependent_nodes {
            let node_config = self.config.nodes.get(node_name)
                .ok_or_else(|| anyhow::anyhow!("Dependent node {} not found in config", node_name))?;

            if !node_config.enabled {
                warn!("Dependent node {} is disabled, skipping", node_name);
                continue;
            }

            if self.maintenance_tracker.is_in_maintenance(node_name).await {
                warn!("Dependent node {} is in maintenance", node_name);
                return Ok(false);
            }

            // Each health check uses its own connection
            let health_check = self.quick_node_health_check(node_name, node_config).await;
            match health_check {
                Ok(true) => {
                    debug!("Dependent node {} is healthy", node_name);
                }
                Ok(false) => {
                    warn!("Dependent node {} is unhealthy", node_name);
                    return Ok(false);
                }
                Err(e) => {
                    error!("Failed to check dependent node {}: {}", node_name, e);
                    return Ok(false);
                }
            }
        }

        info!("All dependent nodes are healthy");
        Ok(true)
    }

    /// Quick health check for dependency validation (independent connection)
    pub async fn quick_node_health_check(&self, node_name: &str, node_config: &NodeConfig) -> Result<bool> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()?;

        let rpc_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "status",
            "params": [],
            "id": 1
        });

        match client.post(&node_config.rpc_url).json(&rpc_request).send().await {
            Ok(response) if response.status().is_success() => {
                debug!("Node {} RPC responded successfully", node_name);
                Ok(true)
            }
            Ok(response) => {
                warn!("Node {} RPC returned error status: {}", node_name, response.status());
                Ok(false)
            }
            Err(e) => {
                warn!("Node {} RPC failed: {}", node_name, e);
                Ok(false)
            }
        }
    }

    /// Check if Hermes has been running long enough to restart (independent connection)
    pub async fn check_hermes_min_uptime(&self, hermes: &HermesConfig) -> Result<bool> {
        let min_uptime_minutes = self.config.hermes_min_uptime_minutes;

        if min_uptime_minutes == 0 {
            return Ok(true);
        }

        let server_name = &hermes.server_host;
        let service_name = &hermes.service_name;

        match self.get_service_uptime(server_name, service_name).await {
            Ok(Some(uptime)) => {
                let uptime_minutes = uptime.as_secs() / 60;
                if uptime_minutes >= min_uptime_minutes {
                    info!(
                        "Hermes {} has been running for {} minutes (minimum: {})",
                        service_name, uptime_minutes, min_uptime_minutes
                    );
                    Ok(true)
                } else {
                    warn!(
                        "Hermes {} has only been running for {} minutes (minimum: {})",
                        service_name, uptime_minutes, min_uptime_minutes
                    );
                    Ok(false)
                }
            }
            Ok(None) => {
                warn!("Could not determine uptime for Hermes {}", service_name);
                Ok(true)
            }
            Err(e) => {
                error!("Failed to check uptime for Hermes {}: {}", service_name, e);
                Ok(true)
            }
        }
    }

    /// Verify Hermes startup using independent SSH connection
    async fn verify_hermes_startup(&self, server_name: &str, service_name: &str) -> Result<bool> {
        let log_cmd = format!(
            "journalctl -u {} --since '1 minute ago' --no-pager | grep -E '(started|ready|listening)' | tail -5",
            service_name
        );

        match self.execute_single_command(server_name, &log_cmd).await {
            Ok(output) => {
                if output.trim().is_empty() {
                    warn!("No startup messages found in Hermes logs");
                    Ok(false)
                } else {
                    debug!("Hermes startup logs: {}", output);
                    let positive_indicators = ["started", "ready", "listening", "connected"];
                    let has_positive = positive_indicators.iter()
                        .any(|indicator| output.to_lowercase().contains(indicator));
                    Ok(has_positive)
                }
            }
            Err(e) => {
                warn!("Failed to check Hermes logs: {}", e);
                Ok(false)
            }
        }
    }

    /// Helper method to find config key for a NodeConfig
    pub async fn find_node_config_key(&self, target_node: &NodeConfig) -> Option<String> {
        for (config_key, node_config) in &self.config.nodes {
            if node_config.rpc_url == target_node.rpc_url
                && node_config.network == target_node.network
                && node_config.server_host == target_node.server_host {
                return Some(config_key.clone());
            }
        }
        None
    }

    /// Send maintenance notification webhook (independent operation)
    async fn send_maintenance_notification(&self, node_name: &str, status: &str, operation: &str) -> Result<()> {
        if self.config.alarm_webhook_url.is_empty() {
            return Ok(());
        }

        let server_host = self.get_server_for_node(node_name).await.unwrap_or_else(|| "unknown".to_string());

        let alarm = AlarmPayload {
            timestamp: chrono::Utc::now(),
            alarm_type: "node_maintenance".to_string(),
            severity: "info".to_string(),
            node_name: node_name.to_string(),
            message: format!("Node {} maintenance {}: {}", node_name, status, operation),
            details: json!({
                "maintenance_status": status,
                "operation_type": operation,
                "server_host": server_host,
                "timestamp": chrono::Utc::now().to_rfc3339()
            }),
        };

        let client = reqwest::Client::new();
        let response = client
            .post(&self.config.alarm_webhook_url)
            .json(&alarm)
            .send()
            .await?;

        if response.status().is_success() {
            info!("Sent maintenance notification for {}: {} {}", node_name, operation, status);
        } else {
            warn!("Failed to send maintenance notification: HTTP {}", response.status());
        }

        Ok(())
    }

    async fn get_server_for_node(&self, node_name: &str) -> Option<String> {
        if let Some(dash_pos) = node_name.find('-') {
            let server_part = &node_name[..dash_pos];
            if self.config.servers.contains_key(server_part) {
                return Some(server_part.to_string());
            }
        }

        for (config_node_name, node_config) in &self.config.nodes {
            if config_node_name == node_name {
                return Some(node_config.server_host.clone());
            }
        }

        None
    }

    /// Test server connectivity using independent connection for each server
    pub async fn validate_all_servers_connectivity(&self) -> HashMap<String, Result<String, String>> {
        info!("Validating connectivity to all servers (each with fresh connection)");

        let mut connectivity_status = HashMap::new();

        for server_name in self.config.servers.keys() {
            match self.execute_single_command(server_name, "echo 'connectivity_test'").await {
                Ok(output) => {
                    if output.trim() == "connectivity_test" {
                        connectivity_status.insert(server_name.clone(), Ok("Connected".to_string()));
                    } else {
                        connectivity_status.insert(server_name.clone(), Err("Unexpected response".to_string()));
                    }
                }
                Err(e) => {
                    connectivity_status.insert(server_name.clone(), Err(e.to_string()));
                }
            }
        }

        connectivity_status
    }

    /// Get status of all services using independent connections
    pub async fn get_all_service_statuses(&self) -> HashMap<String, HashMap<String, String>> {
        info!("Getting status of all services (each with fresh connection)");

        let mut all_statuses = HashMap::new();

        for (node_name, node) in &self.config.nodes {
            if let Some(service_name) = &node.pruning_service_name {
                match self.check_service_status(&node.server_host, service_name).await {
                    Ok(status) => {
                        all_statuses
                            .entry(node.server_host.clone())
                            .or_insert_with(HashMap::new)
                            .insert(node_name.clone(), format!("{:?}", status));
                    }
                    Err(e) => {
                        all_statuses
                            .entry(node.server_host.clone())
                            .or_insert_with(HashMap::new)
                            .insert(node_name.clone(), format!("Error: {}", e));
                    }
                }
            }
        }

        for (hermes_name, hermes) in &self.config.hermes {
            match self.check_service_status(&hermes.server_host, &hermes.service_name).await {
                Ok(status) => {
                    all_statuses
                        .entry(hermes.server_host.clone())
                        .or_insert_with(HashMap::new)
                        .insert(hermes_name.clone(), format!("{:?}", status));
                }
                Err(e) => {
                    all_statuses
                        .entry(hermes.server_host.clone())
                        .or_insert_with(HashMap::new)
                        .insert(hermes_name.clone(), format!("Error: {}", e));
                }
            }
        }

        all_statuses
    }

    /// Check if a pruning process is running (independent connection)
    pub async fn check_pruning_process_status(&self, server_name: &str, deploy_path: &str) -> Result<bool> {
        let check_command = format!(
            "pgrep -f 'cosmos-pruner.*{}' > /dev/null && echo 'running' || echo 'not_running'",
            deploy_path
        );

        match self.execute_single_command(server_name, &check_command).await {
            Ok(output) => {
                let is_running = output.trim() == "running";
                debug!("Pruning process status check for {}: {}", deploy_path, if is_running { "running" } else { "not running" });
                Ok(is_running)
            }
            Err(e) => {
                warn!("Failed to check pruning process status: {}", e);
                Ok(false)
            }
        }
    }

    /// Emergency cleanup - kill any stuck pruning processes (independent connection)
    pub async fn kill_stuck_pruning_process(&self, server_name: &str, deploy_path: &str) -> Result<()> {
        let kill_command = format!("pkill -f 'cosmos-pruner.*{}'", deploy_path);

        match self.execute_single_command(server_name, &kill_command).await {
            Ok(_) => {
                info!("Killed stuck pruning process for path: {}", deploy_path);
                Ok(())
            }
            Err(e) => {
                warn!("Failed to kill stuck pruning process: {}", e);
                Ok(())
            }
        }
    }

    /// Get connection status (test connectivity to all servers)
    pub async fn get_connection_status(&self) -> HashMap<String, bool> {
        let mut status = HashMap::new();

        for server_name in self.config.servers.keys() {
            let connected = match self.execute_single_command(server_name, "echo 'test'").await {
                Ok(output) => output.trim() == "test",
                Err(_) => false,
            };
            status.insert(server_name.clone(), connected);
        }

        status
    }

    /// Get active connections count (always 0 since we don't keep persistent connections)
    pub async fn get_active_connections(&self) -> usize {
        0 // No persistent connections in this implementation
    }
}

// Helper function for parsing process uptime
fn parse_process_uptime(uptime_str: &str) -> Option<Duration> {
    let parts: Vec<&str> = uptime_str.split('-').collect();
    let time_part = parts.last()?;

    let time_components: Vec<&str> = time_part.split(':').collect();

    match time_components.len() {
        2 => {
            let minutes: u64 = time_components[0].parse().ok()?;
            let seconds: u64 = time_components[1].parse().ok()?;
            Some(Duration::from_secs(minutes * 60 + seconds))
        }
        3 => {
            let hours: u64 = time_components[0].parse().ok()?;
            let minutes: u64 = time_components[1].parse().ok()?;
            let seconds: u64 = time_components[2].parse().ok()?;
            Some(Duration::from_secs(hours * 3600 + minutes * 60 + seconds))
        }
        _ => None,
    }
}

impl Clone for SshManager {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            maintenance_tracker: self.maintenance_tracker.clone(),
        }
    }
}
