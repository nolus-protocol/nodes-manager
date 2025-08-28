// File: manager/src/services/hermes_service.rs
use anyhow::Result;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

use crate::maintenance_tracker::MaintenanceTracker;
use crate::http::HttpAgentManager; // CHANGED: Use HTTP agent instead of SSH
use crate::{Config, HermesConfig};

pub struct HermesService {
    config: Arc<Config>,
    http_manager: Arc<HttpAgentManager>, // CHANGED: HTTP agent manager instead of SSH
    maintenance_tracker: Arc<MaintenanceTracker>,
}

impl HermesService {
    pub fn new(
        config: Arc<Config>,
        http_manager: Arc<HttpAgentManager>, // CHANGED: HTTP agent manager parameter
        maintenance_tracker: Arc<MaintenanceTracker>,
    ) -> Self {
        Self {
            config,
            http_manager, // CHANGED: Use HTTP agent manager
            maintenance_tracker,
        }
    }

    pub async fn get_all_instances(&self) -> Result<Vec<crate::web::HermesInstance>> {
        let mut instances = Vec::with_capacity(self.config.hermes.len());

        for (hermes_name, hermes_config) in &self.config.hermes {
            let instance = self.get_instance_info(hermes_name, hermes_config).await;
            instances.push(instance);
        }

        Ok(instances)
    }

    pub async fn get_instance(&self, hermes_name: &str) -> Result<Option<crate::web::HermesInstance>> {
        self.validate_hermes_name(hermes_name)?;

        let hermes_config = self.config.hermes.get(hermes_name).unwrap();
        let instance = self.get_instance_info(hermes_name, hermes_config).await;
        Ok(Some(instance))
    }

    pub async fn restart_instance(&self, hermes_name: &str) -> Result<String> {
        self.validate_hermes_name(hermes_name)?;

        let hermes_config = self.config.hermes.get(hermes_name).unwrap().clone();

        info!("Restarting Hermes instance via HTTP agent: {}", hermes_name);
        self.http_manager.restart_hermes(&hermes_config).await?; // CHANGED: Use HTTP agent

        Ok(format!("Hermes instance {} restarted successfully via HTTP agent", hermes_name))
    }

    pub async fn restart_all_instances(&self) -> Result<serde_json::Value> {
        let hermes_configs: Vec<HermesConfig> = self.config.hermes.values().cloned().collect();

        info!("Restarting all {} Hermes instances via HTTP agents", hermes_configs.len());

        let result = self.http_manager.restart_multiple_hermes(hermes_configs).await?; // CHANGED: Use HTTP agent

        Ok(json!({
            "total_instances": result.results.len(),
            "successful": result.success_count,
            "failed": result.failure_count,
            "results": result.results,
            "summary": result.summary,
            "connection_type": "http_agent"
        }))
    }

    // OPTIMIZED: Inline validation for performance
    #[inline]
    fn validate_hermes_name(&self, hermes_name: &str) -> Result<()> {
        if self.config.hermes.contains_key(hermes_name) {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Hermes instance '{}' not found", hermes_name))
        }
    }

    async fn get_instance_info(&self, hermes_name: &str, hermes_config: &HermesConfig) -> crate::web::HermesInstance {
        // Check if instance is in maintenance
        let in_maintenance = self.maintenance_tracker.is_in_maintenance(hermes_name).await;

        // Get service status via HTTP agent
        let status = match self.http_manager.check_service_status(&hermes_config.server_host, &hermes_config.service_name).await {
            Ok(service_status) => format!("{:?}", service_status),
            Err(_) => "Unknown".to_string(),
        };

        // Get uptime via HTTP agent
        let uptime_formatted = match self.http_manager.get_service_uptime(&hermes_config.server_host, &hermes_config.service_name).await {
            Ok(Some(uptime)) => format_duration(uptime),
            Ok(None) => "Unknown".to_string(),
            Err(_) => "Error".to_string(),
        };

        // Get dependent nodes with display names
        let dependent_nodes = hermes_config.dependent_nodes.clone().unwrap_or_default();

        crate::web::HermesInstance {
            name: hermes_name.to_string(),
            server_host: hermes_config.server_host.clone(),
            service_name: hermes_config.service_name.clone(),
            status,
            uptime_formatted: Some(uptime_formatted),
            dependent_nodes,
            in_maintenance,
        }
    }

    // NEW: Get service statistics for monitoring with HTTP agent
    pub async fn get_service_statistics(&self) -> Result<serde_json::Value> {
        let total_instances = self.config.hermes.len();
        let mut active_instances = 0;
        let mut healthy_dependencies = 0;

        for (_hermes_name, hermes_config) in &self.config.hermes {
            // Check if service is active via HTTP agent
            if let Ok(status) = self.http_manager.check_service_status(&hermes_config.server_host, &hermes_config.service_name).await {
                if status.is_running() {
                    active_instances += 1;
                }
            }

            // Check dependencies via HTTP agent
            if self.http_manager.check_node_dependencies(&hermes_config.dependent_nodes).await.unwrap_or(false) {
                healthy_dependencies += 1;
            }
        }

        Ok(json!({
            "total_instances": total_instances,
            "active_instances": active_instances,
            "instances_with_healthy_dependencies": healthy_dependencies,
            "service_health_percentage": if total_instances > 0 {
                (active_instances as f64 / total_instances as f64 * 100.0) as u32
            } else {
                0
            },
            "dependency_health_percentage": if total_instances > 0 {
                (healthy_dependencies as f64 / total_instances as f64 * 100.0) as u32
            } else {
                0
            },
            "connection_type": "http_agent",
            "timestamp": chrono::Utc::now().to_rfc3339()
        }))
    }
}

// Helper function to format duration in human-readable form
fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

impl Clone for HermesService {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            http_manager: self.http_manager.clone(), // CHANGED: HTTP agent manager
            maintenance_tracker: self.maintenance_tracker.clone(),
        }
    }
}
