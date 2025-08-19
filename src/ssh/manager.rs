// File: src/ssh/manager.rs

use anyhow::Result;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, RwLock, Semaphore};
use tracing::{debug, error, info, warn};

use crate::maintenance_tracker::MaintenanceTracker;
use crate::ssh::{ServiceStatus, SshConnection};
use crate::{AlarmPayload, Config, HermesConfig, NodeConfig, ServerConfig};

pub struct SshManager {
    pub connections: Arc<RwLock<HashMap<String, Arc<Mutex<SshConnection>>>>>,
    pub server_semaphores: Arc<RwLock<HashMap<String, Arc<Semaphore>>>>,
    pub config: Arc<Config>,
    pub maintenance_tracker: Arc<MaintenanceTracker>,
}

impl SshManager {
    pub fn new(config: Arc<Config>, maintenance_tracker: Arc<MaintenanceTracker>) -> Self {
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
            maintenance_tracker,
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

    /// Execute long-running command with extended timeout
    pub async fn execute_long_running_command(
        &self,
        server_name: &str,
        command: &str,
        timeout_minutes: u64,
    ) -> Result<String> {
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

        info!(
            "Executing long-running command on server {} with {}m timeout: {}",
            server_name, timeout_minutes, command
        );

        // Get or create connection
        let connection = self
            .get_or_create_connection(server_name, server_config)
            .await?;

        // Execute command with extended timeout
        let result = tokio::time::timeout(
            Duration::from_secs(timeout_minutes * 60),
            async {
                let mut conn = connection.lock().await;
                conn.execute_command(command).await
            },
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                info!(
                    "Long-running command completed successfully on server {}: {} chars output",
                    server_name,
                    output.len()
                );
                Ok(output)
            }
            Ok(Err(e)) => {
                error!("Long-running SSH command failed on server {}: {}", server_name, e);
                Err(e)
            }
            Err(_) => {
                error!(
                    "Long-running SSH command timed out on server {} after {}m",
                    server_name, timeout_minutes
                );
                // Remove failed connection
                self.remove_connection(server_name).await;
                Err(anyhow::anyhow!(
                    "Long-running command timed out after {}m",
                    timeout_minutes
                ))
            }
        }
    }

