// File: manager/src/maintenance_tracker.rs

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceWindow {
    pub node_name: String,
    pub operation_type: String,
    pub started_at: DateTime<Utc>,
    pub estimated_duration_minutes: u32,
    pub server_host: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceStats {
    pub total_active: usize,
    pub total_completed_today: usize,
    pub average_duration_minutes: u32,
    pub longest_running_minutes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceReport {
    pub active_operations: Vec<MaintenanceWindow>,
    pub overdue_operations: Vec<MaintenanceWindow>,
    pub stats: MaintenanceStats,
    pub timestamp: DateTime<Utc>,
}

pub struct MaintenanceTracker {
    active_maintenance: Arc<RwLock<HashMap<String, MaintenanceWindow>>>,
}

impl MaintenanceTracker {
    pub fn new() -> Self {
        Self {
            active_maintenance: Arc::new(RwLock::new(HashMap::with_capacity(32))),
        }
    }

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
            "Started maintenance for node: {} ({}) on server: {} (estimated: {}m)",
            node_name, operation_type, server_host, estimated_duration_minutes
        );
        Ok(())
    }

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
            warn!("Tried to end maintenance for node {} but it was not in maintenance", node_name);
        }
        Ok(())
    }

    #[inline]
    pub async fn is_in_maintenance(&self, node_name: &str) -> bool {
        let active = self.active_maintenance.read().await;
        active.contains_key(node_name)
    }

    pub async fn get_maintenance_status(&self, node_name: &str) -> Option<MaintenanceWindow> {
        let active = self.active_maintenance.read().await;
        active.get(node_name).cloned()
    }

    pub async fn get_all_in_maintenance(&self) -> Vec<MaintenanceWindow> {
        let active = self.active_maintenance.read().await;
        active.values().cloned().collect()
    }

    pub async fn get_maintenance_stats(&self) -> MaintenanceStats {
        let active = self.active_maintenance.read().await;
        let now = Utc::now();

        let mut longest_running_minutes = 0;
        for maintenance in active.values() {
            let duration = (now - maintenance.started_at).num_minutes() as u32;
            if duration > longest_running_minutes {
                longest_running_minutes = duration;
            }
        }

        MaintenanceStats {
            total_active: active.len(),
            total_completed_today: 0, // Would need database to track this
            average_duration_minutes: 60, // Default estimate
            longest_running_minutes,
        }
    }

    pub async fn get_maintenance_report(&self) -> MaintenanceReport {
        let active_operations = self.get_all_in_maintenance().await;
        let overdue_operations = self.get_overdue_maintenance().await;
        let stats = self.get_maintenance_stats().await;

        MaintenanceReport {
            active_operations,
            overdue_operations,
            stats,
            timestamp: Utc::now(),
        }
    }

    pub async fn get_overdue_maintenance(&self) -> Vec<MaintenanceWindow> {
        let active = self.active_maintenance.read().await;
        let now = Utc::now();

        active.values()
            .filter(|maintenance| {
                let duration = (now - maintenance.started_at).num_minutes() as u32;
                duration > (maintenance.estimated_duration_minutes * 3) // 3x estimated duration
            })
            .cloned()
            .collect()
    }

    pub async fn emergency_clear_all_maintenance(&self) -> u32 {
        let mut active = self.active_maintenance.write().await;
        let count = active.len() as u32;
        active.clear();

        if count > 0 {
            warn!("Emergency cleared all {} maintenance operations", count);
        }

        count
    }

    pub async fn cleanup_expired_maintenance(&self, max_duration_hours: u32) -> u32 {
        let mut active = self.active_maintenance.write().await;
        let cutoff_timestamp = Utc::now().timestamp_millis() - (max_duration_hours as i64 * 3600 * 1000);
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
            error!("Cleaned up {} expired maintenance windows ({}h max): {:?}",
                   cleaned_count, max_duration_hours, cleaned_nodes);
        }

        cleaned_count as u32
    }
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
