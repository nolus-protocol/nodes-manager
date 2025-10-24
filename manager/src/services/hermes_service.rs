// File: manager/src/services/hermes_service.rs
use crate::config::Config;
use crate::http::HttpAgentManager;
use crate::services::alert_service::{AlertService, AlertSeverity, AlertType};
use anyhow::Result;
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info};

pub struct HermesService {
    config: Arc<Config>,
    http_manager: Arc<HttpAgentManager>,
    alert_service: Arc<AlertService>,
}

impl HermesService {
    pub fn new(
        config: Arc<Config>,
        http_manager: Arc<HttpAgentManager>,
        alert_service: Arc<AlertService>,
    ) -> Self {
        Self {
            config,
            http_manager,
            alert_service,
        }
    }

    pub async fn restart_instance(&self, hermes_name: &str) -> Result<String> {
        let hermes_config = self
            .config
            .hermes
            .get(hermes_name)
            .ok_or_else(|| anyhow::anyhow!("Hermes {} not found", hermes_name))?;

        info!("Restarting Hermes instance: {}", hermes_name);

        // Send start notification
        if let Err(e) = self
            .alert_service
            .send_immediate_alert(
                AlertType::Hermes,
                AlertSeverity::Info,
                hermes_name,
                &hermes_config.server_host,
                format!("Hermes restart started for {}", hermes_name),
                Some(json!({
                    "operation_type": "hermes_restart",
                    "hermes_name": hermes_name,
                    "status": "started"
                })),
            )
            .await
        {
            error!("Failed to send Hermes restart start alert: {}", e);
        }

        // Execute restart
        match self.http_manager.restart_hermes(hermes_config).await {
            Ok(_) => {
                info!("Hermes restart completed successfully for {}", hermes_name);

                // Send success notification
                if let Err(e) = self
                    .alert_service
                    .send_immediate_alert(
                        AlertType::Hermes,
                        AlertSeverity::Info,
                        hermes_name,
                        &hermes_config.server_host,
                        format!("Hermes restart completed successfully for {}", hermes_name),
                        Some(json!({
                            "operation_type": "hermes_restart",
                            "hermes_name": hermes_name,
                            "status": "completed"
                        })),
                    )
                    .await
                {
                    error!("Failed to send Hermes restart success alert: {}", e);
                }

                Ok(format!("Hermes {} restarted successfully", hermes_name))
            }
            Err(e) => {
                error!("Hermes restart failed for {}: {}", hermes_name, e);

                // Send failure notification
                if let Err(alert_err) = self
                    .alert_service
                    .send_immediate_alert(
                        AlertType::Hermes,
                        AlertSeverity::Critical,
                        hermes_name,
                        &hermes_config.server_host,
                        format!("Hermes restart failed for {}: {}", hermes_name, e),
                        Some(json!({
                            "operation_type": "hermes_restart",
                            "hermes_name": hermes_name,
                            "status": "failed",
                            "error_message": e.to_string()
                        })),
                    )
                    .await
                {
                    error!("Failed to send Hermes restart failure alert: {}", alert_err);
                }

                Err(e)
            }
        }
    }
}
