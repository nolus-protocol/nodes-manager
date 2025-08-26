// File: src/scheduler/operations.rs

use anyhow::Result;
use chrono::{Datelike, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock};
use tokio_cron_scheduler::JobScheduler;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::database::Database;
use crate::scheduler::{
    create_operation_summary, OperationResult, OperationStatus,
    OperationType, ScheduledOperation, SchedulerConfig,
};
use crate::snapshot::SnapshotManager;
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
    snapshot_manager: Arc<SnapshotManager>,  // NEW: Added snapshot manager
}

impl MaintenanceScheduler {
    pub fn new(
        database: Arc<Database>,
        ssh_manager: Arc<SshManager>,
        config: Arc<Config>,
        snapshot_manager: Arc<SnapshotManager>,  // NEW: Accept snapshot manager
    ) -> Self {
        Self {
            database,
            ssh_manager,
            config,
            scheduler: Arc::new(Mutex::new(None)),
            scheduled_operations: Arc::new(RwLock::new(HashMap::new())),
            running_operations: Arc::new(RwLock::new(HashMap::new())),
            scheduler_config: SchedulerConfig::default(),
            snapshot_manager,
        }
    }

    pub async fn start_scheduler(&self) -> Result<()> {
        info!("Starting maintenance scheduler with scheduled snapshot support");

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

        info!("Maintenance scheduler started successfully with automatic scheduling and snapshot support enabled");
        Ok(())
    }

    // Calculate next run time based on cron schedule
    fn calculate_next_run(&self, schedule: &str, after: &chrono::DateTime<Utc>) -> Option<chrono::DateTime<Utc>> {
        let parts: Vec<&str> = schedule.split_whitespace().collect();
        if parts.len() != 6 {
            warn!("Invalid schedule format for next run calculation: {}", schedule);
            return None;
        }

        let sec: u32 = parts[0].parse().ok()?;
        let min: u32 = parts[1].parse().ok()?;
        let hour: u32 = parts[2].parse().ok()?;
        // parts[3] and parts[4] are * (day and month) - not used in current implementation
        let weekday: u32 = parts[5].parse().ok()?; // 0=Sunday, 1=Monday, 2=Tuesday...

        // Start from tomorrow to avoid running today if we're past the time
        let mut candidate = *after + chrono::Duration::days(1);

        // Find next occurrence of the target weekday
        for _ in 0..7 {
            // Use consistent weekday numbering (0=Sunday, 1=Monday, etc.)
            let current_weekday = match candidate.weekday().number_from_monday() {
                7 => 0, // Sunday = 0
                n => n, // Monday=1, Tuesday=2, etc.
            };

            if current_weekday == weekday {
                // Found the right weekday, set the time
                if let Some(target_time) = candidate.date_naive().and_hms_opt(hour, min, sec) {
                    if let Some(target_datetime) = target_time.and_local_timezone(chrono::Utc).single() {
                        return Some(target_datetime);
                    }
                }
            }
            candidate = candidate + chrono::Duration::days(1);
        }

        None
    }

