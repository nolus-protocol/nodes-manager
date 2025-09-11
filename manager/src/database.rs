// File: manager/src/database.rs
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite, SqlitePool};
use std::path::Path;
use tracing::{debug, info, warn, error};

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

pub struct Database {
    pool: Pool<Sqlite>,
}

impl Database {
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
                info!("✅ Successfully connected to SQLite database");
                pool
            }
            Err(e) => {
                error!("❌ FAILED to connect to database: {}", e);
                error!("   Database path: {}", database_path);
                error!("   Connection URL: {}", database_url);
                return Err(e.into());
            }
        };

        let database = Self { pool };

        info!("Starting table initialization...");
        match database.initialize_tables().await {
            Ok(_) => info!("✅ Database tables initialized successfully"),
            Err(e) => {
                error!("❌ CRITICAL: Database table initialization failed: {}", e);
                return Err(e);
            }
        }

        // STARTUP CLEANUP: Clean up stuck maintenance operations on every restart
        info!("Performing startup cleanup of stuck maintenance operations...");
        match database.cleanup_stuck_maintenance_operations().await {
            Ok(cleaned_count) => {
                if cleaned_count > 0 {
                    warn!("🧹 Cleaned up {} stuck maintenance operations on startup", cleaned_count);
                } else {
                    info!("✅ No stuck maintenance operations found");
                }
            }
            Err(e) => {
                error!("❌ Failed to cleanup stuck maintenance operations: {}", e);
                // Don't fail startup for cleanup issues, just log
                warn!("Continuing with startup despite cleanup failure");
            }
        }

        // Test database with a simple query
        info!("Testing database connectivity...");
        match database.test_database().await {
            Ok(_) => info!("✅ Database test successful"),
            Err(e) => {
                error!("❌ Database test failed: {}", e);
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
        info!("✅ health_records table created");

        info!("Step 2: Creating health_records index...");
        let health_index_sql = "CREATE INDEX IF NOT EXISTS idx_health_node_timestamp ON health_records(node_name, timestamp DESC)";
        if let Err(e) = sqlx::query(health_index_sql).execute(&self.pool).await {
            error!("FAILED to create health_records index: {}", e);
            return Err(e.into());
        }
        info!("✅ health_records index created");

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
        info!("✅ maintenance_operations table created");

        info!("Step 4: Creating maintenance_operations indexes...");
        let maintenance_index1_sql = "CREATE INDEX IF NOT EXISTS idx_maintenance_target ON maintenance_operations(target_name, started_at DESC)";
        if let Err(e) = sqlx::query(maintenance_index1_sql).execute(&self.pool).await {
            error!("FAILED to create maintenance target index: {}", e);
            return Err(e.into());
        }

        let maintenance_index2_sql = "CREATE INDEX IF NOT EXISTS idx_maintenance_status ON maintenance_operations(status, started_at DESC)";
        if let Err(e) = sqlx::query(maintenance_index2_sql).execute(&self.pool).await {
            error!("FAILED to create maintenance status index: {}", e);
            return Err(e.into());
        }
        info!("✅ maintenance_operations indexes created");

        info!("All database tables and indexes created successfully");
        Ok(())
    }

    // NEW: Startup cleanup method to fix stuck maintenance operations
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
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            debug!("No stuck maintenance operations found");
            return Ok(0);
        }

        info!("Found {} stuck maintenance operations that need cleanup", rows.len());

        let mut cleaned_count = 0u32;
        let cleanup_time = Utc::now();

        for row in &rows {
            let operation_id: String = row.try_get("id")?;
            let operation_type: String = row.try_get("operation_type")?;
            let target_name: String = row.try_get("target_name")?;
            let status: String = row.try_get("status")?;
            let started_at: String = row.try_get("started_at")?;

            warn!("Cleaning up stuck operation: {} ({}) on {} - started at {} (status: {})",
                  operation_id, operation_type, target_name, started_at, status);

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
            warn!("Successfully cleaned up {} stuck maintenance operations", cleaned_count);
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
        info!("✅ Both required tables exist: {:?}", tables);

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
        info!("✅ Test health record inserted successfully");

        // Test 3: Read the test record back
        info!("Testing health_records query...");
        if let Err(e) = self.get_latest_health_record("test-node").await {
            error!("Failed to query test health record: {}", e);
            return Err(e);
        }
        info!("✅ Test health record queried successfully");

        // Test 4: Cleanup test record
        if let Err(e) = sqlx::query("DELETE FROM health_records WHERE node_name = 'test-node'")
            .execute(&self.pool)
            .await {
            warn!("Failed to cleanup test record (non-critical): {}", e);
        } else {
            info!("✅ Test record cleaned up");
        }

        Ok(())
    }

    pub async fn store_health_record(&self, record: &HealthRecord) -> Result<()> {
        debug!("Storing health record for: {}", record.node_name);

        match sqlx::query(
            r#"
            INSERT INTO health_records (
                node_name, is_healthy, error_message, timestamp,
                block_height, is_syncing, is_catching_up, validator_address
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&record.node_name)
        .bind(record.is_healthy)
        .bind(&record.error_message)
        .bind(record.timestamp)
        .bind(record.block_height)
        .bind(record.is_syncing)
        .bind(record.is_catching_up)
        .bind(&record.validator_address)
        .execute(&self.pool)
        .await
        {
            Ok(_) => {
                debug!("✅ Health record stored for: {}", record.node_name);
                Ok(())
            }
            Err(e) => {
                error!("❌ Failed to store health record for {}: {}", record.node_name, e);
                Err(e.into())
            }
        }
    }

    pub async fn get_latest_health_record(&self, node_name: &str) -> Result<Option<HealthRecord>> {
        debug!("Querying latest health record for: {}", node_name);

        let row = sqlx::query(
            r#"
            SELECT node_name, is_healthy, error_message, timestamp,
                   block_height, is_syncing, is_catching_up, validator_address
            FROM health_records
            WHERE node_name = ?
            ORDER BY timestamp DESC
            LIMIT 1
            "#,
        )
        .bind(node_name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let record = HealthRecord {
                node_name: row.try_get("node_name")?,
                is_healthy: row.try_get("is_healthy")?,
                error_message: row.try_get("error_message")?,
                timestamp: row.try_get("timestamp")?,
                block_height: row.try_get("block_height")?,
                is_syncing: row.try_get("is_syncing")?,
                is_catching_up: row.try_get("is_catching_up")?,
                validator_address: row.try_get("validator_address")?,
            };
            debug!("✅ Found health record for: {}", node_name);
            Ok(Some(record))
        } else {
            debug!("No health record found for: {}", node_name);
            Ok(None)
        }
    }

    pub async fn store_maintenance_operation(&self, operation: &MaintenanceOperation) -> Result<()> {
        debug!("Storing maintenance operation: {}", operation.id);

        match sqlx::query(
            r#"
            INSERT OR REPLACE INTO maintenance_operations (
                id, operation_type, target_name, status, started_at,
                completed_at, error_message, details
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&operation.id)
        .bind(&operation.operation_type)
        .bind(&operation.target_name)
        .bind(&operation.status)
        .bind(operation.started_at)
        .bind(operation.completed_at)
        .bind(&operation.error_message)
        .bind(&operation.details)
        .execute(&self.pool)
        .await
        {
            Ok(_) => {
                debug!("✅ Maintenance operation stored: {}", operation.id);
                Ok(())
            }
            Err(e) => {
                error!("❌ Failed to store maintenance operation {}: {}", operation.id, e);
                Err(e.into())
            }
        }
    }

    pub async fn get_maintenance_operations(&self, limit: Option<i32>) -> Result<Vec<MaintenanceOperation>> {
        debug!("Querying maintenance operations with limit: {:?}", limit);

        let limit_val = limit.unwrap_or(100);

        let rows = sqlx::query(
            r#"
            SELECT id, operation_type, target_name, status, started_at,
                   completed_at, error_message, details
            FROM maintenance_operations
            ORDER BY started_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit_val)
        .fetch_all(&self.pool)
        .await?;

        let mut operations = Vec::new();
        for row in rows {
            let operation = MaintenanceOperation {
                id: row.try_get("id")?,
                operation_type: row.try_get("operation_type")?,
                target_name: row.try_get("target_name")?,
                status: row.try_get("status")?,
                started_at: row.try_get("started_at")?,
                completed_at: row.try_get("completed_at")?,
                error_message: row.try_get("error_message")?,
                details: row.try_get("details")?,
            };
            operations.push(operation);
        }

        debug!("✅ Retrieved {} maintenance operations", operations.len());
        Ok(operations)
    }

    pub async fn get_connection_pool(&self) -> &SqlitePool {
        &self.pool
    }
}
