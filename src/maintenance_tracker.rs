// File: src/maintenance_tracker.rs

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
            "Started maintenance for node: {} ({}) on server: {} (estimated: {}m)",
            node_name, operation_type, server_host, estimated_duration_minutes
        );
        Ok(())
    }

    /// End maintenance mode for a node
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

    /// Get maintenance windows that have exceeded their estimated duration
    pub async fn get_overdue_maintenance(&self) -> Vec<MaintenanceWindow> {
        let active = self.active_maintenance.read().await;
        let now = Utc::now();

        active.values()
            .filter(|maintenance| {
                let estimated_end = maintenance.started_at
                    + chrono::Duration::minutes(maintenance.estimated_duration_minutes as i64);
                now > estimated_end
            })
            .cloned()
            .collect()
    }

    /// Cleanup expired maintenance windows (safety cleanup) - EXTENDED for long operations
    pub async fn cleanup_expired_maintenance(&self, max_duration_hours: u32) -> u32 {
        let mut active = self.active_maintenance.write().await;
        let cutoff = Utc::now() - chrono::Duration::hours(max_duration_hours as i64);
        let initial_count = active.len();

        let mut cleaned_nodes = Vec::new();
        active.retain(|node_name, maintenance| {
            if maintenance.started_at < cutoff {
                let duration = Utc::now().signed_duration_since(maintenance.started_at);
                warn!(
                    "Cleaning up expired maintenance for node: {} (started: {}, duration: {}h, operation: {})",
                    node_name,
                    maintenance.started_at,
                    duration.num_hours(),
                    maintenance.operation_type
                );
                cleaned_nodes.push(format!("{}:{}", node_name, maintenance.operation_type));
                false
            } else {
                true
            }
        });

        let cleaned_count = initial_count - active.len();
        if cleaned_count > 0 {
            error!("Cleaned up {} expired maintenance windows ({}h max): {:?}",
                   cleaned_count, max_duration_hours, cleaned_nodes);
        }

        cleaned_count as u32
    }

    /// Cleanup maintenance windows that have exceeded their estimated duration by a factor
    pub async fn cleanup_overdue_maintenance(&self, overdue_factor: f64) -> u32 {
        let mut active = self.active_maintenance.write().await;
        let now = Utc::now();
        let initial_count = active.len();

        let mut cleaned_nodes = Vec::new();
        active.retain(|node_name, maintenance| {
            let estimated_duration_hours = maintenance.estimated_duration_minutes as f64 / 60.0;
            let max_allowed_hours = estimated_duration_hours * overdue_factor;
            let actual_duration = now.signed_duration_since(maintenance.started_at);
            let actual_hours = actual_duration.num_minutes() as f64 / 60.0;

            if actual_hours > max_allowed_hours {
                warn!(
                    "Cleaning up overdue maintenance for node: {} (operation: {}, estimated: {:.1}h, actual: {:.1}h, max allowed: {:.1}h)",
                    node_name,
                    maintenance.operation_type,
                    estimated_duration_hours,
                    actual_hours,
                    max_allowed_hours
                );
                cleaned_nodes.push(format!("{}:{}({:.1}h)", node_name, maintenance.operation_type, actual_hours));
                false
            } else {
                true
            }
        });

        let cleaned_count = initial_count - active.len();
        if cleaned_count > 0 {
            error!("Cleaned up {} overdue maintenance windows ({}x factor): {:?}",
                   cleaned_count, overdue_factor, cleaned_nodes);
        }

        cleaned_count as u32
    }

    /// Get maintenance statistics with enhanced information for long operations
    pub async fn get_maintenance_stats(&self) -> MaintenanceStats {
        let active = self.active_maintenance.read().await;
        let total_active = active.len();
        let now = Utc::now();

        let mut by_operation = HashMap::new();
        let mut by_server = HashMap::new();
        let mut overdue_count = 0;
        let mut long_running_count = 0; // NEW: Track operations over 2 hours

        for maintenance in active.values() {
            *by_operation
                .entry(maintenance.operation_type.clone())
                .or_insert(0) += 1;
            *by_server
                .entry(maintenance.server_host.clone())
                .or_insert(0) += 1;

            let actual_duration = now.signed_duration_since(maintenance.started_at);

            // Check if this maintenance is overdue
            let estimated_end = maintenance.started_at
                + chrono::Duration::minutes(maintenance.estimated_duration_minutes as i64);
            if now > estimated_end {
                overdue_count += 1;
            }

            // Check if this is a long-running operation (over 2 hours)
            if actual_duration.num_hours() >= 2 {
                long_running_count += 1;
            }
        }

        MaintenanceStats {
            total_active,
            overdue_count,
            long_running_count, // NEW field
            by_operation_type: by_operation,
            by_server: by_server,
        }
    }

    /// Emergency cleanup - force end all maintenance
    pub async fn emergency_clear_all_maintenance(&self) -> u32 {
        let mut active = self.active_maintenance.write().await;
        let count = active.len();

        if count > 0 {
            let node_operations: Vec<String> = active.iter()
                .map(|(node_name, maintenance)| {
                    let duration = Utc::now().signed_duration_since(maintenance.started_at);
                    format!("{}:{}({}h)", node_name, maintenance.operation_type, duration.num_hours())
                })
                .collect();

            active.clear();
            error!("Emergency cleared {} maintenance windows: {:?}", count, node_operations);
        } else {
            info!("Emergency clear requested but no maintenance windows were active");
        }

        count as u32
    }

    /// Get detailed maintenance report with long-running operation analysis
    pub async fn get_maintenance_report(&self) -> MaintenanceReport {
        let active = self.active_maintenance.read().await;
        let now = Utc::now();

        let mut active_windows = Vec::new();
        let mut overdue_windows = Vec::new();
        let mut long_running_windows = Vec::new(); // NEW: Track long-running operations

        for maintenance in active.values() {
            let duration = now.signed_duration_since(maintenance.started_at);
            let estimated_end = maintenance.started_at
                + chrono::Duration::minutes(maintenance.estimated_duration_minutes as i64);
            let is_overdue = now > estimated_end;
            let is_long_running = duration.num_hours() >= 2;

            let window_info = MaintenanceWindowInfo {
                node_name: maintenance.node_name.clone(),
                operation_type: maintenance.operation_type.clone(),
                server_host: maintenance.server_host.clone(),
                started_at: maintenance.started_at,
                estimated_duration_minutes: maintenance.estimated_duration_minutes,
                actual_duration_minutes: duration.num_minutes() as u32,
                actual_duration_hours: duration.num_hours() as u32, // NEW field
                is_overdue,
                is_long_running, // NEW field
                estimated_completion: estimated_end,
            };

            if is_overdue {
                overdue_windows.push(window_info.clone());
            }
            if is_long_running {
                long_running_windows.push(window_info.clone());
            }
            active_windows.push(window_info);
        }

        MaintenanceReport {
            total_active: active_windows.len(),
            total_overdue: overdue_windows.len(),
            total_long_running: long_running_windows.len(), // NEW field
            active_windows,
            overdue_windows,
            long_running_windows, // NEW field
            report_generated_at: now,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MaintenanceStats {
    pub total_active: usize,
    pub overdue_count: usize,
    pub long_running_count: usize, // NEW: Operations over 2 hours
    pub by_operation_type: HashMap<String, u32>,
    pub by_server: HashMap<String, u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MaintenanceWindowInfo {
    pub node_name: String,
    pub operation_type: String,
    pub server_host: String,
    pub started_at: DateTime<Utc>,
    pub estimated_duration_minutes: u32,
    pub actual_duration_minutes: u32,
    pub actual_duration_hours: u32, // NEW: For easier reading of long operations
    pub is_overdue: bool,
    pub is_long_running: bool, // NEW: Flag for operations over 2 hours
    pub estimated_completion: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct MaintenanceReport {
    pub total_active: usize,
    pub total_overdue: usize,
    pub total_long_running: usize, // NEW: Count of long-running operations
    pub active_windows: Vec<MaintenanceWindowInfo>,
    pub overdue_windows: Vec<MaintenanceWindowInfo>,
    pub long_running_windows: Vec<MaintenanceWindowInfo>, // NEW: Separate list for long operations
    pub report_generated_at: DateTime<Utc>,
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

        // Start maintenance with realistic duration for pruning
        tracker
            .start_maintenance("test-node", "pruning", 300, "test-server") // 5 hours
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
            .start_maintenance("node1", "pruning", 300, "server1") // 5 hours
            .await
            .unwrap();
        tracker
            .start_maintenance("node2", "restart", 10, "server1")
            .await
            .unwrap();
        tracker
            .start_maintenance("node3", "pruning", 240, "server2") // 4 hours
            .await
            .unwrap();

        let stats = tracker.get_maintenance_stats().await;
        assert_eq!(stats.total_active, 3);
        assert_eq!(stats.by_operation_type.get("pruning"), Some(&2));
        assert_eq!(stats.by_server.get("server1"), Some(&2));
    }

    #[tokio::test]
    async fn test_long_operation_tolerance() {
        let tracker = MaintenanceTracker::new();

        // Test that cleanup doesn't interfere with long but legitimate operations
        tracker
            .start_maintenance("long-node", "pruning", 300, "test-server") // 5 hours estimate
            .await
            .unwrap();

        // Should not be cleaned up by standard cleanup (6 hours)
        let cleaned = tracker.cleanup_expired_maintenance(6).await;
        assert_eq!(cleaned, 0);

        // Should not be cleaned up by moderate overdue factor (2x = 10 hours)
        let cleaned = tracker.cleanup_overdue_maintenance(2.0).await;
        assert_eq!(cleaned, 0);
    }
}
