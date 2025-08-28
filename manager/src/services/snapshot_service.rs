// File: manager/src/services/snapshot_service.rs

use anyhow::Result;
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use tracing::{info, warn};

use crate::maintenance_tracker::MaintenanceTracker;
use crate::snapshot::SnapshotManager;
use crate::{Config, NodeConfig};

pub struct SnapshotService {
    config: Arc<Config>,
    snapshot_manager: Arc<SnapshotManager>,
    _maintenance_tracker: Arc<MaintenanceTracker>,
}

impl SnapshotService {
    pub fn new(
        config: Arc<Config>,
        snapshot_manager: Arc<SnapshotManager>,
        maintenance_tracker: Arc<MaintenanceTracker>,
    ) -> Self {
        Self {
            config,
            snapshot_manager,
            _maintenance_tracker: maintenance_tracker,
        }
    }

    pub async fn create_snapshot(&self, node_name: &str) -> Result<crate::snapshot::SnapshotInfo> {
        self.validate_node_name(node_name)?;
        self.validate_snapshots_enabled(node_name)?;

        info!("Creating LZ4 snapshot for node: {}", node_name);
        self.snapshot_manager.create_snapshot(node_name).await
    }

    pub async fn list_snapshots(&self, node_name: &str) -> Result<Vec<crate::snapshot::SnapshotInfo>> {
        self.validate_node_name(node_name)?;
        self.validate_snapshots_enabled(node_name)?;

        self.snapshot_manager.list_snapshots(node_name).await
    }

    pub async fn restore_from_snapshot(&self, node_name: &str) -> Result<crate::snapshot::SnapshotInfo> {
        self.validate_node_name(node_name)?;
        self.validate_snapshots_enabled(node_name)?;

        info!("Restoring latest snapshot for node: {}", node_name);
        self.snapshot_manager.restore_from_snapshot(node_name).await
    }

    pub async fn delete_snapshot(&self, node_name: &str, filename: &str) -> Result<()> {
        self.validate_node_name(node_name)?;
        self.validate_snapshots_enabled(node_name)?;

        info!("Deleting snapshot {} for node: {}", filename, node_name);
        self.snapshot_manager.delete_snapshot(node_name, filename).await
    }

    pub async fn check_auto_restore_trigger(&self, node_name: &str) -> Result<bool> {
        self.validate_node_name(node_name)?;
        self.validate_auto_restore_enabled(node_name)?;

        info!("Checking auto-restore triggers for node: {}", node_name);
        self.snapshot_manager.check_auto_restore_trigger(node_name).await
    }

    pub async fn get_snapshot_stats(&self, node_name: &str) -> Result<crate::snapshot::SnapshotStats> {
        self.validate_node_name(node_name)?;
        self.validate_snapshots_enabled(node_name)?;

        self.snapshot_manager.get_snapshot_stats(node_name).await
    }

    pub async fn cleanup_old_snapshots(&self, node_name: &str, retention_count: u32) -> Result<serde_json::Value> {
        self.validate_node_name(node_name)?;
        self.validate_snapshots_enabled(node_name)?;

        if retention_count == 0 {
            return Err(anyhow::anyhow!("Retention count must be at least 1"));
        }

        info!("Cleaning up old snapshots for node {} (keeping {})", node_name, retention_count);

        let deleted_count = self.snapshot_manager.cleanup_old_snapshots(node_name, retention_count).await?;

        Ok(json!({
            "node_name": node_name,
            "retention_count": retention_count,
            "deleted_count": deleted_count,
            "timestamp": Utc::now().to_rfc3339()
        }))
    }

