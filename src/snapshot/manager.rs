// File: src/snapshot/manager.rs

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::maintenance_tracker::MaintenanceTracker;
use crate::ssh::SshManager;
use crate::{AlarmPayload, Config, NodeConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub node_name: String,
    pub network: String,
    pub filename: String,
    pub created_at: DateTime<Utc>,
    pub file_size_bytes: Option<u64>,
    pub snapshot_path: String,
    pub compression_type: String,  // NEW: Track compression type
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotStats {
    pub total_snapshots: usize,
    pub total_size_bytes: u64,
    pub oldest_snapshot: Option<DateTime<Utc>>,
    pub newest_snapshot: Option<DateTime<Utc>>,
    pub by_network: std::collections::HashMap<String, usize>,
    pub compression_type: String,  // NEW: Stats include compression info
}

pub struct SnapshotManager {
    config: Arc<Config>,
    ssh_manager: Arc<SshManager>,
    maintenance_tracker: Arc<MaintenanceTracker>,
}

impl SnapshotManager {
    pub fn new(
        config: Arc<Config>,
        ssh_manager: Arc<SshManager>,
        maintenance_tracker: Arc<MaintenanceTracker>,
    ) -> Self {
        Self {
            config,
            ssh_manager,
            maintenance_tracker,
        }
    }

    /// Create snapshot for a node using LZ4 compression
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

        info!("Starting LZ4 snapshot creation for node {} on server {}", node_name, node_config.server_host);

        // Start maintenance tracking with 24-hour timeout for all snapshots
        self.maintenance_tracker
            .start_maintenance(node_name, "snapshot_creation", 1440, &node_config.server_host) // 24 hours
            .await?;

        let snapshot_result = self.execute_snapshot_creation(node_name, &node_config, deploy_path, backup_path).await;

        // End maintenance tracking
        if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
            error!("Failed to end maintenance mode for {}: {}", node_name, e);
        }

        // Send notification
        let status = if snapshot_result.is_ok() { "completed" } else { "failed" };
        if let Err(e) = self.send_snapshot_notification(node_name, status, "snapshot_creation").await {
            warn!("Failed to send snapshot notification: {}", e);
        }

        snapshot_result
    }

    async fn execute_snapshot_creation(
        &self,
        node_name: &str,
        node_config: &NodeConfig,
        deploy_path: &str,
        backup_path: &str,
    ) -> Result<SnapshotInfo> {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("{}_{}.lz4", node_config.network, timestamp);  // CHANGED: .lz4 extension
        let snapshot_path = format!("{}/{}", backup_path, filename);

        // Step 1: Stop service if configured
        if let Some(service_name) = &node_config.pruning_service_name {
            info!("Step 1: Stopping service {}", service_name);
            self.ssh_manager.stop_service(&node_config.server_host, service_name).await?;
        } else {
            warn!("No service name configured for {}, skipping service stop", node_name);
        }

        // Step 2: Backup validator state if it exists
        let validator_state_backup_path = format!("{}/validator_state_backup_{}.json", backup_path, timestamp);
        let backup_validator_state_cmd = format!(
            "if [ -f '{}/data/priv_validator_state.json' ]; then cp '{}/data/priv_validator_state.json' '{}'; fi",
            deploy_path, deploy_path, validator_state_backup_path
        );

        match self.ssh_manager.execute_single_command(&node_config.server_host, &backup_validator_state_cmd).await {
            Ok(_) => info!("Step 2: Validator state backed up to {}", validator_state_backup_path),
            Err(e) => warn!("Step 2: Could not backup validator state: {}", e),
        }

        // Step 3: Create LZ4-compressed snapshot
        info!("Step 3: Creating LZ4-compressed snapshot (this may take several hours for archive nodes)");

        // CHANGED: Use LZ4 compression instead of gzip
        let create_snapshot_cmd = format!(
            "cd '{}' && tar -cf - data wasm 2>/dev/null | lz4 -z -c > '{}'",
            deploy_path, snapshot_path
        );

        self.ssh_manager.execute_single_command(&node_config.server_host, &create_snapshot_cmd).await?;

        // Step 4: Get snapshot file size
        let file_size_cmd = format!("stat -c%s '{}'", snapshot_path);
        let file_size_bytes = self.ssh_manager
            .execute_single_command(&node_config.server_host, &file_size_cmd)
            .await
            .ok()
            .and_then(|size_str| size_str.trim().parse::<u64>().ok());

        // Step 5: Start service if configured
        if let Some(service_name) = &node_config.pruning_service_name {
            info!("Step 5: Starting service {}", service_name);
            self.ssh_manager.start_service(&node_config.server_host, service_name).await?;
        }

        let snapshot_info = SnapshotInfo {
            node_name: node_name.to_string(),
            network: node_config.network.clone(),
            filename: filename.clone(),
            created_at: Utc::now(),
            file_size_bytes,
            snapshot_path: snapshot_path.clone(),
            compression_type: "lz4".to_string(),  // NEW: Track compression type
        };

        info!("LZ4 snapshot creation completed for {}: {}", node_name, filename);
        Ok(snapshot_info)
    }

    /// List all snapshots for a node (now supports both .lz4 and legacy .tar.gz)
    pub async fn list_snapshots(&self, node_name: &str) -> Result<Vec<SnapshotInfo>> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        let backup_path = node_config.snapshot_backup_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No snapshot backup path configured for node {}", node_name))?;

        // List both LZ4 and legacy tar.gz snapshots
        let list_cmd = format!(
            "find '{}' -name '{}_*.lz4' -o -name '{}_*.tar.gz' | xargs -r stat -c '%n %s %Y' | sort -k3 -nr",
            backup_path, node_config.network, node_config.network
        );

        let output = self.ssh_manager
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

    /// Restore from latest snapshot (supports both LZ4 and legacy formats)
    pub async fn restore_from_snapshot(&self, node_name: &str) -> Result<SnapshotInfo> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        let snapshots = self.list_snapshots(node_name).await?;
        let latest_snapshot = snapshots.first()
            .ok_or_else(|| anyhow::anyhow!("No snapshots found for node {}", node_name))?;

        let deploy_path = node_config.pruning_deploy_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No deploy path configured for node {}", node_name))?;

        info!("Starting snapshot restore for node {} from {} ({})",
              node_name, latest_snapshot.filename, latest_snapshot.compression_type);

        // Start maintenance tracking for restore operation
        self.maintenance_tracker
            .start_maintenance(node_name, "snapshot_restore", 60, &node_config.server_host) // 1 hour for restore
            .await?;

        let restore_result = self.execute_snapshot_restore(node_name, &node_config, deploy_path, latest_snapshot).await;

        // End maintenance tracking
        if let Err(e) = self.maintenance_tracker.end_maintenance(node_name).await {
            error!("Failed to end maintenance mode for {}: {}", node_name, e);
        }

        // Send notification
        let status = if restore_result.is_ok() { "completed" } else { "failed" };
        if let Err(e) = self.send_snapshot_notification(node_name, status, "snapshot_restore").await {
            warn!("Failed to send snapshot notification: {}", e);
        }

        restore_result
    }

    async fn execute_snapshot_restore(
        &self,
        node_name: &str,
        node_config: &NodeConfig,
        deploy_path: &str,
        snapshot: &SnapshotInfo,
    ) -> Result<SnapshotInfo> {
        // Step 1: Stop service
        if let Some(service_name) = &node_config.pruning_service_name {
            info!("Step 1: Stopping service {}", service_name);
            self.ssh_manager.stop_service(&node_config.server_host, service_name).await?;
        }

        // Step 2: Backup current validator state
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let current_validator_backup = format!("{}/validator_state_pre_restore_{}.json",
            node_config.snapshot_backup_path.as_ref().unwrap(), timestamp);

        let backup_current_validator_cmd = format!(
            "if [ -f '{}/data/priv_validator_state.json' ]; then cp '{}/data/priv_validator_state.json' '{}'; fi",
            deploy_path, deploy_path, current_validator_backup
        );

        match self.ssh_manager.execute_single_command(&node_config.server_host, &backup_current_validator_cmd).await {
            Ok(_) => info!("Step 2: Current validator state backed up"),
            Err(e) => warn!("Step 2: Could not backup current validator state: {}", e),
        }

        // Step 3: Remove existing data and wasm directories
        info!("Step 3: Removing existing data and wasm directories");
        let cleanup_cmd = format!("cd '{}' && rm -rf data wasm", deploy_path);
        self.ssh_manager.execute_single_command(&node_config.server_host, &cleanup_cmd).await?;

        // Step 4: Extract snapshot based on compression type
        info!("Step 4: Extracting {} snapshot {}", snapshot.compression_type, snapshot.filename);

        let extract_cmd = match snapshot.compression_type.as_str() {
            "lz4" => {
                // CHANGED: LZ4 decompression
                format!("cd '{}' && lz4 -d -c '{}' | tar -xf -", deploy_path, snapshot.snapshot_path)
            },
            "gzip" => {
                // Legacy gzip support
                format!("cd '{}' && tar -xzf '{}'", deploy_path, snapshot.snapshot_path)
            },
            _ => {
                return Err(anyhow::anyhow!("Unsupported compression type: {}", snapshot.compression_type));
            }
        };

        self.ssh_manager.execute_single_command(&node_config.server_host, &extract_cmd).await?;

        // Step 5: Restore validator state from backup (use the one from current backup, not snapshot backup)
        let restore_validator_cmd = format!(
            "if [ -f '{}' ]; then cp '{}' '{}/data/priv_validator_state.json'; fi",
            current_validator_backup, current_validator_backup, deploy_path
        );

        match self.ssh_manager.execute_single_command(&node_config.server_host, &restore_validator_cmd).await {
            Ok(_) => info!("Step 5: Validator state restored"),
            Err(e) => warn!("Step 5: Could not restore validator state: {}", e),
        }

        // Step 6: Start service
        if let Some(service_name) = &node_config.pruning_service_name {
            info!("Step 6: Starting service {}", service_name);
            self.ssh_manager.start_service(&node_config.server_host, service_name).await?;
        }

        info!("Snapshot restore completed for {}", node_name);
        Ok(snapshot.clone())
    }

    /// Check if auto-restore should trigger for a node
    pub async fn check_auto_restore_trigger(&self, node_name: &str) -> Result<bool> {
        let node_config = self.get_node_config(node_name)?;

        if !node_config.snapshots_enabled.unwrap_or(false) || !node_config.auto_restore_enabled.unwrap_or(false) {
            return Ok(false);
        }

        // Check if node is in maintenance (don't auto-restore if in maintenance)
        if self.maintenance_tracker.is_in_maintenance(node_name).await {
            debug!("Node {} is in maintenance, skipping auto-restore check", node_name);
            return Ok(false);
        }

        // Get global trigger words from main config
        if self.config.auto_restore_trigger_words.is_empty() {
            debug!("No global trigger words configured, skipping auto-restore check");
            return Ok(false);
        }

        // Check logs for trigger words (last 500 lines from /var/log/out1.log)
        let log_path = "/var/log/out1.log";
        let check_logs_cmd = format!(
            "tail -n 500 '{}' 2>/dev/null || echo ''",
            log_path
        );

        let log_output = self.ssh_manager
            .execute_single_command(&node_config.server_host, &check_logs_cmd)
            .await
            .unwrap_or_default();

        // Check for any trigger words in logs
        for trigger_word in &self.config.auto_restore_trigger_words {
            if log_output.to_lowercase().contains(&trigger_word.to_lowercase()) {
                info!("Auto-restore trigger found for {}: '{}' detected in logs", node_name, trigger_word);
                return Ok(true);
            }
        }

        debug!("No auto-restore triggers found for {}", node_name);
        Ok(false)
    }

    /// Execute auto-restore for a node
    pub async fn execute_auto_restore(&self, node_name: &str) -> Result<SnapshotInfo> {
        info!("Executing auto-restore for node {}", node_name);

        let result = self.restore_from_snapshot(node_name).await;

        // Send critical alert if auto-restore fails
        if result.is_err() {
            if let Err(e) = self.send_critical_auto_restore_alert(node_name).await {
                error!("Failed to send critical auto-restore alert: {}", e);
            }
        }

        result
    }

    /// NEW: Clean up old snapshots based on retention count
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

        info!("Cleaning up {} old snapshots for node {} (keeping {} most recent)",
              snapshots_to_delete.len(), node_name, retention_count);

        for snapshot in snapshots_to_delete {
            match self.delete_snapshot_file(&node_config.server_host, &snapshot.snapshot_path).await {
                Ok(_) => {
                    info!("Deleted old snapshot: {}", snapshot.filename);
                    deleted_count += 1;
                }
                Err(e) => {
                    warn!("Failed to delete snapshot {}: {}", snapshot.filename, e);
                }
            }

            // Also clean up associated validator state backup files
            if let Some(backup_path) = &node_config.snapshot_backup_path {
                let timestamp_from_filename = snapshot.filename
                    .strip_prefix(&format!("{}_", node_config.network))
                    .and_then(|s| s.strip_suffix(".lz4").or_else(|| s.strip_suffix(".tar.gz")));

                if let Some(timestamp) = timestamp_from_filename {
                    let validator_backup_file = format!("{}/validator_state_backup_{}.json", backup_path, timestamp);
                    if let Err(e) = self.delete_snapshot_file(&node_config.server_host, &validator_backup_file).await {
                        debug!("Could not delete validator backup file {}: {}", validator_backup_file, e);
                    }
                }
            }
        }

        info!("Cleaned up {} old snapshots for node {}", deleted_count, node_name);
        Ok(deleted_count)
    }

    /// Delete a specific snapshot
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

        info!("Deleted snapshot {} for node {}", filename, node_name);
        Ok(())
    }

    /// Helper method to delete a snapshot file
    async fn delete_snapshot_file(&self, server_host: &str, file_path: &str) -> Result<()> {
        let delete_cmd = format!("rm -f '{}'", file_path);
        self.ssh_manager.execute_single_command(server_host, &delete_cmd).await?;
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

    async fn send_snapshot_notification(&self, node_name: &str, status: &str, operation: &str) -> Result<()> {
        if self.config.alarm_webhook_url.is_empty() {
            return Ok(());
        }

        let server_host = self.get_server_for_node(node_name).await.unwrap_or_else(|| "unknown".to_string());

        let alarm = AlarmPayload {
            timestamp: Utc::now(),
            alarm_type: "node_snapshot".to_string(),
            severity: if status == "failed" { "high".to_string() } else { "info".to_string() },
            node_name: node_name.to_string(),
            message: format!("Node {} LZ4 snapshot operation {}: {}", node_name, status, operation),
            details: serde_json::json!({
                "snapshot_status": status,
                "operation_type": operation,
                "server_host": server_host,
                "compression_type": "lz4",
                "timestamp": Utc::now().to_rfc3339()
            }),
        };

        let client = reqwest::Client::new();
        let response = client
            .post(&self.config.alarm_webhook_url)
            .json(&alarm)
            .send()
            .await?;

        if response.status().is_success() {
            info!("Sent snapshot notification for {}: {} {}", node_name, operation, status);
        } else {
            warn!("Failed to send snapshot notification: HTTP {}", response.status());
        }

        Ok(())
    }

    async fn send_critical_auto_restore_alert(&self, node_name: &str) -> Result<()> {
        if self.config.alarm_webhook_url.is_empty() {
            return Ok(());
        }

        let server_host = self.get_server_for_node(node_name).await.unwrap_or_else(|| "unknown".to_string());

        let alarm = AlarmPayload {
            timestamp: Utc::now(),
            alarm_type: "auto_restore_failed".to_string(),
            severity: "critical".to_string(),
            node_name: node_name.to_string(),
            message: format!("CRITICAL: Auto-restore failed for node {}", node_name),
            details: serde_json::json!({
                "alert_type": "auto_restore_failure",
                "node_name": node_name,
                "server_host": server_host,
                "compression_type": "lz4",
                "timestamp": Utc::now().to_rfc3339(),
                "requires_immediate_attention": true
            }),
        };

        let client = reqwest::Client::new();
        client
            .post(&self.config.alarm_webhook_url)
            .json(&alarm)
            .send()
            .await?;

        error!("Sent critical auto-restore failure alert for node {}", node_name);
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
            ssh_manager: self.ssh_manager.clone(),
            maintenance_tracker: self.maintenance_tracker.clone(),
        }
    }
}
