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

    /// Create snapshot for a node using FAST COPY method via HTTP agent
    pub async fn create_snapshot(&self, node_name: &str) -> Result<SnapshotInfo> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        info!("Starting FAST COPY snapshot creation for node {} via HTTP agent", node_name);

        // Start maintenance tracking with reduced timeout (copy is much faster than compression)
        self.maintenance_tracker
            .start_maintenance(node_name, "fast_copy_snapshot", 60, &node_config.server_host) // 1 hour for copy
            .await?;

        let snapshot_result = self.http_manager.create_node_snapshot(node_name).await;

        // End maintenance tracking
        if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
            error!("Failed to end maintenance mode for {}: {}", node_name, e);
        }

        // Handle result and cleanup
        match &snapshot_result {
            Ok(snapshot_info) => {
                info!("FAST COPY snapshot created successfully: {} (compression running in background)", snapshot_info.filename);

                // NEW: Automatic cleanup based on retention count
                if let Some(retention_count) = node_config.snapshot_retention_count {
                    info!("Running automatic cleanup for {} (keeping {} snapshots)", node_name, retention_count);
                    match self.cleanup_old_snapshots(node_name, retention_count as u32).await {
                        Ok(deleted_count) => {
                            if deleted_count > 0 {
                                info!("Automatic cleanup: deleted {} old snapshots for {}", deleted_count, node_name);
                            }
                        },
                        Err(e) => {
                            warn!("Automatic cleanup failed for {}: {}", node_name, e);
                        }
                    }
                }

                if let Err(e) = self.send_snapshot_notification(node_name, "completed", "fast_copy_snapshot").await {
                    warn!("Failed to send completion notification: {}", e);
                }
            }
            Err(_) => {
                if let Err(e) = self.send_snapshot_notification(node_name, "failed", "fast_copy_snapshot").await {
                    warn!("Failed to send error notification: {}", e);
                }
            }
        }

        snapshot_result
    }

    /// Restore from latest snapshot using FAST COPY method via HttpAgentManager
    pub async fn restore_from_snapshot(&self, node_name: &str) -> Result<SnapshotInfo> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        if !node_config.auto_restore_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Auto restore not enabled for node {}", node_name));
        }

        info!("Starting FAST COPY snapshot restore for node {} via HTTP agent", node_name);

        // Start maintenance tracking with reduced timeout (copy is much faster than extraction)
        self.maintenance_tracker
            .start_maintenance(node_name, "fast_copy_restore", 60, &node_config.server_host) // 1 hour for copy
            .await?;

        let restore_result = self.http_manager.restore_node_from_snapshot(node_name).await;

        // End maintenance tracking
        if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
            error!("Failed to end maintenance mode for {}: {}", node_name, e);
        }

        // Send notification based on result
        match &restore_result {
            Ok(_) => {
                if let Err(e) = self.send_snapshot_notification(node_name, "completed", "fast_copy_restore").await {
                    warn!("Failed to send completion notification: {}", e);
                }
            }
            Err(_) => {
                if let Err(e) = self.send_snapshot_notification(node_name, "failed", "fast_copy_restore").await {
                    warn!("Failed to send error notification: {}", e);
                }
            }
        }

        restore_result
    }

    /// Check if auto-restore should be triggered using HttpAgentManager
    pub async fn check_auto_restore_trigger(&self, node_name: &str) -> Result<bool> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.auto_restore_enabled.unwrap_or(false) || !node_config.snapshots_enabled.unwrap_or(false) {
            return Ok(false);
        }

        self.http_manager.check_auto_restore_triggers(node_name).await
    }

    /// List all snapshots for a node via HTTP agent (supports both directory and compressed formats)
    pub async fn list_snapshots(&self, node_name: &str) -> Result<Vec<SnapshotInfo>> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        let backup_path = node_config.snapshot_backup_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No snapshot backup path configured for node {}", node_name))?;

        // NEW: Search for both directory-based snapshots AND compressed files
        let list_cmd = format!(
            r#"
            (
                # List directory-based snapshots
                find '{}' -maxdepth 1 -type d -name '{}_*' -exec stat -c '%n DIR %Y' {{}} \;
                # List compressed snapshots
                find '{}' -maxdepth 1 -type f -name '{}_*.tar.gz' -exec stat -c '%n %s %Y' {{}} \;
            ) | sort -k3 -nr
            "#,
            backup_path, node_name, backup_path, node_name
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
                let timestamp_unix = parts.last().map_or("0", |&v| v).parse::<i64>().unwrap_or(0);

                // Determine if this is a directory or compressed file
                let (file_size_bytes, compression_type) = if parts.contains(&"DIR") {
                    // Directory-based snapshot
                    let size_cmd = format!("du -sb '{}' | cut -f1", full_path);
                    let size = self.http_manager
                        .execute_single_command(&node_config.server_host, &size_cmd)
                        .await
                        .ok()
                        .and_then(|s| s.trim().parse::<u64>().ok());
                    (size, "directory".to_string())
                } else {
                    // Compressed file
                    let size = parts.get(1).and_then(|s| s.parse::<u64>().ok());
                    (size, "gzip".to_string())
                };

                // Parse timestamp from filename using node_name as prefix
                let created_at = if let Some(ts_part) = filename.strip_prefix(&format!("{}_", node_name)) {
                    let ts_clean = ts_part
                        .strip_suffix(".tar.gz").unwrap_or(ts_part); // Remove .tar.gz if present

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
                    compression_type,
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
            match self.delete_snapshot_file(&node_config.server_host, &snapshot.snapshot_path, &snapshot.compression_type).await {
                Ok(_) => {
                    info!("Deleted old snapshot via HTTP agent: {} ({})", snapshot.filename, snapshot.compression_type);
                    deleted_count += 1;
                }
                Err(e) => {
                    warn!("Failed to delete snapshot {} via HTTP agent: {}", snapshot.filename, e);
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

        // Determine if it's a directory or file
        let snapshot_path = format!("{}/{}", backup_path, filename);
        let compression_type = if filename.ends_with(".tar.gz") { "gzip" } else { "directory" };

        self.delete_snapshot_file(&node_config.server_host, &snapshot_path, compression_type).await?;

        info!("Deleted snapshot {} for node {} via HTTP agent ({})", filename, node_name, compression_type);
        Ok(())
    }

    /// Helper method to delete a snapshot file or directory via HTTP agent
    async fn delete_snapshot_file(&self, server_host: &str, file_path: &str, compression_type: &str) -> Result<()> {
        let delete_cmd = match compression_type {
            "directory" => format!("rm -rf '{}'", file_path), // Directory
            _ => format!("rm -f '{}'", file_path), // File
        };

        self.http_manager.execute_single_command(server_host, &delete_cmd).await?;
        Ok(())
    }

    /// Get snapshot statistics with compression information (supports both formats)
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

        // Mixed compression type (both directory and gzip supported)
        let compression_type = "mixed".to_string();

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
            message: format!("Node {} {} operation {}: {} via HTTP agent (FAST COPY method)", node_name, operation, status, operation),
            server_host,
            details: Some(serde_json::json!({
                "operation_status": status,
                "operation_type": operation,
                "server_host": server_host_clone,
                "method": "fast_copy",
                "compression_type": "mixed",
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
            info!("Sent {} notification for {}: {} {} via HTTP agent (FAST COPY)", operation, node_name, operation, status);
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
