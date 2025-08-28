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

    let test_output = AsyncCommand::new("test")
        .arg("-d")
        .arg(log_path)
        .output()
        .await?;

    if test_output.status.success() {
        info!("Log path is a directory, truncating all log files within");
        truncate_log_directory(log_path).await
    } else {
        let file_test_output = AsyncCommand::new("test")
            .arg("-f")
            .arg(log_path)
            .output()
            .await?;

        if file_test_output.status.success() {
            info!("Log path is a file, truncating directly");
            truncate_log_file(log_path).await
        } else {
            warn!("Log path does not exist or is not accessible: {}", log_path);
            Err(anyhow!("Log path does not exist or is not accessible: {}", log_path))
        }
    }
}

pub async fn truncate_service_logs(service_name: &str, log_path: &str) -> Result<()> {
    info!("Truncating logs for service: {} at path: {}", service_name, log_path);

    systemctl::stop_service(service_name).await?;

    if let Err(e) = truncate_log_path(log_path).await {
        warn!("Log truncation failed: {}", e);
        if let Err(start_err) = systemctl::start_service(service_name).await {
            return Err(anyhow!(
                "Log truncation failed: {} AND service restart failed: {}",
                e, start_err
            ));
        }
        return Err(e);
    }

    systemctl::start_service(service_name).await?;

    info!("Service logs truncated successfully for: {}", service_name);
    Ok(())
}
