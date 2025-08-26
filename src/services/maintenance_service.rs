// File: src/services/maintenance_service.rs

use anyhow::Result;
use chrono::Utc;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::info;

use crate::database::Database;
use crate::maintenance_tracker::MaintenanceTracker;
use crate::scheduler::MaintenanceScheduler;
use crate::ssh::SshManager;
use crate::{Config, MaintenanceOperation};

pub struct MaintenanceService {
    config: Arc<Config>,
    database: Arc<Database>,
    maintenance_tracker: Arc<MaintenanceTracker>,
    scheduler: Arc<MaintenanceScheduler>,
    ssh_manager: Arc<SshManager>,
}

impl MaintenanceService {
    pub fn new(
        config: Arc<Config>,
        database: Arc<Database>,
        maintenance_tracker: Arc<MaintenanceTracker>,
        scheduler: Arc<MaintenanceScheduler>,
        ssh_manager: Arc<SshManager>,
    ) -> Self {
        Self {
            config,
            database,
            maintenance_tracker,
            scheduler,
            ssh_manager,
        }
    }

    // OPTIMIZED: Batch operation result processing
    pub async fn execute_batch_pruning(&self, node_names: Vec<String>) -> Result<Value> {
        // Validate all node names upfront
        for node_name in &node_names {
            if !self.config.nodes.contains_key(node_name) {
                return Err(anyhow::anyhow!("Node '{}' not found", node_name));
            }
        }

        info!("Starting batch pruning for {} nodes with enhanced monitoring", node_names.len());

        let result = self.scheduler.execute_batch_pruning(node_names).await?;

        Ok(json!({
            "operation_id": result.operation_id,
            "total_operations": result.total_operations,
            "successful": result.successful,
            "failed": result.failed,
            "results": result.results,
            "monitoring_enabled": true
        }))
    }

    pub async fn execute_batch_hermes_restart(&self, hermes_names: Vec<String>) -> Result<Value> {
        // Validate all hermes names upfront
        for hermes_name in &hermes_names {
            if !self.config.hermes.contains_key(hermes_name) {
                return Err(anyhow::anyhow!("Hermes instance '{}' not found", hermes_name));
            }
        }

        info!("Starting batch Hermes restart for {} instances", hermes_names.len());

        let result = self.scheduler.execute_batch_hermes_restart(hermes_names).await?;

        Ok(json!({
            "operation_id": result.operation_id,
            "total_operations": result.total_operations,
            "successful": result.successful,
            "failed": result.failed,
            "results": result.results
        }))
    }

    pub async fn get_scheduled_operations(&self) -> Result<Value> {
        let operations = self.scheduler.get_scheduled_operations().await;
        let running_operations = self.scheduler.get_running_operations().await;

        Ok(json!({
            "scheduled": operations,
            "running": running_operations,
            "total": operations.len() + running_operations.len()
        }))
    }

    pub async fn execute_immediate_operation(&self, operation_type: &str, target_name: &str) -> Result<String> {
        let message = match operation_type.to_lowercase().as_str() {
            "pruning" => {
                self.validate_node_name(target_name)?;
                self.scheduler.execute_immediate_pruning(target_name).await?;
                format!("Immediate pruning completed for node {}", target_name)
            }
            "hermes_restart" => {
                self.validate_hermes_name(target_name)?;
                self.scheduler.execute_immediate_hermes_restart(target_name).await?;
                format!("Immediate Hermes restart completed for {}", target_name)
            }
            "snapshot_creation" => {
                self.validate_node_name(target_name)?;
                self.scheduler.execute_immediate_snapshot_creation(target_name).await?;
                format!("Immediate LZ4 snapshot creation completed for node {}", target_name)
            }
            _ => return Err(anyhow::anyhow!("Invalid operation type: {}", operation_type)),
        };

        Ok(message)
    }

    pub async fn get_maintenance_logs(&self, limit: i32) -> Result<Vec<MaintenanceOperation>> {
        let validated_limit = if limit > 0 && limit <= 1000 { limit } else { 50 };
        self.scheduler.get_maintenance_logs(validated_limit).await
    }

    pub async fn get_active_maintenance(&self) -> Result<Vec<crate::maintenance_tracker::MaintenanceWindow>> {
        Ok(self.maintenance_tracker.get_all_in_maintenance().await)
    }

