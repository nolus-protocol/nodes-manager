// File: manager/src/health/mod.rs

pub mod monitor;

pub use monitor::HealthMonitor;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcStatus {
    pub jsonrpc: String,
    pub id: i32,
    pub result: RpcResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResult {
    pub node_info: NodeInfo,
    pub sync_info: SyncInfo,
    pub validator_info: ValidatorInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub protocol_version: ProtocolVersion,
    pub id: String,
    pub listen_addr: String,
    pub network: String,
    pub version: String,
    pub channels: String,
    pub moniker: String,
    pub other: NodeOther,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub p2p: String,
    pub block: String,
    pub app: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeOther {
    pub tx_index: String,
    pub rpc_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncInfo {
    pub latest_block_hash: String,
    pub latest_app_hash: String,
    pub latest_block_height: String,
    pub latest_block_time: String,
    pub earliest_block_hash: String,
    pub earliest_app_hash: String,
    pub earliest_block_height: String,
    pub earliest_block_time: String,
    pub catching_up: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    pub address: String,
    pub pub_key: PubKey,
    pub voting_power: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubKey {
    #[serde(rename = "type")]
    pub key_type: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub jsonrpc: String,
    pub id: i32,
    pub error: ErrorDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetails {
    pub code: i32,
    pub message: String,
    pub data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetrics {
    pub node_name: String,
    pub network: String,
    pub block_height: Option<u64>,
    pub block_time: Option<DateTime<Utc>>,
    pub catching_up: bool,
    pub response_time_ms: u64,
    pub peers_count: Option<u32>,
    pub last_check: DateTime<Utc>,
    pub consecutive_failures: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthThresholds {
    pub max_response_time_ms: u64,
    pub max_consecutive_failures: u32,
    pub min_block_progression_checks: u32,
    pub block_stuck_threshold_minutes: u64,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            max_response_time_ms: 5000,
            max_consecutive_failures: 3,
            min_block_progression_checks: 3,
            block_stuck_threshold_minutes: 10,
        }
    }
}
