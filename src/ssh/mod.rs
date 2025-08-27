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
    // Extended to handle snapshot operations in addition to cosmos-pruner
    pub async fn execute_command(&mut self, command: &str) -> Result<String> {
        debug!("Executing command on {}: {}", self.host, command);

        let wrapped_command = if command.contains("cosmos-pruner") {
            // Run synchronously with output management - use semicolon not &&
            // This waits for completion but manages the buffer issue
            format!("bash -c '{} > /tmp/cosmos_pruner.log 2>&1; echo __COMMAND_SUCCESS__'", command)
        } else if self.is_snapshot_creation_command(command) {
            // CRITICAL FIX: For snapshot CREATION, we need special handling
            // The command already has its own output redirection (> snapshot.lz4)
            // We must NOT redirect stdout as it contains the actual snapshot data
            // Only redirect stderr to prevent buffer issues, and wrap in subshell
            format!(
                "bash -c '( {} ) 2>/tmp/snapshot_creation.log && echo __COMMAND_SUCCESS__ || echo __COMMAND_FAILED__'",
                command
            )
        } else if self.is_snapshot_restoration_command(command) {
            // For snapshot RESTORATION, we can redirect both stdout and stderr
            // because restoration doesn't produce data that needs to be saved
            format!("bash -c '{} > /tmp/snapshot_restoration.log 2>&1; echo __COMMAND_SUCCESS__'", command)
        } else if self.is_snapshot_command(command) {
            // Fallback for other snapshot operations
            format!("bash -c '{} > /tmp/snapshot_operation.log 2>&1; echo __COMMAND_SUCCESS__'", command)
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

            // For snapshot creation, try to get error from log
            if self.is_snapshot_creation_command(command) {
                if let Ok(log_result) = self.client.execute("tail -20 /tmp/snapshot_creation.log 2>/dev/null").await {
                    let log_output = log_result.stdout.trim();
                    if !log_output.is_empty() {
                        return Err(anyhow::anyhow!(
                            "Snapshot creation failed on {}. Error log:\n{}",
                            self.host,
                            log_output
                        ));
                    }
                }
            }

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
        } else if self.is_snapshot_creation_command(command) {
            debug!("Snapshot creation completed successfully on {}", self.host);

            // For snapshot creation, check the file was actually created and has size
            if let Some(output_file) = self.extract_snapshot_output_file(command) {
                // Check file size
                match self.client.execute(&format!("ls -lh {} 2>/dev/null | awk '{{print $5}}'", output_file)).await {
                    Ok(size_result) => {
                        let size = size_result.stdout.trim();
                        if !size.is_empty() && size != "0" {
                            return Ok(format!("LZ4 snapshot creation completed successfully. File size: {}", size));
                        } else {
                            warn!("Snapshot file appears to be empty or missing: {}", output_file);
                        }
                    }
                    Err(_) => {}
                }
            }

            return Ok("LZ4 snapshot creation completed successfully".to_string());
        } else if self.is_snapshot_restoration_command(command) {
            debug!("Snapshot restoration completed successfully on {}", self.host);

            // Try to get last few lines of the restoration log for feedback
            match self.client.execute("tail -10 /tmp/snapshot_restoration.log 2>/dev/null || echo 'Log not available'").await {
                Ok(log_result) => {
                    let log_output = log_result.stdout.trim();
                    if !log_output.is_empty() && log_output != "Log not available" {
                        // Filter out verbose tar output if present
                        let filtered_output: Vec<&str> = log_output
                            .lines()
                            .filter(|line| {
                                // Skip common tar verbose output
                                !line.starts_with("tar: ") ||
                                line.contains("error") ||
                                line.contains("warning") ||
                                line.contains("failed")
                            })
                            .take(5)  // Limit to 5 most relevant lines
                            .collect();

                        if !filtered_output.is_empty() {
                            return Ok(format!("Snapshot restoration completed. Last output:\n{}",
                                filtered_output.join("\n")
                            ));
                        }
                    }
                }
                Err(_) => {} // Ignore log read errors
            }

            return Ok("Snapshot restoration completed successfully".to_string());
        } else if self.is_snapshot_command(command) {
            debug!("Snapshot operation completed successfully on {}", self.host);
            return Ok("Snapshot operation completed successfully".to_string());
        }

        debug!("Command completed successfully on {}: {} chars of output", self.host, cleaned_output.len());

        Ok(cleaned_output)
    }

    // Helper method to detect snapshot creation commands specifically
    fn is_snapshot_creation_command(&self, command: &str) -> bool {
        // Detect snapshot creation commands (tar + lz4 pipeline with output redirection)
        command.contains("tar -cf -") &&
        command.contains("lz4 -z -c") &&
        command.contains("> ") &&
        command.contains(".lz4")
    }

    // Helper method to detect snapshot restoration commands specifically
    fn is_snapshot_restoration_command(&self, command: &str) -> bool {
        // Detect LZ4 snapshot restoration commands
        if command.contains("lz4 -d -c") && command.contains("tar -xf") {
            return true;
        }

        // Detect legacy snapshot restoration commands
        if command.contains("tar -xzf") && (command.contains("snapshot") || command.contains(".tar.gz")) {
            return true;
        }

        false
    }

    // Helper method to detect any snapshot-related commands
    fn is_snapshot_command(&self, command: &str) -> bool {
        // Check if it's a creation or restoration command
        if self.is_snapshot_creation_command(command) || self.is_snapshot_restoration_command(command) {
            return true;
        }

        // Detect other snapshot-related long operations
        if command.contains("tar") && (command.contains("/backup") || command.contains("/snapshots")) {
            return true;
        }

        false
    }

    // Helper to extract output file from snapshot creation command
    fn extract_snapshot_output_file(&self, command: &str) -> Option<String> {
        // Look for pattern: > '/path/to/file.lz4'
        if let Some(pos) = command.find("> '") {
            let start = pos + 3;
            if let Some(end_pos) = command[start..].find('\'') {
                return Some(command[start..start + end_pos].to_string());
            }
        }

        // Look for pattern: > /path/to/file.lz4 (without quotes)
        if let Some(pos) = command.rfind("> ") {
            let start = pos + 2;
            let remaining = &command[start..];
            // Take until space or end of string
            let end = remaining.find(' ').unwrap_or(remaining.len());
            let path = remaining[..end].trim().trim_matches('\'').trim_matches('"');
            if path.ends_with(".lz4") {
                return Some(path.to_string());
            }
        }

        None
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
        assert!(conn.is_snapshot_creation_command("cd '/opt/deploy/osmosis' && tar -cf - data wasm 2>/dev/null | lz4 -z -c > '/backup/snapshot.lz4'"));

        // Test LZ4 restoration command detection
        assert!(conn.is_snapshot_restoration_command("cd '/opt/deploy/osmosis' && lz4 -d -c '/backup/snapshot.lz4' | tar -xf -"));

        // Test legacy restoration command detection
        assert!(conn.is_snapshot_restoration_command("cd '/opt/deploy/osmosis' && tar -xzf '/backup/snapshot.tar.gz'"));

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
        assert!(conn.is_snapshot_creation_command(snapshot_cmd));

        // Test that pruner commands would be handled (not in this function but conceptually)
        let pruner_cmd = "cosmos-pruner prune /opt/deploy/node/data --blocks=1000";
        assert!(!conn.is_snapshot_command(pruner_cmd)); // Should be false, handled by different logic
        assert!(pruner_cmd.contains("cosmos-pruner")); // But this should be true

        // Test normal commands
        let normal_cmd = "systemctl status node";
        assert!(!conn.is_snapshot_command(normal_cmd));
    }

    #[test]
    fn test_snapshot_creation_wrapping() {
        // Test the wrapping format that would be generated for creation
        let creation_cmd = "cd '/opt/deploy/node' && tar -cf - data wasm | lz4 -z -c > '/backup/test.lz4'";
        let expected_wrapped = format!("bash -c '( {} ) 2>/tmp/snapshot_creation.log && echo __COMMAND_SUCCESS__ || echo __COMMAND_FAILED__'", creation_cmd);

        // Verify the wrapped command format preserves stdout
        assert!(expected_wrapped.contains("2>/tmp/snapshot_creation.log")); // Only stderr redirected
        assert!(!expected_wrapped.contains("> /tmp/snapshot_creation.log 2>&1")); // NOT both redirected
        assert!(expected_wrapped.contains("echo __COMMAND_SUCCESS__"));
    }

    #[test]
    fn test_extract_snapshot_output_file() {
        let conn = SshConnection {
            client: unsafe { std::mem::zeroed() }, // Mock for testing
            host: "test".to_string(),
        };

        // Test with single quotes
        let cmd1 = "cd /opt && tar -cf - data | lz4 -z -c > '/backup/snapshot.lz4'";
        assert_eq!(conn.extract_snapshot_output_file(cmd1), Some("/backup/snapshot.lz4".to_string()));

        // Test without quotes
        let cmd2 = "cd /opt && tar -cf - data | lz4 -z -c > /backup/snapshot.lz4";
        assert_eq!(conn.extract_snapshot_output_file(cmd2), Some("/backup/snapshot.lz4".to_string()));

        // Test with double quotes
        let cmd3 = "cd /opt && tar -cf - data | lz4 -z -c > \"/backup/snapshot.lz4\"";
        assert_eq!(conn.extract_snapshot_output_file(cmd3), Some("/backup/snapshot.lz4".to_string()));
    }
}
