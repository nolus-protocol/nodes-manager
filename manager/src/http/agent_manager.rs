// File: manager/src/http/agent_manager.rs
use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;
use chrono::{DateTime, Utc};

use crate::config::{Config, HermesConfig, NodeConfig};
use crate::operation_tracker::SimpleOperationTracker;

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
}

impl HttpAgentManager {
    pub fn new(config: Arc<Config>, operation_tracker: Arc<SimpleOperationTracker>) -> Self {
        let client = Client::new();

        Self {
            config,
            client,
            operation_tracker,
        }
    }

    /// Execute command - Agent handles everything synchronously until done
    async fn execute_operation(&self, server_name: &str, endpoint: &str, payload: Value) -> Result<Value> {
        let server_config = self.config.servers.get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server {} not found", server_name))?;

        let agent_url = format!("http://{}:{}{}", server_config.host, server_config.agent_port, endpoint);

        info!("Starting operation on {}", server_name);

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

    // === All Operations - Simple and Consistent ===

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

    pub async fn execute_pruning(&self, server_name: &str, deploy_path: &str, keep_blocks: u64, keep_versions: u64, service_name: &str, log_path: Option<&str>) -> Result<()> {
        let payload = json!({
            "deploy_path": deploy_path,
            "keep_blocks": keep_blocks,
            "keep_versions": keep_versions,
            "service_name": service_name,
            "log_path": log_path
        });

        info!("Starting FULL pruning sequence on {} - agent will complete synchronously", server_name);
        self.execute_operation(server_name, "/pruning/execute", payload).await?;
        info!("Full pruning sequence completed successfully on {}", server_name);
        Ok(())
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

        self.execute_pruning(
            &node_config.server_host,
            deploy_path,
            keep_blocks as u64,
            keep_versions as u64,
            service_name,
            node_config.log_path.as_deref()
        ).await
    }

    pub async fn restart_hermes(&self, hermes_config: &HermesConfig) -> Result<()> {
        info!("Restarting Hermes {}", hermes_config.service_name);

        // Simple sequential restart
        self.stop_service(&hermes_config.server_host, &hermes_config.service_name).await?;
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        self.start_service(&hermes_config.server_host, &hermes_config.service_name).await?;

        // Verify it started
        let status = self.check_service_status(&hermes_config.server_host, &hermes_config.service_name).await?;
        if !status.is_running() {
            return Err(anyhow::anyhow!("Hermes {} failed to start after restart", hermes_config.service_name));
        }

        info!("Hermes {} restarted successfully", hermes_config.service_name);
        Ok(())
    }

    // === High-Level Operations with Operation Tracking ===

    pub async fn restart_node(&self, node_name: &str) -> Result<()> {
        // 1. Check if operation is allowed
        self.operation_tracker.try_start_operation(node_name, "node_restart", None).await?;

        // 2. Execute the operation with guaranteed cleanup
        let result = self.restart_node_impl(node_name).await;

        // 3. Always cleanup (even on error)
        self.operation_tracker.finish_operation(node_name).await;

        result
    }

    async fn restart_node_impl(&self, node_name: &str) -> Result<()> {
        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        let service_name = node_config.pruning_service_name.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No service name configured for {}", node_name))?;

        info!("Restarting node {}", node_name);

        // Simple sequential restart
        self.stop_service(&node_config.server_host, service_name).await?;
        tokio::time::sleep(std::time::Duration::from_secs(5)).await; // Brief pause
        self.start_service(&node_config.server_host, service_name).await?;

        // Verify it started
        let status = self.check_service_status(&node_config.server_host, service_name).await?;
        if !status.is_running() {
            return Err(anyhow::anyhow!("Node {} failed to start after restart", node_name));
        }

        info!("Node {} restarted successfully", node_name);
        Ok(())
    }

    pub async fn execute_node_pruning(&self, node_name: &str) -> Result<()> {
        // 1. Check if operation is allowed
        self.operation_tracker.try_start_operation(node_name, "pruning", None).await?;

        // 2. Execute the operation with guaranteed cleanup
        let result = self.execute_node_pruning_impl(node_name).await;

        // 3. Always cleanup (even on error)
        self.operation_tracker.finish_operation(node_name).await;

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

        self.execute_pruning(
            &node_config.server_host,
            deploy_path,
            keep_blocks as u64,
            keep_versions as u64,
            service_name,
            node_config.log_path.as_deref()
        ).await?;

        info!("Full pruning sequence completed for node {}", node_name);
        Ok(())
    }

    pub async fn create_node_snapshot(&self, node_name: &str) -> Result<crate::snapshot::SnapshotInfo> {
        // 1. Check if operation is allowed
        self.operation_tracker.try_start_operation(node_name, "snapshot_creation", None).await?;

        // 2. Execute the operation with guaranteed cleanup
        let result = self.create_node_snapshot_impl(node_name).await;

        // 3. Always cleanup (even on error)
        self.operation_tracker.finish_operation(node_name).await;

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

        let agent_snapshot = self.create_snapshot(
            &node_config.server_host,
            deploy_path,
            backup_path,
            node_name,
            service_name,
            node_config.log_path.as_deref()
        ).await?;

        // Convert to manager's SnapshotInfo type
        Ok(crate::snapshot::SnapshotInfo {
            node_name: node_name.to_string(),
            network: node_config.network.clone(),
            filename: agent_snapshot.filename,
            created_at: agent_snapshot.created_at,
            file_size_bytes: Some(agent_snapshot.size_bytes),
            snapshot_path: agent_snapshot.path,
            compression_type: agent_snapshot.compression,
        })
    }

    pub async fn create_snapshot(&self, server_name: &str, deploy_path: &str, backup_path: &str, node_name: &str, service_name: &str, log_path: Option<&str>) -> Result<SnapshotInfo> {
        let payload = json!({
            "node_name": node_name,
            "deploy_path": deploy_path,
            "backup_path": backup_path,
            "service_name": service_name,
            "log_path": log_path
        });

        info!("Starting full snapshot sequence for {} - agent will complete synchronously (may take hours)", node_name);
        let result = self.execute_operation(server_name, "/snapshot/create", payload).await?;

        let snapshot_info = SnapshotInfo {
            filename: result["filename"].as_str().unwrap_or_default().to_string(),
            size_bytes: result["size_bytes"].as_u64().unwrap_or(0),
            created_at: Utc::now(),
            path: result["path"].as_str().unwrap_or_default().to_string(),
            compression: result["compression"].as_str().unwrap_or("lz4").to_string(),
        };

        info!("Full snapshot sequence completed successfully: {} ({} bytes)", snapshot_info.filename, snapshot_info.size_bytes);
        Ok(snapshot_info)
    }

    // === Operation Management ===

    pub async fn cancel_operation(&self, target_name: &str) -> Result<()> {
        self.operation_tracker.cancel_operation(target_name).await
    }

    pub async fn get_active_operations(&self) -> crate::operation_tracker::OperationStatus {
        self.operation_tracker.get_operation_status().await
    }

    pub async fn is_target_busy(&self, target_name: &str) -> bool {
        self.operation_tracker.is_busy(target_name).await
    }

    /// Emergency cleanup for stuck operations
    pub async fn emergency_cleanup_operations(&self, max_hours: i64) -> u32 {
        self.operation_tracker.cleanup_old_operations(max_hours).await
    }
}

impl Clone for HttpAgentManager {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            client: self.client.clone(),
            operation_tracker: self.operation_tracker.clone(),
        }
    }
}
