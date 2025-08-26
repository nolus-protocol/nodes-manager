// File: src/config/mod.rs

pub mod manager;

pub use manager::ConfigManager;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{Config, HermesConfig, NodeConfig, ServerConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainConfig {
    pub host: String,
    pub port: u16,
    pub check_interval_seconds: u64,
    pub rpc_timeout_seconds: u64,
    pub alarm_webhook_url: String,
    pub hermes_min_uptime_minutes: u64,
    pub auto_restore_trigger_words: Option<Vec<String>>,
    // NEW: Log monitoring configuration
    pub log_monitoring_enabled: Option<bool>,
    pub log_monitoring_patterns: Option<Vec<String>>,
    pub log_monitoring_interval_minutes: Option<u64>,
    pub log_monitoring_context_lines: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfigFile {
    pub server: ServerConfig,
    pub nodes: Option<HashMap<String, NodeConfigFile>>,
    pub hermes: Option<HashMap<String, HermesConfigFile>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfigFile {
    pub rpc_url: String,
    pub network: String,
    pub server_host: String,
    pub enabled: bool,
    pub pruning_enabled: Option<bool>,
    pub pruning_schedule: Option<String>,
    pub pruning_keep_blocks: Option<u64>,
    pub pruning_keep_versions: Option<u64>,
    pub pruning_deploy_path: Option<String>,
    pub pruning_service_name: Option<String>,
    pub log_path: Option<String>,
    pub truncate_logs_enabled: Option<bool>,
    pub snapshots_enabled: Option<bool>,
    pub snapshot_backup_path: Option<String>,
    pub auto_restore_enabled: Option<bool>,
    // NEW: Scheduled snapshot fields
    pub snapshot_schedule: Option<String>,
    pub snapshot_retention_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesConfigFile {
    pub server_host: String,
    pub service_name: String,
    pub log_path: String,
    pub restart_schedule: String,
    pub dependent_nodes: Vec<String>,
    pub truncate_logs_enabled: Option<bool>,
}

impl From<NodeConfigFile> for NodeConfig {
    fn from(file_config: NodeConfigFile) -> Self {
        Self {
            rpc_url: file_config.rpc_url,
            network: file_config.network,
            server_host: file_config.server_host,
            enabled: file_config.enabled,
            pruning_enabled: file_config.pruning_enabled,
            pruning_schedule: file_config.pruning_schedule,
            pruning_keep_blocks: file_config.pruning_keep_blocks,
            pruning_keep_versions: file_config.pruning_keep_versions,
            pruning_deploy_path: file_config.pruning_deploy_path,
            pruning_service_name: file_config.pruning_service_name,
            log_path: file_config.log_path,
            truncate_logs_enabled: file_config.truncate_logs_enabled,
            snapshots_enabled: file_config.snapshots_enabled,
            snapshot_backup_path: file_config.snapshot_backup_path,
            auto_restore_enabled: file_config.auto_restore_enabled,
            // NEW: Map scheduled snapshot fields
            snapshot_schedule: file_config.snapshot_schedule,
            snapshot_retention_count: file_config.snapshot_retention_count,
        }
    }
}

impl From<HermesConfigFile> for HermesConfig {
    fn from(file_config: HermesConfigFile) -> Self {
        Self {
            server_host: file_config.server_host,
            service_name: file_config.service_name,
            log_path: file_config.log_path,
            restart_schedule: file_config.restart_schedule,
            dependent_nodes: file_config.dependent_nodes,
            truncate_logs_enabled: file_config.truncate_logs_enabled,
        }
    }
}

pub fn validate_config(config: &Config) -> Result<()> {
    // Validate server references in nodes
    for (node_name, node) in &config.nodes {
        if !config.servers.contains_key(&node.server_host) {
            return Err(anyhow::anyhow!(
                "Node '{}' references unknown server '{}'",
                node_name,
                node.server_host
            ));
        }

        // Validate log truncation configuration
        if node.truncate_logs_enabled.unwrap_or(false) && node.log_path.is_none() {
            return Err(anyhow::anyhow!(
                "Node '{}' has truncate_logs_enabled=true but no log_path specified",
                node_name
            ));
        }

        // Validate snapshot configurations
        if node.snapshots_enabled.unwrap_or(false) {
            if node.snapshot_backup_path.is_none() {
                return Err(anyhow::anyhow!(
                    "Node '{}' has snapshots enabled but no snapshot_backup_path specified",
                    node_name
                ));
            }

            if node.pruning_deploy_path.is_none() {
                return Err(anyhow::anyhow!(
                    "Node '{}' has snapshots enabled but no pruning_deploy_path specified (required for snapshot operations)",
                    node_name
                ));
            }
        }

        if node.auto_restore_enabled.unwrap_or(false) && !node.snapshots_enabled.unwrap_or(false) {
            return Err(anyhow::anyhow!(
                "Node '{}' has auto_restore_enabled=true but snapshots_enabled=false",
                node_name
            ));
        }

        // NEW: Validate scheduled snapshot configuration
        if node.snapshot_schedule.is_some() {
            if !node.snapshots_enabled.unwrap_or(false) {
                return Err(anyhow::anyhow!(
                    "Node '{}' has snapshot_schedule but snapshots_enabled=false",
                    node_name
                ));
            }

            if let Some(schedule) = &node.snapshot_schedule {
                if let Err(e) = validate_cron_schedule(schedule) {
                    return Err(anyhow::anyhow!(
                        "Invalid snapshot schedule for node '{}': {}",
                        node_name,
                        e
                    ));
                }
            }

            // Validate retention count
            if let Some(retention_count) = node.snapshot_retention_count {
                if retention_count == 0 {
                    return Err(anyhow::anyhow!(
                        "Node '{}' has snapshot_retention_count=0, must be at least 1",
                        node_name
                    ));
                }
            }
        }
    }

    // Validate server references in hermes
    for (hermes_name, hermes) in &config.hermes {
        if !config.servers.contains_key(&hermes.server_host) {
            return Err(anyhow::anyhow!(
                "Hermes '{}' references unknown server '{}'",
                hermes_name,
                hermes.server_host
            ));
        }

        // Validate dependent nodes exist
        for dep_node in &hermes.dependent_nodes {
            if !config.nodes.contains_key(dep_node) {
                return Err(anyhow::anyhow!(
                    "Hermes '{}' depends on unknown node '{}'",
                    hermes_name,
                    dep_node
                ));
            }
        }

        // Validate log truncation configuration for hermes
        if hermes.truncate_logs_enabled.unwrap_or(false) && hermes.log_path.is_empty() {
            return Err(anyhow::anyhow!(
                "Hermes '{}' has truncate_logs_enabled=true but log_path is empty",
                hermes_name
            ));
        }
    }

    // Validate cron schedules
    for (node_name, node) in &config.nodes {
        if let Some(schedule) = &node.pruning_schedule {
            if let Err(e) = validate_cron_schedule(schedule) {
                return Err(anyhow::anyhow!(
                    "Invalid pruning schedule for node '{}': {}",
                    node_name,
                    e
                ));
            }
        }
    }

    for (hermes_name, hermes) in &config.hermes {
        if let Err(e) = validate_cron_schedule(&hermes.restart_schedule) {
            return Err(anyhow::anyhow!(
                "Invalid restart schedule for hermes '{}': {}",
                hermes_name,
                e
            ));
        }
    }

    // Validate global auto-restore trigger words
    if config.auto_restore_trigger_words.is_empty() {
        println!("Warning: No auto-restore trigger words configured - auto-restore will be disabled");
    }

    // NEW: Validate log monitoring configuration
    if config.log_monitoring_enabled {
        if config.log_monitoring_patterns.is_empty() {
            println!("Warning: Log monitoring enabled but no patterns configured");
        }

        if config.log_monitoring_interval_minutes == 0 {
            return Err(anyhow::anyhow!(
                "Log monitoring interval must be greater than 0 minutes"
            ));
        }
    }

    Ok(())
}

fn validate_cron_schedule(schedule: &str) -> Result<()> {
    // Basic cron validation - should have 6 parts for tokio-cron-scheduler
    let parts: Vec<&str> = schedule.split_whitespace().collect();
    if parts.len() != 6 {
        return Err(anyhow::anyhow!(
            "Cron schedule must have 6 parts (sec min hour day month weekday), got: {}",
            schedule
        ));
    }

    // Additional validation could be added here
    Ok(())
}
