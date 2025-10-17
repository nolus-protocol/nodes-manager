// File: manager/src/state_sync/rpc_client.rs
use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSyncParams {
    pub rpc_servers: Vec<String>,
    pub trust_height: i64,
    pub trust_hash: String,
}

/// Fetch state sync parameters from RPC sources - FAIL FAST
pub async fn fetch_state_sync_params(
    rpc_sources: &[String],
    trust_height_offset: u32,
) -> Result<StateSyncParams> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let mut last_error = None;

    // Try each RPC source once - NO RETRY
    for rpc_url in rpc_sources {
        info!("Trying RPC source: {}", rpc_url);

        match try_fetch_from_rpc(&client, rpc_url, rpc_sources, trust_height_offset).await {
            Ok(params) => {
                info!("✓ Successfully fetched parameters from {}", rpc_url);
                return Ok(params);
            }
            Err(e) => {
                warn!("RPC {} failed: {}", rpc_url, e);
                last_error = Some(e);
            }
        }
    }

    // All RPCs failed - FAIL FAST
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("No RPC sources available")))
}

/// Try to fetch state sync parameters from a single RPC - FAIL FAST
/// Returns parameters with ALL configured RPC servers for redundancy
async fn try_fetch_from_rpc(
    client: &Client,
    rpc_url: &str,
    all_rpc_sources: &[String],
    trust_height_offset: u32,
) -> Result<StateSyncParams> {
    // Step 1: Get latest block height - FAIL FAST
    let latest_height = query_latest_height(client, rpc_url).await?;
    info!("Latest height from {}: {}", rpc_url, latest_height);

    // Step 2: Calculate trust height
    let trust_height = latest_height.saturating_sub(trust_height_offset as i64);
    info!(
        "Trust height: {} (latest {} - offset {})",
        trust_height, latest_height, trust_height_offset
    );

    // Step 3: Get trust hash at trust height - FAIL FAST
    let trust_hash = query_block_hash(client, rpc_url, trust_height).await?;
    info!("Trust hash at height {}: {}", trust_height, trust_hash);

    Ok(StateSyncParams {
        rpc_servers: all_rpc_sources.to_vec(),
        trust_height,
        trust_hash,
    })
}

/// Query latest block height from RPC
async fn query_latest_height(client: &Client, rpc_url: &str) -> Result<i64> {
    let url = format!("{}/block", rpc_url);

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query latest block: {}", e))?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "RPC returned status: {}",
            response.status()
        ));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse block response: {}", e))?;

    let height_str = json
        .get("result")
        .and_then(|r| r.get("block"))
        .and_then(|b| b.get("header"))
        .and_then(|h| h.get("height"))
        .and_then(|h| h.as_str())
        .ok_or_else(|| anyhow::anyhow!("Could not extract height from response"))?;

    let height = height_str
        .parse::<i64>()
        .map_err(|e| anyhow::anyhow!("Failed to parse height: {}", e))?;

    Ok(height)
}

/// Query block hash at specific height from RPC
async fn query_block_hash(client: &Client, rpc_url: &str, height: i64) -> Result<String> {
    let url = format!("{}/block?height={}", rpc_url, height);

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query block at height {}: {}", height, e))?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!(
            "RPC returned status: {}",
            response.status()
        ));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse block response: {}", e))?;

    let hash = json
        .get("result")
        .and_then(|r| r.get("block_id"))
        .and_then(|b| b.get("hash"))
        .and_then(|h| h.as_str())
        .ok_or_else(|| anyhow::anyhow!("Could not extract hash from response"))?;

    Ok(hash.to_string())
}
