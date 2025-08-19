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
        debug!("Establishing SSH connection to {}@{}", username, host);

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

        // Create client with connection timeout (FIXED: actually use the timeout parameter)
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

        debug!("SSH connection established to {}@{}", username, host);

        Ok(Self {
            client,
            host: host.to_string(),
        })
    }

    pub async fn execute_command(&mut self, command: &str) -> Result<String> {
        debug!("Executing command on {}: {}", self.host, command);

        // Execute command with NO timeout - let it run as long as needed (n8n style)
        let result = self
            .client
            .execute(command)
            .await
            .map_err(|e| anyhow::anyhow!("SSH command execution failed: {}", e))?;

        let exit_code = result.exit_status;

        if exit_code != 0 {
            let stderr = &result.stderr;
            warn!(
                "Command failed on {} with exit code {}: {}",
                self.host, exit_code, stderr
            );
            return Err(anyhow::anyhow!(
                "Command failed with exit code {}: {}",
                exit_code,
                stderr
            ));
        }

        let output_str = result.stdout.trim().to_string();
        debug!(
            "Command completed on {} with output length: {} bytes",
            self.host,
            output_str.len()
        );

        Ok(output_str)
    }

    pub async fn check_service_status(&mut self, service_name: &str) -> Result<ServiceStatus> {
        let command = format!("systemctl is-active {}", service_name);

        match self.execute_command(&command).await {
            Ok(output) => {
                match output.trim() {
                    "active" => Ok(ServiceStatus::Active),
                    "inactive" => Ok(ServiceStatus::Inactive),
                    "failed" => Ok(ServiceStatus::Failed),
                    "activating" => Ok(ServiceStatus::Activating),
                    "deactivating" => Ok(ServiceStatus::Deactivating),
                    other => Ok(ServiceStatus::Unknown(other.to_string())),
                }
            }
            Err(_) => {
                // If systemctl command fails, service might not exist
                Ok(ServiceStatus::NotFound)
            }
        }
    }

    pub async fn get_service_uptime(&mut self, service_name: &str) -> Result<Option<Duration>> {
        let command = format!(
            "systemctl show {} --property=ActiveEnterTimestamp --value",
            service_name
        );

        match self.execute_command(&command).await {
            Ok(output) => {
                if output.trim().is_empty() || output.trim() == "n/a" {
                    return Ok(None);
                }

                // Use a simpler approach: check how long the process has been running
                let pid_cmd = format!("systemctl show {} --property=MainPID --value", service_name);
                if let Ok(pid_output) = self.execute_command(&pid_cmd).await {
                    if let Ok(pid) = pid_output.trim().parse::<u32>() {
                        if pid > 0 {
                            let uptime_cmd = format!("ps -o etime= -p {}", pid);
                            if let Ok(uptime_str) = self.execute_command(&uptime_cmd).await {
                                return Ok(parse_process_uptime(&uptime_str.trim()));
                            }
                        }
                    }
                }
                Ok(None)
            }
            Err(_) => Ok(None),
        }
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

fn parse_process_uptime(uptime_str: &str) -> Option<Duration> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_process_uptime() {
        assert_eq!(
            parse_process_uptime("01:23"),
            Some(Duration::from_secs(83))
        );
        assert_eq!(
            parse_process_uptime("01:23:45"),
            Some(Duration::from_secs(5025))
        );
        assert_eq!(
            parse_process_uptime("1-01:23:45"),
            Some(Duration::from_secs(5025))
        );
        assert_eq!(parse_process_uptime("invalid"), None);
    }
}
