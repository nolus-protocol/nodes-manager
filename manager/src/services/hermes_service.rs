// File: manager/src/services/hermes_service.rs
use crate::config::Config;
use crate::http::HttpAgentManager;
use crate::services::alert_service::AlertService;
use anyhow::Result;
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

        // Alert: Hermes restart started
        if let Err(e) = self
            .alert_service
            .alert_hermes_started(hermes_name, &hermes_config.server_host)
            .await
        {
            error!("Failed to send Hermes start alert: {}", e);
        }

        // Execute restart
        match self.http_manager.restart_hermes(hermes_config).await {
            Ok(_) => {
                info!("Hermes restart completed successfully for {}", hermes_name);

                // Alert: Hermes restart completed
                if let Err(e) = self
                    .alert_service
                    .alert_hermes_completed(hermes_name, &hermes_config.server_host)
                    .await
                {
                    error!("Failed to send Hermes completion alert: {}", e);
                }

                Ok(format!("Hermes {} restarted successfully", hermes_name))
            }
            Err(e) => {
                error!("Hermes restart failed for {}: {}", hermes_name, e);

                // Alert: Hermes restart failed
                if let Err(alert_err) = self
                    .alert_service
                    .alert_hermes_failed(hermes_name, &hermes_config.server_host, &e.to_string())
                    .await
                {
                    error!("Failed to send Hermes restart failure alert: {}", alert_err);
                }

                Err(e)
            }
        }
    }
}
