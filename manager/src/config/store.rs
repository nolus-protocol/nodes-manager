// File: manager/src/config/store.rs
//! Configuration store that provides DB-backed configuration management.
//!
//! This module handles:
//! - Loading configuration from the database
//! - Converting between database records and application config structs
//! - CRUD operations for servers, nodes, and hermes relayers
//! - Import from TOML files (merge mode)

use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::config::{Config, HermesConfig, NodeConfig, ServerConfig};
use crate::database::{Database, GlobalSettingRecord, HermesRecord, NodeRecord, ServerRecord};

/// ConfigStore provides high-level configuration management backed by SQLite.
pub struct ConfigStore {
    db: Arc<Database>,
}

impl ConfigStore {
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Load the full configuration from the database.
    /// Returns None if no configuration data exists (first run).
    pub async fn load_config(&self) -> Result<Option<Config>> {
        if !self.db.has_config_data().await? {
            debug!("No configuration data in database");
            return Ok(None);
        }

        // Load global settings
        let settings = self.load_global_settings().await?;

        // Load servers (we need them for resolving server_host references)
        let server_records = self.db.get_all_servers().await?;
        let server_map: HashMap<String, ServerRecord> = server_records
            .into_iter()
            .map(|s| (s.id.clone(), s))
            .collect();

        // Build servers config
        let mut servers = HashMap::new();
        for (id, record) in &server_map {
            servers.insert(
                record.name.clone(),
                ServerConfig {
                    host: record.host.clone(),
                    agent_port: record.agent_port as u16,
                    api_key: String::new(), // Will be filled from secrets
                    request_timeout_seconds: record.request_timeout_seconds as u64,
                    max_concurrent_requests: None,
                },
            );
            debug!("Loaded server config: {} (id: {})", record.name, id);
        }

        // Load nodes
        let node_records = self.db.get_all_nodes().await?;
        let mut nodes = HashMap::new();
        for record in node_records {
            let server = server_map.get(&record.server_id);
            let server_host = server.map(|s| s.name.clone()).unwrap_or_default();

            nodes.insert(
                record.name.clone(),
                self.node_record_to_config(&record, &server_host),
            );
            debug!("Loaded node config: {}", record.name);
        }

        // Load hermes
        let hermes_records = self.db.get_all_hermes().await?;
        let mut hermes = HashMap::new();
        for record in hermes_records {
            let server = server_map.get(&record.server_id);
            let server_host = server.map(|s| s.name.clone()).unwrap_or_default();

            hermes.insert(
                record.name.clone(),
                self.hermes_record_to_config(&record, &server_host),
            );
            debug!("Loaded hermes config: {}", record.name);
        }

        let config = Config {
            host: settings
                .get("host")
                .cloned()
                .unwrap_or_else(|| "0.0.0.0".to_string()),
            port: settings
                .get("port")
                .and_then(|v| v.parse().ok())
                .unwrap_or(8095),
            check_interval_seconds: settings
                .get("check_interval_seconds")
                .and_then(|v| v.parse().ok())
                .unwrap_or(90),
            rpc_timeout_seconds: settings
                .get("rpc_timeout_seconds")
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
            alarm_webhook_url: settings
                .get("alarm_webhook_url")
                .cloned()
                .unwrap_or_default(),
            hermes_min_uptime_minutes: settings
                .get("hermes_min_uptime_minutes")
                .and_then(|v| v.parse().ok()),
            auto_restore_trigger_words: settings
                .get("auto_restore_trigger_words")
                .and_then(|v| serde_json::from_str(v).ok()),
            log_monitoring_context_lines: settings
                .get("log_monitoring_context_lines")
                .and_then(|v| v.parse().ok()),
            servers,
            nodes,
            hermes,
        };

        info!(
            "Loaded configuration from database: {} servers, {} nodes, {} hermes",
            config.servers.len(),
            config.nodes.len(),
            config.hermes.len()
        );

        Ok(Some(config))
    }

    async fn load_global_settings(&self) -> Result<HashMap<String, String>> {
        let records = self.db.get_all_settings().await?;
        Ok(records.into_iter().map(|r| (r.key, r.value)).collect())
    }

