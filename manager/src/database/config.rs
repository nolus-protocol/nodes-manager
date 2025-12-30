//! Configuration CRUD database operations.
//!
//! This module provides CRUD operations for servers, nodes, hermes,
//! and global settings stored in the database.

use anyhow::Result;
use sqlx::Row;

use super::records::{GlobalSettingRecord, HermesRecord, NodeRecord, ServerRecord};
use super::Database;

impl Database {
    // ========================================================================
    // Servers
    // ========================================================================

    pub async fn get_all_servers(&self) -> Result<Vec<ServerRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, host, agent_port, api_key_ref, request_timeout_seconds,
                   created_at, updated_at
            FROM config_servers
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut servers = Vec::new();
        for row in rows {
            servers.push(ServerRecord {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                host: row.try_get("host")?,
                agent_port: row.try_get("agent_port")?,
                api_key_ref: row.try_get("api_key_ref")?,
                request_timeout_seconds: row.try_get("request_timeout_seconds")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            });
        }
        Ok(servers)
    }

    pub async fn get_server_by_id(&self, id: &str) -> Result<Option<ServerRecord>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, host, agent_port, api_key_ref, request_timeout_seconds,
                   created_at, updated_at
            FROM config_servers
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(ServerRecord {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                host: row.try_get("host")?,
                agent_port: row.try_get("agent_port")?,
                api_key_ref: row.try_get("api_key_ref")?,
                request_timeout_seconds: row.try_get("request_timeout_seconds")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_server_by_name(&self, name: &str) -> Result<Option<ServerRecord>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, host, agent_port, api_key_ref, request_timeout_seconds,
                   created_at, updated_at
            FROM config_servers
            WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(ServerRecord {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                host: row.try_get("host")?,
                agent_port: row.try_get("agent_port")?,
                api_key_ref: row.try_get("api_key_ref")?,
                request_timeout_seconds: row.try_get("request_timeout_seconds")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn upsert_server(&self, server: &ServerRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO config_servers (id, name, host, agent_port, api_key_ref, 
                                        request_timeout_seconds, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                host = excluded.host,
                agent_port = excluded.agent_port,
                api_key_ref = excluded.api_key_ref,
                request_timeout_seconds = excluded.request_timeout_seconds,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&server.id)
        .bind(&server.name)
        .bind(&server.host)
        .bind(server.agent_port)
        .bind(&server.api_key_ref)
        .bind(server.request_timeout_seconds)
        .bind(server.created_at)
        .bind(server.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_server(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM config_servers WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ========================================================================
    // Nodes
    // ========================================================================

    pub async fn get_all_nodes(&self) -> Result<Vec<NodeRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, server_id, network, rpc_url, enabled, service_name,
                   deploy_path, log_path, snapshot_backup_path,
                   pruning_enabled, pruning_schedule, pruning_keep_blocks, pruning_keep_versions,
                   snapshots_enabled, snapshot_schedule, snapshot_retention_count, auto_restore_enabled,
                   state_sync_enabled, state_sync_schedule, state_sync_rpc_sources,
                   state_sync_trust_height_offset, state_sync_max_sync_timeout_seconds,
                   log_monitoring_enabled, log_monitoring_patterns, truncate_logs_enabled,
                   created_at, updated_at
            FROM config_nodes
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut nodes = Vec::new();
        for row in rows {
            nodes.push(Self::row_to_node_record(&row)?);
        }
        Ok(nodes)
    }

    pub async fn get_node_by_id(&self, id: &str) -> Result<Option<NodeRecord>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, server_id, network, rpc_url, enabled, service_name,
                   deploy_path, log_path, snapshot_backup_path,
                   pruning_enabled, pruning_schedule, pruning_keep_blocks, pruning_keep_versions,
                   snapshots_enabled, snapshot_schedule, snapshot_retention_count, auto_restore_enabled,
                   state_sync_enabled, state_sync_schedule, state_sync_rpc_sources,
                   state_sync_trust_height_offset, state_sync_max_sync_timeout_seconds,
                   log_monitoring_enabled, log_monitoring_patterns, truncate_logs_enabled,
                   created_at, updated_at
            FROM config_nodes
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(Self::row_to_node_record(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn get_node_by_name(&self, name: &str) -> Result<Option<NodeRecord>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, server_id, network, rpc_url, enabled, service_name,
                   deploy_path, log_path, snapshot_backup_path,
                   pruning_enabled, pruning_schedule, pruning_keep_blocks, pruning_keep_versions,
                   snapshots_enabled, snapshot_schedule, snapshot_retention_count, auto_restore_enabled,
                   state_sync_enabled, state_sync_schedule, state_sync_rpc_sources,
                   state_sync_trust_height_offset, state_sync_max_sync_timeout_seconds,
                   log_monitoring_enabled, log_monitoring_patterns, truncate_logs_enabled,
                   created_at, updated_at
            FROM config_nodes
            WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(Self::row_to_node_record(&row)?))
        } else {
            Ok(None)
        }
    }

    pub async fn get_nodes_by_server(&self, server_id: &str) -> Result<Vec<NodeRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, server_id, network, rpc_url, enabled, service_name,
                   deploy_path, log_path, snapshot_backup_path,
                   pruning_enabled, pruning_schedule, pruning_keep_blocks, pruning_keep_versions,
                   snapshots_enabled, snapshot_schedule, snapshot_retention_count, auto_restore_enabled,
                   state_sync_enabled, state_sync_schedule, state_sync_rpc_sources,
                   state_sync_trust_height_offset, state_sync_max_sync_timeout_seconds,
                   log_monitoring_enabled, log_monitoring_patterns, truncate_logs_enabled,
                   created_at, updated_at
            FROM config_nodes
            WHERE server_id = ?
            ORDER BY name
            "#,
        )
        .bind(server_id)
        .fetch_all(&self.pool)
        .await?;

        let mut nodes = Vec::new();
        for row in rows {
            nodes.push(Self::row_to_node_record(&row)?);
        }
        Ok(nodes)
    }

    fn row_to_node_record(row: &sqlx::sqlite::SqliteRow) -> Result<NodeRecord> {
        Ok(NodeRecord {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            server_id: row.try_get("server_id")?,
            network: row.try_get("network")?,
            rpc_url: row.try_get("rpc_url")?,
            enabled: row.try_get("enabled")?,
            service_name: row.try_get("service_name")?,
            deploy_path: row.try_get("deploy_path")?,
            log_path: row.try_get("log_path")?,
            snapshot_backup_path: row.try_get("snapshot_backup_path")?,
            pruning_enabled: row.try_get("pruning_enabled")?,
            pruning_schedule: row.try_get("pruning_schedule")?,
            pruning_keep_blocks: row.try_get("pruning_keep_blocks")?,
            pruning_keep_versions: row.try_get("pruning_keep_versions")?,
            snapshots_enabled: row.try_get("snapshots_enabled")?,
            snapshot_schedule: row.try_get("snapshot_schedule")?,
            snapshot_retention_count: row.try_get("snapshot_retention_count")?,
            auto_restore_enabled: row.try_get("auto_restore_enabled")?,
            state_sync_enabled: row.try_get("state_sync_enabled")?,
            state_sync_schedule: row.try_get("state_sync_schedule")?,
            state_sync_rpc_sources: row.try_get("state_sync_rpc_sources")?,
            state_sync_trust_height_offset: row.try_get("state_sync_trust_height_offset")?,
            state_sync_max_sync_timeout_seconds: row
                .try_get("state_sync_max_sync_timeout_seconds")?,
            log_monitoring_enabled: row.try_get("log_monitoring_enabled")?,
            log_monitoring_patterns: row.try_get("log_monitoring_patterns")?,
            truncate_logs_enabled: row.try_get("truncate_logs_enabled")?,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        })
    }

    pub async fn upsert_node(&self, node: &NodeRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO config_nodes (
                id, name, server_id, network, rpc_url, enabled, service_name,
                deploy_path, log_path, snapshot_backup_path,
                pruning_enabled, pruning_schedule, pruning_keep_blocks, pruning_keep_versions,
                snapshots_enabled, snapshot_schedule, snapshot_retention_count, auto_restore_enabled,
                state_sync_enabled, state_sync_schedule, state_sync_rpc_sources,
                state_sync_trust_height_offset, state_sync_max_sync_timeout_seconds,
                log_monitoring_enabled, log_monitoring_patterns, truncate_logs_enabled,
                created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                server_id = excluded.server_id,
                network = excluded.network,
                rpc_url = excluded.rpc_url,
                enabled = excluded.enabled,
                service_name = excluded.service_name,
                deploy_path = excluded.deploy_path,
                log_path = excluded.log_path,
                snapshot_backup_path = excluded.snapshot_backup_path,
                pruning_enabled = excluded.pruning_enabled,
                pruning_schedule = excluded.pruning_schedule,
                pruning_keep_blocks = excluded.pruning_keep_blocks,
                pruning_keep_versions = excluded.pruning_keep_versions,
                snapshots_enabled = excluded.snapshots_enabled,
                snapshot_schedule = excluded.snapshot_schedule,
                snapshot_retention_count = excluded.snapshot_retention_count,
                auto_restore_enabled = excluded.auto_restore_enabled,
                state_sync_enabled = excluded.state_sync_enabled,
                state_sync_schedule = excluded.state_sync_schedule,
                state_sync_rpc_sources = excluded.state_sync_rpc_sources,
                state_sync_trust_height_offset = excluded.state_sync_trust_height_offset,
                state_sync_max_sync_timeout_seconds = excluded.state_sync_max_sync_timeout_seconds,
                log_monitoring_enabled = excluded.log_monitoring_enabled,
                log_monitoring_patterns = excluded.log_monitoring_patterns,
                truncate_logs_enabled = excluded.truncate_logs_enabled,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&node.id)
        .bind(&node.name)
        .bind(&node.server_id)
        .bind(&node.network)
        .bind(&node.rpc_url)
        .bind(node.enabled)
        .bind(&node.service_name)
        .bind(&node.deploy_path)
        .bind(&node.log_path)
        .bind(&node.snapshot_backup_path)
        .bind(node.pruning_enabled)
        .bind(&node.pruning_schedule)
        .bind(node.pruning_keep_blocks)
        .bind(node.pruning_keep_versions)
        .bind(node.snapshots_enabled)
        .bind(&node.snapshot_schedule)
        .bind(node.snapshot_retention_count)
        .bind(node.auto_restore_enabled)
        .bind(node.state_sync_enabled)
        .bind(&node.state_sync_schedule)
        .bind(&node.state_sync_rpc_sources)
        .bind(node.state_sync_trust_height_offset)
        .bind(node.state_sync_max_sync_timeout_seconds)
        .bind(node.log_monitoring_enabled)
        .bind(&node.log_monitoring_patterns)
        .bind(node.truncate_logs_enabled)
        .bind(node.created_at)
        .bind(node.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_node(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM config_nodes WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ========================================================================
    // Hermes
    // ========================================================================

    pub async fn get_all_hermes(&self) -> Result<Vec<HermesRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT id, name, server_id, service_name, log_path, restart_schedule,
                   dependent_nodes, truncate_logs_enabled, created_at, updated_at
            FROM config_hermes
            ORDER BY name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut hermes = Vec::new();
        for row in rows {
            hermes.push(HermesRecord {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                server_id: row.try_get("server_id")?,
                service_name: row.try_get("service_name")?,
                log_path: row.try_get("log_path")?,
                restart_schedule: row.try_get("restart_schedule")?,
                dependent_nodes: row.try_get("dependent_nodes")?,
                truncate_logs_enabled: row.try_get("truncate_logs_enabled")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            });
        }
        Ok(hermes)
    }

    pub async fn get_hermes_by_id(&self, id: &str) -> Result<Option<HermesRecord>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, server_id, service_name, log_path, restart_schedule,
                   dependent_nodes, truncate_logs_enabled, created_at, updated_at
            FROM config_hermes
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(HermesRecord {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                server_id: row.try_get("server_id")?,
                service_name: row.try_get("service_name")?,
                log_path: row.try_get("log_path")?,
                restart_schedule: row.try_get("restart_schedule")?,
                dependent_nodes: row.try_get("dependent_nodes")?,
                truncate_logs_enabled: row.try_get("truncate_logs_enabled")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_hermes_by_name(&self, name: &str) -> Result<Option<HermesRecord>> {
        let row = sqlx::query(
            r#"
            SELECT id, name, server_id, service_name, log_path, restart_schedule,
                   dependent_nodes, truncate_logs_enabled, created_at, updated_at
            FROM config_hermes
            WHERE name = ?
            "#,
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            Ok(Some(HermesRecord {
                id: row.try_get("id")?,
                name: row.try_get("name")?,
                server_id: row.try_get("server_id")?,
                service_name: row.try_get("service_name")?,
                log_path: row.try_get("log_path")?,
                restart_schedule: row.try_get("restart_schedule")?,
                dependent_nodes: row.try_get("dependent_nodes")?,
                truncate_logs_enabled: row.try_get("truncate_logs_enabled")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn upsert_hermes(&self, hermes: &HermesRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO config_hermes (
                id, name, server_id, service_name, log_path, restart_schedule,
                dependent_nodes, truncate_logs_enabled, created_at, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                server_id = excluded.server_id,
                service_name = excluded.service_name,
                log_path = excluded.log_path,
                restart_schedule = excluded.restart_schedule,
                dependent_nodes = excluded.dependent_nodes,
                truncate_logs_enabled = excluded.truncate_logs_enabled,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&hermes.id)
        .bind(&hermes.name)
        .bind(&hermes.server_id)
        .bind(&hermes.service_name)
        .bind(&hermes.log_path)
        .bind(&hermes.restart_schedule)
        .bind(&hermes.dependent_nodes)
        .bind(hermes.truncate_logs_enabled)
        .bind(hermes.created_at)
        .bind(hermes.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_hermes(&self, id: &str) -> Result<bool> {
        let result = sqlx::query("DELETE FROM config_hermes WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    // ========================================================================
    // Global Settings
    // ========================================================================

    pub async fn get_all_settings(&self) -> Result<Vec<GlobalSettingRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT key, value, updated_at
            FROM global_settings
            ORDER BY key
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut settings = Vec::new();
        for row in rows {
            settings.push(GlobalSettingRecord {
                key: row.try_get("key")?,
                value: row.try_get("value")?,
                updated_at: row.try_get("updated_at")?,
            });
        }
        Ok(settings)
    }

    pub async fn upsert_setting(&self, setting: &GlobalSettingRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO global_settings (key, value, updated_at)
            VALUES (?, ?, ?)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&setting.key)
        .bind(&setting.value)
        .bind(setting.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Check if configuration tables have any data
    pub async fn has_config_data(&self) -> Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM config_servers")
            .fetch_one(&self.pool)
            .await?;
        Ok(count > 0)
    }
}