    async fn load_operations_from_config(&self) -> Result<()> {
        let mut operations = HashMap::new();
        let now = Utc::now();

        // Load node pruning schedules
        for (node_name, node_config) in &self.config.nodes {
            if let Some(schedule) = &node_config.pruning_schedule {
                if node_config.pruning_enabled.unwrap_or(false) {
                    let mut operation = ScheduledOperation::new_pruning(
                        node_name.clone(),
                        schedule.clone(),
                    );

                    operation.next_run = self.calculate_next_run(schedule, &now);
                    info!("Loaded pruning schedule for {}: {} -> next run: {:?}",
                          node_name, schedule, operation.next_run);

                    operations.insert(operation.id.clone(), operation);
                }
            }
        }

        // NEW: Load scheduled snapshot creation
        for (node_name, node_config) in &self.config.nodes {
            if let Some(schedule) = &node_config.snapshot_schedule {
                if node_config.snapshots_enabled.unwrap_or(false) {
                    let mut operation = ScheduledOperation::new_snapshot_creation(
                        node_name.clone(),
                        schedule.clone(),
                    );

                    operation.next_run = self.calculate_next_run(schedule, &now);
                    info!("Loaded snapshot schedule for {}: {} -> next run: {:?}",
                          node_name, schedule, operation.next_run);

                    operations.insert(operation.id.clone(), operation);
                }
            }
        }

        // Load hermes restart schedules
        for (hermes_name, hermes_config) in &self.config.hermes {
            let mut operation = ScheduledOperation::new_hermes_restart(
                hermes_name.clone(),
                hermes_config.restart_schedule.clone(),
            );

            operation.next_run = self.calculate_next_run(&hermes_config.restart_schedule, &now);
            info!("Loaded hermes restart schedule for {}: {} -> next run: {:?}",
                  hermes_name, hermes_config.restart_schedule, operation.next_run);

            operations.insert(operation.id.clone(), operation);
        }

        let operation_count = operations.len();
        {
            let mut scheduled_ops = self.scheduled_operations.write().await;
            *scheduled_ops = operations;
        }

        info!("Loaded {} scheduled operations from configuration with calculated next run times", operation_count);

        // Log all loaded operations for debugging
        let scheduled_ops = self.scheduled_operations.read().await;
        for (id, op) in scheduled_ops.iter() {
            info!("Operation {}: {:?} for {} (enabled: {}, next_run: {:?})",
                  id, op.operation_type, op.target_name, op.enabled, op.next_run);
        }

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

                debug!("Scheduler check at {}: {} operations to evaluate", now.format("%Y-%m-%d %H:%M:%S UTC"), scheduled_ops.len());

                for (operation_id, operation) in scheduled_ops.iter() {
                    if !operation.enabled {
                        debug!("Operation {} is disabled, skipping", operation_id);
                        continue;
                    }

                    // Use next_run time instead of cron pattern matching
                    if let Some(next_run) = operation.next_run {
                        // Allow a 2-minute window for execution (in case we miss the exact minute)
                        let execution_window_start = next_run;
                        let execution_window_end = next_run + chrono::Duration::minutes(2);

                        if now >= execution_window_start && now <= execution_window_end {
                            // Additional check: don't run if we just ran this recently
                            let mut should_execute = true;
                            if let Some(last_run) = operation.last_run {
                                let time_since_last = now.signed_duration_since(last_run);
                                if time_since_last.num_hours() < 1 {
                                    should_execute = false;
                                    debug!("Operation {} ran recently ({}m ago), skipping",
                                           operation_id, time_since_last.num_minutes());
                                }
                            }

                            if should_execute {
                                info!("Triggering scheduled operation: {} ({}) - next_run: {}, current: {}",
                                      operation_id, operation.target_name, next_run.format("%Y-%m-%d %H:%M:%S UTC"),
                                      now.format("%Y-%m-%d %H:%M:%S UTC"));

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
                        } else {
                            debug!("Operation {} not due yet. Next run: {}, Current: {}",
                                   operation_id, next_run.format("%Y-%m-%d %H:%M:%S UTC"),
                                   now.format("%Y-%m-%d %H:%M:%S UTC"));
                        }
                    } else {
                        warn!("Operation {} has no next_run time calculated", operation_id);
                    }
                }

                drop(scheduled_ops); // Release the lock
            }
        });

        info!("Simple scheduler background task started - checking every 60 seconds");
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
            // NEW: Handle scheduled snapshot creation
            OperationType::SnapshotCreation => {
                self.execute_snapshot_creation(target_name).await
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

        // Calculate next run time outside of any locks to prevent deadlock
        let now = Utc::now();
        let next_run_time = {
            let scheduled_ops = self.scheduled_operations.read().await;
            if let Some(operation) = scheduled_ops.get(operation_id) {
                self.calculate_next_run(&operation.schedule, &now)
            } else {
                return Err(anyhow::anyhow!("Operation {} not found for update", operation_id));
            }
        }; // Read lock released here

        // Now acquire write lock only for the update with minimal scope
        {
            let mut scheduled_ops = self.scheduled_operations.write().await;
            if let Some(operation) = scheduled_ops.get_mut(operation_id) {
                operation.last_run = Some(end_time);
                operation.update_result(operation_result.clone());
                operation.next_run = next_run_time;
                info!("Updated next run time for operation {}: {:?}", operation_id, next_run_time);
            }
        } // Write lock released immediately

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

    // NEW: Execute scheduled snapshot creation with retention management
    async fn execute_snapshot_creation(&self, node_name: &str) -> Result<()> {
        let node_config = self
            .config
            .nodes
            .get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?
            .clone();

        if !node_config.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("Snapshots not enabled for node {}", node_name));
        }

        info!("Starting scheduled snapshot creation for node: {}", node_name);

        // Create the snapshot
        let snapshot_info = self.snapshot_manager.create_snapshot(node_name).await?;
        info!("Scheduled snapshot created: {}", snapshot_info.filename);

        // Apply retention policy if configured
        if let Some(retention_count) = node_config.snapshot_retention_count {
            info!("Applying retention policy: keeping {} most recent snapshots", retention_count);

            match self.snapshot_manager.cleanup_old_snapshots(node_name, retention_count).await {
                Ok(deleted_count) => {
                    if deleted_count > 0 {
                        info!("Cleaned up {} old snapshots for node {}", deleted_count, node_name);
                    }
                }
                Err(e) => {
                    warn!("Failed to cleanup old snapshots for node {}: {}", node_name, e);
                    // Don't fail the entire operation if cleanup fails
                }
            }
        }

        info!("Scheduled snapshot creation completed for node: {}", node_name);
        Ok(())
    }

    // Update scheduled operation when manual operation completes
    pub async fn update_scheduled_operation_result(
        &self,
        target_name: &str,
        operation_type: &OperationType,
        success: bool,
        error_message: Option<String>,
    ) -> Result<()> {
        // Pre-calculate data outside any locks
        let now = Utc::now();
        let result = OperationResult {
            success,
            message: if success {
                "Manual operation completed successfully".to_string()
            } else {
                error_message.unwrap_or_else(|| "Manual operation failed".to_string())
            },
            duration_seconds: 0, // Manual operations duration not tracked here
            executed_at: now,
        };

        // Step 1: Find the matching operation and get its schedule (read lock only)
        let schedule_for_next_run = {
            let scheduled_ops = self.scheduled_operations.read().await;
            let mut found_schedule = None;

            for operation in scheduled_ops.values() {
                if operation.target_name == target_name && operation.operation_type == *operation_type {
                    found_schedule = Some(operation.schedule.clone());
                    break;
                }
            }
            found_schedule
        }; // Read lock released here

        // Step 2: Calculate next run time outside of any locks
        let next_run_time = if let Some(schedule) = schedule_for_next_run {
            self.calculate_next_run(&schedule, &now)
        } else {
            // No matching operation found
            return Ok(());
        };

        // Step 3: Update the operation with minimal write lock scope
        {
            let mut scheduled_ops = self.scheduled_operations.write().await;

            for (operation_id, operation) in scheduled_ops.iter_mut() {
                if operation.target_name == target_name && operation.operation_type == *operation_type {
                    operation.last_run = Some(now);
                    operation.update_result(result.clone());
                    operation.next_run = next_run_time;

                    info!(
                        "Updated scheduled operation {} with manual execution result: {} (next run: {:?})",
                        operation_id,
                        if success { "success" } else { "failure" },
                        operation.next_run
                    );
                    break;
                }
            }
        } // Write lock released immediately

        Ok(())
    }

    // Execute immediate operations with scheduled operation tracking
    pub async fn execute_immediate_pruning(&self, node_name: &str) -> Result<()> {
        info!("Executing immediate pruning for node: {}", node_name);

        let result = self.execute_node_pruning(node_name).await;
        let success = result.is_ok();
        let error_message = if let Err(ref e) = result {
            Some(e.to_string())
        } else {
            None
        };

        // Update corresponding scheduled operation
        if let Err(e) = self.update_scheduled_operation_result(
            node_name,
            &OperationType::NodePruning,
            success,
            error_message.clone(),
        ).await {
            warn!("Failed to update scheduled operation for manual pruning: {}", e);
        }

        result
    }

    pub async fn execute_immediate_hermes_restart(&self, hermes_name: &str) -> Result<()> {
        info!("Executing immediate Hermes restart: {}", hermes_name);

        let result = self.execute_hermes_restart(hermes_name).await;
        let success = result.is_ok();
        let error_message = if let Err(ref e) = result {
            Some(e.to_string())
        } else {
            None
        };

        // Update corresponding scheduled operation
        if let Err(e) = self.update_scheduled_operation_result(
            hermes_name,
            &OperationType::HermesRestart,
            success,
            error_message.clone(),
        ).await {
            warn!("Failed to update scheduled operation for manual Hermes restart: {}", e);
        }

        result
    }

    // NEW: Execute immediate snapshot creation with scheduled operation tracking
    pub async fn execute_immediate_snapshot_creation(&self, node_name: &str) -> Result<()> {
        info!("Executing immediate snapshot creation for node: {}", node_name);

        let result = self.execute_snapshot_creation(node_name).await;
        let success = result.is_ok();
        let error_message = if let Err(ref e) = result {
            Some(e.to_string())
        } else {
            None
        };

        // Update corresponding scheduled operation
        if let Err(e) = self.update_scheduled_operation_result(
            node_name,
            &OperationType::SnapshotCreation,
            success,
            error_message.clone(),
        ).await {
            warn!("Failed to update scheduled operation for manual snapshot creation: {}", e);
        }

        result
    }

    // Batch operations that also update scheduled operations
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

        // Update scheduled operations for each completed node
        for operation_result in &result.results {
            if let Err(e) = self.update_scheduled_operation_result(
                &operation_result.target_name,
                &OperationType::NodePruning,
                operation_result.success,
                operation_result.error_message.clone(),
            ).await {
                warn!("Failed to update scheduled operation for batch pruning {}: {}", operation_result.target_name, e);
            }
        }

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

        // Update scheduled operations for each completed hermes instance
        for operation_result in &result.results {
            if let Err(e) = self.update_scheduled_operation_result(
                &operation_result.target_name,
                &OperationType::HermesRestart,
                operation_result.success,
                operation_result.error_message.clone(),
            ).await {
                warn!("Failed to update scheduled operation for batch hermes restart {}: {}", operation_result.target_name, e);
            }
        }

        info!(
            "Batch Hermes restart completed: {}/{} successful",
            result.successful,
            result.total_operations
        );

        Ok(result)
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
            snapshot_manager: self.snapshot_manager.clone(),
        }
    }
}
