// File: manager/src/http/agent_manager.rs
use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{info, warn, error, debug, instrument};
use chrono::Utc;
use tokio::time::{sleep, Duration as TokioDuration};

use crate::config::{Config, HermesConfig};
use crate::constants::http;
use crate::operation_tracker::SimpleOperationTracker;
use crate::maintenance_tracker::MaintenanceTracker;
use crate::snapshot::SnapshotInfo;

#[non_exhaustive]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ServiceStatus {
    Running,
    Stopped,
    Failed,
    Unknown,
}

impl ServiceStatus {
    pub fn is_running(&self) -> bool {
        matches!(self, ServiceStatus::Running)
    }
}

// REMOVED: Duplicate SnapshotInfo struct - now using crate::snapshot::SnapshotInfo

pub struct HttpAgentManager {
    pub config: Arc<Config>,
    pub client: Client,
    pub operation_tracker: Arc<SimpleOperationTracker>,
    pub maintenance_tracker: Arc<MaintenanceTracker>,
}

impl HttpAgentManager {
    pub fn new(
        config: Arc<Config>,
        operation_tracker: Arc<SimpleOperationTracker>,
        maintenance_tracker: Arc<MaintenanceTracker>
    ) -> Self {
        let client = Client::builder()
            .timeout(http::REQUEST_TIMEOUT)
            .connect_timeout(http::CONNECT_TIMEOUT)
            .build()
            .expect("Failed to create HTTP client for HttpAgentManager");

        Self {
            config,
            client,
            operation_tracker,
            maintenance_tracker,
        }
    }

    fn is_long_running_operation(endpoint: &str) -> bool {
        matches!(endpoint, "/pruning/execute" | "/snapshot/create" | "/snapshot/restore" | "/state-sync/execute")
    }

    async fn execute_operation(&self, server_name: &str, endpoint: &str, payload: Value) -> Result<Value> {
        let server_config = self.config.servers.get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server {} not found", server_name))?;

        let agent_url = format!("http://{}:{}{}", server_config.host, server_config.agent_port, endpoint);

        info!("Starting operation on {}: {}", server_name, endpoint);

        let response = self.client.post(&agent_url)
            .header("Authorization", format!("Bearer {}", server_config.api_key))
            .json(&payload)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("HTTP request failed on {}: {}", server_name, e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("Operation failed on {} with status {}: {}", server_name, status, error_text));
        }

        let result: Value = response.json().await
            .map_err(|e| anyhow::anyhow!("Failed to parse response from {}: {}", server_name, e))?;

