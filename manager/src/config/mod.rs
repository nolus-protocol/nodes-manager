// File: manager/src/config/mod.rs
pub mod manager;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
pub use manager::ConfigManager;

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
    /// Base deployment directory (e.g., "/opt/deploy")
    /// If set, derives: pruning_deploy_path = "{base_deploy_path}/{service_name}/data"
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
    // Pruning configuration
    pub pruning_enabled: Option<bool>,
    pub pruning_schedule: Option<String>,
    pub pruning_keep_blocks: Option<u32>,
    pub pruning_keep_versions: Option<u32>,
    pub pruning_deploy_path: Option<String>,
    pub pruning_service_name: Option<String>,
    // Log configuration
    pub log_path: Option<String>,
    pub truncate_logs_enabled: Option<bool>,
    // Per-node log monitoring configuration
    pub log_monitoring_enabled: Option<bool>,
    pub log_monitoring_patterns: Option<Vec<String>>,
    // Snapshot configuration
    pub snapshots_enabled: Option<bool>,
    pub snapshot_backup_path: Option<String>,
    pub snapshot_deploy_path: Option<String>,
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
    Some(600) // 10 minutes
}

impl NodeConfig {
    /// Apply smart defaults and derive paths from node name + base paths
    /// Base paths come from server config, node name determines the service subdirectory
    pub fn with_defaults(mut self, defaults: &Option<NodeDefaults>, node_name: &str) -> Self {
        // Extract the simple node name (remove server prefix if present)
        // Example: "enterprise-osmosis" -> "osmosis"
        let simple_name = self.extract_simple_name(node_name);
        
        // Auto-derive pruning_service_name if not set
        if self.pruning_service_name.is_none() {
            self.pruning_service_name = Some(simple_name.clone());
        }
        
        // Auto-derive pruning_deploy_path if not set
        // Uses base_deploy_path from server config if available
        if self.pruning_deploy_path.is_none() {
            if let Some(defaults) = defaults {
                if let Some(ref base) = defaults.base_deploy_path {
                    self.pruning_deploy_path = Some(format!("{}/{}/data", base, simple_name));
                }
            }
        }
        
        // Auto-derive snapshot_deploy_path if not set (remove /data suffix from pruning_deploy_path)
        if self.snapshot_deploy_path.is_none() {
            if let Some(ref pruning_path) = self.pruning_deploy_path {
                // Remove /data suffix if present
                self.snapshot_deploy_path = Some(
                    pruning_path.strip_suffix("/data")
                        .unwrap_or(pruning_path)
                        .to_string()
                );
            }
        }
        
        // Auto-derive log_path if not set
        // Uses base_log_path from server config if available
        if self.log_path.is_none() {
            if let Some(defaults) = defaults {
                if let Some(ref base) = defaults.base_log_path {
                    self.log_path = Some(format!("{}/{}", base, simple_name));
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
    
    /// Extract simple node name from full node name
    /// Examples:
    /// - "enterprise-osmosis" -> "osmosis"
    /// - "discovery-neutron-1" -> "neutron"
    /// - "nolus" -> "nolus"
    /// - "osmosis-archive" -> "osmosis"
    fn extract_simple_name(&self, node_name: &str) -> String {
        let parts: Vec<&str> = node_name.split('-').collect();
        
        if parts.len() == 1 {
            // Single part, use as-is
            return parts[0].to_string();
        }
        
        // Multiple parts - take the second part (skip server prefix)
        // "enterprise-osmosis" -> "osmosis"
        // "discovery-neutron-1" -> "neutron"
        if parts.len() >= 2 {
            // Skip first part (server name) and last part if it's a number
            let last_is_number = parts.last()
                .map(|p| p.chars().all(|c| c.is_numeric()))
                .unwrap_or(false);
            
            if last_is_number && parts.len() >= 3 {
                // Has numeric suffix: "discovery-osmosis-1" -> use middle part
                return parts[1].to_string();
            } else {
                // No numeric suffix: "enterprise-osmosis" -> use second part
                return parts[1].to_string();
            }
        }
        
        // Fallback
        parts[0].to_string()
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
