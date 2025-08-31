// File: manager/src/http/agent_manager.rs
use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;
use chrono::{DateTime, Utc};

use crate::config::{Config, HermesConfig, NodeConfig};
use crate::operation_tracker::SimpleOperationTracker;
use crate::maintenance_tracker::MaintenanceTracker;

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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SnapshotInfo {
    pub filename: String,
    pub size_bytes: u64,
    pub created_at: DateTime<Utc>,
    pub path: String,
    pub compression: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BatchOperationResult {
    pub success_count: usize,
    pub failure_count: usize,
    pub results: Vec<OperationResult>,
    pub summary: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OperationResult {
    pub target_name: String,
    pub operation_type: String,
    pub success: bool,
    pub message: String,
    pub duration_seconds: Option<f64>,
    pub details: Option<serde_json::Value>,
}

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
        // No timeout - let operations run as long as needed
        let client = Client::new();

        Self {
            config,
            client,
            operation_tracker,
            maintenance_tracker,
        }
    }

    /// Execute operation with no timeout
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

        Ok(result)
    }

    // === Basic Service Operations ===

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

    // === High-Level Operations with Proper Maintenance Coordination ===

    pub async fn restart_node(&self, node_name: &str) -> Result<()> {
        self.operation_tracker.try_start_operation(node_name, "node_restart", None).await?;

        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        let maintenance_result = self.maintenance_tracker.start_maintenance(
            node_name,
            "node_restart",
            15,
            &node_config.server_host
        ).await;

        if let Err(e) = maintenance_result {
            self.operation_tracker.finish_operation(node_name).await;
            return Err(e);
        }

        let result = self.restart_node_impl(node_name).await;

        self.operation_tracker.finish_operation(node_name).await;
        if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
            info!("Failed to end maintenance for {}: {}", node_name, e);
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

    pub async fn execute_node_pruning(&self, node_name: &str) -> Result<()> {
        self.operation_tracker.try_start_operation(node_name, "pruning", None).await?;

        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        let maintenance_result = self.maintenance_tracker.start_maintenance(
            node_name,
            "pruning",
            300,
            &node_config.server_host
        ).await;

        if let Err(e) = maintenance_result {
            self.operation_tracker.finish_operation(node_name).await;
            return Err(e);
        }

        let result = self.execute_node_pruning_impl(node_name).await;

        self.operation_tracker.finish_operation(node_name).await;
        if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
            info!("Failed to end maintenance for {}: {}", node_name, e);
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

        info!("Starting full pruning sequence for node {}", node_name);

        let payload = json!({
            "deploy_path": deploy_path,
            "keep_blocks": keep_blocks,
            "keep_versions": keep_versions,
            "service_name": service_name,
            "log_path": node_config.log_path
        });

        self.execute_operation(&node_config.server_host, "/pruning/execute", payload).await?;

        info!("Full pruning sequence completed for node {}", node_name);
        Ok(())
    }

    pub async fn create_node_snapshot(&self, node_name: &str) -> Result<crate::snapshot::SnapshotInfo> {
        self.operation_tracker.try_start_operation(node_name, "snapshot_creation", None).await?;

        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        let maintenance_result = self.maintenance_tracker.start_maintenance(
            node_name,
            "snapshot_creation",
            1440,
            &node_config.server_host
        ).await;

        if let Err(e) = maintenance_result {
            self.operation_tracker.finish_operation(node_name).await;
            return Err(e);
        }

        let result = self.create_node_snapshot_impl(node_name).await;

        self.operation_tracker.finish_operation(node_name).await;
        if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
            info!("Failed to end maintenance for {}: {}", node_name, e);
        }

        result
    }

    async fn create_node_snapshot_impl(&self, node_name: &str) -> Result<crate::snapshot::SnapshotInfo> {
        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        let deploy_path = node_config.pruning_deploy_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No deploy path configured for {}", node_name))?;

        let backup_path = node_config.snapshot_backup_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No backup path configured for {}", node_name))?;

        let service_name = node_config.pruning_service_name.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No service name configured for {}", node_name))?;

        // FIXED: Pass network field for network-based snapshot naming
        let payload = json!({
            "node_name": node_name,
            "network": node_config.network,
            "deploy_path": deploy_path,
            "backup_path": backup_path,
            "service_name": service_name,
            "log_path": node_config.log_path
        });

        let result = self.execute_operation(&node_config.server_host, "/snapshot/create", payload).await?;

        let snapshot_info = crate::snapshot::SnapshotInfo {
            node_name: node_name.to_string(),
            network: node_config.network.clone(),
            filename: result["filename"].as_str().unwrap_or_default().to_string(),
            created_at: Utc::now(),
            file_size_bytes: result["size_bytes"].as_u64(),
            snapshot_path: result["path"].as_str().unwrap_or_default().to_string(),
            compression_type: result["compression"].as_str().unwrap_or("directory").to_string(),
        };

        Ok(snapshot_info)
    }

    pub async fn run_pruning(&self, node_config: &NodeConfig) -> Result<()> {
        if !node_config.pruning_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Pruning not enabled for node"));
        }

        let deploy_path = node_config.pruning_deploy_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No deploy path configured"))?;

        let service_name = node_config.pruning_service_name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No service name configured"))?;

        let keep_blocks = node_config.pruning_keep_blocks.unwrap_or(50000);
        let keep_versions = node_config.pruning_keep_versions.unwrap_or(100);

        let payload = json!({
            "deploy_path": deploy_path,
            "keep_blocks": keep_blocks,
            "keep_versions": keep_versions,
            "service_name": service_name,
            "log_path": node_config.log_path
        });

        self.execute_operation(&node_config.server_host, "/pruning/execute", payload).await?;
        Ok(())
    }

    pub async fn restart_hermes(&self, hermes_config: &HermesConfig) -> Result<()> {
        info!("Restarting Hermes {}", hermes_config.service_name);

        self.stop_service(&hermes_config.server_host, &hermes_config.service_name).await?;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        self.start_service(&hermes_config.server_host, &hermes_config.service_name).await?;

        let status = self.check_service_status(&hermes_config.server_host, &hermes_config.service_name).await?;
        if !status.is_running() {
            return Err(anyhow::anyhow!("Hermes {} failed to start after restart", hermes_config.service_name));
        }

        info!("Hermes {} restarted successfully", hermes_config.service_name);
        Ok(())
    }

    // === RESTORE FUNCTIONALITY ===

    pub async fn restore_node_from_snapshot(&self, node_name: &str) -> Result<crate::snapshot::SnapshotInfo> {
        self.operation_tracker.try_start_operation(node_name, "snapshot_restore", None).await?;

        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        let maintenance_result = self.maintenance_tracker.start_maintenance(
            node_name,
            "snapshot_restore",
            1440,
            &node_config.server_host
        ).await;

        if let Err(e) = maintenance_result {
            self.operation_tracker.finish_operation(node_name).await;
            return Err(e);
        }

        let result = self.restore_node_from_snapshot_impl(node_name).await;

        self.operation_tracker.finish_operation(node_name).await;
        if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
            info!("Failed to end maintenance for {}: {}", node_name, e);
        }

        result
    }

    async fn restore_node_from_snapshot_impl(&self, node_name: &str) -> Result<crate::snapshot::SnapshotInfo> {
        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        if !node_config.auto_restore_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Auto restore not enabled for node {}", node_name));
        }

        let deploy_path = node_config.pruning_deploy_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No deploy path configured for {}", node_name))?;

        let backup_path = node_config.snapshot_backup_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No backup path configured for {}", node_name))?;

        let service_name = node_config.pruning_service_name.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No service name configured for {}", node_name))?;

        // FIXED: Find latest network snapshot directory (not node-specific)
        let latest_snapshot_dir = self.find_latest_network_snapshot_directory(&node_config.server_host, backup_path, &node_config.network).await?;

        info!("Restoring node {} from network snapshot: {}", node_name, latest_snapshot_dir);

        // FIXED: Pass network field for restore
        let payload = json!({
            "node_name": node_name,
            "network": node_config.network,
            "deploy_path": deploy_path,
            "snapshot_dir": latest_snapshot_dir,
            "service_name": service_name,
            "log_path": node_config.log_path
        });

        let result = self.execute_operation(&node_config.server_host, "/snapshot/restore", payload).await?;

        // Create SnapshotInfo from restore result
        let snapshot_info = crate::snapshot::SnapshotInfo {
            node_name: node_name.to_string(),
            network: node_config.network.clone(),
            filename: latest_snapshot_dir.split('/').last().unwrap_or("unknown").to_string(),
            created_at: Utc::now(), // We don't have the original creation time from restore
            file_size_bytes: None, // We don't have size info from restore
            snapshot_path: latest_snapshot_dir,
            compression_type: "directory".to_string(),
        };

        info!("Restore completed successfully for node {} (validator state preserved)", node_name);
        Ok(snapshot_info)
    }

    // FIXED: Find latest network snapshot (not node-specific)
    async fn find_latest_network_snapshot_directory(&self, server_host: &str, backup_path: &str, network: &str) -> Result<String> {
        let list_cmd = format!(
            "find '{}' -maxdepth 1 -type d -name '{}_*' | sort -r | head -1",
            backup_path, network
        );

        let output = self.execute_single_command(server_host, &list_cmd).await?;

        let snapshot_dir = output.trim();
        if snapshot_dir.is_empty() {
            return Err(anyhow::anyhow!("No network snapshots found for {} in {}", network, backup_path));
        }

        // Verify the snapshot directory exists and contains data
        let verify_cmd = format!("test -d '{}/data'", snapshot_dir);
        self.execute_single_command(server_host, &verify_cmd).await
            .map_err(|_| anyhow::anyhow!("Network snapshot directory {} does not contain data subdirectory", snapshot_dir))?;

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

    pub async fn restart_multiple_hermes(&self, _hermes_configs: Vec<HermesConfig>) -> Result<BatchOperationResult> {
        Ok(BatchOperationResult {
            success_count: 0,
            failure_count: 0,
            results: vec![],
            summary: "Not implemented".to_string(),
        })
    }

    pub async fn batch_prune_nodes(&self, _node_names: Vec<String>) -> Result<BatchOperationResult> {
        Ok(BatchOperationResult {
            success_count: 0,
            failure_count: 0,
            results: vec![],
            summary: "Not implemented".to_string(),
        })
    }

    pub async fn check_node_dependencies(&self, _dependent_nodes: &Option<Vec<String>>) -> Result<bool> {
        Ok(true)
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