        if !result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
            let error_msg = result.get("error").and_then(|v| v.as_str()).unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("Operation failed on {}: {}", server_name, error_msg));
        }

        if Self::is_long_running_operation(endpoint) {
            if let Some(job_id) = result.get("job_id").and_then(|v| v.as_str()) {
                info!("Long operation started with job_id: {} on {}", job_id, server_name);
                return self.poll_for_completion(server_name, job_id).await;
            } else {
                warn!("Long operation endpoint {} did not return job_id, treating as synchronous", endpoint);
            }
        }

        Ok(result)
    }

    async fn poll_for_completion(&self, server_name: &str, job_id: &str) -> Result<Value> {
        let server_config = self.config.servers.get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server {} not found", server_name))?;

        let status_url = format!("http://{}:{}/operation/status/{}",
                               server_config.host, server_config.agent_port, job_id);

        const POLL_INTERVAL_SECONDS: u64 = 60;
        let mut poll_count = 0;
        let start_time = std::time::Instant::now();

        info!("Starting polling for job {} on {} every {}s", job_id, server_name, POLL_INTERVAL_SECONDS);

        loop {
            poll_count += 1;
            let elapsed_minutes = start_time.elapsed().as_secs() / 60;

            debug!("Poll #{} for job {} on {} ({}m elapsed)", poll_count, job_id, server_name, elapsed_minutes);

            tokio::task::yield_now().await;

            let poll_result = self.client.get(&status_url)
                .header("Authorization", format!("Bearer {}", server_config.api_key))
                .send()
                .await;

            match poll_result {
                Ok(response) if response.status().is_success() => {
                    match response.json::<Value>().await {
                        Ok(status_result) => {
                            if let Some(job_status) = status_result.get("job_status").and_then(|v| v.as_str()) {
                                match job_status {
                                    "Completed" => {
                                        info!("Job {} completed after {} polls ({}m elapsed)", job_id, poll_count, elapsed_minutes);
                                        let output = status_result.get("output")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("{}");

                                        let operation_result: Value = serde_json::from_str(output)
                                            .unwrap_or_else(|_| json!({"output": output}));

                                        return Ok(json!({
                                            "success": true,
                                            "job_id": job_id,
                                            "status": "completed",
                                            "result": operation_result
                                        }));
                                    }
                                    "Failed" => {
                                        let error_msg = status_result.get("error")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("Job failed with unknown error");
                                        error!("Job {} failed: {}", job_id, error_msg);
                                        return Err(anyhow::anyhow!("Job {} failed: {}", job_id, error_msg));
                                    }
                                    "Running" => {
                                        debug!("Job {} still running, sleeping {}s until next poll", job_id, POLL_INTERVAL_SECONDS);
                                    }
                                    _ => {
                                        warn!("Unexpected job status '{}', treating as running", job_status);
                                    }
                                }
                            } else {
                                warn!("No job_status in polling response");
                            }
                        }
                        Err(e) => {
                            warn!("JSON parse error during polling: {}", e);
                        }
                    }
                }
                Ok(response) => {
                    warn!("HTTP error during polling: {}", response.status());
                }
                Err(e) => {
                    warn!("Network error during polling: {}", e);
                }
            }

            tokio::select! {
                _ = sleep(TokioDuration::from_secs(POLL_INTERVAL_SECONDS)) => {}
                _ = tokio::signal::ctrl_c() => {
                    error!("Received cancellation signal during polling");
                    return Err(anyhow::anyhow!("Polling cancelled by signal"));
                }
            }
        }
    }

    pub async fn start_service(&self, server_name: &str, service_name: &str) -> Result<()> {
        let payload = json!({"service_name": service_name});
        self.execute_operation(server_name, "/service/start", payload).await?;
        info!("Service {} started successfully on {}", service_name, server_name);
        Ok(())
    }

    pub async fn stop_service(&self, server_name: &str, service_name: &str) -> Result<()> {
        let payload = json!({"service_name": service_name});
        self.execute_operation(server_name, "/service/stop", payload).await?;
        info!("Service {} stopped successfully on {}", service_name, server_name);
        Ok(())
    }

    pub async fn check_service_status(&self, server_name: &str, service_name: &str) -> Result<ServiceStatus> {
        let payload = json!({"service_name": service_name});
        let result = self.execute_operation(server_name, "/service/status", payload).await?;

        let status_str = result.get("status").and_then(|v| v.as_str()).unwrap_or("unknown");
        let status = match status_str {
            "active" | "running" => ServiceStatus::Running,
            "inactive" | "stopped" => ServiceStatus::Stopped,
            "failed" | "error" => ServiceStatus::Failed,
            _ => ServiceStatus::Unknown,
        };

        Ok(status)
    }

    pub async fn get_service_uptime(&self, server_name: &str, service_name: &str) -> Result<Option<std::time::Duration>> {
        let payload = json!({"service_name": service_name});
        let result = self.execute_operation(server_name, "/service/uptime", payload).await?;

        let uptime_seconds = result.get("uptime_seconds")
            .and_then(|v| v.as_u64())
            .map(std::time::Duration::from_secs);

        Ok(uptime_seconds)
    }

    pub async fn execute_single_command(&self, server_name: &str, command: &str) -> Result<String> {
        let payload = json!({"command": command});
        let result = self.execute_operation(server_name, "/command/execute", payload).await?;

        let output = result.get("output")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(output)
    }

    pub async fn delete_all_files_in_directory(&self, server_name: &str, log_path: &str) -> Result<()> {
        let payload = json!({"log_path": log_path});
        self.execute_operation(server_name, "/logs/delete-all", payload).await?;
        info!("All files deleted successfully from {} on {}", log_path, server_name);
        Ok(())
    }

    #[instrument(skip(self), fields(node = %node_name))]
    pub async fn restart_node(&self, node_name: &str) -> Result<()> {
        self.operation_tracker.try_start_operation(node_name, "node_restart", None).await?;

        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| {
                let tracker = self.operation_tracker.clone();
                let node_name_clone = node_name.to_string();
                tokio::spawn(async move {
                    tracker.finish_operation(&node_name_clone).await;
                });
                anyhow::anyhow!("Node {} not found", node_name)
            })?;

        let maintenance_started = match self.maintenance_tracker.start_maintenance(
            node_name,
            "node_restart",
            15,
            &node_config.server_host
        ).await {
            Ok(()) => true,
            Err(e) => {
                self.operation_tracker.finish_operation(node_name).await;
                return Err(e);
            }
        };

        let result = self.restart_node_impl(node_name).await;

        self.operation_tracker.finish_operation(node_name).await;

        if maintenance_started {
            if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
                warn!("Failed to end maintenance for {}: {}", node_name, e);
            }
        }

        result
    }

    async fn restart_node_impl(&self, node_name: &str) -> Result<()> {
        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        let service_name = node_config.pruning_service_name.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No service name configured for {}", node_name))?;

        info!("Restarting node {}", node_name);

        self.stop_service(&node_config.server_host, service_name).await?;
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        self.start_service(&node_config.server_host, service_name).await?;

        let status = self.check_service_status(&node_config.server_host, service_name).await?;
        if !status.is_running() {
            return Err(anyhow::anyhow!("Node {} failed to start after restart", node_name));
        }

        info!("Node {} restarted successfully", node_name);
        Ok(())
    }

    #[instrument(skip(self), fields(node = %node_name))]
    pub async fn execute_node_pruning(&self, node_name: &str) -> Result<()> {
        self.operation_tracker.try_start_operation(node_name, "pruning", None).await?;

        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| {
                let tracker = self.operation_tracker.clone();
                let node_name_clone = node_name.to_string();
                tokio::spawn(async move {
                    tracker.finish_operation(&node_name_clone).await;
                });
                anyhow::anyhow!("Node {} not found", node_name)
            })?;

        let maintenance_started = match self.maintenance_tracker.start_maintenance(
            node_name,
            "pruning",
            300,
            &node_config.server_host
        ).await {
            Ok(()) => true,
            Err(e) => {
                self.operation_tracker.finish_operation(node_name).await;
                return Err(e);
            }
        };

        let result = self.execute_node_pruning_impl(node_name).await;

        self.operation_tracker.finish_operation(node_name).await;

        if maintenance_started {
            if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
                warn!("Failed to end maintenance for pruning {}: {}", node_name, e);
            }
        }

        result
    }

    async fn execute_node_pruning_impl(&self, node_name: &str) -> Result<()> {
        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        if !node_config.pruning_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Pruning not enabled for node {}", node_name));
        }

        let deploy_path = node_config.pruning_deploy_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No deploy path configured for {}", node_name))?;

        let service_name = node_config.pruning_service_name.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No service name configured for {}", node_name))?;

        let keep_blocks = node_config.pruning_keep_blocks.unwrap_or(50000);
        let keep_versions = node_config.pruning_keep_versions.unwrap_or(100);

        info!("Starting pruning sequence for node {}", node_name);

        let payload = json!({
            "deploy_path": deploy_path,
            "keep_blocks": keep_blocks,
            "keep_versions": keep_versions,
            "service_name": service_name,
            "log_path": node_config.log_path
        });

        let result = self.execute_operation(&node_config.server_host, "/pruning/execute", payload).await;

        match result {
            Ok(_) => {
                info!("Pruning sequence completed successfully for node {}", node_name);
                Ok(())
            }
            Err(e) => {
                error!("Pruning sequence failed for node {}: {}", node_name, e);
                Err(e)
            }
        }
    }

    pub async fn execute_state_sync(&self, node_name: &str) -> Result<()> {
        self.operation_tracker.try_start_operation(node_name, "state_sync", None).await?;

        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| {
                let tracker = self.operation_tracker.clone();
                let node_name_clone = node_name.to_string();
                tokio::spawn(async move {
                    tracker.finish_operation(&node_name_clone).await;
                });
                anyhow::anyhow!("Node {} not found", node_name)
            })?;

        let maintenance_started = match self.maintenance_tracker.start_maintenance(
            node_name,
            "state_sync",
            600,
            &node_config.server_host
        ).await {
            Ok(()) => true,
            Err(e) => {
                self.operation_tracker.finish_operation(node_name).await;
                return Err(e);
            }
        };

        let result = self.execute_state_sync_impl(node_name).await;

        self.operation_tracker.finish_operation(node_name).await;

        if maintenance_started {
            if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
                warn!("Failed to end maintenance for state sync {}: {}", node_name, e);
            }
        }

        result
    }

    async fn execute_state_sync_impl(&self, node_name: &str) -> Result<()> {
        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        if !node_config.state_sync_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("State sync not enabled for node {}", node_name));
        }

        let rpc_sources = node_config.state_sync_rpc_sources.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No RPC sources configured for state sync on {}", node_name))?;

        let trust_height_offset = node_config.state_sync_trust_height_offset.unwrap_or(2000);
        let max_sync_timeout = node_config.state_sync_max_sync_timeout_seconds.unwrap_or(600);

        info!("Starting state sync sequence for node {}", node_name);

        let home_dir = node_config.pruning_deploy_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No home directory configured for {}", node_name))?;

        let config_path = format!("{}/config/config.toml", home_dir);

        let service_name = node_config.pruning_service_name.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No service name configured for {}", node_name))?;

        // Fetch state sync parameters from RPC
        info!("Fetching state sync parameters from RPC sources");

        let sync_params = crate::state_sync::fetch_state_sync_params(
            rpc_sources,
            trust_height_offset,
        ).await?;

        info!("✓ State sync parameters fetched: height={}, hash={}",
              sync_params.trust_height, sync_params.trust_hash);

        let daemon_binary = self.determine_daemon_binary(&node_config.network);

        info!("Sending state sync request to agent on {}", node_config.server_host);

        let payload = json!({
            "service_name": service_name,
            "home_dir": home_dir,
            "config_path": config_path,
            "daemon_binary": daemon_binary,
            "rpc_servers": sync_params.rpc_servers,
            "trust_height": sync_params.trust_height,
            "trust_hash": sync_params.trust_hash,
            "timeout_seconds": max_sync_timeout,
            "log_path": node_config.log_path,
        });

        let result = self.execute_operation(&node_config.server_host, "/state-sync/execute", payload).await?;

        if let Some(job_id) = result.get("job_id").and_then(|v| v.as_str()) {
            info!("State sync job started with ID: {}", job_id);
            self.poll_for_completion(&node_config.server_host, job_id).await?;
        }

        info!("✓ State sync completed successfully for {}", node_name);
        Ok(())
    }

    fn determine_daemon_binary(&self, network: &str) -> String {
        match network {
            n if n.starts_with("pirin") || n.starts_with("nolus") => "nolusd".to_string(),
            n if n.starts_with("osmosis") => "osmosisd".to_string(),
            n if n.starts_with("neutron") => "neutrond".to_string(),
            n if n.starts_with("rila") => "rila".to_string(),
            n if n.starts_with("cosmos") => "gaiad".to_string(),
            _ => format!("{}d", network.split('-').next().unwrap_or(network))
        }
    }

    #[instrument(skip(self), fields(node = %node_name))]
    pub async fn create_node_snapshot(&self, node_name: &str) -> Result<SnapshotInfo> {
        self.operation_tracker.try_start_operation(node_name, "snapshot_creation", None).await?;

        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| {
                let tracker = self.operation_tracker.clone();
                let node_name_clone = node_name.to_string();
                tokio::spawn(async move {
                    tracker.finish_operation(&node_name_clone).await;
                });
                anyhow::anyhow!("Node {} not found", node_name)
            })?;

        let maintenance_started = match self.maintenance_tracker.start_maintenance(
            node_name,
            "snapshot_creation",
            1440,
            &node_config.server_host
        ).await {
            Ok(()) => true,
            Err(e) => {
                self.operation_tracker.finish_operation(node_name).await;
                return Err(e);
            }
        };

        let result = self.create_node_snapshot_impl(node_name).await;

        self.operation_tracker.finish_operation(node_name).await;

        if maintenance_started {
            if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
                info!("Failed to end maintenance for {}: {}", node_name, e);
            }
        }

        result
    }

    async fn create_node_snapshot_impl(&self, node_name: &str) -> Result<SnapshotInfo> {
        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        let deploy_path = node_config.snapshot_deploy_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No snapshot deploy path configured for {}", node_name))?;

        let backup_path = node_config.snapshot_backup_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No backup path configured for {}", node_name))?;

        let service_name = node_config.pruning_service_name.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No service name configured for {}", node_name))?;

        let payload = json!({
            "node_name": node_name,
            "network": node_config.network,
            "deploy_path": deploy_path,
            "backup_path": backup_path,
            "service_name": service_name,
            "log_path": node_config.log_path
        });

        let result = self.execute_operation(&node_config.server_host, "/snapshot/create", payload).await?;

        let operation_result = result.get("result").unwrap_or(&result);

        let snapshot_info = SnapshotInfo {
            node_name: node_name.to_string(),
            network: node_config.network.clone(),
            filename: operation_result["filename"].as_str().unwrap_or_default().to_string(),
            created_at: Utc::now(),
            file_size_bytes: operation_result["size_bytes"].as_u64(),
            snapshot_path: operation_result["path"].as_str().unwrap_or_default().to_string(),
            compression_type: operation_result["compression"].as_str().unwrap_or("directory").to_string(),
        };

        Ok(snapshot_info)
    }

    #[instrument(skip(self, hermes_config), fields(service = %hermes_config.service_name))]
    pub async fn restart_hermes(&self, hermes_config: &HermesConfig) -> Result<()> {
        info!("Restarting Hermes {} with log cleanup: {}",
              hermes_config.service_name,
              hermes_config.truncate_logs_enabled.unwrap_or(false));

        self.stop_service(&hermes_config.server_host, &hermes_config.service_name).await?;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;

        if hermes_config.truncate_logs_enabled.unwrap_or(false) {
            if let Some(log_path) = &hermes_config.log_path {
                info!("Deleting all log files in {} for Hermes {}", log_path, hermes_config.service_name);

                match self.delete_all_files_in_directory(&hermes_config.server_host, log_path).await {
                    Ok(_) => {
                        info!("Successfully deleted all log files for Hermes {}", hermes_config.service_name);
                    }
                    Err(e) => {
                        info!("Warning: Failed to delete log files for Hermes {}: {}. Continuing with restart.",
                              hermes_config.service_name, e);
                    }
                }
            } else {
                info!("Log truncation enabled for Hermes {} but no log_path configured", hermes_config.service_name);
            }
        }

        self.start_service(&hermes_config.server_host, &hermes_config.service_name).await?;

        let status = self.check_service_status(&hermes_config.server_host, &hermes_config.service_name).await?;
        if !status.is_running() {
            return Err(anyhow::anyhow!("Hermes {} failed to start after restart", hermes_config.service_name));
        }

        info!("Hermes {} restarted successfully with log cleanup: {}",
              hermes_config.service_name,
              hermes_config.truncate_logs_enabled.unwrap_or(false));
        Ok(())
    }

    pub async fn restore_node_from_snapshot(&self, node_name: &str) -> Result<SnapshotInfo> {
        self.operation_tracker.try_start_operation(node_name, "snapshot_restore", None).await?;

        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| {
                let tracker = self.operation_tracker.clone();
                let node_name_clone = node_name.to_string();
                tokio::spawn(async move {
                    tracker.finish_operation(&node_name_clone).await;
                });
                anyhow::anyhow!("Node {} not found", node_name)
            })?;

        let maintenance_started = match self.maintenance_tracker.start_maintenance(
            node_name,
            "snapshot_restore",
            1440,
            &node_config.server_host
        ).await {
            Ok(()) => true,
            Err(e) => {
                self.operation_tracker.finish_operation(node_name).await;
                return Err(e);
            }
        };

        let result = self.restore_node_from_snapshot_impl(node_name).await;

        self.operation_tracker.finish_operation(node_name).await;

        if maintenance_started {
            if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
                info!("Failed to end maintenance for {}: {}", node_name, e);
            }
        }

        result
    }

    async fn restore_node_from_snapshot_impl(&self, node_name: &str) -> Result<SnapshotInfo> {
        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        if !node_config.auto_restore_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Auto restore not enabled for node {}", node_name));
        }

        let deploy_path = node_config.snapshot_deploy_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No snapshot deploy path configured for {}", node_name))?;

        let backup_path = node_config.snapshot_backup_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No backup path configured for {}", node_name))?;

        let service_name = node_config.pruning_service_name.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No service name configured for {}", node_name))?;

        let latest_snapshot_dir = self.find_latest_network_snapshot_directory(&node_config.server_host, backup_path, &node_config.network).await?;

        info!("Restoring node {} from network snapshot: {}", node_name, latest_snapshot_dir);

        let payload = json!({
            "node_name": node_name,
            "deploy_path": deploy_path,
            "snapshot_dir": latest_snapshot_dir,
            "service_name": service_name,
            "log_path": node_config.log_path
        });

        let _result = self.execute_operation(&node_config.server_host, "/snapshot/restore", payload).await?;

        let snapshot_info = SnapshotInfo {
            node_name: node_name.to_string(),
            network: node_config.network.clone(),
            filename: latest_snapshot_dir.rsplit('/').next().unwrap_or("unknown").to_string(),
            created_at: Utc::now(),
            file_size_bytes: None,
            snapshot_path: latest_snapshot_dir,
            compression_type: "directory".to_string(),
        };

        info!("Restore completed successfully for node {}", node_name);
        Ok(snapshot_info)
    }

    async fn find_latest_network_snapshot_directory(&self, server_host: &str, backup_path: &str, network: &str) -> Result<String> {
        let list_cmd = format!(
            "find '{}' -maxdepth 1 -type d -name '{}_*' | sort | tail -1",
            backup_path, network
        );

        let output = self.execute_single_command(server_host, &list_cmd).await?;

        let snapshot_dir = output.trim();
        if snapshot_dir.is_empty() {
            return Err(anyhow::anyhow!("No network snapshots found for network {} in {}", network, backup_path));
        }

        let verify_data_cmd = format!("test -d '{}/data'", snapshot_dir);
        self.execute_single_command(server_host, &verify_data_cmd).await
            .map_err(|_| anyhow::anyhow!("Network snapshot directory {} does not contain data subdirectory", snapshot_dir))?;

        let verify_wasm_cmd = format!("test -d '{}/wasm'", snapshot_dir);
        self.execute_single_command(server_host, &verify_wasm_cmd).await
            .map_err(|_| anyhow::anyhow!("Network snapshot directory {} does not contain wasm subdirectory", snapshot_dir))?;

        Ok(snapshot_dir.to_string())
    }

    pub async fn cancel_operation(&self, target_name: &str) -> Result<()> {
        self.operation_tracker.cancel_operation(target_name).await
    }

    pub async fn get_active_operations(&self) -> crate::operation_tracker::OperationStatus {
        self.operation_tracker.get_operation_status().await
    }

    pub async fn is_target_busy(&self, target_name: &str) -> bool {
        self.operation_tracker.is_busy(target_name).await
    }

    pub async fn emergency_cleanup_operations(&self, max_hours: i64) -> u32 {
        self.operation_tracker.cleanup_old_operations(max_hours).await
    }

    pub async fn check_auto_restore_triggers(&self, node_name: &str) -> Result<bool> {
        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        let trigger_words = match &self.config.auto_restore_trigger_words {
            Some(words) if !words.is_empty() => words,
            _ => return Ok(false),
        };

        let log_path = match &node_config.log_path {
            Some(path) => path,
            None => return Ok(false),
        };

        let log_file = format!("{}/out1.log", log_path);
        let payload = json!({
            "log_file": log_file,
            "trigger_words": trigger_words
        });

        let result = self.execute_operation(&node_config.server_host, "/snapshot/check-triggers", payload).await?;

        let output = result.get("output")
            .and_then(|v| v.as_str())
            .unwrap_or("{}");

        let parsed: serde_json::Value = serde_json::from_str(output).unwrap_or_default();
        let triggers_found = parsed.get("triggers_found")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        Ok(triggers_found)
    }

    #[allow(dead_code)]
    pub async fn restart_multiple_hermes(&self, hermes_configs: Vec<HermesConfig>) -> Result<Value> {
        let mut results = Vec::new();
        let mut errors = Vec::new();

        for hermes_config in hermes_configs {
            info!("Restarting Hermes: {}", hermes_config.service_name);

            match self.restart_hermes(&hermes_config).await {
                Ok(_) => {
                    results.push(json!({
                        "service_name": hermes_config.service_name,
                        "status": "success"
                    }));
                }
                Err(e) => {
                    error!("Failed to restart Hermes {}: {}", hermes_config.service_name, e);
                    errors.push(json!({
                        "service_name": hermes_config.service_name,
                        "status": "error",
                        "error": e.to_string()
                    }));
                }
            }
        }

        Ok(json!({
            "successes": results,
            "errors": errors
        }))
    }
}

impl Clone for HttpAgentManager {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            client: self.client.clone(),
            operation_tracker: self.operation_tracker.clone(),
            maintenance_tracker: self.maintenance_tracker.clone(),
        }
    }
}