    /// FIXED: Execute pruning command with robust monitoring and completion detection
    pub async fn execute_monitored_pruning(
        &self,
        server_name: &str,
        prune_command: &str,
        deploy_path: &str,
        check_interval_minutes: u64,
        max_duration_minutes: u64,
    ) -> Result<String> {
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

        info!(
            "Starting monitored pruning on server {} with {}m max duration, checking every {}m",
            server_name, max_duration_minutes, check_interval_minutes
        );

        // Create a unique temporary file for this pruning operation
        let timestamp = chrono::Utc::now().timestamp();
        let safe_deploy_path = deploy_path.replace("/", "_");
        let temp_log_file = format!("/tmp/pruning_{}_{}_{}.log", server_name, safe_deploy_path, timestamp);
        let temp_pid_file = format!("/tmp/pruning_{}_{}_{}.pid", server_name, safe_deploy_path, timestamp);

        // Start the pruning process in background and capture PID
        let background_command = format!(
            "nohup bash -c 'cd / && ({}) > {} 2>&1 & echo $! > {}; wait'",
            prune_command, temp_log_file, temp_pid_file
        );

        info!("Starting background pruning process: {}", background_command);

        // Start the background process
        match self.execute_command(server_name, &background_command).await {
            Ok(_) => info!("Background pruning process started successfully"),
            Err(e) => {
                error!("Failed to start background pruning process: {}", e);
                return Err(e);
            }
        }

        // Give the process a moment to start and write PID
        tokio::time::sleep(Duration::from_secs(3)).await;

        // Read the PID with retry
        let get_pid_command = format!("cat {} 2>/dev/null || echo 'NO_PID'", temp_pid_file);
        let mut process_pid = 0u32;

        for attempt in 1..=3 {
            match self.execute_command(server_name, &get_pid_command).await {
                Ok(pid_str) => {
                    let pid_str = pid_str.trim();
                    if pid_str != "NO_PID" && !pid_str.is_empty() {
                        match pid_str.parse::<u32>() {
                            Ok(pid) => {
                                process_pid = pid;
                                info!("Pruning process started with PID: {}", pid);
                                break;
                            }
                            Err(e) => {
                                warn!("Could not parse PID '{}' on attempt {}: {}", pid_str, attempt, e);
                            }
                        }
                    } else {
                        warn!("PID file empty or not found on attempt {}", attempt);
                    }
                }
                Err(e) => {
                    warn!("Could not read PID file on attempt {}: {}", attempt, e);
                }
            }

            if attempt < 3 {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }

        if process_pid == 0 {
            warn!("Could not read PID, falling back to process name monitoring");
        } else {
            info!("Successfully started monitoring PID: {}", process_pid);
        }

        // FIXED: More robust monitoring loop
        let start_time = std::time::Instant::now();
        let max_duration = Duration::from_secs(max_duration_minutes * 60);
        let check_interval = Duration::from_secs(check_interval_minutes * 60);

        let mut last_health_check = start_time;
        let mut consecutive_failures = 0;
        const MAX_CONSECUTIVE_FAILURES: u32 = 3;
        const COMPLETION_CHECK_INTERVAL: Duration = Duration::from_secs(30);

        info!("Starting monitoring loop for PID {} with {}-minute health checks", process_pid, check_interval_minutes);

        loop {
            // Check if we've exceeded maximum duration
            if start_time.elapsed() >= max_duration {
                error!("Pruning operation timed out after {} minutes, force cleanup", max_duration_minutes);
                self.force_cleanup_pruning_process(server_name, process_pid, &temp_log_file, &temp_pid_file).await?;
                return Err(anyhow::anyhow!(
                    "Pruning operation timed out after {} minutes",
                    max_duration_minutes
                ));
            }

            // FIXED: More frequent completion checks (every 30 seconds)
            let is_running = if process_pid > 0 {
                self.robust_process_check(server_name, process_pid).await?
            } else {
                self.check_pruning_process_status(server_name, deploy_path).await.unwrap_or(false)
            };

            if !is_running {
                info!("Process is no longer running, checking completion...");

                // FIXED: Double-check process completion with multiple methods
                let definitely_finished = self.confirm_process_completion(server_name, process_pid, deploy_path).await?;

                if definitely_finished {
                    info!("Process completion confirmed, getting final output");
                    // Process completed, get the output and cleanup
                    let output = self.get_pruning_log_output(server_name, &temp_log_file).await
                        .unwrap_or_else(|e| {
                            warn!("Could not retrieve final log output: {}", e);
                            "Pruning completed but log output unavailable".to_string()
                        });

                    self.cleanup_pruning_process(server_name, process_pid, &temp_log_file, &temp_pid_file).await?;

                    let elapsed_minutes = start_time.elapsed().as_secs() / 60;
                    info!("Pruning process completed successfully after {} minutes", elapsed_minutes);
                    return Ok(output);
                } else {
                    consecutive_failures += 1;
                    warn!("Process appears stopped but completion not confirmed (attempt {}/{})",
                           consecutive_failures, MAX_CONSECUTIVE_FAILURES);

                    if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                        error!("Process completion could not be confirmed after {} attempts", MAX_CONSECUTIVE_FAILURES);
                        // Get what output we can and cleanup
                        let output = self.get_pruning_log_output(server_name, &temp_log_file).await
                            .unwrap_or_else(|_| "Process stopped but final status unclear".to_string());

                        self.cleanup_pruning_process(server_name, process_pid, &temp_log_file, &temp_pid_file).await?;

                        return Err(anyhow::anyhow!(
                            "Process stopped but completion could not be confirmed after {} attempts. Last output: {}",
                            MAX_CONSECUTIVE_FAILURES,
                            output.chars().take(500).collect::<String>()
                        ));
                    }
                }
            } else {
                // Process is running, reset failure count
                consecutive_failures = 0;
                debug!("Process PID {} is still running", process_pid);
            }

            // Periodic health check and logging
            if last_health_check.elapsed() >= check_interval {
                let elapsed_minutes = start_time.elapsed().as_secs() / 60;
                if is_running {
                    info!("Pruning process health check passed for {} (running for {}m)", deploy_path, elapsed_minutes);

                    // ADDED: Show recent log output during health checks
                    if let Ok(recent_output) = self.get_recent_log_output(server_name, &temp_log_file, 5).await {
                        if !recent_output.trim().is_empty() {
                            info!("Recent activity: {}", recent_output.lines().last().unwrap_or("").trim());
                        }
                    }
                } else {
                    warn!("Pruning process health check failed for {} (running for {}m)", deploy_path, elapsed_minutes);
                }
                last_health_check = std::time::Instant::now();
            }

            // Wait before next check (more frequent completion checks)
            tokio::time::sleep(COMPLETION_CHECK_INTERVAL).await;
        }
    }

