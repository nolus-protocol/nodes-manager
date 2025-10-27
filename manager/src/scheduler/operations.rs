// File: manager/src/scheduler/operations.rs
use crate::config::Config;
use crate::services::{HermesService, MaintenanceService};
use anyhow::{anyhow, Result};
use chrono::Utc;
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info, instrument, warn};

pub struct MaintenanceScheduler {
    config: Arc<Config>,
    maintenance_service: Arc<MaintenanceService>,
    hermes_service: Arc<HermesService>,
    scheduler: JobScheduler,
}

impl MaintenanceScheduler {
    pub async fn new(
        config: Arc<Config>,
        maintenance_service: Arc<MaintenanceService>,
        hermes_service: Arc<HermesService>,
    ) -> Result<Self> {
        let scheduler = JobScheduler::new()
            .await
            .map_err(|e| anyhow!("Failed to create JobScheduler: {}", e))?;

        Ok(Self {
            config,
            maintenance_service,
            hermes_service,
            scheduler,
        })
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
            "⏰ Scheduler timezone info - UTC: {}, Local: {}, Offset: {} hours",
            now_utc.format("%Y-%m-%d %H:%M:%S %Z"),
            now_local.format("%Y-%m-%d %H:%M:%S %Z"),
            now_local.offset().local_minus_utc() / 3600
        );
        info!("✅ Scheduler configured to use UTC timezone explicitly");
        info!("✅ All cron schedules will execute in UTC (not system local time)");
        let mut scheduled_count = 0;

        // Schedule pruning operations
        for (node_name, node_config) in &self.config.nodes {
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
                            info!("✓ Scheduled pruning for {}: {}", node_name, schedule);
                        }
                        Err(e) => {
                            error!(
                                "✗ Failed to schedule pruning for {}: {} (schedule: {})",
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
        for (node_name, node_config) in &self.config.nodes {
            if let Some(schedule) = &node_config.snapshot_schedule {
                if node_config.snapshots_enabled.unwrap_or(false) {
                    info!(
                        "Attempting to schedule snapshot for {}: '{}'",
                        node_name, schedule
                    );
                    match self
                        .schedule_snapshot_job(node_name.clone(), schedule.clone())
                        .await
                    {
                        Ok(_) => {
                            scheduled_count += 1;
                            info!("✓ Scheduled snapshot for {}: {}", node_name, schedule);
                        }
                        Err(e) => {
                            error!(
                                "✗ Failed to schedule snapshot for {}: {} (schedule: {})",
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
        for (hermes_name, hermes_config) in &self.config.hermes {
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
                        info!(
                            "✓ Scheduled Hermes restart for {}: {}",
                            hermes_name, schedule
                        );
                    }
                    Err(e) => {
                        error!(
                            "✗ Failed to schedule Hermes restart for {}: {} (schedule: {})",
                            hermes_name, e, schedule
                        );
                    }
                }
            }
        }

        if scheduled_count > 0 {
            self.scheduler
                .start()
                .await
                .map_err(|e| anyhow!("Failed to start scheduler: {}", e))?;
            info!(
                "✅ Scheduler started with {} scheduled jobs",
                scheduled_count
            );
        } else {
            warn!("⚠️ No jobs scheduled. Scheduler not started.");
        }

        Ok(())
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
                info!("🔧 Executing scheduled pruning for {}", node_name);

                // Use MaintenanceService (emits complete event stream)
                match maintenance_service
                    .execute_immediate_operation("pruning", &node_name)
                    .await
                {
                    Ok(operation_id) => {
                        info!(
                            "✓ Scheduled pruning completed for {} (operation_id: {})",
                            node_name, operation_id
                        );
                    }
                    Err(e) => {
                        error!("✗ Scheduled pruning failed for {}: {}", node_name, e);
                    }
                }
            })
        })
        .map_err(|e| anyhow!("Failed to create cron job: {}", e))?;

        self.scheduler
            .add(job)
            .await
            .map_err(|e| anyhow!("Failed to add job to scheduler: {}", e))?;

        Ok(())
    }

    async fn schedule_snapshot_job(&self, node_name: String, schedule: String) -> Result<()> {
        self.validate_6_field_cron(&schedule)
            .map_err(|e| anyhow!("Invalid 6-field cron schedule '{}': {}", schedule, e))?;

        let maintenance_service = self.maintenance_service.clone();
        let node_name_clone = node_name.clone();

        let job = Job::new_async_tz(schedule.as_str(), Utc, move |_uuid, _scheduler| {
            let maintenance_service = maintenance_service.clone();
            let node_name = node_name_clone.clone();

            Box::pin(async move {
                info!("📸 Executing scheduled snapshot for {}", node_name);

                // Use MaintenanceService (emits complete event stream)
                match maintenance_service
                    .execute_immediate_operation("snapshot_creation", &node_name)
                    .await
                {
                    Ok(operation_id) => {
                        info!(
                            "✓ Scheduled snapshot completed for {} (operation_id: {})",
                            node_name, operation_id
                        );
                    }
                    Err(e) => {
                        error!("✗ Scheduled snapshot failed for {}: {}", node_name, e);
                    }
                }
            })
        })
        .map_err(|e| anyhow!("Failed to create cron job: {}", e))?;

        self.scheduler
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
                info!("🔄 Executing scheduled Hermes restart for {}", hermes_name);

                // Use HermesService which includes AlertService integration
                match hermes_service.restart_instance(&hermes_name).await {
                    Ok(_) => {
                        info!("✓ Scheduled Hermes restart completed for {}", hermes_name);
                    }
                    Err(e) => {
                        error!(
                            "✗ Scheduled Hermes restart failed for {}: {}",
                            hermes_name, e
                        );
                    }
                }
            })
        })
        .map_err(|e| anyhow!("Failed to create cron job: {}", e))?;

        self.scheduler
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
