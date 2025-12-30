// File: manager/src/scheduler/operations.rs
use crate::config::Config;
use crate::services::{HermesService, MaintenanceService, SnapshotService, StateSyncService};
use anyhow::{anyhow, Result};
use chrono::Utc;
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info, instrument, warn};

pub struct MaintenanceScheduler {
    config: Arc<tokio::sync::RwLock<Arc<Config>>>,
    maintenance_service: Arc<MaintenanceService>,
    snapshot_service: Arc<SnapshotService>,
    hermes_service: Arc<HermesService>,
    state_sync_service: Arc<StateSyncService>,
    scheduler: tokio::sync::RwLock<JobScheduler>,
}

impl MaintenanceScheduler {
    pub async fn new(
        config: Arc<Config>,
        maintenance_service: Arc<MaintenanceService>,
        snapshot_service: Arc<SnapshotService>,
        hermes_service: Arc<HermesService>,
        state_sync_service: Arc<StateSyncService>,
    ) -> Result<Self> {
        let scheduler = JobScheduler::new()
            .await
            .map_err(|e| anyhow!("Failed to create JobScheduler: {}", e))?;

        Ok(Self {
            config: Arc::new(tokio::sync::RwLock::new(config)),
            maintenance_service,
            snapshot_service,
            hermes_service,
            state_sync_service,
            scheduler: tokio::sync::RwLock::new(scheduler),
        })
    }

    /// Update the configuration and re-register all scheduled jobs.
    /// This should be called when the configuration changes via the API.
    #[instrument(skip(self, new_config))]
    pub async fn reload_config(&self, new_config: Arc<Config>) -> Result<()> {
        info!("Reloading scheduler with updated configuration");

        // Update the stored config
        {
            let mut config = self.config.write().await;
            *config = new_config;
        }

        // Shutdown the old scheduler and create a new one
        {
            let mut scheduler = self.scheduler.write().await;
            if let Err(e) = scheduler.shutdown().await {
                warn!("Error shutting down old scheduler: {}", e);
            }
            *scheduler = JobScheduler::new()
                .await
                .map_err(|e| anyhow!("Failed to create new JobScheduler: {}", e))?;
        }

        // Re-register all jobs with the new config
        self.register_all_jobs().await?;

        // Start the new scheduler
        {
            let scheduler = self.scheduler.read().await;
            scheduler
                .start()
                .await
                .map_err(|e| anyhow!("Failed to start scheduler: {}", e))?;
        }

        info!("Scheduler reloaded successfully");
        Ok(())
    }

    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<()> {
        // Log timezone information for debugging
        let now_utc = Utc::now();
        let now_local = chrono::Local::now();
        info!(
            "Starting maintenance scheduler with 6-field cron format (sec min hour day month dow)"
        );
        info!(
            "Scheduler timezone info - UTC: {}, Local: {}, Offset: {} hours",
            now_utc.format("%Y-%m-%d %H:%M:%S %Z"),
            now_local.format("%Y-%m-%d %H:%M:%S %Z"),
            now_local.offset().local_minus_utc() / 3600
        );
        info!("Scheduler configured to use UTC timezone explicitly");
        info!("All cron schedules will execute in UTC (not system local time)");

        let scheduled_count = self.register_all_jobs().await?;

        if scheduled_count > 0 {
            let scheduler = self.scheduler.read().await;
            scheduler
                .start()
                .await
                .map_err(|e| anyhow!("Failed to start scheduler: {}", e))?;
            info!("Scheduler started with {} scheduled jobs", scheduled_count);
        } else {
            warn!("No jobs scheduled. Scheduler not started.");
        }

        Ok(())
    }

