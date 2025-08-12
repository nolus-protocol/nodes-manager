// File: src/scheduler/operations.rs

use anyhow::Result;
use chrono::{Datelike, Timelike, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio_cron_scheduler::JobScheduler;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::database::Database;
use crate::scheduler::{
    create_operation_summary, validate_cron_expression, OperationResult, OperationStatus,
    OperationType, ScheduledOperation, SchedulerConfig,
};
use crate::ssh::{manager::SshManager, operations::BatchOperationResult};
use crate::{Config, MaintenanceOperation};

pub struct MaintenanceScheduler {
    database: Arc<Database>,
    ssh_manager: Arc<SshManager>,
    config: Arc<Config>,
    scheduler: Arc<Mutex<Option<JobScheduler>>>,
    scheduled_operations: Arc<RwLock<HashMap<String, ScheduledOperation>>>,
    running_operations: Arc<RwLock<HashMap<String, MaintenanceOperation>>>,
    scheduler_config: SchedulerConfig,
}

impl MaintenanceScheduler {
    pub fn new(
        database: Arc<Database>,
        ssh_manager: Arc<SshManager>,
        config: Arc<Config>,
    ) -> Self {
        Self {
            database,
            ssh_manager,
            config,
            scheduler: Arc::new(Mutex::new(None)),
            scheduled_operations: Arc::new(RwLock::new(HashMap::new())),
            running_operations: Arc::new(RwLock::new(HashMap::new())),
            scheduler_config: SchedulerConfig::default(),
        }
    }

    pub async fn start_scheduler(&self) -> Result<()> {
        info!("Starting maintenance scheduler");

        let job_scheduler = JobScheduler::new().await?;

        // Load existing scheduled operations from configuration
        self.load_operations_from_config().await?;

        // Register all scheduled operations with the simple time-based scheduler
        self.register_scheduled_jobs(&job_scheduler).await?;

        // Start the job scheduler (even though we're not using it for now)
        job_scheduler.start().await?;

        // Store the scheduler
        {
            let mut scheduler = self.scheduler.lock().await;
            *scheduler = Some(job_scheduler);
        }

        // Start cleanup task
        self.start_cleanup_task().await;

        info!("Maintenance scheduler started successfully with automatic scheduling enabled");
        Ok(())
    }

    async fn load_operations_from_config(&self) -> Result<()> {
        let mut operations = HashMap::new();

        // Load node pruning schedules
        for (node_name, node_config) in &self.config.nodes {
            if let Some(schedule) = &node_config.pruning_schedule {
                if node_config.pruning_enabled.unwrap_or(false) {
                    let operation = ScheduledOperation::new_pruning(
                        node_name.clone(),
                        schedule.clone(),
                    );
                    operations.insert(operation.id.clone(), operation);
                }
            }
        }

        // Load hermes restart schedules
        for (hermes_name, hermes_config) in &self.config.hermes {
            let operation = ScheduledOperation::new_hermes_restart(
                hermes_name.clone(),
                hermes_config.restart_schedule.clone(),
            );
            operations.insert(operation.id.clone(), operation);
        }

        let operation_count = operations.len();
        {
            let mut scheduled_ops = self.scheduled_operations.write().await;
            *scheduled_ops = operations;
        }

        info!("Loaded {} scheduled operations from configuration", operation_count);
        Ok(())
    }

    async fn register_scheduled_jobs(&self, _job_scheduler: &JobScheduler) -> Result<()> {
        let scheduled_ops = self.scheduled_operations.read().await;
        info!("Starting simple time-based scheduler for {} operations", scheduled_ops.len());

        // Start a simple background scheduler
        self.start_simple_scheduler().await;

        Ok(())
    }

    async fn start_simple_scheduler(&self) {
        let scheduler = self.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60)); // Check every minute

            loop {
                interval.tick().await;

                let now = Utc::now();
                let scheduled_ops = scheduler.scheduled_operations.read().await;

                for (operation_id, operation) in scheduled_ops.iter() {
                    if !operation.enabled {
                        continue;
                    }

                    // Check if this operation should run now
                    if scheduler.should_run_operation(operation, &now) {
                        info!("Triggering scheduled operation: {} ({})", operation_id, operation.target_name);

                        let op_id = operation_id.clone();
                        let op_type = operation.operation_type.clone();
                        let target = operation.target_name.clone();
                        let sched = scheduler.clone();

                        // Execute the operation in a separate task
                        tokio::spawn(async move {
                            if let Err(e) = sched.execute_scheduled_operation(&op_id, &op_type, &target).await {
                                error!("Scheduled operation {} failed: {}", op_id, e);
                            }
                        });
                    }
                }

                drop(scheduled_ops); // Release the lock
            }
        });

        info!("Simple scheduler background task started");
    }

    fn should_run_operation(&self, operation: &ScheduledOperation, now: &chrono::DateTime<Utc>) -> bool {
        // Simple cron parser for basic patterns like "0 0 12 * * 2"
        // Format: SEC MIN HOUR DAY MONTH WEEKDAY

        let parts: Vec<&str> = operation.schedule.split_whitespace().collect();
        if parts.len() != 6 {
            warn!("Invalid schedule format for operation {}: {}", operation.id, operation.schedule);
            return false;
        }

        // Parse schedule parts
        let sec = parts[0];
        let min = parts[1];
        let hour = parts[2];
        let _day = parts[3];   // Currently not used (always *)
        let _month = parts[4]; // Currently not used (always *)
        let weekday = parts[5];

        // Check if current time matches the schedule
        if !self.matches_time_component(sec, now.second() as i32) {
            return false;
        }

        if !self.matches_time_component(min, now.minute() as i32) {
            return false;
        }

        if !self.matches_time_component(hour, now.hour() as i32) {
            return false;
        }

        if !self.matches_weekday_component(weekday, now.weekday().number_from_monday() as i32) {
            return false;
        }

        // Additional check: don't run the same operation multiple times in the same minute
        if let Some(last_run) = operation.last_run {
            let time_since_last = now.signed_duration_since(last_run);
            if time_since_last.num_minutes() < 1 {
                return false;
            }
        }

        true
    }

    fn matches_time_component(&self, pattern: &str, current_value: i32) -> bool {
        if pattern == "*" {
            return true;
        }

        // Handle simple numeric values
        if let Ok(target_value) = pattern.parse::<i32>() {
            return current_value == target_value;
        }

        // Handle ranges like "1-5" (not implemented yet, but could be added)
        // Handle lists like "1,3,5" (not implemented yet, but could be added)

        false
    }

    fn matches_weekday_component(&self, pattern: &str, current_weekday: i32) -> bool {
        if pattern == "*" {
            return true;
        }

        // Convert current weekday (1=Monday, 7=Sunday) to cron format (0=Sunday, 6=Saturday)
        let cron_weekday = if current_weekday == 7 { 0 } else { current_weekday };

        if let Ok(target_weekday) = pattern.parse::<i32>() {
            return cron_weekday == target_weekday;
        }

        false
    }

    async fn execute_scheduled_operation(
        &self,
        operation_id: &str,
        operation_type: &OperationType,
        target_name: &str,
    ) -> Result<()> {
        info!(
            "Executing scheduled operation {}: {:?} for {}",
            operation_id, operation_type, target_name
        );

        let maintenance_op = MaintenanceOperation {
            id: Uuid::new_v4().to_string(),
            operation_type: format!("{:?}", operation_type),
            target_name: target_name.to_string(),
            status: "running".to_string(),
            started_at: Some(Utc::now()),
            completed_at: None,
            error_message: None,
        };

        // Track the running operation
        {
            let mut running_ops = self.running_operations.write().await;
            running_ops.insert(maintenance_op.id.clone(), maintenance_op.clone());
        }

        // Save to database
        self.database.save_maintenance_operation(&maintenance_op).await?;

        let start_time = Utc::now();
        let result = match operation_type {
            OperationType::NodePruning => {
                self.execute_node_pruning(target_name).await
            }
            OperationType::HermesRestart => {
                self.execute_hermes_restart(target_name).await
            }
            OperationType::SystemMaintenance => {
                self.execute_system_maintenance(target_name).await
            }
        };

        let end_time = Utc::now();
        let duration = end_time.signed_duration_since(start_time);

        // Create operation result and handle the result
        let (operation_result, final_status, error_msg) = match result {
            Ok(_) => (
                OperationResult {
                    success: true,
                    message: "Operation completed successfully".to_string(),
                    duration_seconds: duration.num_seconds() as u64,
                    executed_at: end_time,
                },
                "completed".to_string(),
                None,
            ),
            Err(e) => (
                OperationResult {
                    success: false,
                    message: e.to_string(),
                    duration_seconds: duration.num_seconds() as u64,
                    executed_at: end_time,
                },
                "failed".to_string(),
                Some(e.to_string()),
            ),
        };

        // Update scheduled operation with execution time and result
        {
            let mut scheduled_ops = self.scheduled_operations.write().await;
            if let Some(operation) = scheduled_ops.get_mut(operation_id) {
                operation.last_run = Some(end_time);
                operation.update_result(operation_result.clone());
            }
        }

        // Update maintenance operation in database
        let mut updated_maintenance_op = maintenance_op;
        updated_maintenance_op.completed_at = Some(end_time);
        updated_maintenance_op.status = final_status;
        updated_maintenance_op.error_message = error_msg;

        self.database.update_maintenance_operation(&updated_maintenance_op).await?;

        // Remove from running operations
        {
            let mut running_ops = self.running_operations.write().await;
            running_ops.remove(&updated_maintenance_op.id);
        }

        if operation_result.success {
            info!(
                "Scheduled operation {} completed successfully in {}s",
                operation_id,
                duration.num_seconds()
            );
        } else {
            error!(
                "Scheduled operation {} failed after {}s: {}",
                operation_id,
                duration.num_seconds(),
                operation_result.message
            );
        }

        Ok(())
    }

    async fn execute_node_pruning(&self, node_name: &str) -> Result<()> {
        let node_config = self
            .config
            .nodes
            .get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?
            .clone();

        if !node_config.pruning_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Pruning not enabled for node {}", node_name));
        }

        info!("Starting pruning for node: {}", node_name);
        self.ssh_manager.run_pruning(&node_config).await?;
        info!("Pruning completed for node: {}", node_name);

        Ok(())
    }

    async fn execute_hermes_restart(&self, hermes_name: &str) -> Result<()> {
        let hermes_config = self
            .config
            .hermes
            .get(hermes_name)
            .ok_or_else(|| anyhow::anyhow!("Hermes {} not found", hermes_name))?
            .clone();

        info!("Starting Hermes restart: {}", hermes_name);
        self.ssh_manager.restart_hermes(&hermes_config).await?;
        info!("Hermes restart completed: {}", hermes_name);

        Ok(())
    }

    async fn execute_system_maintenance(&self, _target_name: &str) -> Result<()> {
        // Placeholder for system maintenance operations
        info!("System maintenance operation executed");
        Ok(())
    }

    pub async fn schedule_pruning(&self, node_name: &str, schedule: &str) -> Result<String> {
        validate_cron_expression(schedule)?;

        let operation = ScheduledOperation::new_pruning(
            node_name.to_string(),
            schedule.to_string(),
        );

        let operation_id = operation.id.clone();

        {
            let mut scheduled_ops = self.scheduled_operations.write().await;
            scheduled_ops.insert(operation_id.clone(), operation);
        }

        info!("Scheduled pruning for node {} with schedule: {}", node_name, schedule);
        Ok(operation_id)
    }

    pub async fn schedule_hermes_restart(&self, hermes_name: &str, schedule: &str) -> Result<String> {
        validate_cron_expression(schedule)?;

        let operation = ScheduledOperation::new_hermes_restart(
            hermes_name.to_string(),
            schedule.to_string(),
        );

        let operation_id = operation.id.clone();

        {
            let mut scheduled_ops = self.scheduled_operations.write().await;
            scheduled_ops.insert(operation_id.clone(), operation);
        }

        info!("Scheduled Hermes restart for {} with schedule: {}", hermes_name, schedule);
        Ok(operation_id)
    }

    pub async fn execute_immediate_pruning(&self, node_name: &str) -> Result<()> {
        info!("Executing immediate pruning for node: {}", node_name);
        self.execute_node_pruning(node_name).await
    }

    pub async fn execute_immediate_hermes_restart(&self, hermes_name: &str) -> Result<()> {
        info!("Executing immediate Hermes restart: {}", hermes_name);
        self.execute_hermes_restart(hermes_name).await
    }

    pub async fn execute_batch_pruning(&self, node_names: Vec<String>) -> Result<BatchOperationResult> {
        info!("Executing batch pruning for {} nodes", node_names.len());

        let mut nodes = Vec::new();
        for node_name in &node_names {
            if let Some(node) = self.config.nodes.get(node_name) {
                if node.pruning_enabled.unwrap_or(false) {
                    nodes.push(node.clone());
                }
            }
        }

        let result = self.ssh_manager.prune_multiple_nodes(nodes).await;

        info!(
            "Batch pruning completed: {}/{} successful",
            result.successful,
            result.total_operations
        );

        Ok(result)
    }

    pub async fn execute_batch_hermes_restart(&self, hermes_names: Vec<String>) -> Result<BatchOperationResult> {
        info!("Executing batch Hermes restart for {} instances", hermes_names.len());

        let mut hermes_instances = Vec::new();
        for hermes_name in &hermes_names {
            if let Some(hermes) = self.config.hermes.get(hermes_name) {
                hermes_instances.push(hermes.clone());
            }
        }

        let result = self.ssh_manager.restart_multiple_hermes(hermes_instances).await;

        info!(
            "Batch Hermes restart completed: {}/{} successful",
            result.successful,
            result.total_operations
        );

        Ok(result)
    }

    pub async fn cancel_scheduled_operation(&self, operation_id: &str) -> Result<()> {
        let mut scheduled_ops = self.scheduled_operations.write().await;

        if let Some(operation) = scheduled_ops.get_mut(operation_id) {
            operation.enabled = false;
            info!("Cancelled scheduled operation: {}", operation_id);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Operation {} not found", operation_id))
        }
    }

    pub async fn get_scheduled_operations(&self) -> Vec<ScheduledOperation> {
        let scheduled_ops = self.scheduled_operations.read().await;
        scheduled_ops.values().cloned().collect()
    }

    pub async fn get_running_operations(&self) -> Vec<MaintenanceOperation> {
        let running_ops = self.running_operations.read().await;
        running_ops.values().cloned().collect()
    }

    pub async fn get_maintenance_logs(&self, limit: i32) -> Result<Vec<MaintenanceOperation>> {
        self.database.get_maintenance_logs(limit).await
    }

    pub async fn get_operations_summary(&self) -> serde_json::Value {
        let scheduled_ops = self.scheduled_operations.read().await;
        let operations: Vec<ScheduledOperation> = scheduled_ops.values().cloned().collect();
        let summary = create_operation_summary(&operations);

        serde_json::json!({
            "scheduled_operations": summary,
            "running_operations": self.get_running_operations().await.len(),
            "scheduler_config": {
                "max_concurrent_operations": self.scheduler_config.max_concurrent_operations,
                "operation_timeout_minutes": self.scheduler_config.operation_timeout_minutes,
                "retry_failed_operations": self.scheduler_config.retry_failed_operations,
            }
        })
    }

    async fn start_cleanup_task(&self) {
        let database = self.database.clone();
        let cleanup_days = self.scheduler_config.cleanup_completed_after_days;

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600)); // Run every hour

            loop {
                interval.tick().await;

                // Clean up old maintenance logs
                match database.cleanup_old_health_records(cleanup_days as i32).await {
                    Ok(deleted) => {
                        if deleted > 0 {
                            debug!("Cleaned up {} old maintenance records", deleted);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to cleanup old maintenance records: {}", e);
                    }
                }
            }
        });
    }

    pub async fn get_operation_status(&self, operation_id: &str) -> Option<OperationStatus> {
        // Check if it's currently running
        {
            let running_ops = self.running_operations.read().await;
            if running_ops.contains_key(operation_id) {
                return Some(OperationStatus::Running);
            }
        }

        // Check scheduled operations
        let scheduled_ops = self.scheduled_operations.read().await;
        scheduled_ops.get(operation_id).map(|op| op.get_status())
    }
}

// Implement Clone for the scheduler to enable async operations
impl Clone for MaintenanceScheduler {
    fn clone(&self) -> Self {
        Self {
            database: self.database.clone(),
            ssh_manager: self.ssh_manager.clone(),
            config: self.config.clone(),
            scheduler: self.scheduler.clone(),
            scheduled_operations: self.scheduled_operations.clone(),
            running_operations: self.running_operations.clone(),
            scheduler_config: self.scheduler_config.clone(),
        }
    }
}
