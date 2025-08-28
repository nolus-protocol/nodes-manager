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

pub async fn truncate_service_logs(service_name: &str, log_path: &str) -> Result<()> {
    info!("Truncating logs for service: {} at path: {}", service_name, log_path);

    // Step 1: Stop service
    systemctl::stop_service(service_name).await?;

    // Step 2: Truncate logs
    if let Err(e) = truncate_log_file(log_path).await {
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
        truncate_log_file(path).await?;
    } else {
        info!("No log path configured for {}, skipping log truncation", service_name);
    }
    Ok(())
}