    /// Register all scheduled jobs based on current configuration.
    /// Returns the number of jobs successfully scheduled.
    async fn register_all_jobs(&self) -> Result<usize> {
        let config = self.config.read().await;
        let mut scheduled_count = 0;

        // Schedule pruning operations
        for (node_name, node_config) in &config.nodes {
            if let Some(schedule) = &node_config.pruning_schedule {
                if node_config.pruning_enabled.unwrap_or(false) {
                    info!(
                        "Attempting to schedule pruning for {}: '{}'",
                        node_name, schedule
                    );
                    match self
                        .schedule_pruning_job(node_name.clone(), schedule.clone())
                        .await
                    {
                        Ok(_) => {
                            scheduled_count += 1;
                            info!("Scheduled pruning for {}: {}", node_name, schedule);
                        }
                        Err(e) => {
                            error!(
                                "Failed to schedule pruning for {}: {} (schedule: {})",
                                node_name, e, schedule
                            );
                        }
                    }
                } else {
                    info!("Pruning disabled for {}, skipping schedule", node_name);
                }
            }
        }

        // Schedule snapshot operations
        for (node_name, node_config) in &config.nodes {
            if let Some(schedule) = &node_config.snapshot_schedule {
                if node_config.snapshots_enabled.unwrap_or(false) {
                    info!(
                        "Attempting to schedule snapshot for {}: '{}'",
                        node_name, schedule
                    );
                    let retention_count = node_config.snapshot_retention_count.map(|c| c as u32);
                    match self
                        .schedule_snapshot_job(node_name.clone(), schedule.clone(), retention_count)
                        .await
                    {
                        Ok(_) => {
                            scheduled_count += 1;
                            info!("Scheduled snapshot for {}: {}", node_name, schedule);
                        }
                        Err(e) => {
                            error!(
                                "Failed to schedule snapshot for {}: {} (schedule: {})",
                                node_name, e, schedule
                            );
                        }
                    }
                } else {
                    info!("Snapshots disabled for {}, skipping schedule", node_name);
                }
            }
        }

        // Schedule Hermes restart operations
        for (hermes_name, hermes_config) in &config.hermes {
            if let Some(schedule) = &hermes_config.restart_schedule {
                info!(
                    "Attempting to schedule Hermes restart for {}: '{}'",
                    hermes_name, schedule
                );
                match self
                    .schedule_hermes_restart_job(hermes_name.clone(), schedule.clone())
                    .await
                {
                    Ok(_) => {
                        scheduled_count += 1;
                        info!("Scheduled Hermes restart for {}: {}", hermes_name, schedule);
                    }
                    Err(e) => {
                        error!(
                            "Failed to schedule Hermes restart for {}: {} (schedule: {})",
                            hermes_name, e, schedule
                        );
                    }
                }
            }
        }

        // Schedule state sync operations
        for (node_name, node_config) in &config.nodes {
            if let Some(schedule) = &node_config.state_sync_schedule {
                if node_config.state_sync_enabled.unwrap_or(false) {
                    info!(
                        "Attempting to schedule state sync for {}: '{}'",
                        node_name, schedule
                    );
                    match self
                        .schedule_state_sync_job(node_name.clone(), schedule.clone())
                        .await
                    {
                        Ok(_) => {
                            scheduled_count += 1;
                            info!("Scheduled state sync for {}: {}", node_name, schedule);
                        }
                        Err(e) => {
                            error!(
                                "Failed to schedule state sync for {}: {} (schedule: {})",
                                node_name, e, schedule
                            );
                        }
                    }
                } else {
                    info!("State sync disabled for {}, skipping schedule", node_name);
                }
            }
        }

        Ok(scheduled_count)
    }

    async fn schedule_pruning_job(&self, node_name: String, schedule: String) -> Result<()> {
        self.validate_6_field_cron(&schedule)
            .map_err(|e| anyhow!("Invalid 6-field cron schedule '{}': {}", schedule, e))?;

        let maintenance_service = self.maintenance_service.clone();
        let node_name_clone = node_name.clone();

        let job = Job::new_async_tz(schedule.as_str(), Utc, move |_uuid, _scheduler| {
            let maintenance_service = maintenance_service.clone();
            let node_name = node_name_clone.clone();

            Box::pin(async move {
                info!("Executing scheduled pruning for {}", node_name);

                // Use MaintenanceService (emits complete event stream)
                match maintenance_service
                    .execute_immediate_operation("pruning", &node_name)
                    .await
                {
                    Ok(operation_id) => {
                        info!(
                            "Scheduled pruning completed for {} (operation_id: {})",
                            node_name, operation_id
                        );
                    }
                    Err(e) => {
                        error!("Scheduled pruning failed for {}: {}", node_name, e);
                    }
                }
            })
        })
        .map_err(|e| anyhow!("Failed to create cron job: {}", e))?;

        let scheduler = self.scheduler.read().await;
        scheduler
            .add(job)
            .await
            .map_err(|e| anyhow!("Failed to add job to scheduler: {}", e))?;

        Ok(())
    }

