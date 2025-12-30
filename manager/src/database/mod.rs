//! Database layer for the nodes manager.
//!
//! This module provides SQLite persistence for:
//! - Health records (node and Hermes health history)
//! - Maintenance operations (tracking operation status)
//! - Configuration (servers, nodes, hermes, settings)
//!
//! The module is organized into submodules:
//! - `records` - All record types (entities)
//! - `health` - Health record operations
//! - `maintenance` - Maintenance operation tracking
//! - `config` - Configuration CRUD operations

mod config;
mod health;
mod maintenance;
mod records;

pub use records::*;

use anyhow::Result;
use chrono::Utc;
use sqlx::{Pool, Row, Sqlite, SqlitePool};
use std::path::Path;
use tracing::{debug, error, info, warn};

pub struct Database {
    pool: Pool<Sqlite>,
}

impl Database {
    /// Expose pool for integration test queries
    #[allow(dead_code)]
    pub fn pool(&self) -> &Pool<Sqlite> {
        &self.pool
    }

    pub async fn new(database_path: &str) -> Result<Self> {
        info!("=== Starting database initialization ===");
        info!("Database path: {}", database_path);

        // Ensure parent directory exists with detailed logging
        if let Some(parent) = Path::new(database_path).parent() {
            info!("Ensuring parent directory exists: {:?}", parent);
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                error!("FAILED to create parent directory {:?}: {}", parent, e);
                return Err(e.into());
            }
            info!("Parent directory OK");
        }

        // Use explicit SQLite connection options for better reliability
        let database_url = format!("sqlite:{}?mode=rwc", database_path);
        info!("Connecting to database with URL: {}", database_url);

        let pool = match SqlitePool::connect(&database_url).await {
            Ok(pool) => {
                info!("Successfully connected to SQLite database");
                pool
            }
            Err(e) => {
                error!("FAILED to connect to database: {}", e);
                error!("   Database path: {}", database_path);
                error!("   Connection URL: {}", database_url);
                return Err(e.into());
            }
        };

        let database = Self { pool };

        info!("Starting table initialization...");
        match database.initialize_tables().await {
            Ok(_) => info!("Database tables initialized successfully"),
            Err(e) => {
                error!("CRITICAL: Database table initialization failed: {}", e);
                return Err(e);
            }
        }

        // STARTUP CLEANUP: Clean up stuck maintenance operations on every restart
        info!("Performing startup cleanup of stuck maintenance operations...");
        match database.cleanup_stuck_maintenance_operations().await {
            Ok(cleaned_count) => {
                if cleaned_count > 0 {
                    warn!(
                        "Cleaned up {} stuck maintenance operations on startup",
                        cleaned_count
                    );
                } else {
                    info!("No stuck maintenance operations found");
                }
            }
            Err(e) => {
                error!("Failed to cleanup stuck maintenance operations: {}", e);
                // Don't fail startup for cleanup issues, just log
                warn!("Continuing with startup despite cleanup failure");
            }
        }

        // Test database with a simple query
        info!("Testing database connectivity...");
        match database.test_database().await {
            Ok(_) => info!("Database test successful"),
            Err(e) => {
                error!("Database test failed: {}", e);
                return Err(e);
            }
        }

