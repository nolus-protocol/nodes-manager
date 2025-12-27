//! Health monitoring types and RPC response structures

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Health status for a blockchain node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub node_name: String,
    pub rpc_url: String,
    pub is_healthy: bool,
    pub error_message: Option<String>,
    pub last_check: DateTime<Utc>,
    pub block_height: Option<i64>,
    pub is_syncing: Option<bool>,
    pub is_catching_up: bool,
    pub validator_address: Option<String>,
    pub network: String,
    pub server_host: String,
    pub enabled: bool,
    pub in_maintenance: bool,
}

/// Cosmos SDK RPC response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: String,
    pub result: Option<StatusResult>,
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResult {
    pub node_info: NodeInfo,
    pub sync_info: SyncInfo,
    pub validator_info: ValidatorInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub network: String,
    pub moniker: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncInfo {
    pub latest_block_height: String,
    pub catching_up: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    pub address: String,
    pub voting_power: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

/// Solana-specific RPC response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolanaRpcResponse {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: serde_json::Value, // Can be string or number
    pub result: Option<serde_json::Value>,
    pub error: Option<RpcError>,
}

/// Auto-restore cooldown tracking
#[derive(Debug, Clone)]
pub struct AutoRestoreCooldown {
    pub last_restore_attempt: DateTime<Utc>,
    pub restore_count: u32,
}

/// Block height tracking for progression detection
#[derive(Debug, Clone)]
pub struct BlockHeightState {
    pub last_height: i64,
    pub last_updated: DateTime<Utc>,
    pub unhealthy_baseline_height: Option<i64>,
    pub unhealthy_since: Option<DateTime<Utc>>,
}
