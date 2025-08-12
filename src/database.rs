// File: src/database.rs

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{Row, SqlitePool};
use std::path::Path;
use tokio::fs;
use tracing::{info, warn};

use crate::{HealthStatus, MaintenanceOperation, NodeHealth};

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn save_node_health(&self, health: &NodeHealth) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO node_health (
                node_name, status, block_height, block_time, catching_up,
                error_message, timestamp
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&health.node_name)
        .bind(serde_json::to_string(&health.status)?)
        .bind(health.latest_block_height.map(|h| h as i64))
        .bind(&health.latest_block_time)
        .bind(health.catching_up)
        .bind(&health.error_message)
        .bind(health.last_check.timestamp())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_latest_node_health(&self, node_name: &str) -> Result<Option<NodeHealth>> {
        let row = sqlx::query(
            r#"
            SELECT node_name, status, block_height, block_time, catching_up,
                   error_message, timestamp
            FROM node_health
            WHERE node_name = ?
            ORDER BY timestamp DESC
            LIMIT 1
            "#,
        )
        .bind(node_name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let status_str: String = row.get("status");
            let status: HealthStatus = serde_json::from_str(&status_str)?;
            let block_height: Option<i64> = row.get("block_height");
            let timestamp: i64 = row.get("timestamp");

            Ok(Some(NodeHealth {
                node_name: row.get("node_name"),
                status,
                latest_block_height: block_height.map(|h| h as u64),
                latest_block_time: row.get("block_time"),
                catching_up: row.get("catching_up"),
                last_check: DateTime::from_timestamp(timestamp, 0).unwrap_or_default().with_timezone(&Utc),
                error_message: row.get("error_message"),
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn get_all_latest_health(&self) -> Result<Vec<NodeHealth>> {
        let rows = sqlx::query(
            r#"
            SELECT DISTINCT node_name FROM node_health
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut health_records = Vec::new();
        for row in rows {
            let node_name: String = row.get("node_name");
            if let Some(health) = self.get_latest_node_health(&node_name).await? {
                health_records.push(health);
            }
        }

        Ok(health_records)
    }

    pub async fn get_node_health_history(
        &self,
        node_name: &str,
        limit: i32,
    ) -> Result<Vec<NodeHealth>> {
        let rows = sqlx::query(
            r#"
            SELECT node_name, status, block_height, block_time, catching_up,
                   error_message, timestamp
            FROM node_health
            WHERE node_name = ?
            ORDER BY timestamp DESC
            LIMIT ?
            "#,
        )
        .bind(node_name)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut health_records = Vec::new();
        for row in rows {
            let status_str: String = row.get("status");
            let status: HealthStatus = serde_json::from_str(&status_str)?;
            let block_height: Option<i64> = row.get("block_height");
            let timestamp: i64 = row.get("timestamp");

            health_records.push(NodeHealth {
                node_name: row.get("node_name"),
                status,
                latest_block_height: block_height.map(|h| h as u64),
                latest_block_time: row.get("block_time"),
                catching_up: row.get("catching_up"),
                last_check: DateTime::from_timestamp(timestamp, 0).unwrap_or_default().with_timezone(&Utc),
                error_message: row.get("error_message"),
            });
        }

        Ok(health_records)
    }

    pub async fn save_maintenance_operation(&self, operation: &MaintenanceOperation) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO maintenance_logs (
                operation_id, operation_type, target_name, status,
                started_at, completed_at, error_message
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&operation.id)
        .bind(&operation.operation_type)
        .bind(&operation.target_name)
        .bind(&operation.status)
        .bind(operation.started_at.map(|dt| dt.timestamp()))
        .bind(operation.completed_at.map(|dt| dt.timestamp()))
        .bind(&operation.error_message)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_maintenance_operation(&self, operation: &MaintenanceOperation) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE maintenance_logs
            SET status = ?, completed_at = ?, error_message = ?
            WHERE operation_id = ?
            "#,
        )
        .bind(&operation.status)
        .bind(operation.completed_at.map(|dt| dt.timestamp()))
        .bind(&operation.error_message)
        .bind(&operation.id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_maintenance_logs(&self, limit: i32) -> Result<Vec<MaintenanceOperation>> {
        let rows = sqlx::query(
            r#"
            SELECT operation_id, operation_type, target_name, status,
                   started_at, completed_at, error_message
            FROM maintenance_logs
            ORDER BY started_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut operations = Vec::new();
        for row in rows {
            let started_at: Option<i64> = row.get("started_at");
            let completed_at: Option<i64> = row.get("completed_at");

            operations.push(MaintenanceOperation {
                id: row.get("operation_id"),
                operation_type: row.get("operation_type"),
                target_name: row.get("target_name"),
                status: row.get("status"),
                started_at: started_at.and_then(|ts| DateTime::from_timestamp(ts, 0)).map(|dt| dt.with_timezone(&Utc)),
                completed_at: completed_at.and_then(|ts| DateTime::from_timestamp(ts, 0)).map(|dt| dt.with_timezone(&Utc)),
                error_message: row.get("error_message"),
            });
        }

        Ok(operations)
    }

    pub async fn cleanup_old_health_records(&self, days: i32) -> Result<u64> {
        let cutoff = Utc::now() - chrono::Duration::days(days as i64);

        let result = sqlx::query(
            "DELETE FROM node_health WHERE timestamp < ?"
        )
        .bind(cutoff.timestamp())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

pub async fn init_database() -> Result<Database> {
    // Ensure data directory exists
    if !Path::new("data").exists() {
        fs::create_dir_all("data").await?;
        info!("Created data directory");
    }

    // Connect to database
    let database_url = "sqlite:data/nodes.db";
    let pool = SqlitePool::connect(database_url).await?;

    info!("Connected to database: {}", database_url);

    // Create tables if they don't exist
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS node_health (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            node_name TEXT NOT NULL,
            status TEXT NOT NULL,
            block_height INTEGER,
            block_time TEXT,
            catching_up BOOLEAN,
            error_message TEXT,
            timestamp DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Create indices separately
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_node_health_name_timestamp
        ON node_health(node_name, timestamp)
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS maintenance_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            operation_id TEXT UNIQUE NOT NULL,
            operation_type TEXT NOT NULL,
            target_name TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at DATETIME,
            completed_at DATETIME,
            error_message TEXT
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Create indices separately
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_maintenance_logs_operation_id
        ON maintenance_logs(operation_id)
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_maintenance_logs_status_started
        ON maintenance_logs(status, started_at)
        "#,
    )
    .execute(&pool)
    .await?;

    info!("Database tables created/verified");

    // Test database connection
    let test_result = sqlx::query("SELECT 1 as test").fetch_one(&pool).await;
    match test_result {
        Ok(_) => info!("Database connection test successful"),
        Err(e) => warn!("Database connection test failed: {}", e),
    }

    Ok(Database::new(pool))
}
