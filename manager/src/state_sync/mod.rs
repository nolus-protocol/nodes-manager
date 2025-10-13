//! State sync orchestration for rapid node synchronization
//!
//! This module provides automated state sync functionality to quickly sync blockchain nodes
//! from a trusted snapshot height instead of syncing from genesis.
//!
//! # Key Features
//!
//! - **Automated RPC Parameter Fetching**: Automatically fetches trust height and hash from RPC sources
//! - **Fail-Fast Design**: Immediate failure on any error with comprehensive logging
//! - **Multi-Chain Support**: Automatic daemon binary detection (nolusd, osmosisd, neutrond, etc.)
//! - **WASM Cache Management**: Smart cleanup of WASM cache during sync
//! - **Config Management**: Automatic state sync parameter injection and cleanup
//! - **Timeout Handling**: Configurable sync timeout with status monitoring
//!
//! # State Sync Process
//!
//! 1. Fetch latest block height from RPC
//! 2. Calculate trust height (latest - offset)
//! 3. Fetch trust hash at trust height
//! 4. Stop blockchain service
//! 5. Update config.toml with state sync parameters
//! 6. Execute unsafe-reset-all to wipe chain state
//! 7. Clean WASM cache (preserve blobs)
//! 8. Start blockchain service
//! 9. Wait for sync completion
//! 10. Disable state sync in config
//! 11. Restart service with clean config
//!
//! # Safety
//!
//! - All steps are fail-fast - any error stops the process
//! - Original config.toml is modified directly (backup recommended)
//! - Validator state is wiped (only for non-validators or during initial sync)

pub mod rpc_client;

// Re-export for easier access
pub use rpc_client::fetch_state_sync_params;

use anyhow::Result;
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info};

use crate::config::Config;
use crate::http::HttpAgentManager;

#[allow(dead_code)]
pub struct StateSyncManager {
    config: Arc<Config>,
}

#[allow(dead_code)]
impl StateSyncManager {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }

    /// Execute state sync for a node - FAIL FAST approach
    pub async fn execute_state_sync(
        &self,
        node_name: &str,
        http_manager: &HttpAgentManager,
    ) -> Result<()> {
        info!("🔄 Starting state sync for {}", node_name);

        // Get node configuration
        let node_config = self
            .config
            .nodes
            .get(node_name)
            .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_name))?;

        if !node_config.state_sync_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!("State sync not enabled for {}", node_name));
        }

        let rpc_sources = node_config.state_sync_rpc_sources.as_ref().ok_or_else(|| {
            anyhow::anyhow!("No RPC sources configured for state sync on {}", node_name)
        })?;

        let trust_height_offset = node_config.state_sync_trust_height_offset.unwrap_or(2000);
        let max_sync_timeout = node_config
            .state_sync_max_sync_timeout_seconds
            .unwrap_or(600);

        // Get required paths
        let home_dir = node_config
            .pruning_deploy_path
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No home directory configured for {}", node_name))?;
        let config_path = format!("{}/config/config.toml", home_dir);
        let service_name = &node_config.service_name;

        // Step 1: Fetch state sync parameters from RPC - FAIL FAST
        info!("Fetching state sync parameters from RPC sources");
        let sync_params = rpc_client::fetch_state_sync_params(rpc_sources, trust_height_offset)
            .await
            .map_err(|e| {
                error!("Failed to fetch state sync parameters: {}", e);
                e
            })?;

        info!(
            "✓ State sync parameters fetched: height={}, hash={}",
            sync_params.trust_height, sync_params.trust_hash
        );

        // Step 2: Determine daemon binary (chain-specific)
        let daemon_binary = self.determine_daemon_binary(&node_config.network);

        // Step 3: Execute state sync via agent - FAIL FAST
        info!(
            "Sending state sync request to agent on {}",
            node_config.server_host
        );

        let payload = json!({
            "service_name": service_name,
            "home_dir": home_dir,
            "config_path": config_path,
            "daemon_binary": daemon_binary,
            "rpc_servers": sync_params.rpc_servers,
            "trust_height": sync_params.trust_height,
            "trust_hash": sync_params.trust_hash,
            "timeout_seconds": max_sync_timeout,
            "log_path": node_config.log_path,
        });

        let result = http_manager
            .config
            .servers
            .get(&node_config.server_host)
            .ok_or_else(|| anyhow::anyhow!("Server {} not found", node_config.server_host))?;

        let agent_url = format!(
            "http://{}:{}/state-sync/execute",
            result.host, result.agent_port
        );

        let response = http_manager
            .client
            .post(&agent_url)
            .header("Authorization", format!("Bearer {}", result.api_key))
            .json(&payload)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "State sync failed with status {}: {}",
                status,
                error_text
            ));
        }

        let result: serde_json::Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

        if !result
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
        {
            let error_msg = result
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            return Err(anyhow::anyhow!("State sync failed: {}", error_msg));
        }

        // Handle async job polling if job_id returned
        if let Some(job_id) = result.get("job_id").and_then(|v| v.as_str()) {
            info!("State sync job started with ID: {}", job_id);
            self.poll_state_sync_job(http_manager, &node_config.server_host, job_id)
                .await?;
        }

        info!("✓ State sync completed successfully for {}", node_name);
        Ok(())
    }

    /// Poll for state sync job completion
    async fn poll_state_sync_job(
        &self,
        http_manager: &HttpAgentManager,
        server_host: &str,
        job_id: &str,
    ) -> Result<()> {
        use tokio::time::{sleep, Duration};

        let server_config = self
            .config
            .servers
            .get(server_host)
            .ok_or_else(|| anyhow::anyhow!("Server {} not found", server_host))?;

        let status_url = format!(
            "http://{}:{}/operation/status/{}",
            server_config.host, server_config.agent_port, job_id
        );

        const POLL_INTERVAL: u64 = 30; // Poll every 30 seconds
        let mut poll_count = 0;

        loop {
            poll_count += 1;
            info!("Polling state sync job status (poll #{})", poll_count);
            sleep(Duration::from_secs(POLL_INTERVAL)).await;

            let response = http_manager
                .client
                .get(&status_url)
                .header("Authorization", format!("Bearer {}", server_config.api_key))
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to poll job status: {}", e))?;

            if !response.status().is_success() {
                continue; // Keep polling on errors
            }

            let status_result: serde_json::Value = response
                .json()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to parse status response: {}", e))?;

            if let Some(job_status) = status_result.get("job_status").and_then(|v| v.as_str()) {
                match job_status {
                    "Completed" => {
                        info!("✓ State sync job completed");
                        return Ok(());
                    }
                    "Failed" => {
                        let error_msg = status_result
                            .get("error")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Job failed");
                        return Err(anyhow::anyhow!("State sync job failed: {}", error_msg));
                    }
                    "Running" => {
                        info!("State sync still running...");
                        continue;
                    }
                    _ => continue,
                }
            }
        }
    }

    /// Determine daemon binary based on network name
    fn determine_daemon_binary(&self, network: &str) -> String {
        // Map network names to daemon binaries
        match network {
            n if n.starts_with("pirin") || n.starts_with("nolus") => "nolusd".to_string(),
            n if n.starts_with("osmosis") => "osmosisd".to_string(),
            n if n.starts_with("neutron") => "neutrond".to_string(),
            n if n.starts_with("rila") => "rila".to_string(),
            n if n.starts_with("cosmos") => "gaiad".to_string(),
            _ => {
                // Default: try to extract daemon name from network
                format!("{}d", network.split('-').next().unwrap_or(network))
            }
        }
    }
}