    pub async fn get_maintenance_stats(&self) -> Result<crate::maintenance_tracker::MaintenanceStats> {
        Ok(self.maintenance_tracker.get_maintenance_stats().await)
    }

    pub async fn get_maintenance_report(&self) -> Result<crate::maintenance_tracker::MaintenanceReport> {
        Ok(self.maintenance_tracker.get_maintenance_report().await)
    }

    pub async fn get_overdue_maintenance(&self) -> Result<Vec<crate::maintenance_tracker::MaintenanceWindow>> {
        Ok(self.maintenance_tracker.get_overdue_maintenance().await)
    }

    pub async fn cleanup_overdue_maintenance(&self, overdue_factor: f64) -> Result<Value> {
        info!("Cleaning up maintenance operations that are {}x longer than estimated", overdue_factor);

        let cleaned_count = self.maintenance_tracker.cleanup_overdue_maintenance(overdue_factor).await;

        Ok(json!({
            "cleaned_count": cleaned_count,
            "overdue_factor": overdue_factor,
            "timestamp": Utc::now().to_rfc3339(),
            "action": "cleanup_overdue"
        }))
    }

    // OPTIMIZED: Extracted and optimized stuck operation checking
    pub async fn check_stuck_operations(&self) -> Result<Value> {
        info!("Checking for stuck operations with enhanced process monitoring");

        let active_maintenance = self.maintenance_tracker.get_all_in_maintenance().await;
        let total_active = active_maintenance.len();

        let mut stuck_operations = Vec::with_capacity(total_active / 4); // Estimate capacity
        let mut monitoring_results = Vec::with_capacity(total_active);

        for maintenance in active_maintenance {
            let result = self.analyze_maintenance_operation(&maintenance).await;

            monitoring_results.push(result.monitoring_info);
            if let Some(stuck_info) = result.stuck_info {
                stuck_operations.push(stuck_info);
            }
        }

        Ok(json!({
            "total_active_maintenance": total_active,
            "stuck_operations": stuck_operations,
            "stuck_count": stuck_operations.len(),
            "monitoring_results": monitoring_results,
            "monitoring_count": monitoring_results.len(),
            "checked_at": Utc::now().to_rfc3339(),
            "monitoring_features": {
                "process_health_checks": true,
                "periodic_monitoring": true,
                "enhanced_error_detection": true,
                "lz4_compression_support": true
            }
        }))
    }

    // OPTIMIZED: Emergency cleanup with batch operations
    pub async fn emergency_kill_stuck_processes(&self) -> Result<Value> {
        info!("Emergency killing stuck pruning processes");

        let active_maintenance = self.maintenance_tracker.get_all_in_maintenance().await;
        let mut killed_processes = Vec::with_capacity(active_maintenance.len());

        for maintenance in active_maintenance {
            if maintenance.operation_type == "pruning" {
                if let Some(process_name) = self.kill_pruning_process(&maintenance).await {
                    killed_processes.push(process_name);
                }
            }
        }

        Ok(json!({
            "killed_processes": killed_processes,
            "killed_count": killed_processes.len(),
            "timestamp": Utc::now().to_rfc3339(),
            "action": "emergency_kill_processes"
        }))
    }

    pub async fn emergency_clear_maintenance(&self) -> Result<Value> {
        info!("Emergency clearing all maintenance windows");

        let cleared_count = self.maintenance_tracker.emergency_clear_all_maintenance().await;

        Ok(json!({
            "cleared_count": cleared_count,
            "timestamp": Utc::now().to_rfc3339(),
            "action": "emergency_clear_all"
        }))
    }

    pub async fn clear_specific_maintenance(&self, node_name: &str) -> Result<Value> {
        self.validate_node_name(node_name)?;

        info!("Clearing maintenance for specific node: {}", node_name);

        if !self.maintenance_tracker.is_in_maintenance(node_name).await {
            return Err(anyhow::anyhow!(
                "Node {} is not currently in maintenance",
                node_name
            ));
        }

        self.maintenance_tracker.end_maintenance(node_name).await?;

        Ok(json!({
            "node_name": node_name,
            "action": "cleared_maintenance",
            "timestamp": Utc::now().to_rfc3339()
        }))
    }