    fn node_record_to_config(&self, record: &NodeRecord, server_host: &str) -> NodeConfig {
        NodeConfig {
            rpc_url: record.rpc_url.clone(),
            network: record.network.clone(),
            server_host: server_host.to_string(),
            enabled: record.enabled,
            service_name: record.service_name.clone(),
            deploy_path: record.deploy_path.clone(),
            pruning_enabled: Some(record.pruning_enabled),
            pruning_schedule: record.pruning_schedule.clone(),
            pruning_keep_blocks: record.pruning_keep_blocks.map(|v| v as u32),
            pruning_keep_versions: record.pruning_keep_versions.map(|v| v as u32),
            log_path: record.log_path.clone(),
            truncate_logs_enabled: Some(record.truncate_logs_enabled),
            log_monitoring_enabled: Some(record.log_monitoring_enabled),
            log_monitoring_patterns: record
                .log_monitoring_patterns
                .as_ref()
                .and_then(|v| serde_json::from_str(v).ok()),
            snapshots_enabled: Some(record.snapshots_enabled),
            snapshot_backup_path: record.snapshot_backup_path.clone(),
            auto_restore_enabled: Some(record.auto_restore_enabled),
            snapshot_schedule: record.snapshot_schedule.clone(),
            snapshot_retention_count: record.snapshot_retention_count.map(|v| v as usize),
            state_sync_enabled: Some(record.state_sync_enabled),
            state_sync_schedule: record.state_sync_schedule.clone(),
            state_sync_rpc_sources: record
                .state_sync_rpc_sources
                .as_ref()
                .and_then(|v| serde_json::from_str(v).ok()),
            state_sync_trust_height_offset: record.state_sync_trust_height_offset.map(|v| v as u32),
            state_sync_max_sync_timeout_seconds: record
                .state_sync_max_sync_timeout_seconds
                .map(|v| v as u64),
        }
    }

    fn hermes_record_to_config(&self, record: &HermesRecord, server_host: &str) -> HermesConfig {
        HermesConfig {
            server_host: server_host.to_string(),
            service_name: record.service_name.clone(),
            log_path: record.log_path.clone(),
            restart_schedule: record.restart_schedule.clone(),
            dependent_nodes: record
                .dependent_nodes
                .as_ref()
                .and_then(|v| serde_json::from_str(v).ok()),
            truncate_logs_enabled: Some(record.truncate_logs_enabled),
        }
    }

    // ========================================================================
    // Server CRUD
    // ========================================================================

    pub async fn get_all_servers(&self) -> Result<Vec<ServerRecord>> {
        self.db.get_all_servers().await
    }

    pub async fn get_server(&self, id: &str) -> Result<Option<ServerRecord>> {
        self.db.get_server_by_id(id).await
    }

    pub async fn get_server_by_name(&self, name: &str) -> Result<Option<ServerRecord>> {
        self.db.get_server_by_name(name).await
    }

    pub async fn create_server(
        &self,
        name: String,
        host: String,
        agent_port: u16,
        api_key_ref: String,
        request_timeout_seconds: u64,
    ) -> Result<ServerRecord> {
        let now = Utc::now();
        let record = ServerRecord {
            id: Uuid::new_v4().to_string(),
            name,
            host,
            agent_port: agent_port as i64,
            api_key_ref,
            request_timeout_seconds: request_timeout_seconds as i64,
            created_at: now,
            updated_at: now,
        };
        self.db.upsert_server(&record).await?;
        info!("Created server: {} ({})", record.name, record.id);
        Ok(record)
    }

    pub async fn update_server(&self, mut record: ServerRecord) -> Result<ServerRecord> {
        record.updated_at = Utc::now();
        self.db.upsert_server(&record).await?;
        info!("Updated server: {} ({})", record.name, record.id);
        Ok(record)
    }

    pub async fn delete_server(&self, id: &str) -> Result<bool> {
        // Check if any nodes or hermes reference this server
        let nodes = self.db.get_nodes_by_server(id).await?;
        if !nodes.is_empty() {
            return Err(anyhow::anyhow!(
                "Cannot delete server: {} nodes are still referencing it",
                nodes.len()
            ));
        }

        let deleted = self.db.delete_server(id).await?;
        if deleted {
            info!("Deleted server: {}", id);
        }
        Ok(deleted)
    }

