// File: manager/src/config/mod.rs
pub mod manager;
pub use manager::ConfigManager;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub check_interval_seconds: u64,
    pub rpc_timeout_seconds: u64,
    pub alarm_webhook_url: String,
    pub hermes_min_uptime_minutes: Option<u32>,
    pub auto_restore_trigger_words: Option<Vec<String>>,
    pub log_monitoring_context_lines: Option<i32>,
    // Populated from individual server config files
    #[serde(skip)]
    pub servers: HashMap<String, ServerConfig>,
    #[serde(skip)]
    pub nodes: HashMap<String, NodeConfig>,
    #[serde(skip)]
    pub hermes: HashMap<String, HermesConfig>,
    #[serde(skip)]
    pub etl: HashMap<String, EtlConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub agent_port: u16,
    pub api_key: String,
    #[serde(default = "default_request_timeout")]
    pub request_timeout_seconds: u64,
    pub max_concurrent_requests: Option<usize>,
}

fn default_request_timeout() -> u64 {
    300 // 5 minutes default, but we won't use it
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfigFile {
    pub server: ServerConfig,
    #[serde(default)]
    pub defaults: Option<NodeDefaults>,
    pub nodes: HashMap<String, NodeConfig>,
    pub hermes: Option<HashMap<String, HermesConfig>>,
    pub etl: Option<HashMap<String, EtlConfig>>,
}

/// Server-level path configuration for auto-derivation
/// These define WHERE your nodes are deployed, not WHAT settings they have
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeDefaults {
    /// Base deployment directory (e.g., "/opt/deploy/nolus")
    /// If set, derives: deploy_path = "{base_deploy_path}/{service_name}"
    pub base_deploy_path: Option<String>,

    /// Base log directory (e.g., "/var/log")
    /// If set, derives: log_path = "{base_log_path}/{service_name}"
    pub base_log_path: Option<String>,

    /// Base backup directory (e.g., "/home/backup/snapshots")
    /// If set, derives: snapshot_backup_path = "{base_backup_path}"
    pub base_backup_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub rpc_url: String,
    /// Network ID - can be omitted or set to "auto" for auto-detection from RPC
    #[serde(default)]
    pub network: String,
    pub server_host: String,
    pub enabled: bool,
    /// Service name - MANDATORY for path auto-derivation
    /// This is the systemd service name and base for all derived paths
    /// Example: "full-node-3" will derive:
    /// - deploy_path: {base_deploy_path}/full-node-3
    /// - log_path: {base_log_path}/full-node-3
    pub service_name: String,

    // Deployment path - home directory for the node
    // Auto-derived from base_deploy_path + service_name
    // Example: /opt/deploy/nolus/full-node-3
    pub deploy_path: Option<String>,

    // Pruning configuration
    pub pruning_enabled: Option<bool>,
    pub pruning_schedule: Option<String>,
    pub pruning_keep_blocks: Option<u32>,
    pub pruning_keep_versions: Option<u32>,

    // Log configuration
    pub log_path: Option<String>,
    pub truncate_logs_enabled: Option<bool>,
    // Per-node log monitoring configuration
    pub log_monitoring_enabled: Option<bool>,
    pub log_monitoring_patterns: Option<Vec<String>>,

    // Snapshot configuration
    pub snapshots_enabled: Option<bool>,
    pub snapshot_backup_path: Option<String>,
    pub auto_restore_enabled: Option<bool>,
    pub snapshot_schedule: Option<String>,
    pub snapshot_retention_count: Option<usize>,

    // NEW: State sync configuration (flat, following existing patterns)
    pub state_sync_enabled: Option<bool>,
    pub state_sync_schedule: Option<String>,
    pub state_sync_rpc_sources: Option<Vec<String>>,
    #[serde(default = "default_state_sync_trust_height_offset")]
    pub state_sync_trust_height_offset: Option<u32>,
    #[serde(default = "default_state_sync_max_sync_timeout")]
    pub state_sync_max_sync_timeout_seconds: Option<u64>,
}

fn default_state_sync_trust_height_offset() -> Option<u32> {
    Some(2000)
}

fn default_state_sync_max_sync_timeout() -> Option<u64> {
    Some(1800) // 30 minutes
}

impl NodeConfig {
    /// Apply smart defaults and derive paths from service_name + base paths
    /// Base paths come from server config, service_name determines the service subdirectory
    pub fn with_defaults(mut self, defaults: &Option<NodeDefaults>, _node_name: &str) -> Self {
        // Use service_name directly for all path derivations
        let service_name = &self.service_name;

        // Auto-derive deploy_path (home directory) if not set
        // Uses base_deploy_path from server config if available
        // Example: /opt/deploy/nolus/full-node-3
        if self.deploy_path.is_none() {
            if let Some(defaults) = defaults {
                if let Some(ref base) = defaults.base_deploy_path {
                    self.deploy_path = Some(format!("{}/{}", base, service_name));
                }
            }
        }

        // Auto-derive log_path if not set
        // Uses base_log_path from server config if available
        if self.log_path.is_none() {
            if let Some(defaults) = defaults {
                if let Some(ref base) = defaults.base_log_path {
                    self.log_path = Some(format!("{}/{}", base, service_name));
                }
            }
        }

        // Auto-derive snapshot_backup_path if not set
        // Uses base_backup_path from server config if available (shared across all nodes)
        if self.snapshot_backup_path.is_none() {
            if let Some(defaults) = defaults {
                if let Some(ref base) = defaults.base_backup_path {
                    self.snapshot_backup_path = Some(base.clone());
                }
            }
        }

        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesConfig {
    pub server_host: String,
    pub service_name: String,
    pub log_path: Option<String>,
    pub restart_schedule: Option<String>,
    pub dependent_nodes: Option<Vec<String>>,
    pub truncate_logs_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtlConfig {
    pub server_host: String,
    pub host: String,
    pub port: u16,
    pub endpoint: Option<String>,
    pub enabled: bool,
    pub timeout_seconds: Option<u64>,
    pub description: Option<String>,
}
