// File: manager/src/scheduler/operations.rs
use crate::config::Config;
use crate::database::{Database, MaintenanceOperation};
use crate::http::HttpAgentManager;
use crate::snapshot::SnapshotManager;
use anyhow::{anyhow, Result};
use chrono::Utc;
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info, warn, instrument};
use uuid::Uuid;

pub struct MaintenanceScheduler {
    database: Arc<Database>,
    http_manager: Arc<HttpAgentManager>,
    _config: Arc<Config>,
    snapshot_manager: Arc<SnapshotManager>,
    scheduler: JobScheduler,
}

impl MaintenanceScheduler {
    pub async fn new(
        database: Arc<Database>,
        http_manager: Arc<HttpAgentManager>,
        config: Arc<Config>,
        snapshot_manager: Arc<SnapshotManager>,
    ) -> Result<Self> {
        let scheduler = JobScheduler::new().await
            .map_err(|e| anyhow!("Failed to create JobScheduler: {}", e))?;

        Ok(Self {
            database,
            http_manager,
            _config: config,
            snapshot_manager,
            scheduler,
        })
    }

    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<()> {
        info!("Starting maintenance scheduler with 6-field cron format (sec min hour day month dow)");
        let mut scheduled_count = 0;

        // Schedule pruning operations
        for (node_name, node_config) in &self._config.nodes {
            if let Some(schedule) = &node_config.pruning_schedule {
                if node_config.pruning_enabled.unwrap_or(false) {
                    info!("Attempting to schedule pruning for {}: '{}'", node_name, schedule);
                    match self.schedule_pruning_job(node_name.clone(), schedule.clone()).await {
                        Ok(_) => {
                            scheduled_count += 1;
                            info!("✓ Scheduled pruning for {}: {}", node_name, schedule);
                        }
                        Err(e) => {
                            error!("✗ Failed to schedule pruning for {}: {} (schedule: {})", node_name, e, schedule);
                        }
                    }
                } else {
                    info!("Pruning disabled for {}, skipping schedule", node_name);
                }
            } else {
                info!("No pruning schedule configured for {}", node_name);
            }

            // Schedule snapshot operations
            if let Some(schedule) = &node_config.snapshot_schedule {
                if node_config.snapshots_enabled.unwrap_or(false) {
                    info!("Attempting to schedule snapshot for {}: '{}'", node_name, schedule);
                    match self.schedule_snapshot_job(node_name.clone(), schedule.clone()).await {
                        Ok(_) => {
                            scheduled_count += 1;
                            info!("✓ Scheduled snapshot for {}: {}", node_name, schedule);
                        }
                        Err(e) => {
                            error!("✗ Failed to schedule snapshot for {}: {} (schedule: {})", node_name, e, schedule);
                        }
                    }
                } else {
                    info!("Snapshots disabled for {}, skipping schedule", node_name);
                }
            } else {
                info!("No snapshot schedule configured for {}", node_name);
            }

            // NEW: Schedule state sync operations
            if let Some(schedule) = &node_config.state_sync_schedule {
                if node_config.state_sync_enabled.unwrap_or(false) {
                    info!("Attempting to schedule state sync for {}: '{}'", node_name, schedule);
                    match self.schedule_state_sync_job(node_name.clone(), schedule.clone()).await {
                        Ok(_) => {
                            scheduled_count += 1;
                            info!("✓ Scheduled state sync for {}: {}", node_name, schedule);
                        }
                        Err(e) => {
                            error!("✗ Failed to schedule state sync for {}: {} (schedule: {})", node_name, e, schedule);
                        }
                    }
                } else {
                    info!("State sync disabled for {}, skipping schedule", node_name);
                }
            } else if node_config.state_sync_enabled.unwrap_or(false) {
                info!("State sync enabled but no schedule configured for {}", node_name);
            }
        }

        // Schedule Hermes restart operations
        for (hermes_name, hermes_config) in &self._config.hermes {
            if let Some(schedule) = &hermes_config.restart_schedule {
                info!("Attempting to schedule Hermes restart for {}: '{}'", hermes_name, schedule);
                match self.schedule_hermes_restart_job(hermes_name.clone(), schedule.clone()).await {
                    Ok(_) => {
                        scheduled_count += 1;
                        info!("✓ Scheduled Hermes restart for {}: {}", hermes_name, schedule);
                    }
                    Err(e) => {
                        error!("✗ Failed to schedule Hermes restart for {}: {} (schedule: {})", hermes_name, e, schedule);
                    }
                }
            } else {
                info!("No restart schedule configured for Hermes {}", hermes_name);
            }
        }

        if scheduled_count > 0 {
            self.scheduler.start().await?;
            info!("✓ Maintenance scheduler started successfully with {} jobs", scheduled_count);
        } else {
            warn!("No scheduled jobs configured - scheduler not started");
        }

        Ok(())
    }

