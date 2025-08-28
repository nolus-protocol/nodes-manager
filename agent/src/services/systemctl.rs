// File: agent/src/services/systemctl.rs
use anyhow::{anyhow, Result};
use tokio::process::Command as AsyncCommand;
use tracing::{debug, info};

pub async fn get_service_status(service_name: &str) -> Result<String> {
    debug!("Checking service status: {}", service_name);

    let output = AsyncCommand::new("systemctl")
        .arg("is-active")
        .arg(service_name)
        .output()
        .await?;

    let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(status)
}

pub async fn start_service(service_name: &str) -> Result<()> {
    info!("Starting service: {}", service_name);

    let output = AsyncCommand::new("sudo")
        .arg("systemctl")
        .arg("start")
        .arg(service_name)
        .output()
        .await?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to start service {}: {}", service_name, error));
    }

    info!("Service {} started successfully", service_name);
    Ok(())
}

pub async fn stop_service(service_name: &str) -> Result<()> {
    info!("Stopping service: {}", service_name);

    let output = AsyncCommand::new("sudo")
        .arg("systemctl")
        .arg("stop")
        .arg(service_name)
        .output()
        .await?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Failed to stop service {}: {}", service_name, error));
    }

    info!("Service {} stopped successfully", service_name);
    Ok(())
}

pub async fn get_service_uptime(service_name: &str) -> Result<u64> {
    debug!("Getting service uptime: {}", service_name);

    let output = AsyncCommand::new("systemctl")
        .arg("show")
        .arg(service_name)
        .arg("--property=ActiveEnterTimestamp")
        .arg("--value")
        .output()
        .await?;

    let timestamp_str = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if timestamp_str.is_empty() || timestamp_str == "n/a" {
        return Ok(0);
    }

    let date_output = AsyncCommand::new("date")
        .arg("-d")
        .arg(&timestamp_str)
        .arg("+%s")
        .output()
        .await?;

    let start_time_str = String::from_utf8_lossy(&date_output.stdout).trim().to_string();
    let start_time = start_time_str.parse::<i64>()
        .map_err(|_| anyhow!("Failed to parse timestamp"))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let uptime_seconds = (now - start_time).max(0) as u64;
    Ok(uptime_seconds)
}
