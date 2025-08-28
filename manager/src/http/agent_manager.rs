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

    // NEW: Restore from snapshot functionality
    pub async fn restore_node_from_snapshot(&self, node_name: &str) -> Result<crate::snapshot::SnapshotInfo> {
        // 1. Check if operation is allowed
        self.operation_tracker.try_start_operation(node_name, "snapshot_restore", None).await?;

        // 2. Execute the operation with guaranteed cleanup
        let result = self.restore_node_from_snapshot_impl(node_name).await;

        // 3. Always cleanup (even on error)
        self.operation_tracker.finish_operation(node_name).await;

        result
    }

    async fn restore_node_from_snapshot_impl(&self, node_name: &str) -> Result<crate::snapshot::SnapshotInfo> {
        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        if !node_config.snapshots_enabled.unwrap_or(false) || !node_config.auto_restore_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots or auto-restore not enabled for node {}", node_name));
        }

        let deploy_path = node_config.pruning_deploy_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No deploy path configured for {}", node_name))?;

        let backup_path = node_config.snapshot_backup_path.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No backup path configured for {}", node_name))?;

        let service_name = node_config.pruning_service_name.as_ref()
            .ok_or_else(|| anyhow::anyhow!("No service name configured for {}", node_name))?;

        info!("Starting full restore sequence for node {} - agent will complete synchronously", node_name);

        // First, find the latest snapshot file
        let find_latest_cmd = format!(
            "find '{}' -name '{}_*.lz4' -o -name '{}_*.tar.gz' | xargs -r stat -c '%Y %n' | sort -nr | head -1 | cut -d' ' -f2-",
            backup_path, node_config.network, node_config.network
        );

        let latest_snapshot_path = self.execute_single_command(&node_config.server_host, &find_latest_cmd).await?;
        let latest_snapshot_path = latest_snapshot_path.trim();

        if latest_snapshot_path.is_empty() {
            return Err(anyhow::anyhow!("No snapshots found for node {}", node_name));
        }

        info!("Found latest snapshot for restore: {}", latest_snapshot_path);

        // Extract filename from path
        let snapshot_filename = latest_snapshot_path.split('/').last()
            .ok_or_else(|| anyhow::anyhow!("Invalid snapshot path"))?;

        // Find corresponding validator backup file
        let validator_backup_file = self.find_validator_backup_for_snapshot(
            &node_config.server_host,
            backup_path,
            snapshot_filename
        ).await?;

        // Execute the full restore sequence via agent
        let payload = json!({
            "node_name": node_name,
            "deploy_path": deploy_path,
            "snapshot_file": latest_snapshot_path,
            "validator_backup_file": validator_backup_file,
            "service_name": service_name,
            "log_path": node_config.log_path
        });

        let _result = self.execute_operation(&node_config.server_host, "/snapshot/restore", payload).await?;

        info!("Full restore sequence completed successfully for node {}", node_name);

        // Return snapshot info for the restored snapshot
        let snapshot_info = crate::snapshot::SnapshotInfo {
            node_name: node_name.to_string(),
            network: node_config.network.clone(),
            filename: snapshot_filename.to_string(),
            created_at: Utc::now(), // Current time since we don't have original creation time
            file_size_bytes: None, // Not relevant for restore operation
            snapshot_path: latest_snapshot_path.to_string(),
            compression_type: if latest_snapshot_path.ends_with(".lz4") { "lz4".to_string() } else { "gzip".to_string() },
        };

        Ok(snapshot_info)
    }

    // NEW: Check auto-restore triggers functionality
    pub async fn check_auto_restore_triggers(&self, node_name: &str) -> Result<bool> {
        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        if !node_config.auto_restore_enabled.unwrap_or(false) || !node_config.snapshots_enabled.unwrap_or(false) {
            return Ok(false);
        }

        let trigger_words = match &self.config.auto_restore_trigger_words {
            Some(words) if !words.is_empty() => words,
            _ => return Ok(false),
        };

        let log_path = match &node_config.log_path {
            Some(path) => path,
            None => return Ok(false),
        };

        // Check the main log file (out1.log) for trigger words
        let log_file = format!("{}/out1.log", log_path);

        let check_payload = json!({
            "log_file": log_file,
            "trigger_words": trigger_words
        });

        match self.execute_operation(&node_config.server_host, "/snapshot/check-triggers", check_payload).await {
            Ok(result) => {
                // Parse the result to check if triggers were found
                if let Some(output) = result.get("output").and_then(|v| v.as_str()) {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(output) {
                        return Ok(parsed.get("triggers_found").and_then(|v| v.as_bool()).unwrap_or(false));
                    }
                }
                Ok(false)
            }
            Err(e) => {
                info!("Failed to check auto-restore triggers for {}: {}", node_name, e);
                Ok(false)
            }
        }
    }

    async fn find_validator_backup_for_snapshot(
        &self,
        server_host: &str,
        backup_path: &str,
        snapshot_filename: &str,
    ) -> Result<Option<String>> {
        // Extract timestamp from snapshot filename (e.g., osmosis-1_20240828_134500.lz4)
        let timestamp = self.extract_timestamp_from_filename(snapshot_filename)?;
        let validator_backup_path = format!("{}/validator_state_backup_{}.json", backup_path, timestamp);

        // Check if validator backup file exists
        let check_cmd = format!("test -f '{}'", validator_backup_path);

        match self.execute_single_command(server_host, &check_cmd).await {
            Ok(_) => Ok(Some(validator_backup_path)),
            Err(_) => Ok(None),
        }
    }

    fn extract_timestamp_from_filename(&self, filename: &str) -> Result<String> {
        let basename = filename.split('/').last().unwrap_or(filename);

        // Handle formats like: network_YYYYMMDD_HHMMSS.lz4
        let parts: Vec<&str> = basename.split('_').collect();
        if parts.len() >= 3 {
            let date_part = parts[1];
            let time_part = parts[2].split('.').next().unwrap_or(parts[2]);
            Ok(format!("{}_{}", date_part, time_part))
        } else {
            Err(anyhow::anyhow!("Could not extract timestamp from filename: {}", filename))
        }
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

    pub async fn restart_multiple_hermes(&self, _hermes_configs: Vec<HermesConfig>) -> Result<BatchOperationResult> {
        // Placeholder implementation
        Ok(BatchOperationResult {
            success_count: 0,
            failure_count: 0,
            results: vec![],
            summary: "Not implemented".to_string(),
        })
    }

    pub async fn batch_prune_nodes(&self, _node_names: Vec<String>) -> Result<BatchOperationResult> {
        // Placeholder implementation
        Ok(BatchOperationResult {
            success_count: 0,
            failure_count: 0,
            results: vec![],
            summary: "Not implemented".to_string(),
        })
    }

    pub async fn check_node_dependencies(&self, _dependent_nodes: &Option<Vec<String>>) -> Result<bool> {
        // Placeholder implementation
        Ok(true)
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