    /// FIXED: More robust process checking with multiple verification methods
    async fn robust_process_check(&self, server_name: &str, pid: u32) -> Result<bool> {
        if pid == 0 {
            return Ok(false);
        }

        // Method 1: kill -0 (most reliable)
        let kill_check = format!("kill -0 {} 2>/dev/null && echo 'EXISTS' || echo 'GONE'", pid);

        match self.execute_command(server_name, &kill_check).await {
            Ok(output) => {
                let exists = output.trim() == "EXISTS";
                debug!("Process PID {} kill-0 check: {}", pid, if exists { "running" } else { "gone" });

                if !exists {
                    return Ok(false);
                }
            }
            Err(e) => {
                warn!("Failed kill-0 check for PID {}: {}", pid, e);
                return Ok(false);
            }
        }

        // Method 2: /proc check for additional verification
        let proc_check = format!("test -d /proc/{} && echo 'EXISTS' || echo 'GONE'", pid);

        match self.execute_command(server_name, &proc_check).await {
            Ok(output) => {
                let exists = output.trim() == "EXISTS";
                debug!("Process PID {} /proc check: {}", pid, if exists { "exists" } else { "gone" });
                Ok(exists)
            }
            Err(e) => {
                warn!("Failed /proc check for PID {}: {}", pid, e);
                // If /proc check fails but kill-0 passed, assume running
                Ok(true)
            }
        }
    }

    /// FIXED: Confirm process completion with multiple verification methods
    async fn confirm_process_completion(&self, server_name: &str, pid: u32, deploy_path: &str) -> Result<bool> {
        debug!("Confirming process completion for PID {} on path {}", pid, deploy_path);

        // Check 1: PID should be gone
        if pid > 0 {
            let still_running = self.robust_process_check(server_name, pid).await?;
            if still_running {
                debug!("Process completion denied: PID {} still running", pid);
                return Ok(false);
            }
        }

        // Check 2: No cosmos-pruner processes for this path
        let process_check = self.check_pruning_process_status(server_name, deploy_path).await?;
        if process_check {
            debug!("Process completion denied: cosmos-pruner still running for {}", deploy_path);
            return Ok(false);
        }

        // Check 3: Look for any cosmos-pruner processes at all
        let general_check = format!("pgrep -f 'cosmos-pruner' | wc -l");
        match self.execute_command(server_name, &general_check).await {
            Ok(output) => {
                if let Ok(count) = output.trim().parse::<u32>() {
                    if count > 0 {
                        debug!("Found {} cosmos-pruner processes still running", count);
                        // This might be other nodes pruning, so we'll allow it
                    }
                }
            }
            Err(e) => {
                warn!("Could not check for general cosmos-pruner processes: {}", e);
            }
        }

        info!("Process completion confirmed for PID {} on path {}", pid, deploy_path);
        Ok(true)
    }

    /// Get recent log output (last N lines)
    async fn get_recent_log_output(&self, server_name: &str, log_file: &str, lines: u32) -> Result<String> {
        let read_log_command = format!("tail -n {} {} 2>/dev/null || echo 'No recent log output'", lines, log_file);
        self.execute_command(server_name, &read_log_command).await
    }

    /// Get the output from the pruning log file
    async fn get_pruning_log_output(&self, server_name: &str, log_file: &str) -> Result<String> {
        let read_log_command = format!("tail -n 100 {} 2>/dev/null || echo 'No log output available'", log_file);
        self.execute_command(server_name, &read_log_command).await
    }

