// File: manager/src/snapshot/manager.rs
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::config::{Config, NodeConfig};
use crate::maintenance_tracker::MaintenanceTracker;
use crate::http::HttpAgentManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub node_name: String,
    pub network: String,
    pub filename: String,
    pub created_at: DateTime<Utc>,
    pub file_size_bytes: Option<u64>,
    pub snapshot_path: String,
    pub compression_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotStats {
    pub total_snapshots: usize,
    pub total_size_bytes: u64,
    pub oldest_snapshot: Option<DateTime<Utc>>,
    pub newest_snapshot: Option<DateTime<Utc>>,
    pub by_network: std::collections::HashMap<String, usize>,
    pub compression_type: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlarmPayload {
    pub timestamp: DateTime<Utc>,
    pub alarm_type: String,
    pub severity: String,
    pub node_name: String,
    pub message: String,
    pub server_host: String,
    pub details: Option<serde_json::Value>,
}

pub struct SnapshotManager {
    config: Arc<Config>,
    http_manager: Arc<HttpAgentManager>,
    maintenance_tracker: Arc<MaintenanceTracker>,
}

impl SnapshotManager {
    pub fn new(
        config: Arc<Config>,
        http_manager: Arc<HttpAgentManager>,
        maintenance_tracker: Arc<MaintenanceTracker>,
    ) -> Self {
        Self {
            config,
            http_manager,
            maintenance_tracker,
        }
    }

    /// Create snapshot for a node using LZ4 compression via HTTP agent
    pub async fn create_snapshot(&self, node_name: &str) -> Result<SnapshotInfo> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        let backup_path = node_config.snapshot_backup_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No snapshot backup path configured for node {}", node_name))?;

        let deploy_path = node_config.pruning_deploy_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No deploy path configured for node {}", node_name))?;

        let service_name = node_config.pruning_service_name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No service name configured for node {}", node_name))?;

        info!("Starting LZ4 snapshot creation for node {} on server {} via HTTP agent", node_name, node_config.server_host);

        // Start maintenance tracking with 24-hour timeout for all snapshots
        self.maintenance_tracker
            .start_maintenance(node_name, "snapshot_creation", 1440, &node_config.server_host)
            .await?;

        let snapshot_result = self.execute_snapshot_creation(
            node_name,
            &node_config,
            deploy_path,
            backup_path,
            service_name
        ).await;

        // End maintenance tracking
        if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
            error!("Failed to end maintenance mode for {}: {}", node_name, e);
        }

        // Send notification ONLY on completion or error, not on start
        match &snapshot_result {
            Ok(_) => {
                if let Err(e) = self.send_snapshot_notification(node_name, "completed", "snapshot_creation").await {
                    warn!("Failed to send completion notification: {}", e);
                }
            }
            Err(_) => {
                if let Err(e) = self.send_snapshot_notification(node_name, "failed", "snapshot_creation").await {
                    warn!("Failed to send error notification: {}", e);
                }
            }
        }

        snapshot_result
    }

    async fn execute_snapshot_creation(
        &self,
        node_name: &str,
        node_config: &NodeConfig,
        deploy_path: &str,
        backup_path: &str,
        service_name: &str,
    ) -> Result<SnapshotInfo> {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("{}_{}.lz4", node_config.network, timestamp);

        // Execute the full snapshot sequence via HTTP agent
        let agent_snapshot = self.http_manager.create_snapshot(
            &node_config.server_host,
            deploy_path,
            backup_path,
            node_name,
            service_name,
            node_config.log_path.as_deref()
        ).await?;

        let snapshot_info = SnapshotInfo {
            node_name: node_name.to_string(),
            network: node_config.network.clone(),
            filename: agent_snapshot.filename,
            created_at: agent_snapshot.created_at,
            file_size_bytes: Some(agent_snapshot.size_bytes),
            snapshot_path: agent_snapshot.path,
            compression_type: agent_snapshot.compression,
        };

        info!("LZ4 snapshot creation completed for {} via HTTP agent: {}", node_name, snapshot_info.filename);
        Ok(snapshot_info)
    }

    // NEW: Restore from latest snapshot
    pub async fn restore_from_snapshot(&self, node_name: &str) -> Result<SnapshotInfo> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        if !node_config.auto_restore_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Auto restore not enabled for node {}", node_name));
        }

        let backup_path = node_config.snapshot_backup_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No snapshot backup path configured for node {}", node_name))?;

        let deploy_path = node_config.pruning_deploy_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No deploy path configured for node {}", node_name))?;

        let service_name = node_config.pruning_service_name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No service name configured for node {}", node_name))?;

        info!("Starting snapshot restore for node {} on server {} via HTTP agent", node_name, node_config.server_host);

        // Start maintenance tracking with 24-hour timeout for restore operations
        self.maintenance_tracker
            .start_maintenance(node_name, "snapshot_restore", 1440, &node_config.server_host)
            .await?;

        let restore_result = self.execute_snapshot_restore(
            node_name,
            &node_config,
            deploy_path,
            backup_path,
            service_name
        ).await;

        // End maintenance tracking
        if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
            error!("Failed to end maintenance mode for {}: {}", node_name, e);
        }

        // Send notification ONLY on completion or error
        match &restore_result {
            Ok(_) => {
                if let Err(e) = self.send_snapshot_notification(node_name, "completed", "snapshot_restore").await {
                    warn!("Failed to send completion notification: {}", e);
                }
            }
            Err(_) => {
                if let Err(e) = self.send_snapshot_notification(node_name, "failed", "snapshot_restore").await {
                    warn!("Failed to send error notification: {}", e);
                }
            }
        }

        restore_result
    }

    async fn execute_snapshot_restore(
        &self,
        node_name: &str,
        node_config: &NodeConfig,
        deploy_path: &str,
        backup_path: &str,
        service_name: &str,
    ) -> Result<SnapshotInfo> {
        // Find latest snapshot file
        let find_latest_cmd = format!(
            "find '{}' -name '{}_*.lz4' -o -name '{}_*.tar.gz' | xargs -r stat -c '%Y %n' | sort -nr | head -1 | cut -d' ' -f2-",
            backup_path, node_config.network, node_config.network
        );

        let latest_snapshot_path = self.http_manager
            .execute_single_command(&node_config.server_host, &find_latest_cmd)
            .await?;

        let latest_snapshot_path = latest_snapshot_path.trim();
        if latest_snapshot_path.is_empty() {
            return Err(anyhow::anyhow!("No snapshots found for node {}", node_name));
        }

        info!("Found latest snapshot: {}", latest_snapshot_path);

        // Extract filename from path
        let snapshot_filename = latest_snapshot_path.split('/').last()
            .ok_or_else(|| anyhow::anyhow!("Invalid snapshot path"))?;

        // Find corresponding validator backup file
        let validator_backup_file = self.find_validator_backup_for_snapshot(
            &node_config.server_host,
            backup_path,
            snapshot_filename
        ).await?;

        // Execute the full restore sequence via HTTP agent
        let restore_payload = serde_json::json!({
            "node_name": node_name,
            "deploy_path": deploy_path,
            "snapshot_file": latest_snapshot_path,
            "validator_backup_file": validator_backup_file,
            "service_name": service_name,
            "log_path": node_config.log_path
        });

        let result = self.http_manager.execute_operation(
            &node_config.server_host,
            "/snapshot/restore",
            restore_payload
        ).await?;

        info!("Snapshot restore completed for {} via HTTP agent", node_name);

        // Return snapshot info for the restored snapshot
        let snapshot_info = SnapshotInfo {
            node_name: node_name.to_string(),
            network: node_config.network.clone(),
            filename: snapshot_filename.to_string(),
            created_at: Utc::now(), // We don't have the original creation time, so use current time
            file_size_bytes: None, // Size is not relevant for restore operation
            snapshot_path: latest_snapshot_path.to_string(),
            compression_type: if latest_snapshot_path.ends_with(".lz4") { "lz4".to_string() } else { "gzip".to_string() },
        };

        Ok(snapshot_info)
    }

    // NEW: Check if auto-restore should be triggered
    pub async fn check_auto_restore_trigger(&self, node_name: &str) -> Result<bool> {
        let node_config = self.get_node_config(node_name)?;

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

        let check_payload = serde_json::json!({
            "log_file": log_file,
            "trigger_words": trigger_words
        });

        match self.http_manager.execute_operation(
            &node_config.server_host,
            "/snapshot/check-triggers",
            check_payload
        ).await {
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
                warn!("Failed to check auto-restore triggers for {}: {}", node_name, e);
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

        match self.http_manager.execute_single_command(server_host, &check_cmd).await {
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

    /// List all snapshots for a node (now supports both .lz4 and legacy .tar.gz) via HTTP agent
    pub async fn list_snapshots(&self, node_name: &str) -> Result<Vec<SnapshotInfo>> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        let backup_path = node_config.snapshot_backup_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No snapshot backup path configured for node {}", node_name))?;

        // List both LZ4 and legacy tar.gz snapshots via HTTP agent
        let list_cmd = format!(
            "find '{}' -name '{}_*.lz4' -o -name '{}_*.tar.gz' | xargs -r stat -c '%n %s %Y' | sort -k3 -nr",
            backup_path, node_config.network, node_config.network
        );

        let output = self.http_manager
            .execute_single_command(&node_config.server_host, &list_cmd)
            .await
            .unwrap_or_default();

        let mut snapshots = Vec::new();
        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                let full_path = parts[0];
                let filename = full_path.split('/').last().unwrap_or(parts[0]).to_string();
                let file_size_bytes = parts[1].parse::<u64>().ok();
                let timestamp_unix = parts[2].parse::<i64>().unwrap_or(0);

                // Determine compression type from filename
                let compression_type = if filename.ends_with(".lz4") {
                    "lz4"
                } else if filename.ends_with(".tar.gz") {
                    "gzip"
                } else {
                    "unknown"
                };

                // Parse timestamp from filename if possible, fallback to file mtime
                let created_at = if let Some(ts_part) = filename.strip_prefix(&format!("{}_", node_config.network)) {
                    let ts_clean = ts_part
                        .strip_suffix(".lz4")
                        .or_else(|| ts_part.strip_suffix(".tar.gz"))
                        .unwrap_or(ts_part);

                    chrono::NaiveDateTime::parse_from_str(ts_clean, "%Y%m%d_%H%M%S")
                        .ok()
                        .map(|dt| DateTime::from_naive_utc_and_offset(dt, Utc))
                        .unwrap_or_else(|| {
                            DateTime::from_timestamp(timestamp_unix, 0)
                                .map(|dt| dt.with_timezone(&Utc))
                                .unwrap_or_else(Utc::now)
                        })
                } else {
                    DateTime::from_timestamp(timestamp_unix, 0)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(Utc::now)
                };

                snapshots.push(SnapshotInfo {
                    node_name: node_name.to_string(),
                    network: node_config.network.clone(),
                    filename,
                    created_at,
                    file_size_bytes,
                    snapshot_path: full_path.to_string(),
                    compression_type: compression_type.to_string(),
                });
            }
        }

        Ok(snapshots)
    }

    /// Clean up old snapshots based on retention count via HTTP agent
    pub async fn cleanup_old_snapshots(&self, node_name: &str, retention_count: u32) -> Result<u32> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        if retention_count == 0 {
            return Err(anyhow::anyhow!("Retention count must be at least 1"));
        }

        let mut snapshots = self.list_snapshots(node_name).await?;

        // Sort by creation time, newest first
        snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        if snapshots.len() <= retention_count as usize {
            info!("No old snapshots to clean up for {} (have {}, keeping {})",
                  node_name, snapshots.len(), retention_count);
            return Ok(0);
        }

        let snapshots_to_delete = &snapshots[(retention_count as usize)..];
        let mut deleted_count = 0;

        info!("Cleaning up {} old snapshots for node {} (keeping {} most recent) via HTTP agent",
              snapshots_to_delete.len(), node_name, retention_count);

        for snapshot in snapshots_to_delete {
            match self.delete_snapshot_file(&node_config.server_host, &snapshot.snapshot_path).await {
                Ok(_) => {
                    info!("Deleted old snapshot via HTTP agent: {}", snapshot.filename);
                    deleted_count += 1;
                }
                Err(e) => {
                    warn!("Failed to delete snapshot {} via HTTP agent: {}", snapshot.filename, e);
                }
            }

            // Also clean up associated validator state backup files via HTTP agent
            if let Some(backup_path) = &node_config.snapshot_backup_path {
                let timestamp_from_filename = snapshot.filename
                    .strip_prefix(&format!("{}_", node_config.network))
                    .and_then(|s| s.strip_suffix(".lz4").or_else(|| s.strip_suffix(".tar.gz")));

                if let Some(timestamp) = timestamp_from_filename {
                    let validator_backup_file = format!("{}/validator_state_backup_{}.json", backup_path, timestamp);
                    if let Err(e) = self.delete_snapshot_file(&node_config.server_host, &validator_backup_file).await {
                        warn!("Could not delete validator backup file {} via HTTP agent: {}", validator_backup_file, e);
                    }
                }
            }
        }

        info!("Cleaned up {} old snapshots for node {} via HTTP agent", deleted_count, node_name);
        Ok(deleted_count)
    }

    /// Delete a specific snapshot via HTTP agent
    pub async fn delete_snapshot(&self, node_name: &str, filename: &str) -> Result<()> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        let backup_path = node_config.snapshot_backup_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No snapshot backup path configured for node {}", node_name))?;

        let snapshot_path = format!("{}/{}", backup_path, filename);
        self.delete_snapshot_file(&node_config.server_host, &snapshot_path).await?;

        info!("Deleted snapshot {} for node {} via HTTP agent", filename, node_name);
        Ok(())
    }

    /// Helper method to delete a snapshot file via HTTP agent
    async fn delete_snapshot_file(&self, server_host: &str, file_path: &str) -> Result<()> {
        let delete_cmd = format!("rm -f '{}'", file_path);
        self.http_manager.execute_single_command(server_host, &delete_cmd).await?;
        Ok(())
    }

    /// Get snapshot statistics with compression information
    pub async fn get_snapshot_stats(&self, node_name: &str) -> Result<SnapshotStats> {
        let snapshots = self.list_snapshots(node_name).await?;

        let total_snapshots = snapshots.len();
        let total_size_bytes = snapshots.iter()
            .filter_map(|s| s.file_size_bytes)
            .sum();

        let oldest_snapshot = snapshots.iter().map(|s| s.created_at).min();
        let newest_snapshot = snapshots.iter().map(|s| s.created_at).max();

        let mut by_network = std::collections::HashMap::new();
        for snapshot in &snapshots {
            *by_network.entry(snapshot.network.clone()).or_insert(0) += 1;
        }

        // Determine primary compression type (most recent)
        let compression_type = snapshots.first()
            .map(|s| s.compression_type.clone())
            .unwrap_or_else(|| "lz4".to_string());

        Ok(SnapshotStats {
            total_snapshots,
            total_size_bytes,
            oldest_snapshot,
            newest_snapshot,
            by_network,
            compression_type,
        })
    }

    fn get_node_config(&self, node_name: &str) -> Result<&NodeConfig> {
        self.config
            .nodes
            .get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))
    }

    // Send notifications only on completion or error, not on start
    async fn send_snapshot_notification(&self, node_name: &str, status: &str, operation: &str) -> Result<()> {
        if self.config.alarm_webhook_url.is_empty() {
            return Ok(());
        }

        let server_host = self.get_server_for_node(node_name).await.unwrap_or_else(|| "unknown".to_string());
        let server_host_clone = server_host.clone();

        // Only send notifications for completion or failure, not for start
        if status != "completed" && status != "failed" {
            return Ok(());
        }

        let alarm = AlarmPayload {
            timestamp: Utc::now(),
            alarm_type: format!("node_{}", operation),
            severity: if status == "failed" { "high".to_string() } else { "info".to_string() },
            node_name: node_name.to_string(),
            message: format!("Node {} {} operation {}: {} via HTTP agent", node_name, operation, status, operation),
            server_host,
            details: Some(serde_json::json!({
                "operation_status": status,
                "operation_type": operation,
                "server_host": server_host_clone,
                "compression_type": "lz4",
                "connection_type": "http_agent",
                "timestamp": Utc::now().to_rfc3339()
            })),
        };

        let client = reqwest::Client::new();
        let response = client
            .post(&self.config.alarm_webhook_url)
            .json(&alarm)
            .send()
            .await?;

        if response.status().is_success() {
            info!("Sent {} notification for {}: {} {} via HTTP agent", operation, node_name, operation, status);
        } else {
            warn!("Failed to send {} notification: HTTP {}", operation, response.status());
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
}

impl Clone for SnapshotManager {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            http_manager: self.http_manager.clone(),
            maintenance_tracker: self.maintenance_tracker.clone(),
        }
    }
}
