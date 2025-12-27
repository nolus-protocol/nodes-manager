//! Cosmos SDK node health checking

use super::types::{BlockHeightState, HealthStatus, RpcResponse};
use anyhow::{anyhow, Result};
use chrono::Utc;
use reqwest::Client as HttpClient;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::config::NodeConfig;

/// Check Cosmos SDK node health
pub async fn check_cosmos_node_health(
    client: &HttpClient,
    node_name: &str,
    node_config: &NodeConfig,
    rpc_timeout_seconds: u64,
    block_height_states: &Arc<Mutex<HashMap<String, BlockHeightState>>>,
) -> Result<HealthStatus> {
    let mut status = HealthStatus {
        node_name: node_name.to_string(),
        rpc_url: node_config.rpc_url.clone(),
        is_healthy: false,
        error_message: None,
        last_check: Utc::now(),
        block_height: None,
        is_syncing: None,
        is_catching_up: false,
        validator_address: None,
        network: node_config.network.clone(),
        server_host: node_config.server_host.clone(),
        enabled: node_config.enabled,
        in_maintenance: false,
    };

    match fetch_node_status(client, &node_config.rpc_url, rpc_timeout_seconds).await {
        Ok(rpc_response) => {
            if let Some(result) = rpc_response.result {
                let current_height = result
                    .sync_info
                    .latest_block_height
                    .parse::<i64>()
                    .unwrap_or(0);
                let is_catching_up = result.sync_info.catching_up;

                status.block_height = Some(current_height);
                status.is_catching_up = is_catching_up;
                status.is_syncing = Some(is_catching_up);
                status.validator_address = Some(result.validator_info.address);

                let block_progression_healthy =
                    check_block_progression(node_name, current_height, block_height_states).await;

                status.is_healthy = block_progression_healthy || is_catching_up;

                if !status.is_healthy && !is_catching_up {
                    status.error_message = Some("Block height not progressing".to_string());
                }
            } else if let Some(error) = rpc_response.error {
                status.error_message = Some(format!("RPC Error: {}", error.message));
            } else {
                status.error_message = Some("Unknown RPC response format".to_string());
            }
        }
        Err(e) => {
            status.error_message = Some(e.to_string());
        }
    }

    Ok(status)
}

/// Fetch node status via Cosmos SDK RPC
async fn fetch_node_status(
    client: &HttpClient,
    rpc_url: &str,
    rpc_timeout_seconds: u64,
) -> Result<RpcResponse> {
    let request_body = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "status",
        "params": [],
        "id": Uuid::new_v4().to_string()
    });

    let response = timeout(
        Duration::from_secs(rpc_timeout_seconds),
        client.post(rpc_url).json(&request_body).send(),
    )
    .await
    .map_err(|_| anyhow!("RPC request timeout"))?
    .map_err(|e| anyhow!("HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "HTTP error {}: {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let rpc_response: RpcResponse = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse JSON response: {}", e))?;

    Ok(rpc_response)
}

/// Check block progression using baseline comparison approach
pub async fn check_block_progression(
    node_name: &str,
    current_height: i64,
    block_height_states: &Arc<Mutex<HashMap<String, BlockHeightState>>>,
) -> bool {
    let mut block_states = block_height_states.lock().await;
    let now = Utc::now();

    match block_states.get_mut(node_name) {
        None => {
            // First time checking this node - initialize and return healthy
            block_states.insert(
                node_name.to_string(),
                BlockHeightState {
                    last_height: current_height,
                    last_updated: now,
                    unhealthy_baseline_height: None,
                    unhealthy_since: None,
                },
            );
            debug!(
                "Initializing block height tracking for {} at height {}",
                node_name, current_height
            );
            true
        }
        Some(state) => {
            // If we have an unhealthy baseline, only recover if we exceed it
            if let Some(baseline_height) = state.unhealthy_baseline_height {
                if current_height > baseline_height {
                    // Recovered! Clear baseline and update state
                    state.last_height = current_height;
                    state.last_updated = now;
                    state.unhealthy_baseline_height = None;
                    state.unhealthy_since = None;
                    info!(
                        "Node {} RECOVERED - progressed beyond baseline {} to {}",
                        node_name, baseline_height, current_height
                    );
                    true
                } else {
                    // Still at or below baseline - remain unhealthy
                    state.last_height = current_height;
                    state.last_updated = now;
                    debug!(
                        "Node {} still unhealthy - height {} not above baseline {}",
                        node_name, current_height, baseline_height
                    );
                    false
                }
            } else {
                // No baseline set yet - check if we should set one
                if current_height > state.last_height {
                    // Height progressed - update and stay healthy
                    state.last_height = current_height;
                    state.last_updated = now;
                    debug!(
                        "Node {} progressed from {} to {} - staying healthy",
                        node_name, state.last_height, current_height
                    );
                    true
                } else {
                    // Height not progressing - check how long
                    let minutes_without_progress = (now - state.last_updated).num_minutes();

                    if minutes_without_progress >= 5 {
                        // Set baseline and become unhealthy
                        state.unhealthy_baseline_height = Some(current_height);
                        state.unhealthy_since = Some(now);
                        warn!(
                            "Setting unhealthy baseline for {} at height {} (no progress for {}m)",
                            node_name, current_height, minutes_without_progress
                        );
                        false
                    } else {
                        // Still in grace period - update last seen height but stay healthy
                        state.last_height = current_height;
                        debug!(
                            "Node {} no progress for {}m (grace period), staying healthy",
                            node_name, minutes_without_progress
                        );
                        true
                    }
                }
            }
        }
    }
}
