//! Auto-restore trigger monitoring

use super::types::{AutoRestoreCooldown, HealthStatus};
use anyhow::{anyhow, Result};
use chrono::Utc;
use futures::future::join_all;
use reqwest::Client as HttpClient;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

use crate::config::{Config, ServerConfig};
use crate::services::alert_service::AlertService;
use crate::snapshot::SnapshotManager;

/// Monitor auto-restore triggers for unhealthy nodes
pub async fn monitor_auto_restore_triggers(
    config: &Config,
    client: &HttpClient,
    health_statuses: &[HealthStatus],
    auto_restore_cooldowns: &Arc<Mutex<HashMap<String, AutoRestoreCooldown>>>,
    auto_restore_checked_states: &Arc<Mutex<HashMap<String, bool>>>,
    snapshot_manager: &SnapshotManager,
    alert_service: &AlertService,
) -> Result<()> {
    let trigger_words = match &config.auto_restore_trigger_words {
        Some(words) if !words.is_empty() => words,
        _ => return Ok(()),
    };

    let unhealthy_nodes: Vec<_> = health_statuses
        .iter()
        .filter(|status| !status.is_healthy && status.enabled && !status.in_maintenance)
        .collect();

    if unhealthy_nodes.is_empty() {
        debug!("No unhealthy nodes to check for auto-restore triggers");
        return Ok(());
    }

    info!(
        "Checking auto-restore triggers for {} unhealthy nodes",
        unhealthy_nodes.len()
    );

    let mut tasks = Vec::new();

    for status in unhealthy_nodes {
        if has_already_checked_auto_restore(&status.node_name, auto_restore_checked_states).await {
            debug!(
                "Auto-restore triggers already checked for {} during current unhealthy state",
                status.node_name
            );
            continue;
        }

        let node_config = match config.nodes.get(&status.node_name) {
            Some(config) => config,
            None => continue,
        };

        if !node_config.auto_restore_enabled.unwrap_or(false) {
            continue;
        }

        let log_path = match &node_config.log_path {
            Some(path) => path,
            None => continue,
        };

        let server_config = match config.servers.get(&status.server_host) {
            Some(config) => config.clone(),
            None => continue,
        };

        let node_name = status.node_name.clone();
        let server_host = status.server_host.clone();
        let log_path = log_path.clone();
        let trigger_words = trigger_words.clone();
        let client = client.clone();
        let auto_restore_cooldowns = Arc::clone(auto_restore_cooldowns);
        let auto_restore_checked_states = Arc::clone(auto_restore_checked_states);
        let snapshot_manager = snapshot_manager.clone();
        let alert_service = alert_service.clone();

        let task = tokio::spawn(async move {
            check_auto_restore_triggers_for_node(
                &client,
                &node_name,
                &server_host,
                &server_config,
                &log_path,
                &trigger_words,
                &auto_restore_cooldowns,
                &auto_restore_checked_states,
                &snapshot_manager,
                &alert_service,
            )
            .await
        });

        tasks.push(task);
    }

    let results = join_all(tasks).await;
    for result in results {
        if let Err(e) = result {
            error!("Auto-restore trigger check task failed: {}", e);
        }
    }

    Ok(())
}

async fn has_already_checked_auto_restore(
    node_name: &str,
    auto_restore_checked_states: &Arc<Mutex<HashMap<String, bool>>>,
) -> bool {
    let checked_states = auto_restore_checked_states.lock().await;
    checked_states.get(node_name).copied().unwrap_or(false)
}

async fn mark_auto_restore_checked(
    node_name: &str,
    auto_restore_checked_states: &Arc<Mutex<HashMap<String, bool>>>,
) {
    let mut checked_states = auto_restore_checked_states.lock().await;
    checked_states.insert(node_name.to_string(), true);
    debug!("Marked {} as checked for auto-restore triggers", node_name);
}

