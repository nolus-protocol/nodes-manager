// File: manager/src/services/health_service.rs
use anyhow::Result;
use std::sync::Arc;
use tracing::info;

use crate::config::Config;
use crate::database::Database;
use crate::health::HealthMonitor;
use crate::maintenance_tracker::MaintenanceTracker;

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

    // Use references instead of owned values where possible
    pub async fn get_all_health(&self, include_disabled: bool) -> Result<Vec<crate::web::NodeHealthSummary>> {
        let health_records = self.health_monitor.check_all_nodes().await?;

        // Pre-allocate with estimated capacity
        let mut summaries = Vec::with_capacity(health_records.len());

        for health in health_records.iter() {
            if include_disabled || self.is_node_enabled(&health.node_name) {
                let summary = self.transform_health_status_to_summary(health).await;
                summaries.push(summary);
            }
        }

        Ok(summaries)
    }

    pub async fn get_node_health(&self, node_name: &str) -> Result<Option<crate::web::NodeHealthSummary>> {
        self.validate_node_name(node_name)?;

        let health = self.health_monitor.get_node_health(node_name).await?;

        match health {
            Some(h) => {
                let summary = self.transform_health_status_to_summary(&h).await;
                Ok(Some(summary))
            }
            None => Ok(None),
        }
    }

    pub async fn get_node_health_history(&self, node_name: &str, limit: i32) -> Result<Vec<crate::web::NodeHealthSummary>> {
        self.validate_node_name(node_name)?;

        let history = self.database.get_health_history(node_name, Some(limit)).await?;

        let mut summaries = Vec::with_capacity(history.len());
        for record in history.iter() {
            // Convert HealthRecord to HealthStatus for transformation
            let health_status = crate::health::monitor::HealthStatus {
                node_name: record.node_name.clone(),
                rpc_url: self.config.nodes.get(&record.node_name)
                    .map(|n| n.rpc_url.clone())
                    .unwrap_or_default(),
                is_healthy: record.is_healthy,
                error_message: record.error_message.clone(),
                last_check: record.timestamp,
                block_height: record.block_height,
                is_syncing: record.is_syncing.map(|s| s != 0),
                is_catching_up: record.is_catching_up.unwrap_or(0) != 0,
                validator_address: record.validator_address.clone(),
                network: self.config.nodes.get(&record.node_name)
                    .map(|n| n.network.clone())
                    .unwrap_or_default(),
                server_host: self.config.nodes.get(&record.node_name)
                    .map(|n| n.server_host.clone())
                    .unwrap_or_default(),
                enabled: self.config.nodes.get(&record.node_name)
                    .map(|n| n.enabled)
                    .unwrap_or(false),
                in_maintenance: false,
            };
            let summary = self.transform_health_status_to_summary(&health_status).await;
            summaries.push(summary);
        }

        Ok(summaries)
    }

    pub async fn force_health_check(&self, node_name: &str) -> Result<crate::web::NodeHealthSummary> {
        self.validate_node_name(node_name)?;

        info!("Forcing health check for node: {}", node_name);

        // Get the node config and perform a direct health check
        let node_config = self.config.nodes.get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        let health_status = self.health_monitor.check_node_health(node_name, node_config).await?;
        let summary = self.transform_health_status_to_summary(&health_status).await;

        Ok(summary)
    }

    // Inline simple validations to reduce function call overhead
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

    // Reduce allocations and use more efficient data access
    async fn transform_health_status_to_summary(&self, health: &crate::health::monitor::HealthStatus) -> crate::web::NodeHealthSummary {
        let node_config = self.config.nodes.get(&health.node_name);
        let server_host = node_config
            .map(|node| &node.server_host)
            .map(|s| s.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Get maintenance info if node is in maintenance
        let maintenance_info = if health.in_maintenance {
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
                    config.snapshot_retention_count.map(|c| c as u32),
                )
            } else {
                (false, false, false, None)
            };

        let status_string = if health.is_healthy {
            "Healthy"
        } else if health.in_maintenance {
            "Maintenance"
        } else {
            "Unhealthy"
        };

        crate::web::NodeHealthSummary {
            node_name: health.node_name.clone(),
            status: status_string.to_string(),
            latest_block_height: health.block_height.map(|h| h as u64),
            catching_up: health.is_syncing,
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

    // Get health statistics for monitoring
    pub async fn get_health_statistics(&self) -> Result<serde_json::Value> {
        let health_records = self.health_monitor.check_all_nodes().await?;

        let mut healthy_count = 0;
        let mut unhealthy_count = 0;
        let mut maintenance_count = 0;

        for health in &health_records {
            if health.is_healthy {
                healthy_count += 1;
            } else if health.in_maintenance {
                maintenance_count += 1;
            } else {
                unhealthy_count += 1;
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
            "unknown_nodes": 0,
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