    // ========================================================================
    // Node CRUD
    // ========================================================================

    pub async fn get_all_nodes(&self) -> Result<Vec<NodeRecord>> {
        self.db.get_all_nodes().await
    }

    pub async fn get_node(&self, id: &str) -> Result<Option<NodeRecord>> {
        self.db.get_node_by_id(id).await
    }

    pub async fn get_node_by_name(&self, name: &str) -> Result<Option<NodeRecord>> {
        self.db.get_node_by_name(name).await
    }

    pub async fn create_node(&self, record: NodeRecord) -> Result<NodeRecord> {
        let mut record = record;
        let now = Utc::now();
        record.id = Uuid::new_v4().to_string();
        record.created_at = now;
        record.updated_at = now;

        self.db.upsert_node(&record).await?;
        info!("Created node: {} ({})", record.name, record.id);
        Ok(record)
    }

    pub async fn update_node(&self, mut record: NodeRecord) -> Result<NodeRecord> {
        record.updated_at = Utc::now();
        self.db.upsert_node(&record).await?;
        info!("Updated node: {} ({})", record.name, record.id);
        Ok(record)
    }

    pub async fn delete_node(&self, id: &str) -> Result<bool> {
        let deleted = self.db.delete_node(id).await?;
        if deleted {
            info!("Deleted node: {}", id);
        }
        Ok(deleted)
    }

    pub async fn toggle_node(&self, id: &str, enabled: bool) -> Result<Option<NodeRecord>> {
        if let Some(mut node) = self.db.get_node_by_id(id).await? {
            node.enabled = enabled;
            node.updated_at = Utc::now();
            self.db.upsert_node(&node).await?;
            info!("Toggled node {} to enabled={}", node.name, enabled);
            Ok(Some(node))
        } else {
            Ok(None)
        }
    }

    // ========================================================================
    // Hermes CRUD
    // ========================================================================

    pub async fn get_all_hermes(&self) -> Result<Vec<HermesRecord>> {
        self.db.get_all_hermes().await
    }

    pub async fn get_hermes(&self, id: &str) -> Result<Option<HermesRecord>> {
        self.db.get_hermes_by_id(id).await
    }

    pub async fn get_hermes_by_name(&self, name: &str) -> Result<Option<HermesRecord>> {
        self.db.get_hermes_by_name(name).await
    }

    pub async fn create_hermes(&self, record: HermesRecord) -> Result<HermesRecord> {
        let mut record = record;
        let now = Utc::now();
        record.id = Uuid::new_v4().to_string();
        record.created_at = now;
        record.updated_at = now;

        self.db.upsert_hermes(&record).await?;
        info!("Created hermes: {} ({})", record.name, record.id);
        Ok(record)
    }

    pub async fn update_hermes(&self, mut record: HermesRecord) -> Result<HermesRecord> {
        record.updated_at = Utc::now();
        self.db.upsert_hermes(&record).await?;
        info!("Updated hermes: {} ({})", record.name, record.id);
        Ok(record)
    }

    pub async fn delete_hermes(&self, id: &str) -> Result<bool> {
        let deleted = self.db.delete_hermes(id).await?;
        if deleted {
            info!("Deleted hermes: {}", id);
        }
        Ok(deleted)
    }

    // ========================================================================
    // Global Settings
    // ========================================================================

    pub async fn get_all_settings(&self) -> Result<HashMap<String, String>> {
        self.load_global_settings().await
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let record = GlobalSettingRecord {
            key: key.to_string(),
            value: value.to_string(),
            updated_at: Utc::now(),
        };
        self.db.upsert_setting(&record).await?;
        debug!("Updated setting: {} = {}", key, value);
        Ok(())
    }

