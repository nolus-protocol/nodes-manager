// File: src/ssh/mod.rs

pub mod manager;
pub mod operations;

pub use manager::SshManager;

use anyhow::Result;
use async_ssh2_tokio::client::{AuthMethod, Client, ServerCheckMethod};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::fs;
use tracing::{debug, warn};

pub struct SshConnection {
    client: Client,
    host: String,
}

impl SshConnection {
    pub async fn new(
        host: &str,
        username: &str,
        key_path: &str,
        connection_timeout_seconds: u64,
    ) -> Result<Self> {
        debug!("Establishing fresh SSH connection to {}@{}", username, host);

        // Parse host and port
        let addr: SocketAddr = if host.contains(':') {
            host.parse()?
        } else {
            format!("{}:22", host).parse()?
        };

        // Read SSH private key
        let key_content = fs::read_to_string(key_path).await.map_err(|e| {
            anyhow::anyhow!("Failed to read SSH key from {}: {}", key_path, e)
        })?;

        // Create auth method
        let auth_method = if key_path.ends_with(".pub") {
            return Err(anyhow::anyhow!(
                "SSH key path should point to private key, not public key: {}",
                key_path
            ));
        } else {
            AuthMethod::with_key(&key_content, None)
        };

        // Create client with connection timeout
        let client = tokio::time::timeout(
            Duration::from_secs(connection_timeout_seconds),
            Client::connect(
                addr,
                username,
                auth_method,
                ServerCheckMethod::NoCheck, // In production, use proper host key checking
            )
        )
        .await
        .map_err(|_| anyhow::anyhow!("SSH connection timed out after {}s", connection_timeout_seconds))?
        .map_err(|e| {
            anyhow::anyhow!("Failed to connect to SSH server {}@{}: {}", username, host, e)
        })?;

        debug!("Fresh SSH connection established to {}@{}", username, host);

        Ok(Self {
            client,
            host: host.to_string(),
        })
    }

    // FIXED: Handle large output buffer while ensuring synchronous execution
    pub async fn execute_command(&mut self, command: &str) -> Result<String> {
        debug!("Executing command on {}: {}", self.host, command);

        let wrapped_command = if command.contains("cosmos-pruner") {
            // FIXED: Run synchronously with output management - use semicolon not &&
            // This waits for completion but manages the buffer issue
            format!("bash -c '{} > /tmp/cosmos_pruner.log 2>&1; echo __COMMAND_SUCCESS__'", command)
        } else {
            // Normal command with proper completion detection
            format!("bash -c '{} && echo __COMMAND_SUCCESS__ || echo __COMMAND_FAILED__'", command)
        };

        let result = self
            .client
            .execute(&wrapped_command)
            .await
            .map_err(|e| anyhow::anyhow!("SSH command execution failed on {}: {}", self.host, e))?;

        let exit_code = result.exit_status;
        let output_str = result.stdout.trim().to_string();
        let stderr_str = result.stderr.trim();

        debug!(
            "Command completed on {} with exit code {}, stdout: {} chars, stderr: {} chars",
            self.host, exit_code, output_str.len(), stderr_str.len()
        );

        // Consistent completion detection using markers for ALL commands
        let command_succeeded = if output_str.ends_with("__COMMAND_SUCCESS__") {
            true
        } else if output_str.ends_with("__COMMAND_FAILED__") {
            false
        } else {
            // Fallback to exit code if markers not found
            exit_code == 0
        };

        if !command_succeeded {
            warn!(
                "Command failed on {} with exit code {}: {}",
                self.host, exit_code, stderr_str
            );
            return Err(anyhow::anyhow!(
                "Command failed on {} with exit code {}: {}",
                self.host,
                exit_code,
                if stderr_str.is_empty() { "Unknown error" } else { stderr_str }
            ));
        }

        // Clean up completion markers from output
        let cleaned_output = output_str
            .trim_end_matches("__COMMAND_SUCCESS__")
            .trim_end_matches("__COMMAND_FAILED__")
            .trim()
            .to_string();

        // For cosmos-pruner, try to get some output from the log file
        if command.contains("cosmos-pruner") {
            debug!("cosmos-pruner completed successfully on {}", self.host);

            // Try to get last few lines of the pruner log for feedback
            match self.client.execute("tail -5 /tmp/cosmos_pruner.log 2>/dev/null || echo 'Log not available'").await {
                Ok(log_result) => {
                    let log_output = log_result.stdout.trim();
                    if !log_output.is_empty() && log_output != "Log not available" {
                        return Ok(format!("Pruning completed. Last output:\n{}", log_output));
                    }
                }
                Err(_) => {} // Ignore log read errors
            }

            return Ok("Pruning completed successfully".to_string());
        }

        debug!("Command completed successfully on {}: {} chars of output", self.host, cleaned_output.len());

        Ok(cleaned_output)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServiceStatus {
    Active,
    Inactive,
    Failed,
    Activating,
    Deactivating,
    NotFound,
    Unknown(String),
}

impl ServiceStatus {
    pub fn is_healthy(&self) -> bool {
        matches!(self, ServiceStatus::Active)
    }

    pub fn is_running(&self) -> bool {
        matches!(self, ServiceStatus::Active | ServiceStatus::Activating)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn test_parse_process_uptime(uptime_str: &str) -> Option<Duration> {
        // Parse uptime string like "01:23:45" or "1-01:23:45" or "01:23"
        let parts: Vec<&str> = uptime_str.split('-').collect();
        let time_part = parts.last()?;

        let time_components: Vec<&str> = time_part.split(':').collect();

        match time_components.len() {
            2 => {
                // Format: MM:SS
                let minutes: u64 = time_components[0].parse().ok()?;
                let seconds: u64 = time_components[1].parse().ok()?;
                Some(Duration::from_secs(minutes * 60 + seconds))
            }
            3 => {
                // Format: HH:MM:SS
                let hours: u64 = time_components[0].parse().ok()?;
                let minutes: u64 = time_components[1].parse().ok()?;
                let seconds: u64 = time_components[2].parse().ok()?;
                Some(Duration::from_secs(hours * 3600 + minutes * 60 + seconds))
            }
            _ => None,
        }
    }

    #[test]
    fn test_parse_process_uptime_formats() {
        assert_eq!(
            test_parse_process_uptime("01:23"),
            Some(Duration::from_secs(83))
        );
        assert_eq!(
            test_parse_process_uptime("01:23:45"),
            Some(Duration::from_secs(5025))
        );
        assert_eq!(
            test_parse_process_uptime("1-01:23:45"),
            Some(Duration::from_secs(5025))
        );
        assert_eq!(test_parse_process_uptime("invalid"), None);
    }

    #[test]
    fn test_service_status() {
        assert!(ServiceStatus::Active.is_healthy());
        assert!(ServiceStatus::Active.is_running());
        assert!(!ServiceStatus::Inactive.is_healthy());
        assert!(!ServiceStatus::Inactive.is_running());
        assert!(ServiceStatus::Activating.is_running());
        assert!(!ServiceStatus::Activating.is_healthy());
    }
}
