// File: src/ssh/operations.rs

use chrono::Utc;
use serde::Serialize;
use std::collections::HashMap;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::ssh::SshManager;
use crate::{HermesConfig, NodeConfig};

#[derive(Debug, Clone)]
pub struct BatchOperationResult {
    pub operation_id: String,
    pub total_operations: usize,
    pub successful: usize,
    pub failed: usize,
    pub results: Vec<OperationResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationResult {
    pub target_name: String,
    pub success: bool,
    pub error_message: Option<String>,
    pub duration_seconds: u64,
}

impl SshManager {
    /// SIMPLIFIED: Sequential batch pruning (no complex parallelization)
    pub async fn prune_multiple_nodes(&self, nodes: Vec<NodeConfig>) -> BatchOperationResult {
        let operation_id = Uuid::new_v4().to_string();
        let start_time = Utc::now();
        let total_nodes = nodes.len(); // Capture length before moving

        info!(
            "Starting sequential batch pruning operation {} for {} nodes",
            operation_id,
            total_nodes
        );

        let mut results = Vec::new();

        // Process nodes sequentially - simple and reliable
        for node in nodes {
            let node_start_time = Utc::now();
            let node_id = self.find_node_config_key(&node).await
                .unwrap_or_else(|| format!("{}-{}", node.server_host, node.network));

            info!("Processing node {} in batch operation", node_id);

            match self.run_pruning(&node).await {
                Ok(_) => {
                    let duration = Utc::now().signed_duration_since(node_start_time);
                    info!("Batch pruning completed for node {} in {}s", node_id, duration.num_seconds());

                    results.push(OperationResult {
                        target_name: node_id,
                        success: true,
                        error_message: None,
                        duration_seconds: duration.num_seconds() as u64,
                    });
                }
                Err(e) => {
                    let duration = Utc::now().signed_duration_since(node_start_time);
                    error!("Batch pruning failed for node {}: {}", node_id, e);

                    results.push(OperationResult {
                        target_name: node_id,
                        success: false,
                        error_message: Some(e.to_string()),
                        duration_seconds: duration.num_seconds() as u64,
                    });
                }
            }
        }

        let successful = results.iter().filter(|r| r.success).count();
        let failed = results.len() - successful;
        let total_duration = Utc::now().signed_duration_since(start_time);

        let result = BatchOperationResult {
            operation_id: operation_id.clone(),
            total_operations: total_nodes,
            successful,
            failed,
            results,
        };

        info!(
            "Sequential batch pruning operation {} completed in {}s: {}/{} successful",
            operation_id,
            total_duration.num_seconds(),
            successful,
            total_nodes
        );

        result
    }

    /// SIMPLIFIED: Sequential batch Hermes restart (no complex parallelization)
    pub async fn restart_multiple_hermes(&self, instances: Vec<HermesConfig>) -> BatchOperationResult {
        let operation_id = Uuid::new_v4().to_string();
        let start_time = Utc::now();
        let total_instances = instances.len(); // Capture length before moving

        info!(
            "Starting sequential batch Hermes restart operation {} for {} instances",
            operation_id,
            total_instances
        );

        let mut results = Vec::new();

        // Process Hermes instances sequentially - simple and reliable
        for hermes in instances {
            let hermes_start_time = Utc::now();
            let hermes_id = self.find_hermes_config_key(&hermes).await
                .unwrap_or_else(|| format!("{}-{}", hermes.server_host, hermes.service_name));

            info!("Processing Hermes {} in batch operation", hermes_id);

            match self.restart_hermes(&hermes).await {
                Ok(_) => {
                    let duration = Utc::now().signed_duration_since(hermes_start_time);
                    info!("Batch Hermes restart completed for {} in {}s", hermes_id, duration.num_seconds());

                    results.push(OperationResult {
                        target_name: hermes_id,
                        success: true,
                        error_message: None,
                        duration_seconds: duration.num_seconds() as u64,
                    });
                }
                Err(e) => {
                    let duration = Utc::now().signed_duration_since(hermes_start_time);
                    error!("Batch Hermes restart failed for {}: {}", hermes_id, e);

                    results.push(OperationResult {
                        target_name: hermes_id,
                        success: false,
                        error_message: Some(e.to_string()),
                        duration_seconds: duration.num_seconds() as u64,
                    });
                }
            }
        }

        let successful = results.iter().filter(|r| r.success).count();
        let failed = results.len() - successful;
        let total_duration = Utc::now().signed_duration_since(start_time);

        let result = BatchOperationResult {
            operation_id: operation_id.clone(),
            total_operations: total_instances,
            successful,
            failed,
            results,
        };

        info!(
            "Sequential batch Hermes restart operation {} completed in {}s: {}/{} successful",
            operation_id,
            total_duration.num_seconds(),
            successful,
            total_instances
        );

        result
    }

    /// Helper method to find config key for a HermesConfig
    pub async fn find_hermes_config_key(&self, target_hermes: &HermesConfig) -> Option<String> {
        for (config_key, hermes_config) in &self.config.hermes {
            if hermes_config.server_host == target_hermes.server_host
                && hermes_config.service_name == target_hermes.service_name {
                return Some(config_key.clone());
            }
        }
        None
    }

    /// SIMPLIFIED: Validate server connectivity using simple SSH commands
    #[allow(dead_code)]
    pub async fn validate_hermes_dependencies(&self) -> HashMap<String, bool> {
        info!("Validating dependencies for all Hermes instances using simple SSH commands");

        let mut dependency_status = HashMap::new();

        for (hermes_name, hermes_config) in &self.config.hermes {
            let dependencies_healthy = self
                .check_node_dependencies(&hermes_config.dependent_nodes)
                .await
                .unwrap_or(false);

            dependency_status.insert(hermes_name.clone(), dependencies_healthy);

            if dependencies_healthy {
                info!("All dependencies healthy for Hermes: {}", hermes_name);
            } else {
                warn!("Some dependencies unhealthy for Hermes: {}", hermes_name);
            }
        }

        dependency_status
    }

    /// SIMPLIFIED: Get detailed dependency health report using simple SSH commands
    #[allow(dead_code)]
    pub async fn get_hermes_dependency_report(&self) -> HashMap<String, serde_json::Value> {
        info!("Generating detailed dependency report for all Hermes instances using simple SSH commands");

        let mut report = HashMap::new();

        for (hermes_name, hermes_config) in &self.config.hermes {
            let mut dependency_details = Vec::new();

            for node_name in &hermes_config.dependent_nodes {
                let node_config = self.config.nodes.get(node_name);

                let status = match node_config {
                    Some(config) => {
                        if !config.enabled {
                            "disabled".to_string()
                        } else {
                            match self.quick_node_health_check(node_name, config).await {
                                Ok(true) => "healthy".to_string(),
                                Ok(false) => "unhealthy".to_string(),
                                Err(e) => format!("error: {}", e),
                            }
                        }
                    }
                    None => "not_found".to_string(),
                };

                dependency_details.push(serde_json::json!({
                    "node_name": node_name,
                    "status": status
                }));
            }

            let all_healthy = dependency_details.iter()
                .all(|detail| detail["status"] == "healthy");

            report.insert(hermes_name.clone(), serde_json::json!({
                "hermes_name": hermes_name,
                "server_host": hermes_config.server_host,
                "service_name": hermes_config.service_name,
                "all_dependencies_healthy": all_healthy,
                "dependency_count": hermes_config.dependent_nodes.len(),
                "dependencies": dependency_details
            }));
        }

        report
    }
}
