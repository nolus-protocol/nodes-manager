// File: src/services/health_service.rs

use anyhow::Result;
use std::sync::Arc;
use tracing::info;

use crate::database::Database;
use crate::health::HealthMonitor;
use crate::maintenance_tracker::MaintenanceTracker;
use crate::{Config, NodeHealth};

pub struct HealthService {
    config: Arc<Config>,
    database: Arc<Database>,
    health_monitor: Arc<HealthMonitor>,
    maintenance_tracker: Arc<MaintenanceTracker>,
}

impl HealthService {
    pub fn new(
        config: Arc<Config>,
        database: Arc<Database>,
        health_monitor: Arc<HealthMonitor>,
        maintenance_tracker: Arc<MaintenanceTracker>,
    ) -> Self {
        Self {
            config,
            database,
            health_monitor,
            maintenance_tracker,
        }
    }

    // OPTIMIZED: Use references instead of owned values where possible
    pub async fn get_all_health(&self, include_disabled: bool) -> Result<Vec<crate::web::NodeHealthSummary>> {
        let health_records = self.health_monitor.get_all_health_status().await?;

        // Pre-allocate with estimated capacity
        let mut summaries = Vec::with_capacity(health_records.len());

        for health in health_records.iter() {
            if include_disabled || self.is_node_enabled(&health.node_name) {
                let summary = self.transform_health_to_summary(health).await;
                summaries.push(summary);
            }
        }

        Ok(summaries)
    }

    pub async fn get_node_health(&self, node_name: &str) -> Result<Option<crate::web::NodeHealthSummary>> {
        self.validate_node_name(node_name)?;

        let health = self.database.get_latest_node_health(node_name).await?;

        match health {
            Some(h) => {
                let summary = self.transform_health_to_summary(&h).await;
                Ok(Some(summary))
            }
            None => Ok(None),
        }
    }

    pub async fn get_node_health_history(&self, node_name: &str, limit: i32) -> Result<Vec<crate::web::NodeHealthSummary>> {
        self.validate_node_name(node_name)?;

        let history = self.health_monitor.get_node_health_history(node_name, limit).await?;

        let mut summaries = Vec::with_capacity(history.len());
        for health in history.iter() {
            let summary = self.transform_health_to_summary(health).await;
            summaries.push(summary);
        }

        Ok(summaries)
    }

    pub async fn force_health_check(&self, node_name: &str) -> Result<crate::web::NodeHealthSummary> {
        self.validate_node_name(node_name)?;

        info!("Forcing health check for node: {}", node_name);

        let health = self.health_monitor.force_health_check(node_name).await?;
        let summary = self.transform_health_to_summary(&health).await;

        Ok(summary)
    }

    // OPTIMIZED: Inline simple validations to reduce function call overhead
    #[inline]
    fn validate_node_name(&self, node_name: &str) -> Result<()> {
        if self.config.nodes.contains_key(node_name) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Node '{}' not found", node_name))
        }
    }

    #[inline]
    fn is_node_enabled(&self, node_name: &str) -> bool {
        self.config.nodes.get(node_name)
            .map(|node| node.enabled)
            .unwrap_or(false)
    }

    // OPTIMIZED: Reduce allocations and use more efficient data access
    async fn transform_health_to_summary(&self, health: &NodeHealth) -> crate::web::NodeHealthSummary {
        let node_config = self.config.nodes.get(&health.node_name);
        let server_host = node_config
            .map(|node| &node.server_host)
            .map(|s| s.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Get maintenance info if node is in maintenance
        let maintenance_info = if matches!(health.status, crate::HealthStatus::Maintenance) {
            self.get_maintenance_info(&health.node_name).await
        } else {
            None
        };

        // Extract configuration data efficiently
        let (snapshot_enabled, auto_restore_enabled, scheduled_snapshots_enabled, snapshot_retention_count) =
            if let Some(config) = node_config {
                (
                    config.snapshots_enabled.unwrap_or(false),
                    config.auto_restore_enabled.unwrap_or(false),
                    config.snapshot_schedule.is_some(),
                    config.snapshot_retention_count,
                )
            } else {
                (false, false, false, None)
            };

        crate::web::NodeHealthSummary {
            node_name: health.node_name.clone(),
            status: format!("{:?}", health.status),
            latest_block_height: health.latest_block_height,
            catching_up: health.catching_up,
            last_check: health.last_check.to_rfc3339(),
            error_message: health.error_message.clone(),
            server_host,
            maintenance_info,
            snapshot_enabled,
            auto_restore_enabled,
            scheduled_snapshots_enabled,
            snapshot_retention_count,
        }
    }

    async fn get_maintenance_info(&self, node_name: &str) -> Option<crate::web::MaintenanceInfo> {
        if let Some(maintenance) = self.maintenance_tracker.get_maintenance_status(node_name).await {
            let elapsed = chrono::Utc::now().signed_duration_since(maintenance.started_at);
            Some(crate::web::MaintenanceInfo {
                operation_type: maintenance.operation_type,
                started_at: maintenance.started_at.to_rfc3339(),
                estimated_duration_minutes: maintenance.estimated_duration_minutes,
                elapsed_minutes: elapsed.num_minutes(),
            })
        } else {
            None
        }
    }

    // NEW: Get health statistics for monitoring
    pub async fn get_health_statistics(&self) -> Result<serde_json::Value> {
        let health_records = self.health_monitor.get_all_health_status().await?;

        let mut healthy_count = 0;
        let mut unhealthy_count = 0;
        let mut maintenance_count = 0;
        let mut unknown_count = 0;

        for health in &health_records {
            match health.status {
                crate::HealthStatus::Healthy => healthy_count += 1,
                crate::HealthStatus::Unhealthy => unhealthy_count += 1,
                crate::HealthStatus::Maintenance => maintenance_count += 1,
                crate::HealthStatus::Unknown => unknown_count += 1,
            }
        }

        let total_nodes = self.config.nodes.len();
        let monitored_nodes = health_records.len();
        let health_percentage = if total_nodes > 0 {
            (healthy_count as f64 / total_nodes as f64 * 100.0) as u32
        } else {
            0
        };

        Ok(serde_json::json!({
            "total_configured_nodes": total_nodes,
            "monitored_nodes": monitored_nodes,
            "healthy_nodes": healthy_count,
            "unhealthy_nodes": unhealthy_count,
            "maintenance_nodes": maintenance_count,
            "unknown_nodes": unknown_count,
            "health_percentage": health_percentage,
            "monitoring_coverage_percentage": if total_nodes > 0 {
                (monitored_nodes as f64 / total_nodes as f64 * 100.0) as u32
            } else {
                0
            }
        }))
    }
}

impl Clone for HealthService {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            database: self.database.clone(),
            health_monitor: self.health_monitor.clone(),
            maintenance_tracker: self.maintenance_tracker.clone(),
        }
    }
}
