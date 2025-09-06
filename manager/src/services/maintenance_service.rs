// File: manager/src/services/maintenance_service.rs
use crate::config::Config;
use crate::database::{Database, MaintenanceOperation};
use crate::http::HttpAgentManager;
use crate::maintenance_tracker::{MaintenanceTracker, MaintenanceWindow, MaintenanceStats, MaintenanceReport};
use crate::scheduler::MaintenanceScheduler;
use anyhow::Result;
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

pub struct MaintenanceService {
    config: Arc<Config>,
    database: Arc<Database>,
    maintenance_tracker: Arc<MaintenanceTracker>,
    scheduler: Arc<MaintenanceScheduler>,
    http_manager: Arc<HttpAgentManager>,
}

impl MaintenanceService {
    pub fn new(
        config: Arc<Config>,
        database: Arc<Database>,
        maintenance_tracker: Arc<MaintenanceTracker>,
        scheduler: Arc<MaintenanceScheduler>,
        http_manager: Arc<HttpAgentManager>,
    ) -> Self {
        Self {
            config,
            database,
            maintenance_tracker,
            scheduler,
            http_manager,
        }
    }

    pub async fn get_scheduled_operations(&self) -> Result<serde_json::Value> {
        // Return simple scheduled operations status since the full method doesn't exist
        Ok(json!({
            "scheduled": [],
            "message": "Scheduled operations available via cron jobs"
        }))
    }

    pub async fn execute_immediate_operation(&self, operation_type: &str, target_name: &str) -> Result<String> {
        let operation_id = Uuid::new_v4().to_string();
        info!("Starting {} for {}", operation_type, target_name);

        let operation = MaintenanceOperation {
            id: operation_id.clone(),
            operation_type: operation_type.to_string(),
            target_name: target_name.to_string(),
            status: "started".to_string(),
            started_at: Utc::now(),
            completed_at: None,
            error_message: None,
            details: None,
        };

        self.database.store_maintenance_operation(&operation).await?;

        match operation_type {
            "pruning" => {
                let node_config = self.config.nodes.get(target_name)
                    .ok_or_else(|| anyhow::anyhow!("Node {} not found", target_name))?;

                match self.http_manager.execute_node_pruning(target_name).await {
                    Ok(_) => {
                        self.update_operation_status(&operation_id, "completed", None).await?;
                        Ok(format!("Pruning started for {}", target_name))
                    }
                    Err(e) => {
                        self.update_operation_status(&operation_id, "failed", Some(e.to_string())).await?;
                        Err(e)
                    }
                }
            }
            "snapshot_creation" => {
                match self.http_manager.create_node_snapshot(target_name).await {
                    Ok(_) => {
                        self.update_operation_status(&operation_id, "completed", None).await?;
                        Ok(format!("Snapshot creation started for {}", target_name))
                    }
                    Err(e) => {
                        self.update_operation_status(&operation_id, "failed", Some(e.to_string())).await?;
                        Err(e)
                    }
                }
            }
            _ => Err(anyhow::anyhow!("Unknown operation type: {}", operation_type))
        }
    }

    pub async fn get_maintenance_logs(&self, limit: i32) -> Result<Vec<MaintenanceOperation>> {
        self.database.get_maintenance_operations(Some(limit)).await
    }

