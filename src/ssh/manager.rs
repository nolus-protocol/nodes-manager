// File: src/ssh/manager.rs

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, Semaphore};
use tracing::{debug, error, info, warn};

use crate::ssh::{ServiceStatus, SshConnection};
use crate::{Config, HermesConfig, NodeConfig, ServerConfig};

pub struct SshManager {
    pub connections: Arc<RwLock<HashMap<String, Arc<Mutex<SshConnection>>>>>,
    pub server_semaphores: Arc<RwLock<HashMap<String, Arc<Semaphore>>>>,
    pub config: Arc<Config>,
}

impl SshManager {
    pub fn new(config: Arc<Config>) -> Self {
        let mut server_semaphores = HashMap::new();

        // Create semaphore for each server based on its max_concurrent_ssh setting
        for (server_name, server_config) in &config.servers {
            server_semaphores.insert(
                server_name.clone(),
                Arc::new(Semaphore::new(server_config.max_concurrent_ssh)),
            );
            debug!(
                "Created semaphore for server {} with {} permits",
                server_name, server_config.max_concurrent_ssh
            );
        }

        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
            server_semaphores: Arc::new(RwLock::new(server_semaphores)),
            config,
        }
    }

    pub async fn execute_command(&self, server_name: &str, command: &str) -> Result<String> {
        let server_config = self
            .config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server {} not found", server_name))?;

        // Get server-specific semaphore
        let semaphore = {
            let semaphores = self.server_semaphores.read().await;
            semaphores
                .get(server_name)
                .ok_or_else(|| anyhow::anyhow!("Semaphore for server {} not found", server_name))?
                .clone()
        };

        let _permit = semaphore.acquire().await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to acquire semaphore for server {}: {}",
                server_name,
                e
            )
        })?;

        debug!(
            "Acquired semaphore permit for server {} (available: {})",
            server_name,
            semaphore.available_permits()
        );

        // Get or create connection
        let connection = self
            .get_or_create_connection(server_name, server_config)
            .await?;

        // Execute command with timeout
        let result = tokio::time::timeout(
            Duration::from_secs(server_config.ssh_timeout_seconds),
            async {
                let mut conn = connection.lock().await;
                conn.execute_command(command).await
            },
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                debug!(
                    "Command executed successfully on server {}: {} chars output",
                    server_name,
                    output.len()
                );
                Ok(output)
            }
            Ok(Err(e)) => {
                error!("SSH command failed on server {}: {}", server_name, e);
                Err(e)
            }
            Err(_) => {
                error!(
                    "SSH command timed out on server {} after {}s",
                    server_name, server_config.ssh_timeout_seconds
                );
                // Remove failed connection
                self.remove_connection(server_name).await;
                Err(anyhow::anyhow!(
                    "SSH command timed out after {}s",
                    server_config.ssh_timeout_seconds
                ))
            }
        }
    }

    pub async fn check_service_status(
        &self,
        server_name: &str,
        service_name: &str,
    ) -> Result<ServiceStatus> {
        let server_config = self
            .config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server {} not found", server_name))?;

        let semaphore = {
            let semaphores = self.server_semaphores.read().await;
            semaphores.get(server_name).unwrap().clone()
        };

        let _permit = semaphore.acquire().await?;
        let connection = self
            .get_or_create_connection(server_name, server_config)
            .await?;

        let result = tokio::time::timeout(
            Duration::from_secs(server_config.ssh_timeout_seconds),
            async {
                let mut conn = connection.lock().await;
                conn.check_service_status(service_name).await
            },
        )
        .await;

        match result {
            Ok(Ok(status)) => Ok(status),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                self.remove_connection(server_name).await;
                Err(anyhow::anyhow!("Service status check timed out"))
            }
        }
    }

    pub async fn get_service_uptime(
        &self,
        server_name: &str,
        service_name: &str,
    ) -> Result<Option<Duration>> {
        let server_config = self
            .config
            .servers
            .get(server_name)
            .ok_or_else(|| anyhow::anyhow!("Server {} not found", server_name))?;

        let semaphore = {
            let semaphores = self.server_semaphores.read().await;
            semaphores.get(server_name).unwrap().clone()
        };

        let _permit = semaphore.acquire().await?;
        let connection = self
            .get_or_create_connection(server_name, server_config)
            .await?;

        let result = tokio::time::timeout(
            Duration::from_secs(server_config.ssh_timeout_seconds),
            async {
                let mut conn = connection.lock().await;
                conn.get_service_uptime(service_name).await
            },
        )
        .await;

        match result {
            Ok(Ok(uptime)) => Ok(uptime),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                self.remove_connection(server_name).await;
                Err(anyhow::anyhow!("Service uptime check timed out"))
            }
        }
    }

    pub async fn stop_service(&self, server_name: &str, service_name: &str) -> Result<()> {
        info!(
            "Stopping service {} on server {}",
            service_name, server_name
        );

        let command = format!("sudo systemctl stop {}", service_name);
        self.execute_command(server_name, &command).await?;

        // Wait a moment and verify it stopped
        tokio::time::sleep(Duration::from_secs(2)).await;

        let status = self.check_service_status(server_name, service_name).await?;
        if status.is_running() {
            warn!(
                "Service {} on server {} is still running after stop command",
                service_name, server_name
            );
        } else {
            info!(
                "Service {} stopped successfully on server {}",
                service_name, server_name
            );
        }

        Ok(())
    }

    pub async fn start_service(&self, server_name: &str, service_name: &str) -> Result<()> {
        info!(
            "Starting service {} on server {}",
            service_name, server_name
        );

        let command = format!("sudo systemctl start {}", service_name);
        self.execute_command(server_name, &command).await?;

        // Wait a moment and verify it started
        tokio::time::sleep(Duration::from_secs(3)).await;

        let status = self.check_service_status(server_name, service_name).await?;
        if !status.is_running() {
            return Err(anyhow::anyhow!(
                "Service {} failed to start on server {}: {:?}",
                service_name,
                server_name,
                status
            ));
        }

        info!(
            "Service {} started successfully on server {}",
            service_name, server_name
        );
        Ok(())
    }

    pub async fn restart_service(&self, server_name: &str, service_name: &str) -> Result<()> {
        info!(
            "Restarting service {} on server {}",
            service_name, server_name
        );

        let command = format!("sudo systemctl restart {}", service_name);
        self.execute_command(server_name, &command).await?;

        // Wait for service to restart
        tokio::time::sleep(Duration::from_secs(5)).await;

        let status = self.check_service_status(server_name, service_name).await?;
        if !status.is_running() {
            return Err(anyhow::anyhow!(
                "Service {} failed to restart on server {}: {:?}",
                service_name,
                server_name,
                status
            ));
        }

        info!(
            "Service {} restarted successfully on server {}",
            service_name, server_name
        );
        Ok(())
    }

    pub async fn run_pruning(&self, node: &NodeConfig) -> Result<()> {
        let server_name = &node.server_host;
        let service_name = node
            .pruning_service_name
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No pruning service name configured for node"))?;
        let deploy_path = node
            .pruning_deploy_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No pruning deploy path configured for node"))?;
        let keep_blocks = node.pruning_keep_blocks.unwrap_or(1000);
        let keep_versions = node.pruning_keep_versions.unwrap_or(1000);

        info!("Starting pruning for node on server {}", server_name);

        // Stop the service
        self.stop_service(server_name, service_name).await?;

        // Run cosmos-pruner command using the exact path from configuration
        let prune_command = format!(
            "cosmos-pruner prune {} --blocks={} --versions={}",
            deploy_path, keep_blocks, keep_versions
        );

        info!("Executing cosmos-pruner command: {}", prune_command);

        let output = self.execute_command(server_name, &prune_command).await?;
        info!("Pruning output: {}", output);

        // Start the service
        self.start_service(server_name, service_name).await?;

        info!(
            "Pruning completed successfully for node on server {}",
            server_name
        );
        Ok(())
    }

    pub async fn restart_hermes(&self, hermes: &HermesConfig) -> Result<()> {
        let server_name = &hermes.server_host;
        let service_name = &hermes.service_name;

        info!(
            "Restarting Hermes {} on server {}",
            service_name, server_name
        );

        // Note: Dependency checking is now handled by the scheduler before calling this method
        // This method focuses only on restarting the Hermes service

        // Restart the hermes service
        self.restart_service(server_name, service_name).await?;

        // Verify Hermes is running properly
        tokio::time::sleep(Duration::from_secs(10)).await;
        let status = self.check_service_status(server_name, service_name).await?;

        if !status.is_healthy() {
            return Err(anyhow::anyhow!(
                "Hermes failed to start properly: {:?}",
                status
            ));
        }

        info!(
            "Hermes {} restarted successfully on server {}",
            service_name, server_name
        );
        Ok(())
    }

    async fn get_or_create_connection(
        &self,
        server_name: &str,
        server_config: &ServerConfig,
    ) -> Result<Arc<Mutex<SshConnection>>> {
        // Try to get existing connection
        {
            let connections = self.connections.read().await;
            if let Some(conn) = connections.get(server_name) {
                return Ok(conn.clone());
            }
        }

        // Create new connection
        let connection = SshConnection::new(
            &server_config.host,
            &server_config.ssh_username,
            &server_config.ssh_key_path,
            server_config.ssh_timeout_seconds,
        )
        .await?;

        let conn_arc = Arc::new(Mutex::new(connection));

        // Store the connection
        {
            let mut connections = self.connections.write().await;
            connections.insert(server_name.to_string(), conn_arc.clone());
        }

        info!("Created new SSH connection to server {}", server_name);
        Ok(conn_arc)
    }

    async fn remove_connection(&self, server_name: &str) {
        let mut connections = self.connections.write().await;
        if connections.remove(server_name).is_some() {
            warn!("Removed failed SSH connection for server {}", server_name);
        }
    }

    pub async fn get_connection_status(&self) -> HashMap<String, bool> {
        let connections = self.connections.read().await;
        let mut status = HashMap::new();

        for server_name in self.config.servers.keys() {
            status.insert(server_name.clone(), connections.contains_key(server_name));
        }

        status
    }

    pub async fn get_active_connections(&self) -> usize {
        let connections = self.connections.read().await;
        connections.len()
    }

    #[allow(dead_code)]
    pub async fn cleanup_idle_connections(&self) {
        // For now, we keep connections alive
        // In the future, we could implement idle timeout logic here
        debug!("Connection cleanup check completed");
    }
}
