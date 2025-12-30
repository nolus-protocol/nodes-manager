//! Health record database operations.

use anyhow::Result;
use sqlx::Row;
use tracing::{debug, error};

use super::records::{HealthRecord, HermesHealthRecord};
use super::Database;

impl Database {
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
                debug!("Health record stored for: {}", record.node_name);
                Ok(())
            }
            Err(e) => {
                error!(
                    "Failed to store health record for {}: {}",
                    record.node_name, e
                );
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
            debug!("Found health record for: {}", node_name);
            Ok(Some(record))
        } else {
            debug!("No health record found for: {}", node_name);
            Ok(None)
        }
    }

    pub async fn store_hermes_health_record(&self, record: &HermesHealthRecord) -> Result<()> {
        debug!("Storing hermes health record for: {}", record.hermes_name);

        match sqlx::query(
            r#"
            INSERT INTO hermes_health_records (
                hermes_name, is_healthy, status, uptime_seconds, error_message,
                timestamp, server_host, service_name
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&record.hermes_name)
        .bind(record.is_healthy)
        .bind(&record.status)
        .bind(record.uptime_seconds)
        .bind(&record.error_message)
        .bind(record.timestamp)
        .bind(&record.server_host)
        .bind(&record.service_name)
        .execute(&self.pool)
        .await
        {
            Ok(_) => {
                debug!("Hermes health record stored for: {}", record.hermes_name);
                Ok(())
            }
            Err(e) => {
                error!(
                    "Failed to store hermes health record for {}: {}",
                    record.hermes_name, e
                );
                Err(e.into())
            }
        }
    }

    pub async fn get_latest_hermes_health_record(
        &self,
        hermes_name: &str,
    ) -> Result<Option<HermesHealthRecord>> {
        debug!("Querying latest hermes health record for: {}", hermes_name);

        let row = sqlx::query(
            r#"
            SELECT hermes_name, is_healthy, status, uptime_seconds, error_message,
                   timestamp, server_host, service_name
            FROM hermes_health_records
            WHERE hermes_name = ?
            ORDER BY timestamp DESC
            LIMIT 1
            "#,
        )
        .bind(hermes_name)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
            let record = HermesHealthRecord {
                hermes_name: row.try_get("hermes_name")?,
                is_healthy: row.try_get("is_healthy")?,
                status: row.try_get("status")?,
                uptime_seconds: row.try_get("uptime_seconds")?,
                error_message: row.try_get("error_message")?,
                timestamp: row.try_get("timestamp")?,
                server_host: row.try_get("server_host")?,
                service_name: row.try_get("service_name")?,
            };
            debug!("Found hermes health record for: {}", hermes_name);
            Ok(Some(record))
        } else {
            debug!("No hermes health record found for: {}", hermes_name);
            Ok(None)
        }
    }
}