#[allow(clippy::too_many_arguments)]
async fn check_auto_restore_triggers_for_node(
    client: &HttpClient,
    node_name: &str,
    server_host: &str,
    server_config: &ServerConfig,
    log_path: &str,
    trigger_words: &[String],
    auto_restore_cooldowns: &Arc<Mutex<HashMap<String, AutoRestoreCooldown>>>,
    auto_restore_checked_states: &Arc<Mutex<HashMap<String, bool>>>,
    snapshot_manager: &SnapshotManager,
    alert_service: &AlertService,
) -> Result<()> {
    if !is_auto_restore_allowed(node_name, auto_restore_cooldowns).await {
        debug!("Auto-restore for {} is in cooldown period", node_name);
        mark_auto_restore_checked(node_name, auto_restore_checked_states).await;
        return Ok(());
    }

    let log_file = format!("{}/out1.log", log_path);
    let command = format!(
        "tail -n 500 '{}' | grep -q -E '{}'",
        log_file,
        trigger_words.join("|")
    );

    mark_auto_restore_checked(node_name, auto_restore_checked_states).await;

    match execute_log_command(client, server_config, &command).await {
        Ok(_) => {
            warn!(
                "Auto-restore trigger words found in {} log file: {}",
                node_name, log_file
            );
            // execute_auto_restore handles all alerting internally
            if let Err(e) = execute_auto_restore(
                node_name,
                server_host,
                trigger_words,
                auto_restore_cooldowns,
                snapshot_manager,
                alert_service,
            )
            .await
            {
                error!("Auto-restore failed for {}: {}", node_name, e);
            } else {
                info!("Auto-restore completed successfully for {}", node_name);
            }
        }
        Err(_) => {
            debug!("No auto-restore trigger words found for {}", node_name);
        }
    }

    Ok(())
}

async fn is_auto_restore_allowed(
    node_name: &str,
    auto_restore_cooldowns: &Arc<Mutex<HashMap<String, AutoRestoreCooldown>>>,
) -> bool {
    let cooldowns = auto_restore_cooldowns.lock().await;
    let now = Utc::now();

    match cooldowns.get(node_name) {
        Some(cooldown) => {
            let hours_since_last = (now - cooldown.last_restore_attempt).num_hours();
            if hours_since_last >= 2 {
                true
            } else {
                debug!(
                    "Auto-restore for {} is in cooldown ({}h remaining)",
                    node_name,
                    2 - hours_since_last
                );
                false
            }
        }
        None => true,
    }
}

async fn execute_auto_restore(
    node_name: &str,
    server_host: &str,
    trigger_words: &[String],
    auto_restore_cooldowns: &Arc<Mutex<HashMap<String, AutoRestoreCooldown>>>,
    snapshot_manager: &SnapshotManager,
    alert_service: &AlertService,
) -> Result<()> {
    info!(
        "Executing auto-restore for {} due to trigger words: {:?}",
        node_name, trigger_words
    );

    {
        let mut cooldowns = auto_restore_cooldowns.lock().await;
        let now = Utc::now();
        let cooldown = cooldowns
            .entry(node_name.to_string())
            .or_insert(AutoRestoreCooldown {
                last_restore_attempt: now,
                restore_count: 0,
            });
        cooldown.last_restore_attempt = now;
        cooldown.restore_count += 1;
    }

    // Alert: auto-restore started
    alert_service
        .alert_auto_restore_started(node_name, server_host, trigger_words)
        .await?;

    match snapshot_manager.restore_from_snapshot(node_name).await {
        Ok(snapshot_info) => {
            info!(
                "Auto-restore completed for {} using snapshot: {}",
                node_name, snapshot_info.filename
            );
            // Alert: auto-restore completed
            alert_service
                .alert_auto_restore_completed(
                    node_name,
                    server_host,
                    &snapshot_info.filename,
                    trigger_words,
                )
                .await?;
            Ok(())
        }
        Err(e) => {
            error!("Auto-restore failed for {}: {}", node_name, e);
            // Alert: auto-restore failed
            alert_service
                .alert_auto_restore_failed(node_name, server_host, &e.to_string(), trigger_words)
                .await?;
            Err(e)
        }
    }
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

/// Clear auto-restore checked state for a node (called when node becomes healthy)
pub async fn clear_auto_restore_checked_state(
    node_name: &str,
    auto_restore_checked_states: &Arc<Mutex<HashMap<String, bool>>>,
) {
    let mut checked_states = auto_restore_checked_states.lock().await;
    if checked_states.remove(node_name).is_some() {
        debug!(
            "Cleared auto-restore checked state for {} (node is now healthy)",
            node_name
        );
    }
}
