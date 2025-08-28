// File: manager/src/scheduler/operations.rs
use crate::config::Config;
use crate::database::{Database, MaintenanceOperation};
use crate::http::HttpAgentManager;
use crate::snapshot::SnapshotManager;
use anyhow::{anyhow, Result};
use chrono::Utc;
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};
use uuid::Uuid;

pub struct MaintenanceScheduler {
    database: Arc<Database>,
    http_manager: Arc<HttpAgentManager>,
    config: Arc<Config>,
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
            config,
            snapshot_manager,
            scheduler,
        })
    }

    pub async fn start(&self) -> Result<()> {
        info!("Starting maintenance scheduler");

        // Schedule pruning operations
        for (node_name, node_config) in &self.config.nodes {
            if let Some(schedule) = &node_config.pruning_schedule {
                if node_config.pruning_enabled.unwrap_or(false) {
                    self.schedule_pruning_job(node_name.clone(), schedule.clone()).await?;
                }
            }

            // Schedule snapshot operations
            if let Some(schedule) = &node_config.snapshot_schedule {
                if node_config.snapshots_enabled.unwrap_or(false) {
                    self.schedule_snapshot_job(node_name.clone(), schedule.clone()).await?;
                }
            }
        }

        // Schedule Hermes restart operations
        for (hermes_name, hermes_config) in &self.config.hermes {
            if let Some(schedule) = &hermes_config.restart_schedule {
                self.schedule_hermes_restart_job(hermes_name.clone(), schedule.clone()).await?;
            }
        }

        self.scheduler.start().await?;
        info!("Maintenance scheduler started");
        Ok(())
    }

    async fn schedule_pruning_job(&self, node_name: String, schedule: String) -> Result<()> {
        let http_manager = self.http_manager.clone();
        let config = self.config.clone();
        let database = self.database.clone();
        let node_name_clone = node_name.clone();

        let job = Job::new_async(schedule.as_str(), move |_uuid, _scheduler| {
            let http_manager = http_manager.clone();
            let config = config.clone();
            let database = database.clone();
            let node_name = node_name_clone.clone();

            Box::pin(async move {
                info!("Starting scheduled pruning for {}", node_name);

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

                if let Some(node_config) = config.nodes.get(&node_name) {
                    match http_manager.run_pruning(node_config).await {
                        Ok(_) => {
                            info!("Scheduled pruning completed for {}", node_name);
                            let mut completed_operation = operation;
                            completed_operation.status = "completed".to_string();
                            completed_operation.completed_at = Some(Utc::now());

                            if let Err(e) = database.store_maintenance_operation(&completed_operation).await {
                                error!("Failed to update operation status: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Scheduled pruning failed for {}: {}", node_name, e);
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
        })?;

        self.scheduler.add(job).await?;
        info!("Scheduled pruning job for {}: {}", node_name, schedule);
        Ok(())
    }

    async fn schedule_snapshot_job(&self, node_name: String, schedule: String) -> Result<()> {
        let snapshot_manager = self.snapshot_manager.clone();
        let database = self.database.clone();
        let node_name_clone = node_name.clone();

        let job = Job::new_async(schedule.as_str(), move |_uuid, _scheduler| {
            let snapshot_manager = snapshot_manager.clone();
            let database = database.clone();
            let node_name = node_name_clone.clone();

            Box::pin(async move {
                info!("Starting scheduled snapshot creation for {}", node_name);

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
                        info!("Scheduled snapshot completed for {}", node_name);
                        let mut completed_operation = operation;
                        completed_operation.status = "completed".to_string();
                        completed_operation.completed_at = Some(Utc::now());

                        if let Err(e) = database.store_maintenance_operation(&completed_operation).await {
                            error!("Failed to update operation status: {}", e);
                        }
                    }
                    Err(e) => {
                        error!("Scheduled snapshot failed for {}: {}", node_name, e);
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
        })?;

        self.scheduler.add(job).await?;
        info!("Scheduled snapshot job for {}: {}", node_name, schedule);
        Ok(())
    }

    async fn schedule_hermes_restart_job(&self, hermes_name: String, schedule: String) -> Result<()> {
        let http_manager = self.http_manager.clone();
        let config = self.config.clone();
        let database = self.database.clone();
        let hermes_name_clone = hermes_name.clone();

        let job = Job::new_async(schedule.as_str(), move |_uuid, _scheduler| {
            let http_manager = http_manager.clone();
            let config = config.clone();
            let database = database.clone();
            let hermes_name = hermes_name_clone.clone();

            Box::pin(async move {
                info!("Starting scheduled Hermes restart for {}", hermes_name);

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

                if let Some(hermes_config) = config.hermes.get(&hermes_name) {
                    match http_manager.restart_hermes(hermes_config).await {
                        Ok(_) => {
                            info!("Scheduled Hermes restart completed for {}", hermes_name);
                            let mut completed_operation = operation;
                            completed_operation.status = "completed".to_string();
                            completed_operation.completed_at = Some(Utc::now());

                            if let Err(e) = database.store_maintenance_operation(&completed_operation).await {
                                error!("Failed to update operation status: {}", e);
                            }
                        }
                        Err(e) => {
                            error!("Scheduled Hermes restart failed for {}: {}", hermes_name, e);
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
        })?;

        self.scheduler.add(job).await?;
        info!("Scheduled Hermes restart job for {}: {}", hermes_name, schedule);
        Ok(())
    }
}
