//! Database record types (entities).
//!
//! This module contains all the record structs used by the database layer.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ============================================================================
// Configuration entities (for DB-backed configuration)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerRecord {
    pub id: String,
    pub name: String,
    pub host: String,
    pub agent_port: i64,
    pub api_key_ref: String, // Reference to secrets file, not the actual key
    pub request_timeout_seconds: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeRecord {
    pub id: String,
    pub name: String,
    pub server_id: String,
    pub network: String,
    pub rpc_url: String,
    pub enabled: bool,
    // Paths
    pub service_name: String,
    pub deploy_path: Option<String>,
    pub log_path: Option<String>,
    pub snapshot_backup_path: Option<String>,
    // Pruning settings
    pub pruning_enabled: bool,
    pub pruning_schedule: Option<String>,
    pub pruning_keep_blocks: Option<i64>,
    pub pruning_keep_versions: Option<i64>,
    // Snapshot settings
    pub snapshots_enabled: bool,
    pub snapshot_schedule: Option<String>,
    pub snapshot_retention_count: Option<i64>,
    pub auto_restore_enabled: bool,
    // State sync settings
    pub state_sync_enabled: bool,
    pub state_sync_schedule: Option<String>,
    pub state_sync_rpc_sources: Option<String>, // JSON array
    pub state_sync_trust_height_offset: Option<i64>,
    pub state_sync_max_sync_timeout_seconds: Option<i64>,
    // Log monitoring
    pub log_monitoring_enabled: bool,
    pub log_monitoring_patterns: Option<String>, // JSON array
    pub truncate_logs_enabled: bool,
    // Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesRecord {
    pub id: String,
    pub name: String,
    pub server_id: String,
    pub service_name: String,
    pub log_path: Option<String>,
    pub restart_schedule: Option<String>,
    pub dependent_nodes: Option<String>, // JSON array of node names
    pub truncate_logs_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSettingRecord {
    pub key: String,
    pub value: String,
    pub updated_at: DateTime<Utc>,
}

// ============================================================================
// Health and maintenance entities
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthRecord {
    pub node_name: String,
    pub is_healthy: bool,
    pub error_message: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub block_height: Option<i64>,
    pub is_syncing: Option<i32>,
    pub is_catching_up: Option<i32>,
    pub validator_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesHealthRecord {
    pub hermes_name: String,
    pub is_healthy: bool,
    pub status: String,
    pub uptime_seconds: Option<i64>,
    pub error_message: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub server_host: String,
    pub service_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceOperation {
    pub id: String,
    pub operation_type: String,
    pub target_name: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub details: Option<String>,
}
