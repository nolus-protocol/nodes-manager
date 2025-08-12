// File: src/maintenance_tracker.rs

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceWindow {
    pub node_name: String,
    pub operation_type: String,
    pub started_at: DateTime<Utc>,
    pub estimated_duration_minutes: u32,
    pub server_host: String,
}

pub struct MaintenanceTracker {
    active_maintenance: Arc<RwLock<HashMap<String, MaintenanceWindow>>>,
}

impl MaintenanceTracker {
    pub fn new() -> Self {
        Self {
            active_maintenance: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Start maintenance mode for a node
    pub async fn start_maintenance(
        &self,
        node_name: &str,
        operation_type: &str,
        estimated_duration_minutes: u32,
        server_host: &str,
    ) -> Result<()> {
        let maintenance = MaintenanceWindow {
            node_name: node_name.to_string(),
            operation_type: operation_type.to_string(),
            started_at: Utc::now(),
            estimated_duration_minutes,
            server_host: server_host.to_string(),
        };

        let mut active = self.active_maintenance.write().await;
        active.insert(node_name.to_string(), maintenance);

        info!(
            "Started maintenance for node: {} ({}) on server: {}",
            node_name, operation_type, server_host
        );
        Ok(())
    }

    /// End maintenance mode for a node
    pub async fn end_maintenance(&self, node_name: &str) -> Result<()> {
        let mut active = self.active_maintenance.write().await;
        if let Some(maintenance) = active.remove(node_name) {
            let duration = Utc::now().signed_duration_since(maintenance.started_at);
            info!(
                "Completed maintenance for node: {} after {}m",
                node_name,
                duration.num_minutes()
            );
        }
        Ok(())
    }

    /// Check if a node is currently in maintenance
    pub async fn is_in_maintenance(&self, node_name: &str) -> bool {
        let active = self.active_maintenance.read().await;
        active.contains_key(node_name)
    }

    /// Get maintenance status for a specific node
    pub async fn get_maintenance_status(&self, node_name: &str) -> Option<MaintenanceWindow> {
        let active = self.active_maintenance.read().await;
        active.get(node_name).cloned()
    }

    /// Get all nodes currently in maintenance
    pub async fn get_all_in_maintenance(&self) -> Vec<MaintenanceWindow> {
        let active = self.active_maintenance.read().await;
        active.values().cloned().collect()
    }

    /// Cleanup expired maintenance windows (safety cleanup)
    pub async fn cleanup_expired_maintenance(&self, max_duration_hours: u32) -> u32 {
        let mut active = self.active_maintenance.write().await;
        let cutoff = Utc::now() - chrono::Duration::hours(max_duration_hours as i64);
        let initial_count = active.len();

        active.retain(|node_name, maintenance| {
            if maintenance.started_at < cutoff {
                warn!(
                    "Cleaning up expired maintenance for node: {} (started: {})",
                    node_name,
                    maintenance.started_at
                );
                false
            } else {
                true
            }
        });

        let cleaned_count = initial_count - active.len();
        if cleaned_count > 0 {
            info!("Cleaned up {} expired maintenance windows", cleaned_count);
        }

        cleaned_count as u32
    }

    /// Get maintenance statistics
    pub async fn get_maintenance_stats(&self) -> MaintenanceStats {
        let active = self.active_maintenance.read().await;
        let total_active = active.len();

        let mut by_operation = HashMap::new();
        let mut by_server = HashMap::new();

        for maintenance in active.values() {
            *by_operation
                .entry(maintenance.operation_type.clone())
                .or_insert(0) += 1;
            *by_server
                .entry(maintenance.server_host.clone())
                .or_insert(0) += 1;
        }

        MaintenanceStats {
            total_active,
            by_operation_type: by_operation,
            by_server: by_server,
        }
    }

    /// Emergency cleanup - force end all maintenance
    pub async fn emergency_clear_all_maintenance(&self) -> u32 {
        let mut active = self.active_maintenance.write().await;
        let count = active.len();
        active.clear();
        warn!("Emergency cleared {} maintenance windows", count);
        count as u32
    }
}

#[derive(Debug, Serialize)]
pub struct MaintenanceStats {
    pub total_active: usize,
    pub by_operation_type: HashMap<String, u32>,
    pub by_server: HashMap<String, u32>,
}

impl Clone for MaintenanceTracker {
    fn clone(&self) -> Self {
        Self {
            active_maintenance: self.active_maintenance.clone(),
        }
    }
}

impl Default for MaintenanceTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_maintenance_tracking() {
        let tracker = MaintenanceTracker::new();

        // Start maintenance
        tracker
            .start_maintenance("test-node", "pruning", 30, "test-server")
            .await
            .unwrap();

        assert!(tracker.is_in_maintenance("test-node").await);
        assert!(!tracker.is_in_maintenance("other-node").await);

        // End maintenance
        tracker.end_maintenance("test-node").await.unwrap();
        assert!(!tracker.is_in_maintenance("test-node").await);
    }

    #[tokio::test]
    async fn test_maintenance_stats() {
        let tracker = MaintenanceTracker::new();

        tracker
            .start_maintenance("node1", "pruning", 30, "server1")
            .await
            .unwrap();
        tracker
            .start_maintenance("node2", "restart", 10, "server1")
            .await
            .unwrap();
        tracker
            .start_maintenance("node3", "pruning", 30, "server2")
            .await
            .unwrap();

        let stats = tracker.get_maintenance_stats().await;
        assert_eq!(stats.total_active, 3);
        assert_eq!(stats.by_operation_type.get("pruning"), Some(&2));
        assert_eq!(stats.by_server.get("server1"), Some(&2));
    }
}
