// File: manager/src/services/state_sync_service.rs
use crate::config::Config;
use crate::http::HttpAgentManager;
use crate::services::alert_service::{AlertService, AlertSeverity, AlertType};
use anyhow::Result;
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info};

pub struct StateSyncService {
    config: Arc<Config>,
    http_manager: Arc<HttpAgentManager>,
    alert_service: Arc<AlertService>,
}

impl StateSyncService {
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

    pub async fn execute_state_sync(&self, node_name: &str) -> Result<()> {
        // Get node configuration
        let node_config = self
            .config
            .nodes
            .get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        if !node_config.state_sync_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!(
                "State sync not enabled for node {}",
                node_name
            ));
        }

        let server_host = &node_config.server_host;

        info!("Starting state sync for {}", node_name);

        // Send start alert
        if let Err(e) = self
            .alert_service
            .send_immediate_alert(
                AlertType::Maintenance,
                AlertSeverity::Info,
                node_name,
                server_host,
                format!("State sync started for {}", node_name),
                Some(json!({
                    "operation_type": "state_sync",
                    "status": "started"
                })),
            )
            .await
        {
            error!("Failed to send state sync start alert: {}", e);
        }

        // Execute state sync via HTTP agent manager
        match self.http_manager.execute_state_sync(node_name).await {
            Ok(_) => {
                info!("State sync completed successfully for {}", node_name);

                // Send success alert
                if let Err(e) = self
                    .alert_service
                    .send_immediate_alert(
                        AlertType::Maintenance,
                        AlertSeverity::Info,
                        node_name,
                        server_host,
                        format!("State sync completed successfully for {}", node_name),
                        Some(json!({
                            "operation_type": "state_sync",
                            "status": "completed"
                        })),
                    )
                    .await
                {
                    error!("Failed to send state sync completion alert: {}", e);
                }

                Ok(())
            }
            Err(e) => {
                error!("State sync failed for {}: {}", node_name, e);

                // Send failure alert
                if let Err(alert_err) = self
                    .alert_service
                    .send_immediate_alert(
                        AlertType::Maintenance,
                        AlertSeverity::Critical,
                        node_name,
                        server_host,
                        format!("State sync failed for {}: {}", node_name, e),
                        Some(json!({
                            "operation_type": "state_sync",
                            "status": "failed",
                            "error_message": e.to_string()
                        })),
                    )
                    .await
                {
                    error!("Failed to send state sync failure alert: {}", alert_err);
                }

                Err(e)
            }
        }
    }
}