        info!("=== Database initialization completed successfully ===");
        Ok(database)
    }

    async fn initialize_tables(&self) -> Result<()> {
        info!("Step 1: Creating health_records table...");

        // Create health records table with explicit error handling
        let health_table_sql = r#"
            CREATE TABLE IF NOT EXISTS health_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                node_name TEXT NOT NULL,
                is_healthy BOOLEAN NOT NULL,
                error_message TEXT,
                timestamp DATETIME NOT NULL,
                block_height INTEGER,
                is_syncing INTEGER,
                is_catching_up INTEGER,
                validator_address TEXT
            )
        "#;

        if let Err(e) = sqlx::query(health_table_sql).execute(&self.pool).await {
            error!("FAILED to create health_records table: {}", e);
            error!("SQL was: {}", health_table_sql);
            return Err(e.into());
        }
        info!("health_records table created");

        info!("Step 2: Creating health_records index...");
        let health_index_sql = "CREATE INDEX IF NOT EXISTS idx_health_node_timestamp ON health_records(node_name, timestamp DESC)";
        if let Err(e) = sqlx::query(health_index_sql).execute(&self.pool).await {
            error!("FAILED to create health_records index: {}", e);
            return Err(e.into());
        }
        info!("health_records index created");

        info!("Step 3: Creating maintenance_operations table...");
        let maintenance_table_sql = r#"
            CREATE TABLE IF NOT EXISTS maintenance_operations (
                id TEXT PRIMARY KEY,
                operation_type TEXT NOT NULL,
                target_name TEXT NOT NULL,
                status TEXT NOT NULL,
                started_at DATETIME NOT NULL,
                completed_at DATETIME,
                error_message TEXT,
                details TEXT
            )
        "#;

        if let Err(e) = sqlx::query(maintenance_table_sql).execute(&self.pool).await {
            error!("FAILED to create maintenance_operations table: {}", e);
            return Err(e.into());
        }
        info!("maintenance_operations table created");

        info!("Step 4: Creating maintenance_operations indexes...");
        let maintenance_index1_sql = "CREATE INDEX IF NOT EXISTS idx_maintenance_target ON maintenance_operations(target_name, started_at DESC)";
        if let Err(e) = sqlx::query(maintenance_index1_sql)
            .execute(&self.pool)
            .await
        {
            error!("FAILED to create maintenance target index: {}", e);
            return Err(e.into());
        }

        let maintenance_index2_sql = "CREATE INDEX IF NOT EXISTS idx_maintenance_status ON maintenance_operations(status, started_at DESC)";
        if let Err(e) = sqlx::query(maintenance_index2_sql)
            .execute(&self.pool)
            .await
        {
            error!("FAILED to create maintenance status index: {}", e);
            return Err(e.into());
        }
        info!("maintenance_operations indexes created");

        info!("Step 5: Creating hermes_health_records table...");
        let hermes_health_table_sql = r#"
            CREATE TABLE IF NOT EXISTS hermes_health_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                hermes_name TEXT NOT NULL,
                is_healthy BOOLEAN NOT NULL,
                status TEXT NOT NULL,
                uptime_seconds INTEGER,
                error_message TEXT,
                timestamp DATETIME NOT NULL,
                server_host TEXT NOT NULL,
                service_name TEXT NOT NULL
            )
        "#;

        if let Err(e) = sqlx::query(hermes_health_table_sql)
            .execute(&self.pool)
            .await
        {
            error!("FAILED to create hermes_health_records table: {}", e);
            return Err(e.into());
        }
        info!("hermes_health_records table created");

        info!("Step 6: Creating hermes_health_records index...");
        let hermes_index_sql = "CREATE INDEX IF NOT EXISTS idx_hermes_health_name_timestamp ON hermes_health_records(hermes_name, timestamp DESC)";
        if let Err(e) = sqlx::query(hermes_index_sql).execute(&self.pool).await {
            error!("FAILED to create hermes_health_records index: {}", e);
            return Err(e.into());
        }
        info!("hermes_health_records index created");

        // ====================================================================
        // Configuration tables (for DB-backed configuration management)
        // ====================================================================

        info!("Step 7: Creating config_servers table...");
        let servers_table_sql = r#"
            CREATE TABLE IF NOT EXISTS config_servers (
                id TEXT PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                host TEXT NOT NULL,
                agent_port INTEGER NOT NULL DEFAULT 8745,
                api_key_ref TEXT NOT NULL,
                request_timeout_seconds INTEGER NOT NULL DEFAULT 300,
                created_at DATETIME NOT NULL,
                updated_at DATETIME NOT NULL
            )
        "#;
        if let Err(e) = sqlx::query(servers_table_sql).execute(&self.pool).await {
            error!("FAILED to create config_servers table: {}", e);
            return Err(e.into());
        }
        info!("config_servers table created");

        info!("Step 8: Creating config_nodes table...");
        let nodes_table_sql = r#"
            CREATE TABLE IF NOT EXISTS config_nodes (
                id TEXT PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                server_id TEXT NOT NULL REFERENCES config_servers(id),
                network TEXT NOT NULL,
                rpc_url TEXT NOT NULL,
                enabled BOOLEAN NOT NULL DEFAULT 1,
                service_name TEXT NOT NULL,
                deploy_path TEXT,
                log_path TEXT,
                snapshot_backup_path TEXT,
                pruning_enabled BOOLEAN NOT NULL DEFAULT 0,
                pruning_schedule TEXT,
                pruning_keep_blocks INTEGER,
                pruning_keep_versions INTEGER,
                snapshots_enabled BOOLEAN NOT NULL DEFAULT 0,
                snapshot_schedule TEXT,
                snapshot_retention_count INTEGER,
                auto_restore_enabled BOOLEAN NOT NULL DEFAULT 0,
                state_sync_enabled BOOLEAN NOT NULL DEFAULT 0,
                state_sync_schedule TEXT,
                state_sync_rpc_sources TEXT,
                state_sync_trust_height_offset INTEGER,
                state_sync_max_sync_timeout_seconds INTEGER,
                log_monitoring_enabled BOOLEAN NOT NULL DEFAULT 0,
                log_monitoring_patterns TEXT,
                truncate_logs_enabled BOOLEAN NOT NULL DEFAULT 0,
                created_at DATETIME NOT NULL,
                updated_at DATETIME NOT NULL
            )
        "#;
        if let Err(e) = sqlx::query(nodes_table_sql).execute(&self.pool).await {
            error!("FAILED to create config_nodes table: {}", e);
            return Err(e.into());
        }
        info!("config_nodes table created");

        info!("Step 9: Creating config_nodes indexes...");
        let nodes_server_idx =
            "CREATE INDEX IF NOT EXISTS idx_config_nodes_server ON config_nodes(server_id)";
        if let Err(e) = sqlx::query(nodes_server_idx).execute(&self.pool).await {
            error!("FAILED to create config_nodes server index: {}", e);
            return Err(e.into());
        }
        let nodes_network_idx =
            "CREATE INDEX IF NOT EXISTS idx_config_nodes_network ON config_nodes(network)";
        if let Err(e) = sqlx::query(nodes_network_idx).execute(&self.pool).await {
            error!("FAILED to create config_nodes network index: {}", e);
            return Err(e.into());
        }
        info!("config_nodes indexes created");

        info!("Step 10: Creating config_hermes table...");
        let hermes_table_sql = r#"
            CREATE TABLE IF NOT EXISTS config_hermes (
                id TEXT PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                server_id TEXT NOT NULL REFERENCES config_servers(id),
                service_name TEXT NOT NULL,
                log_path TEXT,
                restart_schedule TEXT,
                dependent_nodes TEXT,
                truncate_logs_enabled BOOLEAN NOT NULL DEFAULT 0,
                created_at DATETIME NOT NULL,
                updated_at DATETIME NOT NULL
            )
        "#;
        if let Err(e) = sqlx::query(hermes_table_sql).execute(&self.pool).await {
            error!("FAILED to create config_hermes table: {}", e);
            return Err(e.into());
        }
        info!("config_hermes table created");

        info!("Step 11: Creating config_hermes index...");
        let hermes_server_idx =
            "CREATE INDEX IF NOT EXISTS idx_config_hermes_server ON config_hermes(server_id)";
        if let Err(e) = sqlx::query(hermes_server_idx).execute(&self.pool).await {
            error!("FAILED to create config_hermes server index: {}", e);
            return Err(e.into());
        }
        info!("config_hermes index created");

        info!("Step 12: Creating global_settings table...");
        let settings_table_sql = r#"
            CREATE TABLE IF NOT EXISTS global_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at DATETIME NOT NULL
            )
        "#;
        if let Err(e) = sqlx::query(settings_table_sql).execute(&self.pool).await {
            error!("FAILED to create global_settings table: {}", e);
            return Err(e.into());
        }
        info!("global_settings table created");

        info!("All database tables and indexes created successfully");
        Ok(())
    }

    // Startup cleanup method to fix stuck maintenance operations
    async fn cleanup_stuck_maintenance_operations(&self) -> Result<u32> {
        info!("Checking for stuck maintenance operations...");

        // Find stuck operations (running/started for more than 1 hour)
        let rows = sqlx::query(
            r#"
            SELECT id, operation_type, target_name, status, started_at
            FROM maintenance_operations
            WHERE status IN ('running', 'started')
            AND started_at < datetime('now', '-1 hour')
            ORDER BY started_at ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            debug!("No stuck maintenance operations found");
            return Ok(0);
        }

        info!(
            "Found {} stuck maintenance operations that need cleanup",
            rows.len()
        );

        let mut cleaned_count = 0u32;
        let cleanup_time = Utc::now();

        for row in &rows {
            let operation_id: String = row.try_get("id")?;
            let operation_type: String = row.try_get("operation_type")?;
            let target_name: String = row.try_get("target_name")?;
            let status: String = row.try_get("status")?;
            let started_at: String = row.try_get("started_at")?;

            warn!(
                "Cleaning up stuck operation: {} ({}) on {} - started at {} (status: {})",
                operation_id, operation_type, target_name, started_at, status
            );

            // Mark as failed with cleanup message
            let result = sqlx::query(
                r#"
                UPDATE maintenance_operations
                SET status = 'failed',
                    completed_at = ?,
                    error_message = 'Marked as failed during startup cleanup - operation was stuck in running/started state'
                WHERE id = ?
                "#
            )
            .bind(cleanup_time)
            .bind(&operation_id)
            .execute(&self.pool)
            .await;

            match result {
                Ok(_) => {
                    cleaned_count += 1;
                    info!("Cleaned up stuck operation: {}", operation_id);
                }
                Err(e) => {
                    error!("Failed to cleanup operation {}: {}", operation_id, e);
                }
            }
        }

        if cleaned_count > 0 {
            warn!(
                "Successfully cleaned up {} stuck maintenance operations",
                cleaned_count
            );
            info!("These operations were stuck in 'running' or 'started' state and have been marked as 'failed'");
        }

        Ok(cleaned_count)
    }

    async fn test_database(&self) -> Result<()> {
        // Test 1: Check if tables exist
        info!("Testing table existence...");
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('health_records', 'maintenance_operations')"
        )
        .fetch_all(&self.pool)
        .await?;

        if tables.len() != 2 {
            error!("Expected 2 tables, found {}: {:?}", tables.len(), tables);
            return Err(anyhow::anyhow!("Database tables not properly created"));
        }
        info!("Both required tables exist: {:?}", tables);

        // Test 2: Insert a test health record
        info!("Testing health_records insert...");
        let test_record = HealthRecord {
            node_name: "test-node".to_string(),
            is_healthy: true,
            error_message: None,
            timestamp: Utc::now(),
            block_height: Some(12345),
            is_syncing: Some(0),
            is_catching_up: Some(0),
            validator_address: Some("test-validator".to_string()),
        };

        if let Err(e) = self.store_health_record(&test_record).await {
            error!("Failed to insert test health record: {}", e);
            return Err(e);
        }
        info!("Test health record inserted successfully");

        // Test 3: Read the test record back
        info!("Testing health_records query...");
        if let Err(e) = self.get_latest_health_record("test-node").await {
            error!("Failed to query test health record: {}", e);
            return Err(e);
        }
        info!("Test health record queried successfully");

        // Test 4: Cleanup test record
        if let Err(e) = sqlx::query("DELETE FROM health_records WHERE node_name = 'test-node'")
            .execute(&self.pool)
            .await
        {
            warn!("Failed to cleanup test record (non-critical): {}", e);
        } else {
            info!("Test record cleaned up");
        }

        Ok(())
    }
}