    /// Clean up temporary files and optionally kill the process
    async fn cleanup_pruning_process(
        &self,
        server_name: &str,
        pid: u32,
        log_file: &str,
        pid_file: &str
    ) -> Result<()> {
        // Don't kill the process if it's already gone
        if pid > 0 {
            let still_running = self.robust_process_check(server_name, pid).await.unwrap_or(false);
            if still_running {
                warn!("Process {} still running during cleanup, attempting graceful termination", pid);
                let kill_command = format!("kill {} 2>/dev/null || true", pid);
                if let Err(e) = self.execute_command(server_name, &kill_command).await {
                    warn!("Failed to gracefully kill process {}: {}", pid, e);
                }

                // Wait a moment for graceful shutdown
                tokio::time::sleep(Duration::from_secs(5)).await;

                // Force kill if still running
                let still_running = self.robust_process_check(server_name, pid).await.unwrap_or(false);
                if still_running {
                    warn!("Force killing stubborn process {}", pid);
                    let force_kill_command = format!("kill -9 {} 2>/dev/null || true", pid);
                    if let Err(e) = self.execute_command(server_name, &force_kill_command).await {
                        warn!("Failed to force kill process {}: {}", pid, e);
                    }
                }
            }
        }

        // Clean up temporary files
        let cleanup_command = format!("rm -f {} {} 2>/dev/null || true", log_file, pid_file);
        if let Err(e) = self.execute_command(server_name, &cleanup_command).await {
            warn!("Failed to cleanup temporary files: {}", e);
        } else {
            info!("Cleaned up temporary files for PID {}", pid);
        }

        Ok(())
    }

    /// ADDED: Force cleanup for timeout situations
    async fn force_cleanup_pruning_process(
        &self,
        server_name: &str,
        pid: u32,
        log_file: &str,
        pid_file: &str
    ) -> Result<()> {
        warn!("Force cleanup initiated for PID {}", pid);

        // Kill any cosmos-pruner processes
        let force_kill_all = "pkill -9 -f cosmos-pruner 2>/dev/null || true".to_string();
        if let Err(e) = self.execute_command(server_name, &force_kill_all).await {
            warn!("Failed to force kill all cosmos-pruner processes: {}", e);
        }

        // Clean up files
        self.cleanup_pruning_process(server_name, pid, log_file, pid_file).await?;

        Ok(())
    }

