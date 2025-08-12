// File: src/ssh/operations.rs

use anyhow::Result;
use chrono::Utc;
use futures::future::join_all;
use serde::Serialize;
use std::collections::HashMap;
use tracing::{error, info};
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
    pub async fn prune_multiple_nodes(&self, nodes: Vec<NodeConfig>) -> BatchOperationResult {
        let operation_id = Uuid::new_v4().to_string();
        let start_time = Utc::now();

        info!(
            "Starting batch pruning operation {} for {} nodes",
            operation_id,
            nodes.len()
        );

        // Group nodes by server to ensure proper sequencing
        let mut nodes_by_server: HashMap<String, Vec<NodeConfig>> = HashMap::new();
        for node in nodes.iter() {
            nodes_by_server
                .entry(node.server_host.clone())
                .or_insert_with(Vec::new)
                .push(node.clone());
        }

        // Create tasks for each server
        let mut tasks = Vec::new();
        for (server_name, server_nodes) in nodes_by_server {
            let ssh_manager = self.clone();
            let task = tokio::spawn(async move {
                ssh_manager.prune_nodes_on_server(server_name, server_nodes).await
            });
            tasks.push(task);
        }

        // Execute all server tasks in parallel
        let results = join_all(tasks).await;

        // Flatten results
        let mut all_results = Vec::new();
        for task_result in results {
            match task_result {
                Ok(server_results) => all_results.extend(server_results),
                Err(e) => {
                    error!("Batch pruning task failed: {}", e);
                    // Add error results for all nodes that would have been processed
                    // This is handled within prune_nodes_on_server
                }
            }
        }

        let successful = all_results.iter().filter(|r| r.success).count();
        let failed = all_results.len() - successful;

        let result = BatchOperationResult {
            operation_id: operation_id.clone(),
            total_operations: nodes.len(),
            successful,
            failed,
            results: all_results,
        };

        let duration = Utc::now().signed_duration_since(start_time);
        info!(
            "Batch pruning operation {} completed in {}s: {}/{} successful",
            operation_id,
            duration.num_seconds(),
            successful,
            nodes.len()
        );

        result
    }

    async fn prune_nodes_on_server(
        &self,
        server_name: String,
        nodes: Vec<NodeConfig>,
    ) -> Vec<OperationResult> {
        let mut results = Vec::new();

        info!(
            "Pruning {} nodes sequentially on server {}",
            nodes.len(),
            server_name
        );

        // Process nodes sequentially on the same server to avoid conflicts
        for node in nodes {
            let start_time = Utc::now();
            let node_id = format!("{}-{}", server_name, node.network);

            match self.run_pruning(&node).await {
                Ok(_) => {
                    let duration = Utc::now().signed_duration_since(start_time);
                    info!("Pruning completed for node {} in {}s", node_id, duration.num_seconds());

                    results.push(OperationResult {
                        target_name: node_id,
                        success: true,
                        error_message: None,
                        duration_seconds: duration.num_seconds() as u64,
                    });
                }
                Err(e) => {
                    let duration = Utc::now().signed_duration_since(start_time);
                    error!("Pruning failed for node {}: {}", node_id, e);

                    results.push(OperationResult {
                        target_name: node_id,
                        success: false,
                        error_message: Some(e.to_string()),
                        duration_seconds: duration.num_seconds() as u64,
                    });
                }
            }
        }

        results
    }

    pub async fn restart_multiple_hermes(
        &self,
        instances: Vec<HermesConfig>,
    ) -> BatchOperationResult {
        let operation_id = Uuid::new_v4().to_string();
        let start_time = Utc::now();

        info!(
            "Starting batch Hermes restart operation {} for {} instances",
            operation_id,
            instances.len()
        );

        // Group hermes instances by server
        let mut hermes_by_server: HashMap<String, Vec<HermesConfig>> = HashMap::new();
        for hermes in instances.iter() {
            hermes_by_server
                .entry(hermes.server_host.clone())
                .or_insert_with(Vec::new)
                .push(hermes.clone());
        }

        // Create tasks for each server
        let mut tasks = Vec::new();
        for (server_name, server_hermes) in hermes_by_server {
            let ssh_manager = self.clone();
            let task = tokio::spawn(async move {
                ssh_manager.restart_hermes_on_server(server_name, server_hermes).await
            });
            tasks.push(task);
        }

        // Execute all server tasks in parallel
        let results = join_all(tasks).await;

        // Flatten results
        let mut all_results = Vec::new();
        for task_result in results {
            match task_result {
                Ok(server_results) => all_results.extend(server_results),
                Err(e) => {
                    error!("Batch Hermes restart task failed: {}", e);
                }
            }
        }

        let successful = all_results.iter().filter(|r| r.success).count();
        let failed = all_results.len() - successful;

        let result = BatchOperationResult {
            operation_id: operation_id.clone(),
            total_operations: instances.len(),
            successful,
            failed,
            results: all_results,
        };

        let duration = Utc::now().signed_duration_since(start_time);
        info!(
            "Batch Hermes restart operation {} completed in {}s: {}/{} successful",
            operation_id,
            duration.num_seconds(),
            successful,
            instances.len()
        );

        result
    }

    async fn restart_hermes_on_server(
        &self,
        server_name: String,
        hermes_instances: Vec<HermesConfig>,
    ) -> Vec<OperationResult> {
        let mut results = Vec::new();

        info!(
            "Restarting {} Hermes instances sequentially on server {}",
            hermes_instances.len(),
            server_name
        );

        // Process Hermes instances sequentially on the same server
        // Note: Dependency checking is now handled at the scheduler level before calling this method
        for hermes in hermes_instances {
            let start_time = Utc::now();
            let hermes_id = format!("{}-{}", server_name, hermes.service_name);

            match self.restart_hermes(&hermes).await {
                Ok(_) => {
                    let duration = Utc::now().signed_duration_since(start_time);
                    info!("Hermes restart completed for {} in {}s", hermes_id, duration.num_seconds());

                    results.push(OperationResult {
                        target_name: hermes_id,
                        success: true,
                        error_message: None,
                        duration_seconds: duration.num_seconds() as u64,
                    });
                }
                Err(e) => {
                    let duration = Utc::now().signed_duration_since(start_time);
                    error!("Hermes restart failed for {}: {}", hermes_id, e);

                    results.push(OperationResult {
                        target_name: hermes_id,
                        success: false,
                        error_message: Some(e.to_string()),
                        duration_seconds: duration.num_seconds() as u64,
                    });
                }
            }
        }

        results
    }

    pub async fn validate_all_servers_connectivity(&self) -> HashMap<String, Result<String, String>> {
        info!("Validating connectivity to all servers");

        let mut tasks = Vec::new();
        let server_names: Vec<String> = self.config.servers.keys().cloned().collect();

        for server_name in server_names {
            let ssh_manager = self.clone();
            let task = tokio::spawn(async move {
                let result = ssh_manager.execute_command(&server_name, "echo 'connectivity_test'").await;
                (server_name, result)
            });
            tasks.push(task);
        }

        let results = join_all(tasks).await;
        let mut connectivity_status = HashMap::new();

        for task_result in results {
            match task_result {
                Ok((server_name, ssh_result)) => {
                    match ssh_result {
                        Ok(output) => {
                            if output.trim() == "connectivity_test" {
                                connectivity_status.insert(server_name, Ok("Connected".to_string()));
                            } else {
                                connectivity_status.insert(server_name, Err("Unexpected response".to_string()));
                            }
                        }
                        Err(e) => {
                            connectivity_status.insert(server_name, Err(e.to_string()));
                        }
                    }
                }
                Err(e) => {
                    error!("Server connectivity task failed: {}", e);
                }
            }
        }

        connectivity_status
    }

    pub async fn get_all_service_statuses(&self) -> HashMap<String, HashMap<String, String>> {
        info!("Getting status of all services across all servers");

        let mut tasks = Vec::new();

        // Check all node services
        for (node_name, node) in &self.config.nodes {
            if let Some(service_name) = &node.pruning_service_name {
                let ssh_manager = self.clone();
                let server_name = node.server_host.clone();
                let service = service_name.clone();
                let name = node_name.clone();

                let task = tokio::spawn(async move {
                    let status = ssh_manager.check_service_status(&server_name, &service).await;
                    (name, format!("{:?}", status.unwrap_or(crate::ssh::ServiceStatus::Unknown("Error".to_string()))))
                });
                tasks.push(task);
            }
        }

        // Check all hermes services
        for (hermes_name, hermes) in &self.config.hermes {
            let ssh_manager = self.clone();
            let server_name = hermes.server_host.clone();
            let service = hermes.service_name.clone();
            let name = hermes_name.clone();

            let task = tokio::spawn(async move {
                let status = ssh_manager.check_service_status(&server_name, &service).await;
                (name, format!("{:?}", status.unwrap_or(crate::ssh::ServiceStatus::Unknown("Error".to_string()))))
            });
            tasks.push(task);
        }

        let results = join_all(tasks).await;
        let mut all_statuses = HashMap::new();

        for task_result in results {
            match task_result {
                Ok((service_name, status)) => {
                    // Group by server for the response
                    let server_name = if let Some(node) = self.config.nodes.get(&service_name) {
                        node.server_host.clone()
                    } else if let Some(hermes) = self.config.hermes.get(&service_name) {
                        hermes.server_host.clone()
                    } else {
                        "unknown".to_string()
                    };

                    all_statuses
                        .entry(server_name)
                        .or_insert_with(HashMap::new)
                        .insert(service_name, status);
                }
                Err(e) => {
                    error!("Service status check task failed: {}", e);
                }
            }
        }

        all_statuses
    }
}

// Implement Clone for SshManager to enable parallel operations
impl Clone for SshManager {
    fn clone(&self) -> Self {
        Self {
            connections: self.connections.clone(),
            server_semaphores: self.server_semaphores.clone(),
            config: self.config.clone(),
        }
    }
}
