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

        match operation_type {
            "pruning" => {
                match self.http_manager.execute_node_pruning(target_name).await {
                    Ok(_) => {
                        self.update_operation_status(&operation_id, "completed", None)
                            .await?;

                        // Send success notification
                        if let Err(e) = self
                            .alert_service
                            .send_immediate_alert(
                                AlertType::Maintenance,
                                AlertSeverity::Info,
                                target_name,
                                &server_host,
                                format!("Pruning completed successfully for {}", target_name),
                                Some(json!({
                                    "operation_id": operation_id,
                                    "operation_type": "pruning",
                                    "status": "completed"
                                })),
                            )
                            .await
                        {
                            error!("Failed to send completion alert: {}", e);
                        }

                        Ok(operation_id.clone())
                    }
                    Err(e) => {
                        self.update_operation_status(&operation_id, "failed", Some(e.to_string()))
                            .await?;

                        // Send failure notification
                        if let Err(alert_err) = self
                            .alert_service
                            .send_immediate_alert(
                                AlertType::Maintenance,
                                AlertSeverity::Critical,
                                target_name,
                                &server_host,
                                format!("Pruning failed for {}: {}", target_name, e),
                                Some(json!({
                                    "operation_id": operation_id,
                                    "operation_type": "pruning",
                                    "status": "failed",
                                    "error_message": e.to_string()
                                })),
                            )
                            .await
                        {
                            error!("Failed to send failure alert: {}", alert_err);
                        }

                        Err(e)
                    }
                }
            }
            "snapshot_creation" => {
                match self.http_manager.create_node_snapshot(target_name).await {
                    Ok(_) => {
                        self.update_operation_status(&operation_id, "completed", None)
                            .await?;

                        // Send success notification
                        if let Err(e) = self
                            .alert_service
                            .send_immediate_alert(
                                AlertType::Maintenance,
                                AlertSeverity::Info,
                                target_name,
                                &server_host,
                                format!(
                                    "Snapshot creation completed successfully for {}",
                                    target_name
                                ),
                                Some(json!({
                                    "operation_id": operation_id,
                                    "operation_type": "snapshot_creation",
                                    "status": "completed"
                                })),
                            )
                            .await
                        {
                            error!("Failed to send completion alert: {}", e);
                        }

                        Ok(operation_id.clone())
                    }
                    Err(e) => {
                        self.update_operation_status(&operation_id, "failed", Some(e.to_string()))
                            .await?;

                        // Send failure notification
                        if let Err(alert_err) = self
                            .alert_service
                            .send_immediate_alert(
                                AlertType::Maintenance,
                                AlertSeverity::Critical,
                                target_name,
                                &server_host,
                                format!("Snapshot creation failed for {}: {}", target_name, e),
                                Some(json!({
                                    "operation_id": operation_id,
                                    "operation_type": "snapshot_creation",
                                    "status": "failed",
                                    "error_message": e.to_string()
                                })),
                            )
                            .await
                        {
                            error!("Failed to send failure alert: {}", alert_err);
                        }

                        Err(e)
                    }
                }
            }
            "node_restart" => {
                match self.http_manager.restart_node(target_name).await {
                    Ok(_) => {
                        self.update_operation_status(&operation_id, "completed", None)
                            .await?;

                        // Send success notification
                        if let Err(e) = self
                            .alert_service
                            .send_immediate_alert(
                                AlertType::Maintenance,
                                AlertSeverity::Info,
                                target_name,
                                &server_host,
                                format!("Node restart completed successfully for {}", target_name),
                                Some(json!({
                                    "operation_id": operation_id,
                                    "operation_type": "node_restart",
                                    "status": "completed"
                                })),
                            )
                            .await
                        {
                            error!("Failed to send completion alert: {}", e);
                        }

                        Ok(operation_id.clone())
                    }
                    Err(e) => {
                        self.update_operation_status(&operation_id, "failed", Some(e.to_string()))
                            .await?;

                        // Send failure notification
                        if let Err(alert_err) = self
                            .alert_service
                            .send_immediate_alert(
                                AlertType::Maintenance,
                                AlertSeverity::Critical,
                                target_name,
                                &server_host,
                                format!("Node restart failed for {}: {}", target_name, e),
                                Some(json!({
                                    "operation_id": operation_id,
                                    "operation_type": "node_restart",
                                    "status": "failed",
                                    "error_message": e.to_string()
                                })),
                            )
                            .await
                        {
                            error!("Failed to send failure alert: {}", alert_err);
                        }

                        Err(e)
                    }
                }
            }
            _ => Err(anyhow::anyhow!(
                "Unknown operation type: {}",
                operation_type
            )),
        }
    }

    async fn update_operation_status(
        &self,
        operation_id: &str,
        status: &str,
        error_message: Option<String>,
    ) -> Result<()> {
        let operations = self.database.get_maintenance_operations(Some(100)).await?;

        if let Some(mut operation) = operations.into_iter().find(|op| op.id == operation_id) {
            operation.status = status.to_string();
            operation.completed_at = Some(Utc::now());
            operation.error_message = error_message;

            self.database
                .store_maintenance_operation(&operation)
                .await?;
        }

        Ok(())
    }
}
