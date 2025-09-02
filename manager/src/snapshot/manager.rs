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

    /// Create snapshot for a node using directory structure via HTTP agent
    pub async fn create_snapshot(&self, node_name: &str) -> Result<SnapshotInfo> {
        let node_config = self.get_node_config(node_name)?;

        // UNCHANGED: Creating snapshots requires snapshots_enabled
        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        info!("Starting network snapshot creation for {} network via node {} (HTTP agent)", node_config.network, node_name);

        // Start maintenance tracking with 24-hour timeout for all snapshots
        self.maintenance_tracker
            .start_maintenance(node_name, "snapshot_creation", 1440, &node_config.server_host)
            .await?;

        let snapshot_result = self.http_manager.create_node_snapshot(node_name).await;

        // End maintenance tracking
        if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
            error!("Failed to end maintenance mode for {}: {}", node_name, e);
        }

        // Handle result and cleanup
        match &snapshot_result {
            Ok(snapshot_info) => {
                info!("Network snapshot created successfully: {} (can be used by any node on {} network)",
                      snapshot_info.filename, snapshot_info.network);

                // FIXED: Automatic cleanup based on retention count for NETWORK snapshots
                if let Some(retention_count) = node_config.snapshot_retention_count {
                    info!("Running automatic cleanup for {} network (keeping {} snapshots)", snapshot_info.network, retention_count);
                    match self.cleanup_old_network_snapshots(&snapshot_info.network, retention_count as u32).await {
                        Ok(deleted_count) => {
                            if deleted_count > 0 {
                                info!("Automatic cleanup: deleted {} old network snapshots for {}", deleted_count, snapshot_info.network);
                            }
                        },
                        Err(e) => {
                            warn!("Automatic cleanup failed for {} network: {}", snapshot_info.network, e);
                        }
                    }
                }

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

    /// Restore from latest snapshot using HttpAgentManager
    pub async fn restore_from_snapshot(&self, node_name: &str) -> Result<SnapshotInfo> {
        let node_config = self.get_node_config(node_name)?;

        // FIXED: Only require auto_restore_enabled for restore operations (removed snapshots_enabled check)
        if !node_config.auto_restore_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Auto restore not enabled for node {}", node_name));
        }

        info!("Starting snapshot restore for node {} from {} network snapshots (preserving validator state)",
              node_name, node_config.network);

        // FIXED: Don't do maintenance tracking here - HttpAgentManager already does it
        let restore_result = self.http_manager.restore_node_from_snapshot(node_name).await;

        // Send notification based on result
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

    /// Check if auto-restore should be triggered using HttpAgentManager
    pub async fn check_auto_restore_trigger(&self, node_name: &str) -> Result<bool> {
        let node_config = self.get_node_config(node_name)?;

        // FIXED: Only require auto_restore_enabled for checking auto-restore triggers
        if !node_config.auto_restore_enabled.unwrap_or(false) {
            return Ok(false);
        }

        self.http_manager.check_auto_restore_triggers(node_name).await
    }

    /// FIXED: List all snapshots for a NETWORK (not specific node) via HTTP agent
    pub async fn list_snapshots(&self, node_name: &str) -> Result<Vec<SnapshotInfo>> {
        let node_config = self.get_node_config(node_name)?;

        // FIXED: Allow listing if either snapshots_enabled OR auto_restore_enabled
        if !node_config.snapshots_enabled.unwrap_or(false) && !node_config.auto_restore_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Neither snapshots nor auto-restore enabled for node {}", node_name));
        }

        let backup_path = node_config.snapshot_backup_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No snapshot backup path configured for node {}", node_name))?;

        // FIXED: List NETWORK snapshots instead of node-specific snapshots
        let list_cmd = format!(
            "find '{}' -maxdepth 1 -type d -name '{}_*' | xargs -r stat -c '%n %s %Y' | sort -k3 -nr",
            backup_path, node_config.network
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

                // FIXED: Parse timestamp from network directory name
                let created_at = if let Some(ts_part) = filename.strip_prefix(&format!("{}_", node_config.network)) {
                    chrono::NaiveDateTime::parse_from_str(ts_part, "%Y%m%d_%H%M%S")
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
                    node_name: node_name.to_string(), // Keep original node name for API compatibility
                    network: node_config.network.clone(),
                    filename,
                    created_at,
                    file_size_bytes,
                    snapshot_path: full_path.to_string(),
                    compression_type: "directory".to_string(),
                });
            }
        }

        Ok(snapshots)
    }

    /// FIXED: Clean up old NETWORK snapshots based on retention count via HTTP agent
    pub async fn cleanup_old_snapshots(&self, node_name: &str, retention_count: u32) -> Result<u32> {
        let node_config = self.get_node_config(node_name)?;

        // UNCHANGED: Cleanup requires snapshots_enabled (only creators can manage snapshots)
        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        if retention_count == 0 {
            return Err(anyhow::anyhow!("Retention count must be at least 1"));
        }

        self.cleanup_old_network_snapshots(&node_config.network, retention_count).await
    }

    /// FIXED: Clean up old snapshots for a specific NETWORK
    async fn cleanup_old_network_snapshots(&self, network: &str, retention_count: u32) -> Result<u32> {
        // Find a node on this network to use for the operation
        let (node_name, node_config) = self.config.nodes.iter()
            .find(|(_, config)| config.network == network && config.snapshots_enabled.unwrap_or(false))
            .ok_or_else(|| anyhow::anyhow!("No nodes found with snapshots enabled for network {}", network))?;

        let mut snapshots = self.list_snapshots(node_name).await?;

        // Sort by creation time, newest first
        snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        if snapshots.len() <= retention_count as usize {
            info!("No old network snapshots to clean up for {} (have {}, keeping {})",
                  network, snapshots.len(), retention_count);
            return Ok(0);
        }

        let snapshots_to_delete = &snapshots[(retention_count as usize)..];
        let mut deleted_count = 0;

        info!("Cleaning up {} old network snapshot directories for {} (keeping {} most recent) via HTTP agent",
              snapshots_to_delete.len(), network, retention_count);

        for snapshot in snapshots_to_delete {
            // Delete network snapshot directory
            match self.delete_snapshot_directory(&node_config.server_host, &snapshot.snapshot_path).await {
                Ok(_) => {
                    info!("Deleted old network snapshot directory via HTTP agent: {}", snapshot.filename);
                    deleted_count += 1;
                }
                Err(e) => {
                    warn!("Failed to delete network snapshot directory {} via HTTP agent: {}", snapshot.filename, e);
                }
            }
        }

        info!("Cleaned up {} old network snapshot directories for {} via HTTP agent", deleted_count, network);
        Ok(deleted_count)
    }

    /// Delete a specific snapshot via HTTP agent
    pub async fn delete_snapshot(&self, node_name: &str, filename: &str) -> Result<()> {
        let node_config = self.get_node_config(node_name)?;

        // UNCHANGED: Deleting requires snapshots_enabled (only creators can delete)
        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        let backup_path = node_config.snapshot_backup_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No snapshot backup path configured for node {}", node_name))?;

        let snapshot_path = format!("{}/{}", backup_path, filename);
        self.delete_snapshot_directory(&node_config.server_host, &snapshot_path).await?;

        info!("Deleted network snapshot directory {} via HTTP agent", filename);
        Ok(())
    }

    /// Helper method to delete a snapshot directory via HTTP agent
    async fn delete_snapshot_directory(&self, server_host: &str, dir_path: &str) -> Result<()> {
        let delete_cmd = format!("rm -rf '{}'", dir_path);
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

        // All snapshots are directories now
        let compression_type = "directory".to_string();

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
                "compression_type": "directory",
                "connection_type": "http_agent",
                "snapshot_type": "network_based",
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
