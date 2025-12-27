//! Per-node log pattern monitoring

use super::types::HealthStatus;
use anyhow::{anyhow, Result};
use futures::future::join_all;
use reqwest::Client as HttpClient;
use std::time::Duration;
use tokio::time::timeout;
use tracing::{debug, error, info};

use crate::config::{Config, ServerConfig};
use crate::services::alert_service::AlertService;

/// Monitor logs for each healthy node
pub async fn monitor_logs_per_node(
    config: &Config,
    client: &HttpClient,
    health_statuses: &[&HealthStatus],
    alert_service: &AlertService,
) -> Result<()> {
    let context_lines_value: i32 = config.log_monitoring_context_lines.unwrap_or(2);
    let mut tasks = Vec::new();

    for status in health_statuses {
        if !status.is_healthy {
            continue;
        }

        let node_config = match config.nodes.get(&status.node_name) {
            Some(config) => config,
            None => continue,
        };

        if !node_config.log_monitoring_enabled.unwrap_or(false) {
            debug!("Log monitoring disabled for node: {}", status.node_name);
            continue;
        }

        let patterns = match &node_config.log_monitoring_patterns {
            Some(patterns) if !patterns.is_empty() => patterns,
            _ => {
                debug!(
                    "No log monitoring patterns configured for node: {}",
                    status.node_name
                );
                continue;
            }
        };

        let log_path = match &node_config.log_path {
            Some(path) => path,
            None => {
                debug!("No log path configured for node: {}", status.node_name);
                continue;
            }
        };

        let server_config = match config.servers.get(&status.server_host) {
            Some(config) => config.clone(),
            None => continue,
        };

        let node_name = status.node_name.clone();
        let server_host = status.server_host.clone();
        let log_path = log_path.clone();
        let patterns = patterns.clone();
        let client = client.clone();
        let alert_service = alert_service.clone();
        let context_lines = context_lines_value;

        let task = tokio::spawn(async move {
            check_node_logs(
                &client,
                &node_name,
                &server_host,
                &server_config,
                &log_path,
                &patterns,
                context_lines,
                &alert_service,
            )
            .await
        });

        tasks.push(task);
    }

    if tasks.is_empty() {
        debug!("No nodes have log monitoring enabled");
        return Ok(());
    }

    info!(
        "Running log monitoring for {} nodes with individual patterns",
        tasks.len()
    );

    let results = join_all(tasks).await;
    for result in results {
        if let Err(e) = result {
            error!("Log monitoring task failed: {}", e);
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn check_node_logs(
    client: &HttpClient,
    node_name: &str,
    server_host: &str,
    server_config: &ServerConfig,
    log_path: &str,
    patterns: &[String],
    context_lines: i32,
    alert_service: &AlertService,
) -> Result<()> {
    let command = format!(
        "tail -n 5000 {}/out1.log | grep -n -A {} -B {} -E '{}'",
        log_path,
        context_lines,
        context_lines,
        patterns.join("|")
    );

    debug!("Checking log patterns for {}: {:?}", node_name, patterns);

    match execute_log_command(client, server_config, &command).await {
        Ok(output) => {
            if !output.trim().is_empty() {
                info!("Log patterns detected for {}, sending alert", node_name);
                // Alert: log pattern match
                alert_service
                    .alert_log_pattern_match(node_name, server_host, log_path, &output, patterns)
                    .await?;
            } else {
                debug!("No log patterns found for {}", node_name);
            }
        }
        Err(e) => {
            debug!("Log monitoring for {} failed: {}", node_name, e);
        }
    }

    Ok(())
}

async fn execute_log_command(
    client: &HttpClient,
    server_config: &ServerConfig,
    command: &str,
) -> Result<String> {
    let agent_url = format!(
        "http://{}:{}/command/execute",
        server_config.host, server_config.agent_port
    );

    let payload = serde_json::json!({
        "command": command
    });

    let response = timeout(
        Duration::from_secs(server_config.request_timeout_seconds),
        client
            .post(&agent_url)
            .header("Authorization", format!("Bearer {}", server_config.api_key))
            .json(&payload)
            .send(),
    )
    .await
    .map_err(|_| anyhow!("Log command timeout"))?
    .map_err(|e| anyhow!("Log command request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "Log command returned status: {}",
            response.status()
        ));
    }

    let result: serde_json::Value = response.json().await?;
    let output = result
        .get("output")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(output)
}