    /// Truncate log files for a service
    pub async fn truncate_logs(&self, server_name: &str, log_path: &str, service_name: &str) -> Result<()> {
        info!("Truncating logs for service {} on server {} at path: {}", service_name, server_name, log_path);

        // Create a comprehensive log cleanup command
        let cleanup_command = format!(
            "if [ -d '{}' ]; then find '{}' -type f -name '*.log*' -delete 2>/dev/null || true; fi && \
             if [ -f '{}' ]; then rm -f '{}' 2>/dev/null || true; fi && \
             journalctl --vacuum-time=1s --user-unit={} 2>/dev/null || true && \
             journalctl --vacuum-time=1s --system --unit={} 2>/dev/null || true",
            log_path, log_path, log_path, log_path, service_name, service_name
        );

        match self.execute_command(server_name, &cleanup_command).await {
            Ok(output) => {
                info!("Log truncation completed for service {} on server {}", service_name, server_name);
                if !output.trim().is_empty() {
                    debug!("Log truncation output: {}", output);
                }
                Ok(())
            }
            Err(e) => {
                warn!("Log truncation failed for service {} on server {}: {}", service_name, server_name, e);
                // Don't fail the entire operation if log truncation fails
                Ok(())
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

    /// Enhanced pruning with maintenance mode integration, log truncation, and ROBUST PROCESS MONITORING
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

        // FIXED: Find the actual node config key instead of generating format
        let node_name = self.find_node_config_key(node).await
            .ok_or_else(|| anyhow::anyhow!("Could not find node config key for pruning"))?;

        info!("Starting pruning for node {} on server {} with robust process monitoring", node_name, server_name);

        // STEP 1: Start maintenance mode with EXTENDED estimate for long pruning operations (5 hours)
        self.maintenance_tracker
            .start_maintenance(&node_name, "pruning", 300, server_name) // 300 minutes = 5 hours
            .await?;

        // STEP 2: Execute pruning with proper error handling, log truncation, and ROBUST PROCESS MONITORING
        let pruning_result = async {
            // Stop the service
            self.stop_service(server_name, service_name).await?;

            // Truncate logs if enabled and configured
            if node.truncate_logs_enabled.unwrap_or(false) {
                if let Some(log_path) = &node.log_path {
                    info!("Truncating logs for node {} before pruning", node_name);
                    self.truncate_logs(server_name, log_path, service_name).await?;
                } else {
                    warn!("Log truncation enabled for node {} but no log_path configured", node_name);
                }
            }

            // Run cosmos-pruner command using the FIXED monitored execution method
            let prune_command = format!(
                "cosmos-pruner prune {} --blocks={} --versions={}",
                deploy_path, keep_blocks, keep_versions
            );

            info!("Executing cosmos-pruner with robust process monitoring: {}", prune_command);

            // Use the FIXED monitored execution method:
            // - Check process health every 15 minutes
            // - Maximum 5-hour duration (300 minutes)
            // - Robust completion detection
            // - Proper cleanup
            let output = self.execute_monitored_pruning(
                server_name,
                &prune_command,
                deploy_path,
                15, // Check every 15 minutes
                300 // Maximum 5 hours
            ).await?;

            info!("Pruning completed successfully with robust monitoring. Output length: {} chars", output.len());

            // Start the service
            self.start_service(server_name, service_name).await?;

            Ok::<(), anyhow::Error>(())
        }
        .await;

        // STEP 3: End maintenance mode (regardless of success/failure)
        if let Err(e) = self.maintenance_tracker.end_maintenance(&node_name).await {
            error!("Failed to end maintenance mode for {}: {}", node_name, e);
        }

        // STEP 4: Send completion notification (success or failure)
        let completion_status = if pruning_result.is_ok() { "completed" } else { "failed" };
        if let Err(e) = self.send_maintenance_notification(&node_name, completion_status, "pruning").await {
            warn!("Failed to send maintenance completion notification: {}", e);
        }

        // STEP 5: Return the actual pruning result
        match pruning_result {
            Ok(_) => {
                info!("Pruning completed successfully for node {} on server {} with robust monitoring", node_name, server_name);
                Ok(())
            }
            Err(e) => {
                error!("Pruning failed for node {} on server {}: {}", node_name, server_name, e);
                Err(e)
            }
        }
    }

    /// Check if a pruning process is actually running on the server
    pub async fn check_pruning_process_status(&self, server_name: &str, deploy_path: &str) -> Result<bool> {
        let check_command = format!(
            "pgrep -f 'cosmos-pruner.*{}' > /dev/null && echo 'running' || echo 'not_running'",
            deploy_path
        );

        match self.execute_command(server_name, &check_command).await {
            Ok(output) => {
                let is_running = output.trim() == "running";
                debug!("Pruning process status check for {}: {}", deploy_path, if is_running { "running" } else { "not running" });
                Ok(is_running)
            }
            Err(e) => {
                warn!("Failed to check pruning process status: {}", e);
                Ok(false) // Assume not running if we can't check
            }
        }
    }

    /// Emergency cleanup - kill any stuck pruning processes
    pub async fn kill_stuck_pruning_process(&self, server_name: &str, deploy_path: &str) -> Result<()> {
        let kill_command = format!(
            "pkill -f 'cosmos-pruner.*{}'",
            deploy_path
        );

        match self.execute_command(server_name, &kill_command).await {
            Ok(_) => {
                info!("Killed stuck pruning process for path: {}", deploy_path);
                Ok(())
            }
            Err(e) => {
                warn!("Failed to kill stuck pruning process: {}", e);
                Ok(()) // Don't fail if we can't kill the process
            }
        }
    }

    // NEW: Helper method to find config key for a NodeConfig
    pub async fn find_node_config_key(&self, target_node: &NodeConfig) -> Option<String> {
        for (config_key, node_config) in &self.config.nodes {
            if node_config.rpc_url == target_node.rpc_url
                && node_config.network == target_node.network
                && node_config.server_host == target_node.server_host {
                return Some(config_key.clone());
            }
        }
        None
    }

    /// Send maintenance notification webhook
    /// Note: Only sends completion/failure notifications, not start notifications
    /// to avoid information overload. The health monitor automatically suppresses
    /// down/unhealthy alerts when nodes are in maintenance mode.
    async fn send_maintenance_notification(&self, node_name: &str, status: &str, operation: &str) -> Result<()> {
        if self.config.alarm_webhook_url.is_empty() {
            return Ok(());
        }

        let server_host = self.get_server_for_node(node_name).await.unwrap_or_else(|| "unknown".to_string());

        let alarm = AlarmPayload {
            timestamp: chrono::Utc::now(),
            alarm_type: "node_maintenance".to_string(),
            severity: "info".to_string(),
            node_name: node_name.to_string(),
            message: format!("Node {} maintenance {}: {}", node_name, status, operation),
            details: json!({
                "maintenance_status": status,
                "operation_type": operation,
                "server_host": server_host,
                "timestamp": chrono::Utc::now().to_rfc3339()
            }),
        };

        let client = reqwest::Client::new();
        let response = client
            .post(&self.config.alarm_webhook_url)
            .json(&alarm)
            .send()
            .await?;

        if response.status().is_success() {
            info!("Sent maintenance notification for {}: {} {}", node_name, operation, status);
        } else {
            warn!("Failed to send maintenance notification: HTTP {}", response.status());
        }

        Ok(())
    }

    // Check if dependent nodes are healthy before Hermes restart
    pub async fn check_node_dependencies(&self, dependent_nodes: &[String]) -> Result<bool> {
        if dependent_nodes.is_empty() {
            return Ok(true); // No dependencies = always OK
        }

        info!("Checking health of {} dependent nodes", dependent_nodes.len());

        for node_name in dependent_nodes {
            // Get node config
            let node_config = self.config.nodes.get(node_name)
                .ok_or_else(|| anyhow::anyhow!("Dependent node {} not found in config", node_name))?;

            if !node_config.enabled {
                warn!("Dependent node {} is disabled, skipping", node_name);
                continue; // Skip disabled nodes
            }

            // Check if node is in maintenance - if so, consider it temporarily unhealthy
            if self.maintenance_tracker.is_in_maintenance(node_name).await {
                warn!("Dependent node {} is in maintenance", node_name);
                return Ok(false);
            }

            // Make a quick RPC call to check if node is responding
            let health_check = self.quick_node_health_check(node_name, node_config).await;

            match health_check {
                Ok(true) => {
                    debug!("Dependent node {} is healthy", node_name);
                }
                Ok(false) => {
                    warn!("Dependent node {} is unhealthy", node_name);
                    return Ok(false);
                }
                Err(e) => {
                    error!("Failed to check dependent node {}: {}", node_name, e);
                    return Ok(false);
                }
            }
        }

        info!("All dependent nodes are healthy");
        Ok(true)
    }

    // Quick health check for dependency validation
    pub async fn quick_node_health_check(&self, node_name: &str, node_config: &NodeConfig) -> Result<bool> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5)) // Shorter timeout for dependency checks
            .build()?;