    async fn schedule_snapshot_job(
        &self,
        node_name: String,
        schedule: String,
        retention_count: Option<u32>,
    ) -> Result<()> {
        self.validate_6_field_cron(&schedule)
            .map_err(|e| anyhow!("Invalid 6-field cron schedule '{}': {}", schedule, e))?;

        let maintenance_service = self.maintenance_service.clone();
        let snapshot_service = self.snapshot_service.clone();
        let node_name_clone = node_name.clone();

        let job = Job::new_async_tz(schedule.as_str(), Utc, move |_uuid, _scheduler| {
            let maintenance_service = maintenance_service.clone();
            let snapshot_service = snapshot_service.clone();
            let node_name = node_name_clone.clone();
            let retention_count = retention_count;

            Box::pin(async move {
                info!("Executing scheduled snapshot for {}", node_name);

                // Use MaintenanceService (emits complete event stream)
                match maintenance_service
                    .execute_immediate_operation("snapshot_creation", &node_name)
                    .await
                {
                    Ok(operation_id) => {
                        info!(
                            "Scheduled snapshot completed for {} (operation_id: {})",
                            node_name, operation_id
                        );

                        // Clean up old snapshots if retention_count is configured
                        if let Some(retention) = retention_count {
                            info!(
                                "Running snapshot cleanup for {} (keeping {} most recent)",
                                node_name, retention
                            );
                            match snapshot_service
                                .cleanup_old_snapshots(&node_name, retention)
                                .await
                            {
                                Ok(result) => {
                                    if let Some(deleted) = result.get("deleted_count").and_then(|v| v.as_u64()) {
                                        if deleted > 0 {
                                            info!(
                                                "Snapshot cleanup completed for {}: deleted {} old snapshots",
                                                node_name, deleted
                                            );
                                        } else {
                                            info!(
                                                "Snapshot cleanup completed for {}: no old snapshots to delete",
                                                node_name
                                            );
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        "Snapshot cleanup failed for {}: {}",
                                        node_name, e
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Scheduled snapshot failed for {}: {}", node_name, e);
                    }
                }
            })
        })
        .map_err(|e| anyhow!("Failed to create cron job: {}", e))?;

        let scheduler = self.scheduler.read().await;
        scheduler
            .add(job)
            .await
            .map_err(|e| anyhow!("Failed to add job to scheduler: {}", e))?;

        Ok(())
    }

    async fn schedule_hermes_restart_job(
        &self,
        hermes_name: String,
        schedule: String,
    ) -> Result<()> {
        self.validate_6_field_cron(&schedule)
            .map_err(|e| anyhow!("Invalid 6-field cron schedule '{}': {}", schedule, e))?;

        let hermes_service = self.hermes_service.clone();
        let hermes_name_clone = hermes_name.clone();

        let job = Job::new_async_tz(schedule.as_str(), Utc, move |_uuid, _scheduler| {
            let hermes_service = hermes_service.clone();
            let hermes_name = hermes_name_clone.clone();

            Box::pin(async move {
                info!("Executing scheduled Hermes restart for {}", hermes_name);

                // Use HermesService which includes AlertService integration
                match hermes_service.restart_instance(&hermes_name).await {
                    Ok(_) => {
                        info!("Scheduled Hermes restart completed for {}", hermes_name);
                    }
                    Err(e) => {
                        error!("Scheduled Hermes restart failed for {}: {}", hermes_name, e);
                    }
                }
            })
        })
        .map_err(|e| anyhow!("Failed to create cron job: {}", e))?;

        let scheduler = self.scheduler.read().await;
        scheduler
            .add(job)
            .await
            .map_err(|e| anyhow!("Failed to add job to scheduler: {}", e))?;

        Ok(())
    }

    async fn schedule_state_sync_job(&self, node_name: String, schedule: String) -> Result<()> {
        self.validate_6_field_cron(&schedule)
            .map_err(|e| anyhow!("Invalid 6-field cron schedule '{}': {}", schedule, e))?;

        let state_sync_service = self.state_sync_service.clone();
        let node_name_clone = node_name.clone();

        let job = Job::new_async_tz(schedule.as_str(), Utc, move |_uuid, _scheduler| {
            let state_sync_service = state_sync_service.clone();
            let node_name = node_name_clone.clone();

            Box::pin(async move {
                info!("Executing scheduled state sync for {}", node_name);

                // Use StateSyncService which includes AlertService integration
                match state_sync_service.execute_state_sync(&node_name).await {
                    Ok(_) => {
                        info!("Scheduled state sync completed for {}", node_name);
                    }
                    Err(e) => {
                        error!("Scheduled state sync failed for {}: {}", node_name, e);
                    }
                }
            })
        })
        .map_err(|e| anyhow!("Failed to create cron job: {}", e))?;

        let scheduler = self.scheduler.read().await;
        scheduler
            .add(job)
            .await
            .map_err(|e| anyhow!("Failed to add job to scheduler: {}", e))?;

        Ok(())
    }

    fn validate_6_field_cron(&self, schedule: &str) -> Result<()> {
        let parts: Vec<&str> = schedule.split_whitespace().collect();
        if parts.len() != 6 {
            return Err(anyhow!(
                "Expected 6 fields (sec min hour day month dow), got {}",
                parts.len()
            ));
        }
        Ok(())
    }
}
