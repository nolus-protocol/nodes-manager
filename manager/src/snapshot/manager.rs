// File: manager/src/snapshot/manager.rs
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::config::{Config, NodeConfig};
use crate::http::HttpAgentManager;
use crate::maintenance_tracker::MaintenanceTracker;
use crate::services::alert_service::AlertService;

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

pub struct SnapshotManager {
    config: Arc<Config>,
    http_manager: Arc<HttpAgentManager>,
    maintenance_tracker: Arc<MaintenanceTracker>,
    alert_service: Arc<AlertService>,
}

impl SnapshotManager {
    pub fn new(
        config: Arc<Config>,
        http_manager: Arc<HttpAgentManager>,
        maintenance_tracker: Arc<MaintenanceTracker>,
        alert_service: Arc<AlertService>,
    ) -> Self {
        Self {
            config,
            http_manager,
            maintenance_tracker,
            alert_service,
        }
    }

    /// Restore from latest snapshot via HTTP agent
    pub async fn restore_from_snapshot(&self, node_name: &str) -> Result<SnapshotInfo> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.auto_restore_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!(
                "Auto-restore not enabled for node {}",
                node_name
            ));
        }

        info!(
            "Starting network snapshot restore for node {} via HTTP agent",
            node_name
        );

        // HttpAgentManager handles all maintenance tracking - no duplicate tracking needed
        let restore_result = self
            .http_manager
            .restore_node_from_snapshot(node_name)
            .await;

        // Handle result and send alerts using AlertService
        match &restore_result {
            Ok(snapshot_info) => {
                info!(
                    "Network snapshot restored successfully for {}: {}",
                    node_name, snapshot_info.filename
                );

                // Alert: Snapshot restore completed
                if let Err(e) = self
                    .alert_service
                    .alert_snapshot_restore_completed(
                        node_name,
                        &node_config.server_host,
                        &snapshot_info.filename,
                    )
                    .await
                {
                    warn!("Failed to send snapshot restore completion alert: {}", e);
                }
            }
            Err(e) => {
                // Alert: Snapshot restore failed
                if let Err(alert_err) = self
                    .alert_service
                    .alert_snapshot_restore_failed(
                        node_name,
                        &node_config.server_host,
                        &e.to_string(),
                    )
                    .await
                {
                    warn!("Failed to send error notification: {}", alert_err);
                }
            }
        }

        restore_result
    }

    /// Check if auto-restore should be triggered using HttpAgentManager
    pub async fn check_auto_restore_trigger(&self, node_name: &str) -> Result<bool> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.auto_restore_enabled.unwrap_or(false) {
            return Ok(false);
        }

        self.http_manager
            .check_auto_restore_triggers(node_name)
            .await
    }

    /// List all snapshots for a NETWORK (not specific node) via HTTP agent
    pub async fn list_snapshots(&self, node_name: &str) -> Result<Vec<SnapshotInfo>> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.snapshots_enabled.unwrap_or(false)
            && !node_config.auto_restore_enabled.unwrap_or(false)
        {
            return Err(anyhow::anyhow!(
                "Neither snapshots nor auto-restore enabled for node {}",
                node_name
            ));
        }

        let backup_path = node_config.snapshot_backup_path.as_ref().ok_or_else(|| {
            anyhow::anyhow!("No snapshot backup path configured for node {}", node_name)
        })?;

        // FIXED: Enhanced listing command with better error handling
        let list_cmd = format!(
            "find '{}' -maxdepth 1 -type d -name '{}_*' 2>/dev/null | while read dir; do \
             if [ -d \"$dir\" ]; then \
               stat -c '%n %s %Y' \"$dir\" 2>/dev/null || echo \"$dir 0 0\"; \
             fi; \
             done | sort -k3 -nr",
            backup_path, node_config.network
        );

        let output = self
            .http_manager
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
                let filename = full_path.rsplit('/').next().unwrap_or(parts[0]).to_string();
                let file_size_bytes = parts[1].parse::<u64>().ok();
                let timestamp_unix = parts[2].parse::<i64>().unwrap_or(0);

                // Use filesystem timestamp as primary source (works with both old timestamp and new block height formats)
                let created_at = DateTime::from_timestamp(timestamp_unix, 0)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(Utc::now);

                snapshots.push(SnapshotInfo {
                    node_name: node_name.to_string(),
                    network: node_config.network.clone(),
                    filename,
                    created_at,
                    file_size_bytes,
                    snapshot_path: full_path.to_string(),
                    compression_type: "directory".to_string(),
                });
            }
        }

        debug!(
            "Found {} snapshots for network {}",
            snapshots.len(),
            node_config.network
        );
        Ok(snapshots)
    }

    /// Clean up old NETWORK snapshots based on retention count via HTTP agent
    pub async fn cleanup_old_snapshots(
        &self,
        node_name: &str,
        retention_count: u32,
    ) -> Result<u32> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!(
                "Snapshots not enabled for node {}",
                node_name
            ));
        }

        if retention_count == 0 {
            return Err(anyhow::anyhow!("Retention count must be at least 1"));
        }

        self.cleanup_old_network_snapshots(&node_config.network, retention_count)
            .await
    }

    /// FIXED: Clean up old snapshots for a specific NETWORK with improved error handling
    async fn cleanup_old_network_snapshots(
        &self,
        network: &str,
        retention_count: u32,
    ) -> Result<u32> {
        let (node_name, node_config) = self
            .config
            .nodes
            .iter()
            .find(|(_, config)| {
                config.network == network && config.snapshots_enabled.unwrap_or(false)
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No nodes found with snapshots enabled for network {}",
                    network
                )
            })?;

        let mut snapshots = self.list_snapshots(node_name).await?;

        // Sort by creation date (newest first)
        snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        debug!(
            "Cleanup analysis for {}: found {} snapshots, keeping {}",
            network,
            snapshots.len(),
            retention_count
        );

        if snapshots.len() <= retention_count as usize {
            info!(
                "No old network snapshots to clean up for {} (have {}, keeping {})",
                network,
                snapshots.len(),
                retention_count
            );
            return Ok(0);
        }

        let snapshots_to_delete = &snapshots[(retention_count as usize)..];
        let mut deleted_count = 0;

        info!(
            "Cleaning up {} old network snapshots for {} (keeping {} most recent) via HTTP agent",
            snapshots_to_delete.len(),
            network,
            retention_count
        );

        for snapshot in snapshots_to_delete {
            info!(
                "Attempting to delete old snapshot: {} (created: {})",
                snapshot.filename, snapshot.created_at
            );

            // FIXED: Improved deletion with better error handling
            match self
                .delete_snapshot_with_robust_cleanup(
                    &node_config.server_host,
                    &snapshot.snapshot_path,
                    &snapshot.filename,
                )
                .await
            {
                Ok(_) => {
                    info!(
                        "Successfully deleted old network snapshot: {}",
                        snapshot.filename
                    );
                    deleted_count += 1;
                }
                Err(e) => {
                    error!(
                        "Failed to delete network snapshot {} via HTTP agent: {}",
                        snapshot.filename, e
                    );
                    // Continue with other deletions instead of failing completely
                }
            }
        }

        info!(
            "Cleanup completed: deleted {} out of {} old network snapshots for {}",
            deleted_count,
            snapshots_to_delete.len(),
            network
        );

        // PHASE 2: Clean up orphaned LZ4 files that no longer have corresponding directories
        let orphaned_lz4_count = self
            .cleanup_orphaned_lz4_files(node_name, node_config, retention_count)
            .await?;

        if orphaned_lz4_count > 0 {
            info!(
                "Cleaned up {} orphaned LZ4 archives for network {}",
                orphaned_lz4_count, network
            );
        }

        Ok(deleted_count + orphaned_lz4_count)
    }

    /// Clean up orphaned LZ4 files (archives without corresponding directories)
    async fn cleanup_orphaned_lz4_files(
        &self,
        node_name: &str,
        node_config: &crate::config::NodeConfig,
        retention_count: u32,
    ) -> Result<u32> {
        let backup_path = node_config
            .snapshot_backup_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No snapshot backup path configured"))?;

        // Find all LZ4 files for this network
        let list_lz4_cmd = format!(
            "find '{}' -maxdepth 1 -type f -name '{}_*.tar.lz4' -printf '%f\\n' | sort",
            backup_path, node_config.network
        );

        let output = self
            .http_manager
            .execute_single_command(&node_config.server_host, &list_lz4_cmd)
            .await
            .unwrap_or_default();

        if output.trim().is_empty() {
            debug!("No LZ4 files found for network {}", node_config.network);
            return Ok(0);
        }

        let lz4_files: Vec<String> = output
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        debug!(
            "Found {} LZ4 files for network {}",
            lz4_files.len(),
            node_config.network
        );

        // Get current snapshot directories
        let snapshots = self.list_snapshots(node_name).await?;
        let snapshot_basenames: std::collections::HashSet<String> =
            snapshots.iter().map(|s| s.filename.clone()).collect();

        debug!(
            "Found {} active snapshot directories for network {}",
            snapshot_basenames.len(),
            node_config.network
        );

        // Find orphaned LZ4 files (those without a corresponding directory)
        let mut orphaned_lz4_files = Vec::new();
        for lz4_file in &lz4_files {
            // Remove the .tar.lz4 extension to get the base name
            if let Some(basename) = lz4_file.strip_suffix(".tar.lz4") {
                if !snapshot_basenames.contains(basename) {
                    orphaned_lz4_files.push((lz4_file.clone(), basename.to_string()));
                }
            }
        }

        if orphaned_lz4_files.is_empty() {
            debug!(
                "No orphaned LZ4 files found for network {}",
                node_config.network
            );
            return Ok(0);
        }

        info!(
            "Found {} orphaned LZ4 files for network {} (no matching directory)",
            orphaned_lz4_files.len(),
            node_config.network
        );

        // Sort orphaned files by their timestamp (newest first)
        orphaned_lz4_files.sort_by(|a, b| b.1.cmp(&a.1));

        // Keep only retention_count of orphaned LZ4 files (to match the retention policy)
        let lz4_files_to_delete = if orphaned_lz4_files.len() > retention_count as usize {
            &orphaned_lz4_files[(retention_count as usize)..]
        } else {
            // If we have fewer orphaned files than retention count, don't delete any
            // (they might be from recent snapshots being compressed)
            return Ok(0);
        };

        let mut deleted_count = 0;

        for (lz4_file, _basename) in lz4_files_to_delete {
            let lz4_path = format!("{}/{}", backup_path, lz4_file);
            let delete_cmd = format!("rm -f '{}'", lz4_path);

            debug!("Deleting orphaned LZ4 file: {}", lz4_file);

            match self
                .http_manager
                .execute_single_command(&node_config.server_host, &delete_cmd)
                .await
            {
                Ok(_) => {
                    info!("Successfully deleted orphaned LZ4 file: {}", lz4_file);
                    deleted_count += 1;
                }
                Err(e) => {
                    warn!(
                        "Failed to delete orphaned LZ4 file {} (continuing): {}",
                        lz4_file, e
                    );
                    // Continue with other deletions
                }
            }
        }

        Ok(deleted_count)
    }

    /// Delete a specific snapshot via HTTP agent - ENHANCED to include LZ4 cleanup
    pub async fn delete_snapshot(&self, node_name: &str, filename: &str) -> Result<()> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!(
                "Snapshots not enabled for node {}",
                node_name
            ));
        }

        let backup_path = node_config.snapshot_backup_path.as_ref().ok_or_else(|| {
            anyhow::anyhow!("No snapshot backup path configured for node {}", node_name)
        })?;

        let snapshot_path = format!("{}/{}", backup_path, filename);
        self.delete_snapshot_with_robust_cleanup(
            &node_config.server_host,
            &snapshot_path,
            filename,
        )
        .await?;

        info!(
            "Deleted network snapshot (directory + LZ4) {} via HTTP agent",
            filename
        );
        Ok(())
    }

    /// FIXED: Robust snapshot deletion with separated commands and better error handling
    async fn delete_snapshot_with_robust_cleanup(
        &self,
        server_host: &str,
        dir_path: &str,
        filename: &str,
    ) -> Result<()> {
        // Extract the backup path from the full directory path
        let backup_path = dir_path
            .rsplit_once('/')
            .map(|(path, _)| path)
            .unwrap_or("");
        let lz4_path = format!("{}/{}.tar.lz4", backup_path, filename);

        debug!("Deleting snapshot directory: {}", dir_path);
        debug!("Checking for LZ4 file: {}", lz4_path);

        // Step 1: Delete the directory first
        let delete_dir_cmd = format!("rm -rf '{}'", dir_path);
        debug!("Executing directory deletion: {}", delete_dir_cmd);

        match self
            .http_manager
            .execute_single_command(server_host, &delete_dir_cmd)
            .await
        {
            Ok(_) => {
                info!("Successfully deleted snapshot directory: {}", filename);
            }
            Err(e) => {
                error!("Failed to delete snapshot directory {}: {}", filename, e);
                return Err(anyhow::anyhow!(
                    "Failed to delete snapshot directory {}: {}",
                    filename,
                    e
                ));
            }
        }

        // Step 2: Check if LZ4 file exists and delete it
        let check_lz4_cmd = format!("test -f '{}'", lz4_path);
        debug!("Checking if LZ4 file exists: {}", check_lz4_cmd);

        match self
            .http_manager
            .execute_single_command(server_host, &check_lz4_cmd)
            .await
        {
            Ok(_) => {
                // LZ4 file exists, delete it
                let delete_lz4_cmd = format!("rm -f '{}'", lz4_path);
                debug!("Deleting LZ4 file: {}", delete_lz4_cmd);

                match self
                    .http_manager
                    .execute_single_command(server_host, &delete_lz4_cmd)
                    .await
                {
                    Ok(_) => {
                        info!("Successfully deleted LZ4 file for snapshot: {}", filename);
                    }
                    Err(e) => {
                        warn!(
                            "Failed to delete LZ4 file for {} (continuing anyway): {}",
                            filename, e
                        );
                        // Don't fail the entire operation for LZ4 cleanup issues
                    }
                }
            }
            Err(_) => {
                debug!("No LZ4 file found for snapshot: {}", filename);
                // This is not an error - LZ4 files might not exist for all snapshots
            }
        }

        Ok(())
    }

    /// Get snapshot statistics with compression information
    pub async fn get_snapshot_stats(&self, node_name: &str) -> Result<SnapshotStats> {
        let snapshots = self.list_snapshots(node_name).await?;

        let total_snapshots = snapshots.len();
        let total_size_bytes = snapshots.iter().filter_map(|s| s.file_size_bytes).sum();

        let oldest_snapshot = snapshots.iter().map(|s| s.created_at).min();
        let newest_snapshot = snapshots.iter().map(|s| s.created_at).max();

        let mut by_network = std::collections::HashMap::new();
        for snapshot in &snapshots {
            *by_network.entry(snapshot.network.clone()).or_insert(0) += 1;
        }

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
}

impl Clone for SnapshotManager {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            http_manager: self.http_manager.clone(),
            maintenance_tracker: self.maintenance_tracker.clone(),
            alert_service: self.alert_service.clone(),
        }
    }
}
