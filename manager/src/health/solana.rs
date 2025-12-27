//! Solana node health checking

use super::cosmos::check_block_progression;
use super::types::{BlockHeightState, HealthStatus, SolanaRpcResponse};
use anyhow::{anyhow, Result};
use chrono::Utc;
use reqwest::Client as HttpClient;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;

use crate::config::NodeConfig;

/// Check if a network is Solana-based
pub fn is_solana_network(network: &str) -> bool {
    let network_lower = network.to_lowercase();
    network_lower.starts_with("solana")
        || network_lower == "mainnet-beta"
        || network_lower == "testnet"
        || network_lower == "devnet"
}

/// Check Solana node health
pub async fn check_solana_node_health(
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

    // Try to get health status and slot number in parallel
    let health_result =
        fetch_solana_health(client, &node_config.rpc_url, rpc_timeout_seconds).await;
    let slot_result = fetch_solana_slot(client, &node_config.rpc_url, rpc_timeout_seconds).await;

    match (health_result, slot_result) {
        (Ok(_), Ok(current_slot)) => {
            // Both health and slot succeeded
            status.block_height = Some(current_slot);

            // Check slot progression (similar to block progression for Cosmos)
            let slot_progression_healthy =
                check_block_progression(node_name, current_slot, block_height_states).await;

            // Solana's getHealth returns "ok" if healthy, so we combine with slot progression
            status.is_healthy = slot_progression_healthy;
            status.is_syncing = Some(!slot_progression_healthy);
            status.is_catching_up = !slot_progression_healthy;

            if !status.is_healthy {
                status.error_message = Some("Slot height not progressing".to_string());
            }
        }
        (Ok(_), Err(slot_err)) => {
            // Health OK but slot fetch failed
            status.error_message = Some(format!("Failed to get slot: {}", slot_err));
        }
        (Err(health_err), Ok(current_slot)) => {
            // Health check failed but got slot
            status.block_height = Some(current_slot);
            status.error_message = Some(format!("Health check failed: {}", health_err));
        }
        (Err(health_err), Err(_)) => {
            // Both failed
            status.error_message = Some(format!("Health check failed: {}", health_err));
        }
    }

    Ok(status)
}

/// Fetch Solana health status via getHealth RPC method
async fn fetch_solana_health(
    client: &HttpClient,
    rpc_url: &str,
    rpc_timeout_seconds: u64,
) -> Result<String> {
    let request_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getHealth"
    });

    let response = timeout(
        Duration::from_secs(rpc_timeout_seconds),
        client.post(rpc_url).json(&request_body).send(),
    )
    .await
    .map_err(|_| anyhow!("Solana RPC request timeout"))?
    .map_err(|e| anyhow!("Solana HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "HTTP error {}: {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let rpc_response: SolanaRpcResponse = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse Solana JSON response: {}", e))?;

    if let Some(error) = rpc_response.error {
        return Err(anyhow!("Solana RPC Error: {}", error.message));
    }

    // getHealth returns "ok" string on success
    if let Some(result) = rpc_response.result {
        if result.is_string() && result.as_str() == Some("ok") {
            return Ok("ok".to_string());
        }
    }

    Err(anyhow!("Unexpected Solana health response format"))
}

/// Fetch Solana current slot via getSlot RPC method
async fn fetch_solana_slot(
    client: &HttpClient,
    rpc_url: &str,
    rpc_timeout_seconds: u64,
) -> Result<i64> {
    let request_body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "getSlot"
    });

    let response = timeout(
        Duration::from_secs(rpc_timeout_seconds),
        client.post(rpc_url).json(&request_body).send(),
    )
    .await
    .map_err(|_| anyhow!("Solana slot request timeout"))?
    .map_err(|e| anyhow!("Solana slot HTTP request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(anyhow!(
            "HTTP error {}: {}",
            response.status(),
            response.text().await.unwrap_or_default()
        ));
    }

    let rpc_response: SolanaRpcResponse = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse Solana slot JSON response: {}", e))?;

    if let Some(error) = rpc_response.error {
        return Err(anyhow!("Solana RPC Error: {}", error.message));
    }

    if let Some(result) = rpc_response.result {
        if let Some(slot) = result.as_u64() {
            return Ok(slot as i64);
        }
    }

    Err(anyhow!("Unexpected Solana slot response format"))
}
