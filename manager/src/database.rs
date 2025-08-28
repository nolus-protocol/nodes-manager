// File: manager/src/database.rs
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Row, Sqlite, SqlitePool};
use std::path::Path;
use tracing::{debug, info};

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
        // Ensure parent directory exists
        if let Some(parent) = Path::new(database_path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let database_url = format!("sqlite:{}", database_path);
        let pool = SqlitePool::connect(&database_url).await?;

        let database = Self { pool };
        database.initialize_tables().await?;

        info!("Database initialized at {}", database_path);
        Ok(database)
    }

    async fn initialize_tables(&self) -> Result<()> {
        // Create health records table
        sqlx::query(
            r#"
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
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create indexes for health records as separate statements
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_health_node_timestamp ON health_records(node_name, timestamp)"
        )
        .execute(&self.pool)
        .await?;

        // Create maintenance operations table
        sqlx::query(
            r#"
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
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create indexes for maintenance operations as separate statements
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_maintenance_target ON maintenance_operations(target_name, started_at)"
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_maintenance_status ON maintenance_operations(status, started_at)"
        )
        .execute(&self.pool)
        .await?;

        debug!("Database tables initialized");
        Ok(())
    }

    pub async fn store_health_record(&self, record: &HealthRecord) -> Result<()> {
        sqlx::query(
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
        .await?;

        Ok(())
    }

    pub async fn get_latest_health_record(&self, node_name: &str) -> Result<Option<HealthRecord>> {
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
                node_name: row.get("node_name"),
                is_healthy: row.get("is_healthy"),
                error_message: row.get("error_message"),
                timestamp: row.get("timestamp"),
                block_height: row.get("block_height"),
                is_syncing: row.get("is_syncing"),
                is_catching_up: row.get("is_catching_up"),
                validator_address: row.get("validator_address"),
            };
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    pub async fn store_maintenance_operation(&self, operation: &MaintenanceOperation) -> Result<()> {
        sqlx::query(
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
        .await?;

        Ok(())
    }
}
