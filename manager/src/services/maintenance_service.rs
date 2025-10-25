// File: manager/src/services/maintenance_service.rs
use crate::config::Config;
use crate::database::{Database, MaintenanceOperation};
use crate::http::HttpAgentManager;
use crate::services::alert_service::{AlertService, AlertSeverity, AlertType};
use anyhow::Result;
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

pub struct MaintenanceService {
    config: Arc<Config>,
    database: Arc<Database>,
    http_manager: Arc<HttpAgentManager>,
    alert_service: Arc<AlertService>,
}

impl MaintenanceService {
    pub fn new(
        config: Arc<Config>,
        database: Arc<Database>,
        http_manager: Arc<HttpAgentManager>,
        alert_service: Arc<AlertService>,
    ) -> Self {
        Self {
            config,
            database,
            http_manager,
            alert_service,
        }
    }

    pub async fn execute_immediate_operation(
        &self,
        operation_type: &str,
        target_name: &str,
    ) -> Result<String> {
        let operation_id = Uuid::new_v4().to_string();
        info!("Starting {} for {}", operation_type, target_name);

        // Get server host for alerts
        let server_host = self
            .config
            .nodes
            .get(target_name)
            .map(|n| n.server_host.clone())
            .unwrap_or_else(|| "unknown".to_string());

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

        self.database
            .store_maintenance_operation(&operation)
            .await?;

        // Send start notification
        if let Err(e) = self
            .alert_service
            .send_immediate_alert(
                AlertType::Maintenance,
                AlertSeverity::Info,
                target_name,
                &server_host,
                format!(
                    "Manual {} operation started for {}",
                    operation_type, target_name
                ),
                Some(json!({
                    "operation_id": operation_id,
                    "operation_type": operation_type,
                    "status": "started"
                })),
            )
            .await
        {
            error!("Failed to send start alert: {}", e);
        }

        // Spawn operation in background to avoid HTTP timeout issues
        let operation_id_clone = operation_id.clone();
        let target_name_owned = target_name.to_string();
        let operation_type_owned = operation_type.to_string();
        let http_manager = self.http_manager.clone();
        let database = self.database.clone();
        let alert_service = self.alert_service.clone();
        let server_host_clone = server_host.clone();

        tokio::spawn(async move {
            let result = match operation_type_owned.as_str() {
                "pruning" => http_manager.execute_node_pruning(&target_name_owned).await,
                "snapshot_creation" => http_manager.create_node_snapshot(&target_name_owned).await.map(|_| ()),
                "node_restart" => http_manager.restart_node(&target_name_owned).await,
                _ => Err(anyhow::anyhow!("Unknown operation type: {}", operation_type_owned)),
            };

            match result {
                Ok(_) => {
                    // Update database status
                    if let Err(e) = Self::update_operation_status_static(
                        &database,
                        &operation_id_clone,
                        "completed",
                        None,
                    )
                    .await
                    {
                        error!("Failed to update operation status: {}", e);
                    }

                    // Send success notification
                    if let Err(e) = alert_service
                        .send_immediate_alert(
                            AlertType::Maintenance,
                            AlertSeverity::Info,
                            &target_name_owned,
                            &server_host_clone,
                            format!(
                                "{} completed successfully for {}",
                                operation_type_owned, target_name_owned
                            ),
                            Some(json!({
                                "operation_id": operation_id_clone,
                                "operation_type": operation_type_owned,
                                "status": "completed"
                            })),
                        )
                        .await
                    {
                        error!("Failed to send completion alert: {}", e);
                    }

                    info!("{} completed successfully for {}", operation_type_owned, target_name_owned);
                }
                Err(e) => {
                    // Update database status
                    if let Err(update_err) = Self::update_operation_status_static(
                        &database,
                        &operation_id_clone,
                        "failed",
                        Some(e.to_string()),
                    )
                    .await
                    {
                        error!("Failed to update operation status: {}", update_err);
                    }

                    // Send failure notification
                    if let Err(alert_err) = alert_service
                        .send_immediate_alert(
                            AlertType::Maintenance,
                            AlertSeverity::Critical,
                            &target_name_owned,
                            &server_host_clone,
                            format!("{} failed for {}: {}", operation_type_owned, target_name_owned, e),
                            Some(json!({
                                "operation_id": operation_id_clone,
                                "operation_type": operation_type_owned,
                                "status": "failed",
                                "error_message": e.to_string()
                            })),
                        )
                        .await
                    {
                        error!("Failed to send failure alert: {}", alert_err);
                    }

                    error!("{} failed for {}: {}", operation_type_owned, target_name_owned, e);
                }
            }
        });

        // Return immediately with operation ID
        Ok(operation_id)
    }

    // Static version for use in spawned tasks
    async fn update_operation_status_static(
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

            database
                .store_maintenance_operation(&operation)
                .await?;
        }

        Ok(())
    }
}
