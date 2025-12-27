//! Shared RPC utilities for network detection and block height fetching
//!
//! This module provides common RPC functionality used across the codebase.

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;
use tracing::info;

use crate::config::NodeConfig;

const RPC_TIMEOUT_SECS: u64 = 5;

/// Create a default HTTP client for RPC requests
fn create_default_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(RPC_TIMEOUT_SECS))
        .build()
        .map_err(|e| anyhow!("Failed to create HTTP client: {}", e))
}

/// Resolve the network for a node - either from config or by auto-detecting from RPC
pub async fn resolve_network(
    client: &Client,
    node_name: &str,
    node_config: &NodeConfig,
) -> Result<String> {
    if node_config.network.is_empty() || node_config.network == "auto" {
        info!(
            "Auto-detecting network for {} from RPC {}",
            node_name, node_config.rpc_url
        );
        let detected_network = fetch_network_from_rpc(client, &node_config.rpc_url).await?;
        info!(
            "✓ Auto-detected network for {}: {}",
            node_name, detected_network
        );
        Ok(detected_network)
    } else {
        Ok(node_config.network.clone())
    }
}

/// Fetch network ID from RPC /status endpoint (standalone version)
/// Creates its own HTTP client - use when a client is not available
pub async fn fetch_network_from_rpc_standalone(rpc_url: &str) -> Result<String> {
    let client = create_default_client()?;
    fetch_network_from_rpc(&client, rpc_url).await
}

/// Fetch network ID from RPC /status endpoint
/// Supports both Cosmos SDK format and Solana JSON-RPC format
pub async fn fetch_network_from_rpc(client: &Client, rpc_url: &str) -> Result<String> {
    // First try Cosmos SDK format (GET /status)
    let status_url = format!("{}/status", rpc_url);
    let response = client
        .get(&status_url)
        .timeout(Duration::from_secs(RPC_TIMEOUT_SECS))
        .send()
        .await;

    if let Ok(response) = response {
        if response.status().is_success() {
            if let Ok(json) = response.json::<Value>().await {
                // Extract network from Cosmos response: result.node_info.network
                if let Some(network) = json["result"]["node_info"]["network"].as_str() {
                    return Ok(network.to_string());
                }
            }
        }
    }

    // If Cosmos format failed, try Solana JSON-RPC format
    let solana_request = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "getVersion",
        "params": [],
        "id": 1
    });

    let response = client
        .post(rpc_url)
        .timeout(Duration::from_secs(RPC_TIMEOUT_SECS))
        .json(&solana_request)
        .send()
        .await
        .map_err(|e| anyhow!("Failed to fetch RPC version from {}: {}", rpc_url, e))?;

    if !response.status().is_success() {
        return Err(anyhow!("RPC returned HTTP {}", response.status()));
    }

    let json: Value = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse RPC response: {}", e))?;

    // If we got a valid Solana response, try to determine the cluster
    if json.get("result").is_some() {
        let network = if rpc_url.contains("mainnet") {
            "solana-mainnet".to_string()
        } else if rpc_url.contains("testnet") {
            "solana-testnet".to_string()
        } else if rpc_url.contains("devnet") {
            "solana-devnet".to_string()
        } else {
            "solana-mainnet".to_string()
        };
        return Ok(network);
    }

    Err(anyhow!("Could not detect network type from RPC"))
}

/// Fetch the current block height from RPC
pub async fn fetch_block_height_from_rpc(client: &Client, rpc_url: &str) -> Result<u64> {
    let status_url = format!("{}/status", rpc_url);

    let response = client
        .get(&status_url)
        .timeout(Duration::from_secs(RPC_TIMEOUT_SECS))
        .send()
        .await
        .map_err(|e| anyhow!("Failed to fetch RPC status from {}: {}", status_url, e))?;

    if !response.status().is_success() {
        return Err(anyhow!("RPC status returned HTTP {}", response.status()));
    }

    let json: Value = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse RPC status response: {}", e))?;

    // Extract block height: result.sync_info.latest_block_height
    let height_str = json["result"]["sync_info"]["latest_block_height"]
        .as_str()
        .ok_or_else(|| anyhow!("Block height not found in RPC response"))?;

    height_str
        .parse::<u64>()
        .map_err(|e| anyhow!("Invalid block height '{}': {}", height_str, e))
}

/// Determine the daemon binary based on network name
pub fn determine_daemon_binary(network: &str) -> String {
    match network {
        n if n.starts_with("pirin") || n.starts_with("nolus") => "nolusd".to_string(),
        n if n.starts_with("osmosis") => "osmosisd".to_string(),
        n if n.starts_with("neutron") => "neutrond".to_string(),
        n if n.starts_with("rila") => "rila".to_string(),
        n if n.starts_with("cosmos") => "gaiad".to_string(),
        n if n.starts_with("solana") => "agave-validator".to_string(),
        _ => format!("{}d", network.split('-').next().unwrap_or(network)),
    }
}
