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
    pub log_monitoring_interval_minutes: Option<u32>,

    // Populated from individual server config files
    #[serde(skip)]
    pub servers: HashMap<String, ServerConfig>,
    #[serde(skip)]
    pub nodes: HashMap<String, NodeConfig>,
    #[serde(skip)]
    pub hermes: HashMap<String, HermesConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub agent_port: u16,
    pub api_key: String,
    #[serde(default = "default_request_timeout")]
    pub request_timeout_seconds: u64,
}

fn default_request_timeout() -> u64 {
    300 // 5 minutes default, but we won't use it
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfigFile {
    pub server: ServerConfig,
    pub nodes: HashMap<String, NodeConfig>,
    pub hermes: Option<HashMap<String, HermesConfig>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub rpc_url: String,
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
    pub log_monitoring_context_lines: Option<i32>,

    // Snapshot configuration
    pub snapshots_enabled: Option<bool>,
    pub snapshot_backup_path: Option<String>,
    pub auto_restore_enabled: Option<bool>,
    pub snapshot_schedule: Option<String>,
    pub snapshot_retention_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesConfig {
    pub server_host: String,
    pub service_name: String,
    pub log_path: Option<String>,
    pub restart_schedule: Option<String>,
    pub dependent_nodes: Option<Vec<String>>,
}