    async fn schedule_pruning_job(&self, node_name: String, schedule: String) -> Result<()> {
        self.validate_6_field_cron(&schedule)
            .map_err(|e| anyhow!("Invalid 6-field cron schedule '{}': {}", schedule, e))?;

        let http_manager = self.http_manager.clone();
        let _config = self._config.clone();
        let database = self.database.clone();
        let node_name_clone = node_name.clone();

        let job = Job::new_async(schedule.as_str(), move |_uuid, _scheduler| {
            let http_manager = http_manager.clone();
            let database = database.clone();
            let node_name = node_name_clone.clone();

            Box::pin(async move {
                info!("🔧 Executing scheduled pruning for {}", node_name);

                let operation_id = Uuid::new_v4().to_string();
                let operation = MaintenanceOperation {
                    id: operation_id.clone(),
                    operation_type: "scheduled_pruning".to_string(),
                    target_name: node_name.clone(),
                    status: "started".to_string(),
                    started_at: Utc::now(),
                    completed_at: None,
                    error_message: None,
                    details: None,
                };

                if let Err(e) = database.store_maintenance_operation(&operation).await {
                    error!("Failed to store maintenance operation: {}", e);
                    return;
                }

                match http_manager.execute_node_pruning(&node_name).await {
                    Ok(_) => {
                        info!("✓ Scheduled pruning completed for {}", node_name);
                        let mut completed_operation = operation;
                        completed_operation.status = "completed".to_string();
                        completed_operation.completed_at = Some(Utc::now());

                        if let Err(e) = database.store_maintenance_operation(&completed_operation).await {
                            error!("Failed to update operation status: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("✗ Scheduled pruning failed for {}: {}", node_name, e);
                        let mut failed_operation = operation;
                        failed_operation.status = "failed".to_string();
                        failed_operation.completed_at = Some(Utc::now());
                        failed_operation.error_message = Some(e.to_string());

                        if let Err(e) = database.store_maintenance_operation(&failed_operation).await {
                            error!("Failed to update operation status: {}", e);
                        }
                    }
                }
            })
        })
        .map_err(|e| anyhow!("Failed to create pruning job for '{}': {}", schedule, e))?;

        self.scheduler.add(job).await
            .map_err(|e| anyhow!("Failed to add pruning job to scheduler: {}", e))?;

        Ok(())
    }

    async fn schedule_snapshot_job(&self, node_name: String, schedule: String) -> Result<()> {
        self.validate_6_field_cron(&schedule)
            .map_err(|e| anyhow!("Invalid 6-field cron schedule '{}': {}", schedule, e))?;

        let snapshot_manager = self.snapshot_manager.clone();
        let database = self.database.clone();
        let node_name_clone = node_name.clone();

        let job = Job::new_async(schedule.as_str(), move |_uuid, _scheduler| {
            let snapshot_manager = snapshot_manager.clone();
            let database = database.clone();
            let node_name = node_name_clone.clone();

            Box::pin(async move {
                info!("📸 Executing scheduled snapshot creation for {}", node_name);

                let operation_id = Uuid::new_v4().to_string();
                let operation = MaintenanceOperation {
                    id: operation_id.clone(),
                    operation_type: "scheduled_snapshot".to_string(),
                    target_name: node_name.clone(),
                    status: "started".to_string(),
                    started_at: Utc::now(),
                    completed_at: None,
                    error_message: None,
                    details: None,
                };

                if let Err(e) = database.store_maintenance_operation(&operation).await {
                    error!("Failed to store maintenance operation: {}", e);
                    return;
                }

                match snapshot_manager.create_snapshot(&node_name).await {
                    Ok(_) => {
                        info!("✓ Scheduled snapshot completed for {}", node_name);
                        let mut completed_operation = operation;
                        completed_operation.status = "completed".to_string();
                        completed_operation.completed_at = Some(Utc::now());

                        if let Err(e) = database.store_maintenance_operation(&completed_operation).await {
                            error!("Failed to update operation status: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("✗ Scheduled snapshot failed for {}: {}", node_name, e);
                        let mut failed_operation = operation;
                        failed_operation.status = "failed".to_string();
                        failed_operation.completed_at = Some(Utc::now());
                        failed_operation.error_message = Some(e.to_string());

                        if let Err(e) = database.store_maintenance_operation(&failed_operation).await {
                            error!("Failed to update operation status: {}", e);
                        }
                    }
                }
            })
        })
        .map_err(|e| anyhow!("Failed to create snapshot job for '{}': {}", schedule, e))?;

        self.scheduler.add(job).await
            .map_err(|e| anyhow!("Failed to add snapshot job to scheduler: {}", e))?;

        Ok(())
    }

    // NEW: Schedule state sync job
    async fn schedule_state_sync_job(&self, node_name: String, schedule: String) -> Result<()> {
        self.validate_6_field_cron(&schedule)
            .map_err(|e| anyhow!("Invalid 6-field cron schedule '{}': {}", schedule, e))?;

        let http_manager = self.http_manager.clone();
        let database = self.database.clone();
        let node_name_clone = node_name.clone();

        let job = Job::new_async(schedule.as_str(), move |_uuid, _scheduler| {
            let http_manager = http_manager.clone();
            let database = database.clone();
            let node_name = node_name_clone.clone();

            Box::pin(async move {
                info!("🔄 Executing scheduled state sync for {}", node_name);

                let operation_id = Uuid::new_v4().to_string();
                let operation = MaintenanceOperation {
                    id: operation_id.clone(),
                    operation_type: "scheduled_state_sync".to_string(),
                    target_name: node_name.clone(),
                    status: "started".to_string(),
                    started_at: Utc::now(),
                    completed_at: None,
                    error_message: None,
                    details: None,
                };

                if let Err(e) = database.store_maintenance_operation(&operation).await {
                    error!("Failed to store maintenance operation: {}", e);
                    return;
                }

                match http_manager.execute_state_sync(&node_name).await {
                    Ok(_) => {
                        info!("✓ Scheduled state sync completed for {}", node_name);
                        let mut completed_operation = operation;
                        completed_operation.status = "completed".to_string();
                        completed_operation.completed_at = Some(Utc::now());

                        if let Err(e) = database.store_maintenance_operation(&completed_operation).await {
                            error!("Failed to update operation status: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("✗ Scheduled state sync failed for {}: {}", node_name, e);
                        let mut failed_operation = operation;
                        failed_operation.status = "failed".to_string();
                        failed_operation.completed_at = Some(Utc::now());
                        failed_operation.error_message = Some(e.to_string());

                        if let Err(e) = database.store_maintenance_operation(&failed_operation).await {
                            error!("Failed to update operation status: {}", e);
                        }
                    }
                }
            })
        })
        .map_err(|e| anyhow!("Failed to create state sync job for '{}': {}", schedule, e))?;

        self.scheduler.add(job).await
            .map_err(|e| anyhow!("Failed to add state sync job to scheduler: {}", e))?;

        Ok(())
    }

    async fn schedule_hermes_restart_job(&self, hermes_name: String, schedule: String) -> Result<()> {
        self.validate_6_field_cron(&schedule)
            .map_err(|e| anyhow!("Invalid 6-field cron schedule '{}': {}", schedule, e))?;

        let http_manager = self.http_manager.clone();
        let _config = self._config.clone();
        let database = self.database.clone();
        let hermes_name_clone = hermes_name.clone();

        let job = Job::new_async(schedule.as_str(), move |_uuid, _scheduler| {
            let http_manager = http_manager.clone();
            let _config = _config.clone();
            let database = database.clone();
            let hermes_name = hermes_name_clone.clone();

            Box::pin(async move {
                info!("🔄 Executing scheduled Hermes restart for {}", hermes_name);

                let operation_id = Uuid::new_v4().to_string();
                let operation = MaintenanceOperation {
                    id: operation_id.clone(),
                    operation_type: "scheduled_hermes_restart".to_string(),
                    target_name: hermes_name.clone(),
                    status: "started".to_string(),
                    started_at: Utc::now(),
                    completed_at: None,
                    error_message: None,
                    details: None,
                };

                if let Err(e) = database.store_maintenance_operation(&operation).await {
                    error!("Failed to store maintenance operation: {}", e);
                    return;
                }

                if let Some(hermes_config) = _config.hermes.get(&hermes_name) {
                    match http_manager.restart_hermes(hermes_config).await {
                        Ok(_) => {
                            info!("✓ Scheduled Hermes restart completed for {}", hermes_name);
                            let mut completed_operation = operation;
                            completed_operation.status = "completed".to_string();
                            completed_operation.completed_at = Some(Utc::now());

                            if let Err(e) = database.store_maintenance_operation(&completed_operation).await {
                                error!("Failed to update operation status: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("✗ Scheduled Hermes restart failed for {}: {}", hermes_name, e);
                            let mut failed_operation = operation;
                            failed_operation.status = "failed".to_string();
                            failed_operation.completed_at = Some(Utc::now());
                            failed_operation.error_message = Some(e.to_string());

                            if let Err(e) = database.store_maintenance_operation(&failed_operation).await {
                                error!("Failed to update operation status: {}", e);
                            }
                        }
                    }
                }
            })
        })
        .map_err(|e| anyhow!("Failed to create Hermes restart job for '{}': {}", schedule, e))?;

        self.scheduler.add(job).await
            .map_err(|e| anyhow!("Failed to add Hermes restart job to scheduler: {}", e))?;

        Ok(())
    }

    fn validate_6_field_cron(&self, schedule: &str) -> Result<()> {
        let parts: Vec<&str> = schedule.split_whitespace().collect();

        if parts.len() != 6 {
            return Err(anyhow!("tokio-cron-scheduler requires exactly 6 fields: second minute hour day month dayofweek. Got {} fields: '{}'", parts.len(), schedule));
        }

        self.validate_cron_field(parts[0], "second", 0, 59)?;
        self.validate_cron_field(parts[1], "minute", 0, 59)?;
        self.validate_cron_field(parts[2], "hour", 0, 23)?;
        self.validate_cron_field(parts[3], "day", 1, 31)?;
        self.validate_cron_field(parts[4], "month", 1, 12)?;
        self.validate_cron_field(parts[5], "dayofweek", 0, 7)?;

        info!("Validated 6-field cron: '{}' → sec:{} min:{} hour:{} day:{} month:{} dow:{}",
              schedule, parts[0], parts[1], parts[2], parts[3], parts[4], parts[5]);

        Ok(())
    }

    fn validate_cron_field(&self, field: &str, name: &str, min: u32, max: u32) -> Result<()> {
        if field == "*" || field == "?" {
            return Ok(());
        }

        if field.contains('-') {
            let range: Vec<&str> = field.split('-').collect();
            if range.len() == 2 {
                let start = range[0].parse::<u32>()
                    .map_err(|_| anyhow!("Invalid {} range start: {}", name, range[0]))?;
                let end = range[1].parse::<u32>()
                    .map_err(|_| anyhow!("Invalid {} range end: {}", name, range[1]))?;

                if start < min || start > max || end < min || end > max {
                    return Err(anyhow!("{} range {}-{} is outside valid range {}-{}", name, start, end, min, max));
                }
                return Ok(());
            }
        }

        if field.contains(',') {
            for part in field.split(',') {
                let value = part.parse::<u32>()
                    .map_err(|_| anyhow!("Invalid {} value in list: {}", name, part))?;
                if value < min || value > max {
                    return Err(anyhow!("{} value {} is outside valid range {}-{}", name, value, min, max));
                }
            }
            return Ok(());
        }

        if let Some(step_str) = field.strip_prefix("*/") {
            let step = step_str.parse::<u32>()
                .map_err(|_| anyhow!("Invalid {} step value: {}", name, step_str))?;
            if step == 0 {
                return Err(anyhow!("{} step value cannot be 0", name));
            }
            return Ok(());
        }

        let value = field.parse::<u32>()
            .map_err(|_| anyhow!("Invalid {} value: {}", name, field))?;

        if value < min || value > max {
            return Err(anyhow!("{} value {} is outside valid range {}-{}", name, value, min, max));
        }

        Ok(())
    }
}
