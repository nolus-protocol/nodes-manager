// File: agent/src/services/logs.rs
use anyhow::{anyhow, Result};
use tokio::process::Command as AsyncCommand;
use tracing::{info, warn};

use super::systemctl;

pub async fn truncate_log_file(log_path: &str) -> Result<()> {
    info!("Truncating log file: {}", log_path);

    let output = AsyncCommand::new("sudo")
        .arg("truncate")
        .arg("-s")
        .arg("0")
        .arg(log_path)
        .output()
        .await?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to truncate log file {}: {}", log_path, error));
    }

    info!("Log file truncated successfully: {}", log_path);
    Ok(())
}

pub async fn truncate_log_directory(log_dir: &str) -> Result<()> {
    info!("Truncating all log files in directory: {}", log_dir);

    // Find all log files in the directory and truncate them
    let find_command = format!(
        "find '{}' -maxdepth 1 -type f \\( -name '*.log' -o -name 'out*.log' -o -name 'error*.log' \\) -exec sudo truncate -s 0 {{}} \\;",
        log_dir
    );

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&find_command)
        .output()
        .await?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to truncate logs in directory {}: {}", log_dir, error));
    }

    // Also list what files were found for logging
    let list_command = format!(
        "find '{}' -maxdepth 1 -type f \\( -name '*.log' -o -name 'out*.log' -o -name 'error*.log' \\) -print",
        log_dir
    );

    let list_output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&list_command)
        .output()
        .await?;

    let files_found = String::from_utf8_lossy(&list_output.stdout);
    if !files_found.trim().is_empty() {
        info!("Truncated log files:\n{}", files_found.trim());
    } else {
        warn!("No log files found in directory: {}", log_dir);
    }

    Ok(())
}

pub async fn truncate_log_path(log_path: &str) -> Result<()> {
    info!("Checking log path type: {}", log_path);

    // Check if path is a directory or file
    let test_output = AsyncCommand::new("test")
        .arg("-d")
        .arg(log_path)
        .output()
        .await?;

    if test_output.status.success() {
        // It's a directory
        info!("Log path is a directory, truncating all log files within");
        truncate_log_directory(log_path).await
    } else {
        // Check if it's a file
        let file_test_output = AsyncCommand::new("test")
            .arg("-f")
            .arg(log_path)
            .output()
            .await?;

        if file_test_output.status.success() {
            // It's a file
            info!("Log path is a file, truncating directly");
            truncate_log_file(log_path).await
        } else {
            // Path doesn't exist or is neither file nor directory
            warn!("Log path does not exist or is not accessible: {}", log_path);
            Err(anyhow!("Log path does not exist or is not accessible: {}", log_path))
        }
    }
}

pub async fn truncate_service_logs(service_name: &str, log_path: &str) -> Result<()> {
    info!("Truncating logs for service: {} at path: {}", service_name, log_path);

    // Step 1: Stop service
    systemctl::stop_service(service_name).await?;

    // Step 2: Truncate logs (handles both files and directories)
    if let Err(e) = truncate_log_path(log_path).await {
        // Try to restart service even if log truncation failed
        warn!("Log truncation failed: {}", e);
        if let Err(start_err) = systemctl::start_service(service_name).await {
            return Err(anyhow!(
                "Log truncation failed: {} AND service restart failed: {}",
                e, start_err
            ));
        }
        return Err(e);
    }

    // Step 3: Start service
    systemctl::start_service(service_name).await?;

    info!("Service logs truncated successfully for: {}", service_name);
    Ok(())
}

pub async fn truncate_logs_if_configured(service_name: &str, log_path: Option<&String>) -> Result<()> {
    if let Some(path) = log_path {
        info!("Truncating logs for {} at: {}", service_name, path);
        truncate_log_path(path).await?;
    } else {
        info!("No log path configured for {}, skipping log truncation", service_name);
    }
    Ok(())
}
