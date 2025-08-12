// File: src/health/mod.rs

pub mod monitor;

pub use monitor::HealthMonitor;

use anyhow::Result;
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
    pub max_block_age_minutes: u64,
    pub min_peers: u32,
    pub max_consecutive_failures: u32,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            max_response_time_ms: 5000,
            max_block_age_minutes: 10,
            min_peers: 3,
            max_consecutive_failures: 3,
        }
    }
}

impl HealthMetrics {
    pub fn is_healthy(&self, thresholds: &HealthThresholds) -> bool {
        // Check response time
        if self.response_time_ms > thresholds.max_response_time_ms {
            return false;
        }

        // Check consecutive failures
        if self.consecutive_failures >= thresholds.max_consecutive_failures {
            return false;
        }

        // Check if node is catching up (might be acceptable for some time)
        if self.catching_up {
            // Allow catching up for a reasonable time
            return true; // We'll handle this in the monitor logic
        }

        // Check block age if we have block time
        if let Some(block_time) = self.block_time {
            let age = Utc::now().signed_duration_since(block_time);
            if age.num_minutes() > thresholds.max_block_age_minutes as i64 {
                return false;
            }
        }

        // Check minimum peers if available
        if let Some(peers) = self.peers_count {
            if peers < thresholds.min_peers {
                return false;
            }
        }

        true
    }
}

pub fn parse_rpc_response(response_text: &str) -> Result<RpcStatus> {
    // First try to parse as successful response
    if let Ok(status) = serde_json::from_str::<RpcStatus>(response_text) {
        return Ok(status);
    }

    // Try to parse as error response
    if let Ok(error) = serde_json::from_str::<RpcError>(response_text) {
        return Err(anyhow::anyhow!(
            "RPC Error {}: {}",
            error.error.code,
            error.error.message
        ));
    }

    // If neither works, return the raw response as error
    Err(anyhow::anyhow!("Invalid RPC response: {}", response_text))
}

pub fn parse_block_height(height_str: &str) -> Option<u64> {
    height_str.parse().ok()
}

pub fn parse_block_time(time_str: &str) -> Option<DateTime<Utc>> {
    // Try multiple timestamp formats

    // RFC3339 format (ISO 8601)
    if let Ok(dt) = DateTime::parse_from_rfc3339(time_str) {
        return Some(dt.with_timezone(&Utc));
    }

    // Custom format without timezone
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(time_str, "%Y-%m-%dT%H:%M:%S%.f") {
        return Some(DateTime::from_naive_utc_and_offset(dt, Utc));
    }

    // Try without fractional seconds
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(time_str, "%Y-%m-%dT%H:%M:%S") {
        return Some(DateTime::from_naive_utc_and_offset(dt, Utc));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_block_height() {
        assert_eq!(parse_block_height("12345"), Some(12345));
        assert_eq!(parse_block_height("0"), Some(0));
        assert_eq!(parse_block_height("invalid"), None);
    }

    #[test]
    fn test_parse_block_time() {
        let time1 = "2024-01-15T10:30:45.123456789Z";
        assert!(parse_block_time(time1).is_some());

        let time2 = "2024-01-15T10:30:45";
        assert!(parse_block_time(time2).is_some());

        assert!(parse_block_time("invalid").is_none());
    }

    #[test]
    fn test_health_metrics() {
        let metrics = HealthMetrics {
            node_name: "test".to_string(),
            network: "test".to_string(),
            block_height: Some(12345),
            block_time: Some(Utc::now()),
            catching_up: false,
            response_time_ms: 1000,
            peers_count: Some(10),
            last_check: Utc::now(),
            consecutive_failures: 0,
        };

        let thresholds = HealthThresholds::default();
        assert!(metrics.is_healthy(&thresholds));
    }
}
