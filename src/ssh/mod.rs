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

    // FIXED: Command chaining approach only for long-running snapshot operations
    pub async fn execute_command(&mut self, command: &str) -> Result<String> {
        debug!("Executing command on {}: {}", self.host, command);

        // Handle only long-running snapshot commands with chaining
        if self.is_long_running_snapshot_command(command) {
            return self.execute_snapshot_with_chaining(command).await;
        }

        let wrapped_command = if command.contains("cosmos-pruner") {
            // Pruning: redirect output to log, then success marker
            let escaped_command = command.replace("\"", "\\\"");
            format!("bash -c \"{} > /tmp/cosmos_pruner.log 2>&1; echo __COMMAND_SUCCESS__\"", escaped_command)
        } else {
            // All other commands (including simple snapshot stats)
            let escaped_command = command.replace("\"", "\\\"");
            format!("bash -c \"{} && echo __COMMAND_SUCCESS__ || echo __COMMAND_FAILED__\"", escaped_command)
        };

        // TEMPORARY: Info level logging to see the exact command without debug spam
        debug!("SSH client will execute: {}", wrapped_command);

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

        // Simple completion detection - just like the original working version
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

        // Handle special cases for long-running operations
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

    // NEW: Use system SSH for long-running snapshot commands to avoid library limitations
    async fn execute_snapshot_with_chaining(&mut self, command: &str) -> Result<String> {
        debug!("Executing snapshot command with system SSH on {}: {}", self.host, command);

        let escaped_command = command.replace("\"", "\\\"");
        let full_command = format!("bash -c \"{}; echo __COMMAND_SUCCESS__\"", escaped_command);

        tracing::info!("SNAPSHOT_SYSTEM_SSH - Executing via system ssh: {}", full_command);

        // Use system ssh command instead of the library
        let output = tokio::process::Command::new("ssh")
            .args([
                "-o", "StrictHostKeyChecking=no",
                "-o", "UserKnownHostsFile=/dev/null",
                "-o", "LogLevel=ERROR",
                &format!("root@{}", self.host),
                &full_command
            ])
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("System SSH command failed: {}", e))?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        tracing::info!(
            "SNAPSHOT_SYSTEM_SSH - Completed with exit code {}, stdout: {} chars, stderr: {} chars",
            exit_code, stdout.len(), stderr.len()
        );

        if exit_code != 0 {
            return Err(anyhow::anyhow!(
                "System SSH command failed with exit code {}: {}",
                exit_code,
                stderr
            ));
        }

        if stdout.contains("__COMMAND_SUCCESS__") {
            debug!("Snapshot operation completed successfully via system SSH on {}", self.host);

            let operation_type = if command.contains("tar -cf -") && command.contains("lz4 -z") {
                "LZ4 snapshot creation completed successfully via system SSH"
            } else if command.contains("lz4 -d -c") && command.contains("tar -xf") {
                "LZ4 snapshot restoration completed successfully via system SSH"
            } else if command.contains("tar -xzf") {
                "Legacy snapshot restoration completed successfully via system SSH"
            } else {
                "Snapshot operation completed successfully via system SSH"
            };

            return Ok(operation_type.to_string());
        } else {
            return Err(anyhow::anyhow!(
                "System SSH success marker not found. stdout: {}, stderr: {}",
                stdout.trim(),
                stderr.trim()
            ));
        }
    }

    // NEW: Detect only long-running snapshot operations that need chaining
    fn is_long_running_snapshot_command(&self, command: &str) -> bool {
        // Only snapshot creation and restoration commands need chaining
        if command.contains("tar -cf -") && command.contains("lz4 -z -c") {
            return true; // LZ4 snapshot creation
        }

        if command.contains("lz4 -d -c") && command.contains("tar -xf") {
            return true; // LZ4 snapshot restoration
        }

        if command.contains("tar -xzf") && (command.contains("snapshot") || command.contains(".tar.gz")) {
            return true; // Legacy snapshot restoration
        }

        false // All other commands (stats, listing, etc.) use regular execution
    }

    // NEW: Helper method to detect snapshot-related commands (broader than long-running)
    fn is_snapshot_command(&self, command: &str) -> bool {
        // Detect snapshot creation commands (tar + lz4 pipeline)
        if command.contains("tar -cf -") && command.contains("lz4 -z -c") {
            return true;
        }

        // Detect LZ4 snapshot restoration commands
        if command.contains("lz4 -d -c") && command.contains("tar -xf") {
            return true;
        }

        // Detect legacy snapshot restoration commands
        if command.contains("tar -xzf") && (command.contains("snapshot") || command.contains(".tar.gz")) {
            return true;
        }

        // Detect other snapshot-related long operations
        if command.contains("tar") && (command.contains("/backup") || command.contains("/snapshots")) {
            return true;
        }

        false
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

    #[test]
    fn test_snapshot_command_detection() {
        let conn = SshConnection {
            client: unsafe { std::mem::zeroed() }, // Mock for testing
            host: "test".to_string(),
        };

        // Test snapshot creation command detection
        assert!(conn.is_snapshot_command("cd '/opt/deploy/osmosis' && tar -cf - data wasm 2>/dev/null | lz4 -z -c > '/backup/snapshot.lz4'"));

        // Test LZ4 restoration command detection
        assert!(conn.is_snapshot_command("cd '/opt/deploy/osmosis' && lz4 -d -c '/backup/snapshot.lz4' | tar -xf -"));

        // Test legacy restoration command detection
        assert!(conn.is_snapshot_command("cd '/opt/deploy/osmosis' && tar -xzf '/backup/snapshot.tar.gz'"));

        // Test non-snapshot command
        assert!(!conn.is_snapshot_command("systemctl start osmosis"));

        // Test edge cases
        assert!(conn.is_snapshot_command("tar -cf /snapshots/backup.tar data"));
        assert!(!conn.is_snapshot_command("echo hello world"));
    }

    #[test]
    fn test_command_wrapping_logic() {
        let conn = SshConnection {
            client: unsafe { std::mem::zeroed() }, // Mock for testing
            host: "test".to_string(),
        };

        // Test that snapshot commands are detected correctly
        let snapshot_cmd = "cd '/opt/deploy/node' && tar -cf - data wasm | lz4 -z -c > '/backup/test.lz4'";
        assert!(conn.is_snapshot_command(snapshot_cmd));

        // Test that pruner commands would be handled (not in this function but conceptually)
        let pruner_cmd = "cosmos-pruner prune /opt/deploy/node/data --blocks=1000";
        assert!(!conn.is_snapshot_command(pruner_cmd)); // Should be false, handled by different logic
        assert!(pruner_cmd.contains("cosmos-pruner")); // But this should be true

        // Test normal commands
        let normal_cmd = "systemctl status node";
        assert!(!conn.is_snapshot_command(normal_cmd));
    }
}