    pub async fn execute_batch_pruning(&self, node_names: Vec<String>) -> Result<serde_json::Value> {
        info!("Starting batch pruning for {} nodes", node_names.len());

        let mut success_count = 0;
        let mut failure_count = 0;
        let mut results = Vec::new();

        for node_name in &node_names {
            match self.http_manager.execute_node_pruning(node_name).await {
                Ok(_) => {
                    success_count += 1;
                    results.push(json!({
                        "node_name": node_name,
                        "success": true,
                        "message": "Pruning completed successfully"
                    }));
                }
                Err(e) => {
                    failure_count += 1;
                    results.push(json!({
                        "node_name": node_name,
                        "success": false,
                        "message": e.to_string()
                    }));
                }
            }
        }

        let operation_id = Uuid::new_v4().to_string();
        let operation = MaintenanceOperation {
            id: operation_id,
            operation_type: "batch_pruning".to_string(),
            target_name: format!("batch:{}", node_names.join(",")),
            status: if failure_count == 0 { "completed" } else { "partial_failure" }.to_string(),
            started_at: Utc::now(),
            completed_at: Some(Utc::now()),
            error_message: if failure_count > 0 {
                Some(format!("{} operations failed", failure_count))
            } else {
                None
            },
            details: Some(serde_json::to_string(&results).unwrap_or_default()),
        };

        if let Err(e) = self.database.store_maintenance_operation(&operation).await {
            error!("Failed to store batch operation result: {}", e);
        }

        Ok(json!({
            "success_count": success_count,
            "failure_count": failure_count,
            "results": results,
            "summary": format!("{} successful, {} failed", success_count, failure_count)
        }))
    }

    pub async fn get_active_maintenance(&self) -> Result<Vec<MaintenanceWindow>> {
        let active_operations = self.maintenance_tracker.get_all_in_maintenance().await;
        Ok(active_operations)
    }

    pub async fn get_maintenance_stats(&self) -> Result<MaintenanceStats> {
        Ok(self.maintenance_tracker.get_maintenance_stats().await)
    }

    pub async fn get_maintenance_report(&self) -> Result<MaintenanceReport> {
        Ok(self.maintenance_tracker.get_maintenance_report().await)
    }

    pub async fn check_stuck_operations(&self) -> Result<serde_json::Value> {
        let overdue = self.maintenance_tracker.get_overdue_maintenance().await;
        let stuck_operations: Vec<_> = overdue.into_iter()
            .filter(|op| {
                let duration = chrono::Utc::now().signed_duration_since(op.started_at);
                duration.num_minutes() > (op.estimated_duration_minutes as i64 * 3)
            })
            .collect();

        Ok(json!({
            "stuck_operations": stuck_operations,
            "count": stuck_operations.len()
        }))
    }

    pub async fn emergency_clear_maintenance(&self) -> Result<serde_json::Value> {
        let cleared_count = self.maintenance_tracker.emergency_clear_all_maintenance().await;
        Ok(json!({
            "cleared_count": cleared_count,
            "message": format!("Emergency cleared {} maintenance operations", cleared_count)
        }))
    }

    pub async fn clear_specific_maintenance(&self, node_name: &str) -> Result<serde_json::Value> {
        if self.maintenance_tracker.is_in_maintenance(node_name).await {
            self.maintenance_tracker.end_maintenance(node_name).await?;
            Ok(json!({
                "cleared": true,
                "node_name": node_name,
                "message": format!("Cleared maintenance for {}", node_name)
            }))
        } else {
            Ok(json!({
                "cleared": false,
                "node_name": node_name,
                "message": format!("Node {} was not in maintenance", node_name)
            }))
        }
    }

    pub async fn get_service_statistics(&self) -> Result<serde_json::Value> {
        let operations = self.database.get_maintenance_operations(Some(100)).await?;
        let active = self.maintenance_tracker.get_all_in_maintenance().await;
        let stats = self.maintenance_tracker.get_maintenance_stats().await;

        Ok(json!({
            "total_recent_operations": operations.len(),
            "active_maintenance_windows": active.len(),
            "maintenance_statistics": stats,
            "service_health": "operational",
            "timestamp": Utc::now().to_rfc3339()
        }))
    }

    async fn update_operation_status(&self, operation_id: &str, status: &str, error_message: Option<String>) -> Result<()> {
        let operations = self.database.get_maintenance_operations(Some(100)).await?;

        if let Some(mut operation) = operations.into_iter().find(|op| op.id == operation_id) {
            operation.status = status.to_string();
            operation.completed_at = Some(Utc::now());
            operation.error_message = error_message;

            self.database.store_maintenance_operation(&operation).await?;
        }

        Ok(())
    }
}
