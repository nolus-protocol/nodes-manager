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
    /// Sequential batch pruning with isolated SSH connections
    /// Each node operation uses its own independent connection
    pub async fn prune_multiple_nodes(&self, nodes: Vec<NodeConfig>) -> BatchOperationResult {
        let operation_id = Uuid::new_v4().to_string();
        let start_time = Utc::now();
        let total_nodes = nodes.len();

        info!(
            "Starting sequential batch pruning operation {} for {} nodes (each with independent SSH connection)",
            operation_id,
            total_nodes
        );

        let mut results = Vec::new();

        // Process nodes sequentially - each gets its own isolated connection set
        for node in nodes {
            let node_start_time = Utc::now();
            let node_id = self.find_node_config_key(&node).await
                .unwrap_or_else(|| format!("{}-{}", node.server_host, node.network));

            info!("Processing node {} in batch operation (independent connection)", node_id);

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
            "Sequential batch pruning operation {} completed in {}s: {}/{} successful (all with isolated connections)",
            operation_id,
            total_duration.num_seconds(),
            successful,
            total_nodes
        );

        result
    }

    /// Sequential batch Hermes restart with isolated SSH connections
    /// Each Hermes operation uses its own independent connection
    pub async fn restart_multiple_hermes(&self, instances: Vec<HermesConfig>) -> BatchOperationResult {
        let operation_id = Uuid::new_v4().to_string();
        let start_time = Utc::now();
        let total_instances = instances.len();

        info!(
            "Starting sequential batch Hermes restart operation {} for {} instances (each with independent SSH connection)",
            operation_id,
            total_instances
        );

        let mut results = Vec::new();

        // Process Hermes instances sequentially - each gets its own isolated connection set
        for hermes in instances {
            let hermes_start_time = Utc::now();
            let hermes_id = self.find_hermes_config_key(&hermes).await
                .unwrap_or_else(|| format!("{}-{}", hermes.server_host, hermes.service_name));

            info!("Processing Hermes {} in batch operation (independent connection)", hermes_id);

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
            "Sequential batch Hermes restart operation {} completed in {}s: {}/{} successful (all with isolated connections)",
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

    /// Validate server connectivity using isolated SSH connections
    /// Each server test uses its own independent connection
    pub async fn validate_hermes_dependencies(&self) -> HashMap<String, bool> {
        info!("Validating dependencies for all Hermes instances using isolated SSH connections");

        let mut dependency_status = HashMap::new();

        for (hermes_name, hermes_config) in &self.config.hermes {
            info!("Checking dependencies for Hermes {} using independent connections", hermes_name);

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

    /// Get detailed dependency health report using isolated SSH connections
    /// Each dependency check uses its own independent connection
    pub async fn get_hermes_dependency_report(&self) -> HashMap<String, serde_json::Value> {
        info!("Generating detailed dependency report for all Hermes instances using isolated SSH connections");

        let mut report = HashMap::new();

        for (hermes_name, hermes_config) in &self.config.hermes {
            let mut dependency_details = Vec::new();

            info!("Checking dependencies for Hermes {} with independent connections per node", hermes_name);

            for node_name in &hermes_config.dependent_nodes {
                let node_config = self.config.nodes.get(node_name);

                let status = match node_config {
                    Some(config) => {
                        if !config.enabled {
                            "disabled".to_string()
                        } else {
                            // Each health check uses its own independent connection
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
                "dependencies": dependency_details,
                "connection_model": "isolated_per_check"
            }));
        }

        info!("Completed dependency report generation using {} isolated connections",
              report.values().map(|v| v["dependency_count"].as_u64().unwrap_or(0)).sum::<u64>());

        report
    }

    /// Enhanced connectivity validation with detailed connection testing
    /// Each server gets multiple independent connection tests
    pub async fn validate_all_server_connectivity_detailed(&self) -> HashMap<String, serde_json::Value> {
        info!("Running detailed connectivity validation with multiple independent connections per server");

        let mut detailed_status = HashMap::new();

        for server_name in self.config.servers.keys() {
            info!("Testing connectivity to server {} with multiple independent connections", server_name);

            let mut test_results = Vec::new();
            let test_start = Utc::now();

            // Test 1: Basic echo test
            let echo_result = match self.execute_single_command(server_name, "echo 'connectivity_test_1'").await {
                Ok(output) => {
                    if output.trim() == "connectivity_test_1" {
                        ("echo_test", true, "Success".to_string())
                    } else {
                        ("echo_test", false, format!("Unexpected response: {}", output))
                    }
                }
                Err(e) => ("echo_test", false, e.to_string()),
            };
            test_results.push(echo_result);

            // Test 2: System info test (independent connection)
            let system_result = match self.execute_single_command(server_name, "uname -a").await {
                Ok(output) => {
                    if !output.trim().is_empty() {
                        ("system_info", true, format!("Got system info: {}", output.len()))
                    } else {
                        ("system_info", false, "Empty response".to_string())
                    }
                }
                Err(e) => ("system_info", false, e.to_string()),
            };
            test_results.push(system_result);

            // Test 3: Date test (independent connection)
            let date_result = match self.execute_single_command(server_name, "date").await {
                Ok(output) => {
                    if !output.trim().is_empty() {
                        ("date_test", true, "Date command successful".to_string())
                    } else {
                        ("date_test", false, "Empty date response".to_string())
                    }
                }
                Err(e) => ("date_test", false, e.to_string()),
            };
            test_results.push(date_result);

            let test_duration = Utc::now().signed_duration_since(test_start);
            let successful_tests = test_results.iter().filter(|(_, success, _)| *success).count();
            let total_tests = test_results.len();

            detailed_status.insert(server_name.clone(), serde_json::json!({
                "server_name": server_name,
                "overall_healthy": successful_tests == total_tests,
                "successful_tests": successful_tests,
                "total_tests": total_tests,
                "test_duration_ms": test_duration.num_milliseconds(),
                "connection_model": "independent_per_test",
                "test_results": test_results.into_iter().map(|(test_name, success, message)| {
                    serde_json::json!({
                        "test": test_name,
                        "success": success,
                        "message": message
                    })
                }).collect::<Vec<_>>(),
                "tested_at": Utc::now().to_rfc3339()
            }));

            info!("Server {} connectivity test completed: {}/{} tests passed in {}ms using independent connections",
                  server_name, successful_tests, total_tests, test_duration.num_milliseconds());
        }

        detailed_status
    }
}
