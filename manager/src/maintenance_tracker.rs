//! Maintenance window tracking to prevent concurrent operations
//!
//! This module tracks active maintenance windows for nodes and services to ensure:
//! - Only one operation runs per node at a time
//! - Health checks don't alert during maintenance
//! - Scheduled operations don't conflict with manual operations
//!
//! # Key Features
//!
//! - **Mutual exclusion**: Prevents concurrent operations on same node
//! - **Estimated duration**: Each operation has estimated completion time
//! - **Automatic cleanup**: Stuck maintenance windows cleaned after 48 hours
//! - **Emergency cleanup**: API endpoint for manual intervention
//!
//! # Usage
//!
//! ```ignore
//! // Start maintenance window
//! tracker.start_maintenance("osmosis-1", "pruning", 300, "server-1").await?;
//!
//! // Perform operation...
//!
//! // End maintenance window
//! tracker.end_maintenance("osmosis-1").await?;
//! ```

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, instrument, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceWindow {
    pub node_name: String,
    pub operation_type: String,
    pub started_at: DateTime<Utc>,
    pub estimated_duration_minutes: u32,
    pub server_name: String,
}

#[derive(Clone, Default)]
pub struct MaintenanceTracker {
    active_maintenance: Arc<RwLock<HashMap<String, MaintenanceWindow>>>,
}

impl MaintenanceTracker {
    pub fn new() -> Self {
        Self {
            active_maintenance: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    #[instrument(skip(self), fields(node = %node_name, operation = %operation_type))]
    pub async fn start_maintenance(
        &self,
        node_name: &str,
        operation_type: &str,
        estimated_duration_minutes: u32,
        server_name: &str,
    ) -> Result<()> {
        let mut active = self.active_maintenance.write().await;

        if active.contains_key(node_name) {
            return Err(anyhow::anyhow!(
                "Node {} is already in maintenance",
                node_name
            ));
        }

        let maintenance = MaintenanceWindow {
            node_name: node_name.to_string(),
            operation_type: operation_type.to_string(),
            started_at: Utc::now(),
            estimated_duration_minutes,
            server_name: server_name.to_string(),
        };

        active.insert(node_name.to_string(), maintenance);

        info!(
            "Started maintenance for node: {} (operation: {}, estimated: {}m)",
            node_name, operation_type, estimated_duration_minutes
        );

        Ok(())
    }

    #[instrument(skip(self), fields(node = %node_name))]
    pub async fn end_maintenance(&self, node_name: &str) -> Result<()> {
        let mut active = self.active_maintenance.write().await;

        if let Some(maintenance) = active.remove(node_name) {
            let duration = Utc::now().signed_duration_since(maintenance.started_at);
            info!(
                "Completed maintenance for node: {} after {}m (estimated: {}m)",
                node_name,
                duration.num_minutes(),
                maintenance.estimated_duration_minutes
            );
        } else {
            warn!(
                "Tried to end maintenance for node {} but it was not in maintenance",
                node_name
            );
        }
        Ok(())
    }

    #[inline]
    pub async fn is_in_maintenance(&self, node_name: &str) -> bool {
        let active = self.active_maintenance.read().await;
        active.contains_key(node_name)
    }

    pub async fn cleanup_expired_maintenance(&self, max_duration_hours: u32) -> u32 {
        let mut active = self.active_maintenance.write().await;
        let cutoff_timestamp =
            Utc::now().timestamp_millis() - (max_duration_hours as i64 * 3600 * 1000);
        let initial_count = active.len();

        let mut cleaned_nodes = Vec::with_capacity(4);

        active.retain(|node_name, maintenance| {
            let started_timestamp = maintenance.started_at.timestamp_millis();
            let should_keep = started_timestamp > cutoff_timestamp;

            if !should_keep {
                let actual_duration_hours = (Utc::now().timestamp_millis() - started_timestamp) / (1000 * 3600);
                warn!(
                    "Cleaning up expired maintenance for node: {} (started: {}, actual_duration: {}h, limit: {}h, operation: {})",
                    node_name,
                    maintenance.started_at.format("%Y-%m-%d %H:%M:%S"),
                    actual_duration_hours,
                    max_duration_hours,
                    maintenance.operation_type
                );
                cleaned_nodes.push(format!("{}:{}", node_name, maintenance.operation_type));
            }

            should_keep
        });

        let cleaned_count = initial_count - active.len();
        if cleaned_count > 0 {
            error!(
                "Cleaned up {} expired maintenance windows ({}h max): {:?}",
                cleaned_count, max_duration_hours, cleaned_nodes
            );
        }

        cleaned_count as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_maintenance_tracking() {
        let tracker = MaintenanceTracker::new();

        tracker
            .start_maintenance("test-node", "pruning", 300, "test-server")
            .await
            .unwrap();

        let in_maintenance = tracker.is_in_maintenance("test-node").await;
        assert!(in_maintenance);

        tracker.end_maintenance("test-node").await.unwrap();
        let in_maintenance = tracker.is_in_maintenance("test-node").await;
        assert!(!in_maintenance);
    }
}
