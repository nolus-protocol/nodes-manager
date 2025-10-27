// File: manager/src/services/operation_executor.rs
//
// Generic operation executor for background tasks with integrated alerting and tracking
//
use crate::config::Config;
use crate::database::{Database, MaintenanceOperation};
use crate::services::alert_service::AlertService;
use anyhow::Result;
use chrono::Utc;
use std::future::Future;
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

#[derive(Clone)]
pub struct OperationExecutor {
    config: Arc<Config>,
    database: Arc<Database>,
    alert_service: Arc<AlertService>,
}

impl OperationExecutor {
    pub fn new(
        config: Arc<Config>,
        database: Arc<Database>,
        alert_service: Arc<AlertService>,
    ) -> Self {
        Self {
            config,
            database,
            alert_service,
        }
    }

    /// Execute an async operation in the background with full tracking and alerting
    ///
    /// # Arguments
    /// * `operation_type` - Type of operation (e.g., "pruning", "snapshot_creation", "state_sync")
    /// * `target_name` - Name of the target node/service
    /// * `operation_fn` - Closure that returns a Future executing the actual operation
    ///
    /// # Returns
    /// * `Ok(operation_id)` - Unique ID for tracking the operation
    /// * `Err(...)` - If operation failed to start (validation, database errors, etc.)
    ///
    /// # Alerting Behavior
    /// Emits ALL events (start, success, failure) - complete event stream.
    /// Filtering happens downstream in n8n, NOT here.
    pub async fn execute_async<F, Fut>(
        &self,
        operation_type: &str,
        target_name: &str,
        operation_fn: F,
    ) -> Result<String>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        let operation_id = Uuid::new_v4().to_string();
        info!(
            "Starting {} for {} (operation_id: {})",
            operation_type, target_name, operation_id
        );

        // Get server host for alerts
        let server_host = self
            .config
            .nodes
            .get(target_name)
            .map(|n| n.server_host.clone())
            .or_else(|| {
                self.config
                    .hermes
                    .get(target_name)
                    .map(|h| h.server_host.clone())
            })
            .unwrap_or_else(|| "unknown".to_string());

        // Record operation start in database
        let operation_record = MaintenanceOperation {
            id: operation_id.clone(),
            operation_type: operation_type.to_string(),
            target_name: target_name.to_string(),
            status: "started".to_string(),
            started_at: Utc::now(),
            completed_at: None,
            error_message: None,
            details: None,
        };

        self.database
            .store_maintenance_operation(&operation_record)
            .await?;

        // Alert: operation started
        if let Err(e) = self
            .alert_service
            .alert_operation_started(operation_type, target_name, &server_host)
            .await
        {
            error!("Failed to send start alert: {}", e);
        }

        // Clone resources for background task
        let operation_id_clone = operation_id.clone();
        let target_name_owned = target_name.to_string();
        let operation_type_owned = operation_type.to_string();
        let database = self.database.clone();
        let alert_service = self.alert_service.clone();
        let server_host_clone = server_host.clone();

        // Spawn operation in background
        tokio::spawn(async move {
            let result = operation_fn().await;

            match result {
                Ok(_) => {
                    // Update database status
                    if let Err(e) = Self::update_operation_status(
                        &database,
                        &operation_id_clone,
                        "completed",
                        None,
                    )
                    .await
                    {
                        error!("Failed to update operation status: {}", e);
                    }

                    // Alert: operation completed successfully
                    if let Err(e) = alert_service
                        .alert_operation_completed(
                            &operation_type_owned,
                            &target_name_owned,
                            &server_host_clone,
                        )
                        .await
                    {
                        error!("Failed to send completion alert: {}", e);
                    }

                    info!(
                        "{} completed successfully for {} (operation_id: {})",
                        operation_type_owned, target_name_owned, operation_id_clone
                    );
                }
                Err(e) => {
                    // Update database status
                    if let Err(update_err) = Self::update_operation_status(
                        &database,
                        &operation_id_clone,
                        "failed",
                        Some(e.to_string()),
                    )
                    .await
                    {
                        error!("Failed to update operation status: {}", update_err);
                    }

                    // Alert: operation failed
                    if let Err(alert_err) = alert_service
                        .alert_operation_failed(
                            &operation_type_owned,
                            &target_name_owned,
                            &server_host_clone,
                            &e.to_string(),
                        )
                        .await
                    {
                        error!("Failed to send failure alert: {}", alert_err);
                    }

                    error!(
                        "{} failed for {} (operation_id: {}): {}",
                        operation_type_owned, target_name_owned, operation_id_clone, e
                    );
                }
            }
        });

        // Return operation ID immediately
        Ok(operation_id)
    }

    /// Update operation status in database
    async fn update_operation_status(
        database: &Arc<Database>,
        operation_id: &str,
        status: &str,
        error_message: Option<String>,
    ) -> Result<()> {
        let operations = database.get_maintenance_operations(Some(100)).await?;

        if let Some(mut operation) = operations.into_iter().find(|op| op.id == operation_id) {
            operation.status = status.to_string();
            operation.completed_at = Some(Utc::now());
            operation.error_message = error_message;

            database.store_maintenance_operation(&operation).await?;
        }

        Ok(())
    }
}
