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
    // OPTIMIZED: Pre-allocate common string literals to reduce allocations
    _operation_types: Vec<&'static str>,
}

impl MaintenanceTracker {
    pub fn new() -> Self {
        Self {
            active_maintenance: Arc::new(RwLock::new(HashMap::with_capacity(32))), // Pre-allocate capacity
            _operation_types: vec!["pruning", "hermes_restart", "snapshot_creation", "snapshot_restore"],
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

    // OPTIMIZED: Single atomic operation to prevent race conditions
    pub async fn get_maintenance_status_atomic(&self, node_name: &str) -> (bool, Option<MaintenanceWindow>) {
        let active = self.active_maintenance.read().await;
        let in_maintenance = active.contains_key(node_name);
        let maintenance_window = active.get(node_name).cloned();
        (in_maintenance, maintenance_window)
    }

    // OPTIMIZED: Use reference instead of clone for simple checks
    #[inline]
    pub async fn is_in_maintenance(&self, node_name: &str) -> bool {
        let active = self.active_maintenance.read().await;
        active.contains_key(node_name)
    }

    pub async fn get_maintenance_status(&self, node_name: &str) -> Option<MaintenanceWindow> {
        let active = self.active_maintenance.read().await;
        active.get(node_name).cloned()
    }

    // OPTIMIZED: Pre-allocate Vec with known capacity
    pub async fn get_all_in_maintenance(&self) -> Vec<MaintenanceWindow> {
        let active = self.active_maintenance.read().await;
        let mut windows = Vec::with_capacity(active.len());
        windows.extend(active.values().cloned());
        windows
    }

    // OPTIMIZED: Filter in single pass to avoid multiple iterations
    pub async fn get_overdue_maintenance(&self) -> Vec<MaintenanceWindow> {
        let active = self.active_maintenance.read().await;
        let now_utc = Utc::now();

        let mut overdue = Vec::with_capacity(active.len() / 4); // Estimate 25% might be overdue

        for maintenance in active.values() {
            let estimated_end_utc = maintenance.started_at
                + chrono::Duration::minutes(maintenance.estimated_duration_minutes as i64);
            if now_utc > estimated_end_utc {
                overdue.push(maintenance.clone());
            }
        }

        overdue
    }

    // OPTIMIZED: Use HashMap::retain for efficient removal
    pub async fn cleanup_expired_maintenance(&self, max_duration_hours: u32) -> u32 {
        let mut active = self.active_maintenance.write().await;
        let cutoff_timestamp = Utc::now().timestamp_millis() - (max_duration_hours as i64 * 3600 * 1000);
        let initial_count = active.len();

        let mut cleaned_nodes = Vec::with_capacity(4); // Most cleanups are small

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

    // OPTIMIZED: Use HashMap::retain for efficient removal
    pub async fn cleanup_overdue_maintenance(&self, overdue_factor: f64) -> u32 {
        let mut active = self.active_maintenance.write().await;
        let now_timestamp = Utc::now().timestamp_millis();
        let initial_count = active.len();

        let mut cleaned_nodes = Vec::with_capacity(4);

        active.retain(|node_name, maintenance| {
            let estimated_duration_ms = maintenance.estimated_duration_minutes as i64 * 60 * 1000;
            let max_allowed_ms = (estimated_duration_ms as f64 * overdue_factor) as i64;
            let started_timestamp = maintenance.started_at.timestamp_millis();
            let actual_duration_ms = now_timestamp - started_timestamp;

            let should_keep = actual_duration_ms <= max_allowed_ms;

            if !should_keep {
                let actual_hours = actual_duration_ms as f64 / (1000.0 * 3600.0);
                let estimated_hours = estimated_duration_ms as f64 / (1000.0 * 3600.0);

                warn!(
                    "Cleaning up overdue maintenance for node: {} (operation: {}, started: {}, estimated: {:.1}h, actual: {:.1}h)",
                    node_name,
                    maintenance.operation_type,
                    maintenance.started_at.format("%Y-%m-%d %H:%M:%S"),
                    estimated_hours,
                    actual_hours
                );
                cleaned_nodes.push(format!("{}:{}({:.1}h)", node_name, maintenance.operation_type, actual_hours));
            }

            should_keep
        });

        let cleaned_count = initial_count - active.len();
        if cleaned_count > 0 {
            error!("Cleaned up {} overdue maintenance windows ({}x factor): {:?}",
                   cleaned_count, overdue_factor, cleaned_nodes);
        }

        cleaned_count as u32
    }

    // OPTIMIZED: Single pass statistics calculation
    pub async fn get_maintenance_stats(&self) -> MaintenanceStats {
        let active = self.active_maintenance.read().await;
        let total_active = active.len();

        if total_active == 0 {
            return MaintenanceStats {
                total_active: 0,
                overdue_count: 0,
                long_running_count: 0,
                by_operation_type: HashMap::new(),
                by_server: HashMap::new(),
            };
        }

        let now_timestamp = Utc::now().timestamp_millis();
        let mut by_operation = HashMap::with_capacity(8);
        let mut by_server = HashMap::with_capacity(16);
        let mut overdue_count = 0;
        let mut long_running_count = 0;

        for maintenance in active.values() {
            // Count by operation type
            *by_operation
                .entry(maintenance.operation_type.clone())
                .or_insert(0) += 1;

            // Count by server
            *by_server
                .entry(maintenance.server_host.clone())
                .or_insert(0) += 1;

            let started_timestamp = maintenance.started_at.timestamp_millis();
            let actual_duration_ms = now_timestamp - started_timestamp;

            // Check if overdue (estimated time passed)
            let estimated_duration_ms = maintenance.estimated_duration_minutes as i64 * 60 * 1000;
            if actual_duration_ms > estimated_duration_ms {
                overdue_count += 1;
            }

            // Check if long-running (over 2 hours)
            if actual_duration_ms >= 2 * 3600 * 1000 {
                long_running_count += 1;
            }
        }

        MaintenanceStats {
            total_active,
            overdue_count,
            long_running_count,
            by_operation_type: by_operation,
            by_server: by_server,
        }
    }

    // OPTIMIZED: Use HashMap::drain for efficient clearing
    pub async fn emergency_clear_all_maintenance(&self) -> u32 {
        let mut active = self.active_maintenance.write().await;
        let count = active.len();

        if count > 0 {
            let now_timestamp = Utc::now().timestamp_millis();
            let mut node_operations = Vec::with_capacity(count);

            // Drain instead of clone then clear for better performance
            for (node_name, maintenance) in active.drain() {
                let duration_hours = (now_timestamp - maintenance.started_at.timestamp_millis()) / (1000 * 3600);
                node_operations.push(format!("{}:{}({}h)", node_name, maintenance.operation_type, duration_hours));
            }

            error!("Emergency cleared {} maintenance windows: {:?}", count, node_operations);
        } else {
            info!("Emergency clear requested but no maintenance windows were active");
        }

        count as u32
    }

    // OPTIMIZED: Single pass maintenance report generation
    pub async fn get_maintenance_report(&self) -> MaintenanceReport {
        let active = self.active_maintenance.read().await;
        let now_timestamp = Utc::now().timestamp_millis();

        let mut active_windows = Vec::with_capacity(active.len());
        let mut overdue_windows = Vec::with_capacity(active.len() / 4);
        let mut long_running_windows = Vec::with_capacity(active.len() / 8);

        for maintenance in active.values() {
            let started_timestamp = maintenance.started_at.timestamp_millis();
            let duration_ms = now_timestamp - started_timestamp;
            let estimated_end_timestamp = started_timestamp + (maintenance.estimated_duration_minutes as i64 * 60 * 1000);
            let is_overdue = now_timestamp > estimated_end_timestamp;
            let is_long_running = duration_ms >= 2 * 3600 * 1000;

            let window_info = MaintenanceWindowInfo {
                node_name: maintenance.node_name.clone(),
                operation_type: maintenance.operation_type.clone(),
                server_host: maintenance.server_host.clone(),
                started_at: maintenance.started_at,
                estimated_duration_minutes: maintenance.estimated_duration_minutes,
                actual_duration_minutes: (duration_ms / (1000 * 60)) as u32,
                actual_duration_hours: (duration_ms / (1000 * 3600)) as u32,
                is_overdue,
                is_long_running,
                estimated_completion: DateTime::from_timestamp_millis(estimated_end_timestamp).unwrap_or(Utc::now()),
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
            total_long_running: long_running_windows.len(),
            active_windows,
            overdue_windows,
            long_running_windows,
            report_generated_at: Utc::now(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MaintenanceStats {
    pub total_active: usize,
    pub overdue_count: usize,
    pub long_running_count: usize,
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
    pub actual_duration_hours: u32,
    pub is_overdue: bool,
    pub is_long_running: bool,
    pub estimated_completion: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct MaintenanceReport {
    pub total_active: usize,
    pub total_overdue: usize,
    pub total_long_running: usize,
    pub active_windows: Vec<MaintenanceWindowInfo>,
    pub overdue_windows: Vec<MaintenanceWindowInfo>,
    pub long_running_windows: Vec<MaintenanceWindowInfo>,
    pub report_generated_at: DateTime<Utc>,
}

impl Clone for MaintenanceTracker {
    fn clone(&self) -> Self {
        Self {
            active_maintenance: self.active_maintenance.clone(),
            _operation_types: self._operation_types.clone(),
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

        let (in_maintenance, window) = tracker.get_maintenance_status_atomic("test-node").await;
        assert!(in_maintenance);
        assert!(window.is_some());

        tracker.end_maintenance("test-node").await.unwrap();
        let (in_maintenance, _) = tracker.get_maintenance_status_atomic("test-node").await;
        assert!(!in_maintenance);
    }

    #[tokio::test]
    async fn test_atomic_maintenance_status() {
        let tracker = MaintenanceTracker::new();

        let (in_maintenance, window) = tracker.get_maintenance_status_atomic("nonexistent").await;
        assert!(!in_maintenance);
        assert!(window.is_none());

        tracker
            .start_maintenance("test-node", "pruning", 300, "test-server")
            .await
            .unwrap();

        let (in_maintenance, window) = tracker.get_maintenance_status_atomic("test-node").await;
        assert!(in_maintenance);
        assert!(window.is_some());
        assert_eq!(window.unwrap().operation_type, "pruning");
    }

    #[tokio::test]
    async fn test_optimized_statistics() {
        let tracker = MaintenanceTracker::new();

        tracker.start_maintenance("node1", "pruning", 300, "server1").await.unwrap();
        tracker.start_maintenance("node2", "hermes_restart", 60, "server1").await.unwrap();
        tracker.start_maintenance("node3", "pruning", 300, "server2").await.unwrap();

        let stats = tracker.get_maintenance_stats().await;
        assert_eq!(stats.total_active, 3);
        assert_eq!(stats.by_operation_type.get("pruning"), Some(&2));
        assert_eq!(stats.by_operation_type.get("hermes_restart"), Some(&1));
        assert_eq!(stats.by_server.get("server1"), Some(&2));
        assert_eq!(stats.by_server.get("server2"), Some(&1));
    }

    #[tokio::test]
    async fn test_cleanup_efficiency() {
        let tracker = MaintenanceTracker::new();

        // Add several maintenance windows
        for i in 0..10 {
            tracker.start_maintenance(
                &format!("node{}", i),
                "pruning",
                300,
                "test-server"
            ).await.unwrap();
        }

        let initial_stats = tracker.get_maintenance_stats().await;
        assert_eq!(initial_stats.total_active, 10);

        // Test emergency clear efficiency
        let cleared = tracker.emergency_clear_all_maintenance().await;
        assert_eq!(cleared, 10);

        let final_stats = tracker.get_maintenance_stats().await;
        assert_eq!(final_stats.total_active, 0);
    }
}
