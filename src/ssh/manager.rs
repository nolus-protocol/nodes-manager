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

    /// Simple SSH command execution - creates fresh connection, executes, closes
    async fn execute_simple_command(&self, server_name: &str, command: &str) -> Result<String> {
        let server_config = self
            .config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server {} not found", server_name))?;

        debug!("Executing command on {}: {}", server_name, command);

        // Create fresh SSH connection for this command only
        let mut connection = SshConnection::new(
            &server_config.host,
            &server_config.ssh_username,
            &server_config.ssh_key_path,
            server_config.ssh_timeout_seconds,
        )
        .await?;

        // Execute command - connection closes automatically when this scope ends
        let result = connection.execute_command(command).await;

        match result {
            Ok(output) => {
                debug!(
                    "Command completed successfully on {}: {} chars output",
                    server_name,
                    output.len()
                );
                Ok(output)
            }
            Err(e) => {
                error!("Command failed on {}: {}", server_name, e);
                Err(e)
            }
        }
    }

    /// FIXED: Long-running command execution with proper output capture
    async fn execute_long_running_command(&self, server_name: &str, command: &str) -> Result<(String, bool)> {
        let server_config = self
            .config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server {} not found", server_name))?;

        info!("Executing long-running command on {}: {}", server_name, command);

        // Create fresh SSH connection for this long-running command
        let mut connection = SshConnection::new(
            &server_config.host,
            &server_config.ssh_username,
            &server_config.ssh_key_path,
            server_config.ssh_timeout_seconds,
        )
        .await?;

        // Execute command and capture both output and success status
        match connection.execute_command(command).await {
            Ok(output) => {
                info!(
                    "Long-running command completed successfully on {}: {} chars output",
                    server_name,
                    output.len()
                );
                if !output.trim().is_empty() {
                    debug!("Command output preview: {}",
                           output.lines().take(5).collect::<Vec<_>>().join("\n"));
                }
                Ok((output, true))
            }
            Err(e) => {
                warn!("Long-running command failed on {}: {}", server_name, e);
                // For long-running commands, we want to capture the error but still return it
                // The error message often contains useful information about what went wrong
                Ok((e.to_string(), false))
            }
        }
    }

    /// Public alias for execute_simple_command (for backward compatibility)
    #[allow(dead_code)]
    pub async fn execute_command(&self, server_name: &str, command: &str) -> Result<String> {
        self.execute_simple_command(server_name, command).await
    }

    /// Check service status using simple SSH command
    pub async fn check_service_status(&self, server_name: &str, service_name: &str) -> Result<ServiceStatus> {
        let command = format!("systemctl is-active {}", service_name);

        match self.execute_simple_command(server_name, &command).await {
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
            Err(_) => {
                // If systemctl command fails, service might not exist
                Ok(ServiceStatus::NotFound)
            }
        }
    }

    /// Get service uptime using simple SSH command
    pub async fn get_service_uptime(&self, server_name: &str, service_name: &str) -> Result<Option<Duration>> {
        let command = format!(
            "systemctl show {} --property=ExecMainStartTimestamp --value",
            service_name
        );

        match self.execute_simple_command(server_name, &command).await {
            Ok(output) => {
                if output.trim().is_empty() || output.trim() == "n/a" {
                    return Ok(None);
                }

                // Use a simpler approach: check how long the process has been running
                let pid_cmd = format!("systemctl show {} --property=MainPID --value", service_name);
                if let Ok(pid_output) = self.execute_simple_command(server_name, &pid_cmd).await {
                    if let Ok(pid) = pid_output.trim().parse::<u32>() {
                        if pid > 0 {
                            let uptime_cmd = format!("ps -o etime= -p {}", pid);
                            if let Ok(uptime_str) = self.execute_simple_command(server_name, &uptime_cmd).await {
                                return Ok(parse_process_uptime(&uptime_str.trim()));
                            }
                        }
                    }
                }
                Ok(None)
            }
            Err(_) => Ok(None),
        }
    }

    /// Stop service using simple SSH command
    pub async fn stop_service(&self, server_name: &str, service_name: &str) -> Result<()> {
        info!("Stopping service {} on server {}", service_name, server_name);

        let command = format!("sudo systemctl stop {}", service_name);
        self.execute_simple_command(server_name, &command).await?;

        // Verify it stopped with a separate SSH command
        tokio::time::sleep(Duration::from_secs(2)).await;

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

    /// Start service using simple SSH command
    pub async fn start_service(&self, server_name: &str, service_name: &str) -> Result<()> {
        info!("Starting service {} on server {}", service_name, server_name);

        let command = format!("sudo systemctl start {}", service_name);
        self.execute_simple_command(server_name, &command).await?;

        // Verify it started with a separate SSH command
        tokio::time::sleep(Duration::from_secs(3)).await;

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

    /// Restart service using simple SSH command
    #[allow(dead_code)]
    pub async fn restart_service(&self, server_name: &str, service_name: &str) -> Result<()> {
        info!("Restarting service {} on server {}", service_name, server_name);

        let command = format!("sudo systemctl restart {}", service_name);
        self.execute_simple_command(server_name, &command).await?;

        // Verify it restarted with a separate SSH command
        tokio::time::sleep(Duration::from_secs(5)).await;

        let status = self.check_service_status(server_name, service_name).await?;
        if !status.is_running() {
            return Err(anyhow::anyhow!(
                "Service {} failed to restart on server {}: {:?}",
                service_name,
                server_name,
                status
            ));
        }

        info!(
            "Service {} restarted successfully on server {}",
            service_name, server_name
        );
        Ok(())
    }

    /// Truncate logs using simple SSH command
    pub async fn truncate_logs(&self, server_name: &str, log_path: &str, service_name: &str) -> Result<()> {
        info!("Truncating logs for service {} on server {} at path: {}", service_name, server_name, log_path);

        let cleanup_command = format!(
            "if [ -d '{}' ]; then find '{}' -type f -name '*.log*' -delete 2>/dev/null || true; fi && \
             if [ -f '{}' ]; then rm -f '{}' 2>/dev/null || true; fi && \
             journalctl --vacuum-time=1s --user-unit={} 2>/dev/null || true && \
             journalctl --vacuum-time=1s --system --unit={} 2>/dev/null || true",
            log_path, log_path, log_path, log_path, service_name, service_name
        );

        match self.execute_simple_command(server_name, &cleanup_command).await {
            Ok(output) => {
                info!("Log truncation completed for service {} on server {}", service_name, server_name);
                if !output.trim().is_empty() {
                    debug!("Log truncation output: {}", output);
                }
                Ok(())
            }
            Err(e) => {
                warn!("Log truncation failed for service {} on server {}: {}", service_name, server_name, e);
                // Don't fail the entire operation if log truncation fails
                Ok(())
            }
        }
    }

    /// FIXED: Run pruning with proper output capture and exit code handling
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

        // Find the actual node config key
        let node_name = self.find_node_config_key(node).await
            .ok_or_else(|| anyhow::anyhow!("Could not find node config key for pruning"))?;

        info!("Starting pruning for node {} on server {}", node_name, server_name);

        // STEP 1: Start maintenance mode
        self.maintenance_tracker
            .start_maintenance(&node_name, "pruning", 300, server_name) // 300 minutes = 5 hours
            .await?;

        // STEP 2: Execute pruning with discrete SSH commands
        let pruning_result = async {
            // SSH Command 1: Stop the service
            info!("Step 1: Stopping service {}", service_name);
            self.stop_service(server_name, service_name).await?;

            // SSH Command 2: Truncate logs if enabled
            if node.truncate_logs_enabled.unwrap_or(false) {
                if let Some(log_path) = &node.log_path {
                    info!("Step 2: Truncating logs for node {}", node_name);
                    self.truncate_logs(server_name, log_path, service_name).await?;
                } else {
                    warn!("Log truncation enabled for node {} but no log_path configured", node_name);
                }
            }

            // SSH Command 3: FIXED - Run cosmos-pruner with proper output capture
            info!("Step 3: Executing pruning command with output capture");
            let start_time = std::time::Instant::now();

            // FIXED: Remove output redirection, capture actual output and exit code
            let prune_command = format!(
                "cosmos-pruner prune {} --blocks={} --versions={}",
                deploy_path, keep_blocks, keep_versions
            );

            info!("Executing: {}", prune_command);

            let (output, success) = self.execute_long_running_command(server_name, &prune_command).await?;

            let duration = start_time.elapsed();

            if success {
                info!("Cosmos-pruner completed successfully for node {} in {:.2} minutes",
                      node_name, duration.as_secs_f64() / 60.0);

                // Log a preview of the output for debugging
                if !output.trim().is_empty() {
                    let preview = output.lines()
                        .take(3)
                        .collect::<Vec<_>>()
                        .join("\n");
                    info!("Pruning output preview: {}", preview);
                }
            } else {
                error!("Cosmos-pruner failed for node {} after {:.2} minutes: {}",
                       node_name, duration.as_secs_f64() / 60.0, output);
                return Err(anyhow::anyhow!("Cosmos-pruner failed: {}", output));
            }

            // SSH Command 4: Start the service
            info!("Step 4: Starting service {}", service_name);
            self.start_service(server_name, service_name).await?;

            Ok::<(), anyhow::Error>(())
        }
        .await;

        // STEP 3: End maintenance mode (regardless of success/failure)
        if let Err(e) = self.maintenance_tracker.end_maintenance(&node_name).await {
            error!("Failed to end maintenance mode for {}: {}", node_name, e);
        }

        // STEP 4: Send completion notification
        let completion_status = if pruning_result.is_ok() { "completed" } else { "failed" };
        if let Err(e) = self.send_maintenance_notification(&node_name, completion_status, "pruning").await {
            warn!("Failed to send maintenance completion notification: {}", e);
        }

        // STEP 5: Return the result
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

    /// SIMPLIFIED: Restart Hermes with discrete SSH commands
    pub async fn restart_hermes(&self, hermes: &HermesConfig) -> Result<()> {
        let server_name = &hermes.server_host;
        let service_name = &hermes.service_name;

        info!("Starting Hermes restart: {} on server {} using discrete SSH commands", service_name, server_name);

        // STEP 1: Check dependent nodes are healthy
        if !self.check_node_dependencies(&hermes.dependent_nodes).await? {
            return Err(anyhow::anyhow!(
                "Cannot restart Hermes {}: dependent nodes are not healthy",
                service_name
            ));
        }

        // STEP 2: Check minimum uptime
        if !self.check_hermes_min_uptime(hermes).await? {
            return Err(anyhow::anyhow!(
                "Cannot restart Hermes {}: minimum uptime not reached",
                service_name
            ));
        }

        // SSH Command 1: Stop the hermes service
        info!("Step 1: Stopping Hermes service {}", service_name);
        self.stop_service(server_name, service_name).await?;

        // SSH Command 2: Truncate logs if enabled
        if hermes.truncate_logs_enabled.unwrap_or(false) {
            info!("Step 2: Truncating logs for Hermes {}", service_name);
            self.truncate_logs(server_name, &hermes.log_path, service_name).await?;
        }

        // SSH Command 3: Start the hermes service
        info!("Step 3: Starting Hermes service {}", service_name);
        self.start_service(server_name, service_name).await?;

        // SSH Command 4: Verify Hermes startup
        info!("Step 4: Waiting for Hermes {} to fully start...", service_name);
        tokio::time::sleep(Duration::from_secs(15)).await;

        let status = self.check_service_status(server_name, service_name).await?;
        if !status.is_healthy() {
            return Err(anyhow::anyhow!(
                "Hermes failed to start properly: {:?}",
                status
            ));
        }

        // SSH Command 5: Additional verification - check Hermes logs
        match self.verify_hermes_startup(server_name, service_name).await {
            Ok(true) => {
                info!("Hermes {} startup verification passed", service_name);
            }
            Ok(false) => {
                warn!("Hermes {} started but verification failed - check logs", service_name);
                // Don't fail the restart, just warn
            }
            Err(e) => {
                warn!("Could not verify Hermes {} startup: {}", service_name, e);
                // Don't fail the restart, just warn
            }
        }

        info!("Hermes {} restarted successfully on server {}", service_name, server_name);
        Ok(())
    }

    /// Check if dependent nodes are healthy
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

    /// Quick health check for dependency validation using simple command
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

    /// Check if Hermes has been running long enough to restart
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

    /// Verify Hermes startup using simple SSH command
    async fn verify_hermes_startup(&self, server_name: &str, service_name: &str) -> Result<bool> {
        let log_cmd = format!(
            "journalctl -u {} --since '1 minute ago' --no-pager | grep -E '(started|ready|listening)' | tail -5",
            service_name
        );

        match self.execute_simple_command(server_name, &log_cmd).await {
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

    /// Send maintenance notification webhook
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

    /// SIMPLIFIED: Test server connectivity
    pub async fn validate_all_servers_connectivity(&self) -> HashMap<String, Result<String, String>> {
        info!("Validating connectivity to all servers using simple SSH commands");

        let mut connectivity_status = HashMap::new();

        for server_name in self.config.servers.keys() {
            match self.execute_simple_command(server_name, "echo 'connectivity_test'").await {
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

    /// SIMPLIFIED: Get status of all services
    pub async fn get_all_service_statuses(&self) -> HashMap<String, HashMap<String, String>> {
        info!("Getting status of all services using simple SSH commands");

        let mut all_statuses = HashMap::new();

        // Check all node services
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

        // Check all hermes services
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

    /// Emergency cleanup - kill any stuck pruning processes
    pub async fn kill_stuck_pruning_process(&self, server_name: &str, deploy_path: &str) -> Result<()> {
        let kill_command = format!("pkill -f 'cosmos-pruner.*{}'", deploy_path);

        match self.execute_simple_command(server_name, &kill_command).await {
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

    /// UPDATED: Check if a pruning process is running
    pub async fn check_pruning_process_status(&self, server_name: &str, deploy_path: &str) -> Result<bool> {
        let check_command = format!(
            "pgrep -f 'cosmos-pruner.*{}' > /dev/null && echo 'running' || echo 'not_running'",
            deploy_path
        );

        match self.execute_simple_command(server_name, &check_command).await {
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

    /// SIMPLIFIED: Get connection status (no persistent connections in n8n model)
    pub async fn get_connection_status(&self) -> HashMap<String, bool> {
        let mut status = HashMap::new();

        // In the simplified model, we don't maintain persistent connections
        // Instead, we test connectivity on-demand
        for server_name in self.config.servers.keys() {
            // Test if we can connect to each server
            let connected = match self.execute_simple_command(server_name, "echo 'test'").await {
                Ok(output) => output.trim() == "test",
                Err(_) => false,
            };
            status.insert(server_name.clone(), connected);
        }

        status
    }

    /// SIMPLIFIED: Get active connections count (always 0 in n8n model)
    pub async fn get_active_connections(&self) -> usize {
        // In the simplified model, connections are created and closed immediately
        // So there are never any "active" persistent connections
        0
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