    /// Save all global settings from a Config struct
    pub async fn save_global_settings(&self, config: &Config) -> Result<()> {
        self.set_setting("host", &config.host).await?;
        self.set_setting("port", &config.port.to_string()).await?;
        self.set_setting(
            "check_interval_seconds",
            &config.check_interval_seconds.to_string(),
        )
        .await?;
        self.set_setting(
            "rpc_timeout_seconds",
            &config.rpc_timeout_seconds.to_string(),
        )
        .await?;
        self.set_setting("alarm_webhook_url", &config.alarm_webhook_url)
            .await?;

        if let Some(v) = config.hermes_min_uptime_minutes {
            self.set_setting("hermes_min_uptime_minutes", &v.to_string())
                .await?;
        }
        if let Some(ref v) = config.auto_restore_trigger_words {
            self.set_setting("auto_restore_trigger_words", &serde_json::to_string(v)?)
                .await?;
        }
        if let Some(v) = config.log_monitoring_context_lines {
            self.set_setting("log_monitoring_context_lines", &v.to_string())
                .await?;
        }

        info!("Saved global settings to database");
        Ok(())
    }

    // ========================================================================
    // Import from TOML (merge mode)
    // ========================================================================

    /// Import configuration from existing Config struct (parsed from TOML files).
    /// Uses merge mode: updates existing records, adds new ones, keeps others.
    pub async fn import_from_config(&self, config: &Config) -> Result<ImportResult> {
        let mut result = ImportResult::default();

        // First, save global settings
        self.save_global_settings(config).await?;
        result.settings_imported = true;

        // Import servers
        for (name, server_config) in &config.servers {
            let existing = self.db.get_server_by_name(name).await?;
            let now = Utc::now();
            let is_update = existing.is_some();

            let record = if let Some(existing) = existing {
                ServerRecord {
                    id: existing.id,
                    name: name.clone(),
                    host: server_config.host.clone(),
                    agent_port: server_config.agent_port as i64,
                    api_key_ref: name.clone(), // Use server name as key reference
                    request_timeout_seconds: server_config.request_timeout_seconds as i64,
                    created_at: existing.created_at,
                    updated_at: now,
                }
            } else {
                ServerRecord {
                    id: Uuid::new_v4().to_string(),
                    name: name.clone(),
                    host: server_config.host.clone(),
                    agent_port: server_config.agent_port as i64,
                    api_key_ref: name.clone(),
                    request_timeout_seconds: server_config.request_timeout_seconds as i64,
                    created_at: now,
                    updated_at: now,
                }
            };

            self.db.upsert_server(&record).await?;
            if is_update {
                result.servers_updated += 1;
            } else {
                result.servers_created += 1;
            }
        }

        // Build server name -> id map
        let servers = self.db.get_all_servers().await?;
        let server_name_to_id: HashMap<String, String> = servers
            .into_iter()
            .map(|s| (s.name.clone(), s.id))
            .collect();

        // Import nodes
        for (name, node_config) in &config.nodes {
            let server_id = server_name_to_id
                .get(&node_config.server_host)
                .cloned()
                .unwrap_or_default();

            if server_id.is_empty() {
                warn!(
                    "Skipping node {}: server '{}' not found",
                    name, node_config.server_host
                );
                result.nodes_skipped += 1;
                continue;
            }

            let existing = self.db.get_node_by_name(name).await?;
            let now = Utc::now();

            let record = NodeRecord {
                id: existing
                    .as_ref()
                    .map(|e| e.id.clone())
                    .unwrap_or_else(|| Uuid::new_v4().to_string()),
                name: name.clone(),
                server_id,
                network: node_config.network.clone(),
                rpc_url: node_config.rpc_url.clone(),
                enabled: node_config.enabled,
                service_name: node_config.service_name.clone(),
                deploy_path: node_config.deploy_path.clone(),
                log_path: node_config.log_path.clone(),
                snapshot_backup_path: node_config.snapshot_backup_path.clone(),
                pruning_enabled: node_config.pruning_enabled.unwrap_or(false),
                pruning_schedule: node_config.pruning_schedule.clone(),
                pruning_keep_blocks: node_config.pruning_keep_blocks.map(|v| v as i64),
                pruning_keep_versions: node_config.pruning_keep_versions.map(|v| v as i64),
                snapshots_enabled: node_config.snapshots_enabled.unwrap_or(false),
                snapshot_schedule: node_config.snapshot_schedule.clone(),
                snapshot_retention_count: node_config.snapshot_retention_count.map(|v| v as i64),
                auto_restore_enabled: node_config.auto_restore_enabled.unwrap_or(false),
                state_sync_enabled: node_config.state_sync_enabled.unwrap_or(false),
                state_sync_schedule: node_config.state_sync_schedule.clone(),
                state_sync_rpc_sources: node_config
                    .state_sync_rpc_sources
                    .as_ref()
                    .map(|v| serde_json::to_string(v).unwrap_or_default()),
                state_sync_trust_height_offset: node_config
                    .state_sync_trust_height_offset
                    .map(|v| v as i64),
                state_sync_max_sync_timeout_seconds: node_config
                    .state_sync_max_sync_timeout_seconds
                    .map(|v| v as i64),
                log_monitoring_enabled: node_config.log_monitoring_enabled.unwrap_or(false),
                log_monitoring_patterns: node_config
                    .log_monitoring_patterns
                    .as_ref()
                    .map(|v| serde_json::to_string(v).unwrap_or_default()),
                truncate_logs_enabled: node_config.truncate_logs_enabled.unwrap_or(false),
                created_at: existing.as_ref().map(|e| e.created_at).unwrap_or(now),
                updated_at: now,
            };

            self.db.upsert_node(&record).await?;
            if existing.is_some() {
                result.nodes_updated += 1;
            } else {
                result.nodes_created += 1;
            }
        }

        // Import hermes
        for (name, hermes_config) in &config.hermes {
            let server_id = server_name_to_id
                .get(&hermes_config.server_host)
                .cloned()
                .unwrap_or_default();

            if server_id.is_empty() {
                warn!(
                    "Skipping hermes {}: server '{}' not found",
                    name, hermes_config.server_host
                );
                result.hermes_skipped += 1;
                continue;
            }

            let existing = self.db.get_hermes_by_name(name).await?;
            let now = Utc::now();

            let record = HermesRecord {
                id: existing
                    .as_ref()
                    .map(|e| e.id.clone())
                    .unwrap_or_else(|| Uuid::new_v4().to_string()),
                name: name.clone(),
                server_id,
                service_name: hermes_config.service_name.clone(),
                log_path: hermes_config.log_path.clone(),
                restart_schedule: hermes_config.restart_schedule.clone(),
                dependent_nodes: hermes_config
                    .dependent_nodes
                    .as_ref()
                    .map(|v| serde_json::to_string(v).unwrap_or_default()),
                truncate_logs_enabled: hermes_config.truncate_logs_enabled.unwrap_or(false),
                created_at: existing.as_ref().map(|e| e.created_at).unwrap_or(now),
                updated_at: now,
            };

            self.db.upsert_hermes(&record).await?;
            if existing.is_some() {
                result.hermes_updated += 1;
            } else {
                result.hermes_created += 1;
            }
        }

        info!(
            "Import completed: {} servers ({} new, {} updated), {} nodes ({} new, {} updated, {} skipped), {} hermes ({} new, {} updated, {} skipped)",
            result.servers_created + result.servers_updated,
            result.servers_created,
            result.servers_updated,
            result.nodes_created + result.nodes_updated,
            result.nodes_created,
            result.nodes_updated,
            result.nodes_skipped,
            result.hermes_created + result.hermes_updated,
            result.hermes_created,
            result.hermes_updated,
            result.hermes_skipped,
        );

        Ok(result)
    }

    /// Check if there's any configuration data in the database
    pub async fn has_config_data(&self) -> Result<bool> {
        self.db.has_config_data().await
    }
}

/// Result of an import operation
#[derive(Debug, Default)]
pub struct ImportResult {
    pub settings_imported: bool,
    pub servers_created: usize,
    pub servers_updated: usize,
    pub nodes_created: usize,
    pub nodes_updated: usize,
    pub nodes_skipped: usize,
    pub hermes_created: usize,
    pub hermes_updated: usize,
    pub hermes_skipped: usize,
}