        let rpc_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "status",
            "params": [],
            "id": 1
        });

        match client.post(&node_config.rpc_url).json(&rpc_request).send().await {
            Ok(response) if response.status().is_success() => {
                debug!("Node {} RPC responded successfully", node_name);
                Ok(true)
            }
            Ok(response) => {
                warn!("Node {} RPC returned error status: {}", node_name, response.status());
                Ok(false)
            }
            Err(e) => {
                warn!("Node {} RPC failed: {}", node_name, e);
                Ok(false)
            }
        }
    }

    // Check if Hermes has been running long enough to restart
    pub async fn check_hermes_min_uptime(&self, hermes: &HermesConfig) -> Result<bool> {
        let min_uptime_minutes = self.config.hermes_min_uptime_minutes;

        if min_uptime_minutes == 0 {
            return Ok(true); // No minimum uptime required
        }

        let server_name = &hermes.server_host;
        let service_name = &hermes.service_name;

        match self.get_service_uptime(server_name, service_name).await {
            Ok(Some(uptime)) => {
                let uptime_minutes = uptime.as_secs() / 60;

                if uptime_minutes >= min_uptime_minutes {
                    info!(
                        "Hermes {} has been running for {} minutes (minimum: {})",
                        service_name, uptime_minutes, min_uptime_minutes
                    );
                    Ok(true)
                } else {
                    warn!(
                        "Hermes {} has only been running for {} minutes (minimum: {})",
                        service_name, uptime_minutes, min_uptime_minutes
                    );
                    Ok(false)
                }
            }
            Ok(None) => {
                warn!("Could not determine uptime for Hermes {}", service_name);
                Ok(true) // Allow restart if uptime unknown
            }
            Err(e) => {
                error!("Failed to check uptime for Hermes {}: {}", service_name, e);
                Ok(true) // Allow restart on error
            }
        }
    }

    // Verify Hermes actually started properly by checking logs
    async fn verify_hermes_startup(&self, server_name: &str, service_name: &str) -> Result<bool> {
        let log_cmd = format!(
            "journalctl -u {} --since '1 minute ago' --no-pager | grep -E '(started|ready|listening)' | tail -5",
            service_name
        );

        match self.execute_command(server_name, &log_cmd).await {
            Ok(output) => {
                if output.trim().is_empty() {
                    warn!("No startup messages found in Hermes logs");
                    Ok(false)
                } else {
                    debug!("Hermes startup logs: {}", output);
                    // Look for positive indicators
                    let positive_indicators = ["started", "ready", "listening", "connected"];
                    let has_positive = positive_indicators.iter()
                        .any(|indicator| output.to_lowercase().contains(indicator));

                    Ok(has_positive)
                }
            }
            Err(e) => {
                warn!("Failed to check Hermes logs: {}", e);
                Ok(false)
            }
        }
    }

    /// Enhanced Hermes restart with log truncation
    pub async fn restart_hermes(&self, hermes: &HermesConfig) -> Result<()> {
        let server_name = &hermes.server_host;
        let service_name = &hermes.service_name;

        info!("Starting Hermes restart: {} on server {}", service_name, server_name);

        // STEP 1: Check dependent nodes are healthy
        if !self.check_node_dependencies(&hermes.dependent_nodes).await? {
            return Err(anyhow::anyhow!(
                "Cannot restart Hermes {}: dependent nodes are not healthy",
                service_name
            ));
        }

        // STEP 2: Check minimum uptime
        if !self.check_hermes_min_uptime(hermes).await? {
            return Err(anyhow::anyhow!(
                "Cannot restart Hermes {}: minimum uptime not reached",
                service_name
            ));
        }

        // STEP 3: Stop the hermes service
        info!("Stopping Hermes service: {}", service_name);
        self.stop_service(server_name, service_name).await?;

        // STEP 4: Truncate logs if enabled
        if hermes.truncate_logs_enabled.unwrap_or(false) {
            info!("Truncating logs for Hermes {} before restart", service_name);
            self.truncate_logs(server_name, &hermes.log_path, service_name).await?;
        }

        // STEP 5: Start the hermes service
        info!("Starting Hermes service: {}", service_name);
        self.start_service(server_name, service_name).await?;

        // STEP 6: Wait longer for Hermes to start (Hermes needs more time than blockchain nodes)
        info!("Waiting for Hermes {} to start...", service_name);
        tokio::time::sleep(Duration::from_secs(15)).await;

        // STEP 7: Verify Hermes is running
        let status = self.check_service_status(server_name, service_name).await?;
        if !status.is_healthy() {
            return Err(anyhow::anyhow!(
                "Hermes failed to start properly: {:?}",
                status
            ));
        }

        // STEP 8: Additional verification - check Hermes logs for startup success
        match self.verify_hermes_startup(server_name, service_name).await {
            Ok(true) => {
                info!("Hermes {} startup verification passed", service_name);
            }
            Ok(false) => {
                warn!("Hermes {} started but verification failed - check logs", service_name);
                // Don't fail the restart, just warn
            }
            Err(e) => {
                warn!("Could not verify Hermes {} startup: {}", service_name, e);
                // Don't fail the restart, just warn
            }
        }

        info!("Hermes {} restarted successfully on server {}", service_name, server_name);
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

    async fn get_server_for_node(&self, node_name: &str) -> Option<String> {
        // Try to extract server from node name format "server-network"
        if let Some(dash_pos) = node_name.find('-') {
            let server_part = &node_name[..dash_pos];
            if self.config.servers.contains_key(server_part) {
                return Some(server_part.to_string());
            }
        }

        // Fallback: search through nodes config
        for (config_node_name, node_config) in &self.config.nodes {
            if config_node_name == node_name {
                return Some(node_config.server_host.clone());
            }
        }

        None
    }

    #[allow(dead_code)]
    pub async fn cleanup_idle_connections(&self) {
        // For now, we keep connections alive
        // In the future, we could implement idle timeout logic here
        debug!("Connection cleanup check completed");
    }
}
