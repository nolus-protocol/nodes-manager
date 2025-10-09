//! Test database utilities for in-memory SQLite testing

use anyhow::Result;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

/// Test database wrapper for in-memory SQLite
pub struct TestDatabase {
    pool: SqlitePool,
}

impl TestDatabase {
    /// Create a new in-memory test database
    pub async fn new() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;

        // Run migrations
        Self::run_migrations(&pool).await?;

        Ok(Self { pool })
    }

    /// Get the database pool
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Run database migrations
    async fn run_migrations(pool: &SqlitePool) -> Result<()> {
        // Create node_health table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS node_health (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                node_name TEXT NOT NULL,
                server_name TEXT NOT NULL,
                network TEXT NOT NULL,
                is_healthy BOOLEAN NOT NULL,
                is_synced BOOLEAN NOT NULL,
                latest_block_height INTEGER,
                catching_up BOOLEAN,
                checked_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                error_message TEXT
            )
            "#,
        )
        .execute(pool)
        .await?;

        // Create maintenance_logs table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS maintenance_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                node_name TEXT NOT NULL,
                server_name TEXT NOT NULL,
                operation_type TEXT NOT NULL,
                started_at TIMESTAMP NOT NULL,
                completed_at TIMESTAMP,
                status TEXT NOT NULL,
                error_message TEXT,
                duration_minutes INTEGER
            )
            "#,
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Clear all data from tables (useful between tests)
    pub async fn clear(&self) -> Result<()> {
        sqlx::query("DELETE FROM node_health")
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM maintenance_logs")
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