    pub async fn get_operations_summary(&self) -> Result<Value> {
        let summary = self.scheduler.get_operations_summary().await;
        Ok(summary)
    }

    pub async fn get_operation_status(&self, operation_id: &str) -> Result<Value> {
        let status = self.scheduler.get_operation_status(operation_id).await;

        Ok(json!({
            "operation_id": operation_id,
            "status": status.map(|s| format!("{:?}", s)).unwrap_or_else(|| "NotFound".to_string()),
            "timestamp": Utc::now().to_rfc3339()
        }))
    }

    // OPTIMIZED: Private helper methods with reduced allocations
    #[inline]
    fn validate_node_name(&self, node_name: &str) -> Result<()> {
        if self.config.nodes.contains_key(node_name) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Node '{}' not found", node_name))
        }
    }

    #[inline]
    fn validate_hermes_name(&self, hermes_name: &str) -> Result<()> {
        if self.config.hermes.contains_key(hermes_name) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Hermes instance '{}' not found", hermes_name))
        }
    }

    async fn analyze_maintenance_operation(&self, maintenance: &crate::maintenance_tracker::MaintenanceWindow) -> MaintenanceAnalysisResult {
        let duration = Utc::now().signed_duration_since(maintenance.started_at);
        let is_overdue = duration.num_minutes() > maintenance.estimated_duration_minutes as i64;

        if maintenance.operation_type == "pruning" {
            return self.analyze_pruning_operation(maintenance, duration, is_overdue).await;
        }

        // For non-pruning operations, just check if they're overdue
        let monitoring_info = json!({
            "node_name": maintenance.node_name,
            "operation_type": maintenance.operation_type,
            "server_host": maintenance.server_host,
            "started_at": maintenance.started_at.to_rfc3339(),
            "duration_minutes": duration.num_minutes(),
            "estimated_duration_minutes": maintenance.estimated_duration_minutes,
            "process_running": "not_applicable",
            "is_overdue": is_overdue
        });

        let stuck_info = if is_overdue {
            Some(json!({
                "node_name": maintenance.node_name,
                "operation_type": maintenance.operation_type,
                "server_host": maintenance.server_host,
                "started_at": maintenance.started_at.to_rfc3339(),
                "duration_minutes": duration.num_minutes(),
                "estimated_duration_minutes": maintenance.estimated_duration_minutes,
                "status": "overdue_operation"
            }))
        } else {
            None
        };

        MaintenanceAnalysisResult {
            monitoring_info,
            stuck_info,
        }
    }

    async fn analyze_pruning_operation(
        &self,
        maintenance: &crate::maintenance_tracker::MaintenanceWindow,
        duration: chrono::Duration,
        is_overdue: bool,
    ) -> MaintenanceAnalysisResult {
        if let Some(node_config) = self.config.nodes.get(&maintenance.node_name) {
            if let Some(deploy_path) = &node_config.pruning_deploy_path {
                return self.check_pruning_process_status(maintenance, deploy_path, duration, is_overdue).await;
            }
        }

        // Fallback if we can't find config
        MaintenanceAnalysisResult {
            monitoring_info: json!({
                "node_name": maintenance.node_name,
                "operation_type": maintenance.operation_type,
                "server_host": maintenance.server_host,
                "started_at": maintenance.started_at.to_rfc3339(),
                "duration_minutes": duration.num_minutes(),
                "estimated_duration_minutes": maintenance.estimated_duration_minutes,
                "process_running": "config_not_found",
                "is_overdue": is_overdue
            }),
            stuck_info: None,
        }
    }

    async fn check_pruning_process_status(
        &self,
        maintenance: &crate::maintenance_tracker::MaintenanceWindow,
        deploy_path: &str,
        duration: chrono::Duration,
        is_overdue: bool,
    ) -> MaintenanceAnalysisResult {
        match self.ssh_manager.check_pruning_process_status(&maintenance.server_host, deploy_path).await {
            Ok(is_running) => {
                let monitoring_info = json!({
                    "node_name": maintenance.node_name,
                    "operation_type": maintenance.operation_type,
                    "server_host": maintenance.server_host,
                    "started_at": maintenance.started_at.to_rfc3339(),
                    "duration_minutes": duration.num_minutes(),
                    "estimated_duration_minutes": maintenance.estimated_duration_minutes,
                    "process_running": is_running,
                    "is_overdue": is_overdue,
                    "deploy_path": deploy_path
                });

                let stuck_info = if !is_running {
                    Some(json!({
                        "node_name": maintenance.node_name,
                        "operation_type": maintenance.operation_type,
                        "server_host": maintenance.server_host,
                        "started_at": maintenance.started_at.to_rfc3339(),
                        "duration_minutes": duration.num_minutes(),
                        "estimated_duration_minutes": maintenance.estimated_duration_minutes,
                        "status": "stuck_no_process",
                        "deploy_path": deploy_path,
                        "monitoring_details": monitoring_info
                    }))
                } else {
                    None
                };

                MaintenanceAnalysisResult {
                    monitoring_info,
                    stuck_info,
                }
            }
            Err(e) => {
                MaintenanceAnalysisResult {
                    monitoring_info: json!({
                        "node_name": maintenance.node_name,
                        "operation_type": maintenance.operation_type,
                        "server_host": maintenance.server_host,
                        "started_at": maintenance.started_at.to_rfc3339(),
                        "duration_minutes": duration.num_minutes(),
                        "estimated_duration_minutes": maintenance.estimated_duration_minutes,
                        "process_running": "unknown",
                        "check_error": e.to_string(),
                        "deploy_path": deploy_path
                    }),
                    stuck_info: None,
                }
            }
        }
    }

    async fn kill_pruning_process(&self, maintenance: &crate::maintenance_tracker::MaintenanceWindow) -> Option<String> {
        if let Some(node_config) = self.config.nodes.get(&maintenance.node_name) {
            if let Some(deploy_path) = &node_config.pruning_deploy_path {
                match self.ssh_manager.kill_stuck_pruning_process(&maintenance.server_host, deploy_path).await {
                    Ok(_) => Some(maintenance.node_name.clone()),
                    Err(e) => {
                        info!("Could not kill process for {}: {}", maintenance.node_name, e);
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    // NEW: Get maintenance service statistics for monitoring
    pub async fn get_service_statistics(&self) -> Result<Value> {
        let maintenance_stats = self.maintenance_tracker.get_maintenance_stats().await;
        let scheduled_ops = self.scheduler.get_scheduled_operations().await;
        let running_ops = self.scheduler.get_running_operations().await;

        Ok(json!({
            "active_maintenance": maintenance_stats.total_active,
            "overdue_maintenance": maintenance_stats.overdue_count,
            "long_running_maintenance": maintenance_stats.long_running_count,
            "scheduled_operations": scheduled_ops.len(),
            "running_operations": running_ops.len(),
            "operations_by_type": maintenance_stats.by_operation_type,
            "operations_by_server": maintenance_stats.by_server,
            "timestamp": Utc::now().to_rfc3339()
        }))
    }

    // Future API methods - marked as allowed dead code for now
    #[allow(dead_code)]
    pub fn validate_operation_type(&self, operation_type: &str) -> Result<()> {
        match operation_type.to_lowercase().as_str() {
            "pruning" | "hermes_restart" | "system_maintenance" | "snapshot_creation" | "snapshot_restore" => Ok(()),
            _ => Err(anyhow::anyhow!(
                "Invalid operation type: {}. Valid types: pruning, hermes_restart, system_maintenance, snapshot_creation, snapshot_restore",
                operation_type
            )),
        }
    }

    #[allow(dead_code)]
    pub async fn get_maintenance_for_server(&self, server_host: &str) -> Result<Vec<crate::maintenance_tracker::MaintenanceWindow>> {
        let all_maintenance = self.maintenance_tracker.get_all_in_maintenance().await;
        let server_maintenance = all_maintenance.into_iter()
            .filter(|m| m.server_host == server_host)
            .collect();
        Ok(server_maintenance)
    }
}

struct MaintenanceAnalysisResult {
    monitoring_info: Value,
    stuck_info: Option<Value>,
}

impl Clone for MaintenanceService {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            database: self.database.clone(),
            maintenance_tracker: self.maintenance_tracker.clone(),
            scheduler: self.scheduler.clone(),
            ssh_manager: self.ssh_manager.clone(),
        }
    }
}