    #[inline]
    fn validate_node_name(&self, node_name: &str) -> Result<()> {
        if self.config.nodes.contains_key(node_name) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Node '{}' not found", node_name))
        }
    }

    #[inline]
    fn validate_snapshots_enabled(&self, node_name: &str) -> Result<()> {
        let node_config = self.config.nodes.get(node_name).unwrap();

        if node_config.snapshots_enabled.unwrap_or(false) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name))
        }
    }

    #[inline]
    fn validate_auto_restore_enabled(&self, node_name: &str) -> Result<()> {
        let node_config = self.config.nodes.get(node_name).unwrap();

        if node_config.auto_restore_enabled.unwrap_or(false) && node_config.snapshots_enabled.unwrap_or(false) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Auto-restore not enabled or snapshots not enabled for node {}", node_name))
        }
    }

    pub fn get_snapshot_enabled_nodes(&self) -> Vec<(&String, &NodeConfig)> {
        self.config.nodes
            .iter()
            .filter(|(_, node)| node.snapshots_enabled.unwrap_or(false))
            .collect()
    }

    pub fn get_auto_restore_enabled_nodes(&self) -> Vec<(&String, &NodeConfig)> {
        self.config.nodes
            .iter()
            .filter(|(_, node)| {
                node.auto_restore_enabled.unwrap_or(false) && node.snapshots_enabled.unwrap_or(false)
            })
            .collect()
    }

    pub fn get_scheduled_snapshot_nodes(&self) -> Vec<(&String, &NodeConfig)> {
        self.config.nodes
            .iter()
            .filter(|(_, node)| node.snapshot_schedule.is_some())
            .collect()
    }

    pub async fn get_batch_snapshot_stats(&self) -> Result<serde_json::Value> {
        let snapshot_nodes = self.get_snapshot_enabled_nodes();
        let mut all_stats = Vec::with_capacity(snapshot_nodes.len());
        let mut total_snapshots = 0;
        let mut total_size_bytes = 0;

        for (node_name, _) in snapshot_nodes {
            match self.snapshot_manager.get_snapshot_stats(node_name).await {
                Ok(stats) => {
                    total_snapshots += stats.total_snapshots;
                    total_size_bytes += stats.total_size_bytes;
                    all_stats.push(json!({
                        "node_name": node_name,
                        "total_snapshots": stats.total_snapshots,
                        "total_size_bytes": stats.total_size_bytes,
                        "compression_type": stats.compression_type
                    }));
                }
                Err(e) => {
                    warn!("Failed to get stats for node {}: {}", node_name, e);
                }
            }
        }

        Ok(json!({
            "total_nodes_with_snapshots": all_stats.len(),
            "total_snapshots_across_all_nodes": total_snapshots,
            "total_size_bytes_across_all_nodes": total_size_bytes,
            "total_size_gb": total_size_bytes as f64 / (1024.0 * 1024.0 * 1024.0),
            "compression_type": "lz4",
            "node_stats": all_stats,
            "timestamp": Utc::now().to_rfc3339()
        }))
    }

    pub async fn get_service_statistics(&self) -> Result<serde_json::Value> {
        let snapshot_enabled_nodes = self.get_snapshot_enabled_nodes().len();
        let auto_restore_enabled_nodes = self.get_auto_restore_enabled_nodes().len();
        let scheduled_snapshot_nodes = self.get_scheduled_snapshot_nodes().len();

        let batch_stats = self.get_batch_snapshot_stats().await.unwrap_or_else(|_| json!({}));

        Ok(json!({
            "snapshot_enabled_nodes": snapshot_enabled_nodes,
            "auto_restore_enabled_nodes": auto_restore_enabled_nodes,
            "scheduled_snapshot_nodes": scheduled_snapshot_nodes,
            "total_snapshots": batch_stats["total_snapshots_across_all_nodes"].as_u64().unwrap_or(0),
            "total_size_gb": batch_stats["total_size_gb"].as_f64().unwrap_or(0.0),
            "compression_type": "lz4",
            "features": {
                "lz4_compression": true,
                "auto_restore": auto_restore_enabled_nodes > 0,
                "scheduled_snapshots": scheduled_snapshot_nodes > 0,
                "retention_management": true
            },
            "timestamp": Utc::now().to_rfc3339()
        }))
    }
}

impl Clone for SnapshotService {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            snapshot_manager: self.snapshot_manager.clone(),
            _maintenance_tracker: self._maintenance_tracker.clone(),
        }
    }
}
