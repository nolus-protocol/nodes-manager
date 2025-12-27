// File: manager/src/services/snapshot_service.rs
use anyhow::Result;
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use tracing::info;

use crate::config::Config;
use crate::snapshot::SnapshotManager;

#[derive(Clone)]
pub struct SnapshotService {
    config: Arc<Config>,
    snapshot_manager: Arc<SnapshotManager>,
}

impl SnapshotService {
    pub fn new(config: Arc<Config>, snapshot_manager: Arc<SnapshotManager>) -> Self {
        Self {
            config,
            snapshot_manager,
        }
    }

    pub async fn list_snapshots(
        &self,
        node_name: &str,
    ) -> Result<Vec<crate::snapshot::SnapshotInfo>> {
        self.validate_node_name(node_name)?;
        self.validate_snapshot_access(node_name)?;

        self.snapshot_manager.list_snapshots(node_name).await
    }

    pub async fn restore_from_snapshot(
        &self,
        node_name: &str,
    ) -> Result<crate::snapshot::SnapshotInfo> {
        self.validate_node_name(node_name)?;
        self.validate_auto_restore_enabled(node_name)?;

        info!("Restoring latest snapshot for node: {}", node_name);
        self.snapshot_manager.restore_from_snapshot(node_name).await
    }

    pub async fn delete_snapshot(&self, node_name: &str, filename: &str) -> Result<()> {
        self.validate_node_name(node_name)?;
        self.validate_snapshots_enabled(node_name)?;

        info!("Deleting snapshot {} for node: {}", filename, node_name);
        self.snapshot_manager
            .delete_snapshot(node_name, filename)
            .await
    }

    pub async fn check_auto_restore_trigger(&self, node_name: &str) -> Result<bool> {
        self.validate_node_name(node_name)?;
        self.validate_auto_restore_enabled(node_name)?;

        info!("Checking auto-restore triggers for node: {}", node_name);
        self.snapshot_manager
            .check_auto_restore_trigger(node_name)
            .await
    }

    pub async fn get_snapshot_stats(
        &self,
        node_name: &str,
    ) -> Result<crate::snapshot::SnapshotStats> {
        self.validate_node_name(node_name)?;
        self.validate_snapshot_access(node_name)?;

        self.snapshot_manager.get_snapshot_stats(node_name).await
    }

    pub async fn cleanup_old_snapshots(
        &self,
        node_name: &str,
        retention_count: u32,
    ) -> Result<serde_json::Value> {
        self.validate_node_name(node_name)?;
        self.validate_snapshots_enabled(node_name)?;

        if retention_count == 0 {
            return Err(anyhow::anyhow!("Retention count must be at least 1"));
        }

        info!(
            "Cleaning up old snapshots for node {} (keeping {})",
            node_name, retention_count
        );

        let deleted_count = self
            .snapshot_manager
            .cleanup_old_snapshots(node_name, retention_count)
            .await?;

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
            Err(anyhow::anyhow!(
                "Snapshots not enabled for node {}",
                node_name
            ))
        }
    }

    #[inline]
    fn validate_auto_restore_enabled(&self, node_name: &str) -> Result<()> {
        let node_config = self.config.nodes.get(node_name).unwrap();

        if node_config.auto_restore_enabled.unwrap_or(false) {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Auto-restore not enabled for node {}",
                node_name
            ))
        }
    }

    #[inline]
    fn validate_snapshot_access(&self, node_name: &str) -> Result<()> {
        let node_config = self.config.nodes.get(node_name).unwrap();

        if node_config.snapshots_enabled.unwrap_or(false)
            || node_config.auto_restore_enabled.unwrap_or(false)
        {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Neither snapshots nor auto-restore enabled for node {}",
                node_name
            ))
        }
    }
}
