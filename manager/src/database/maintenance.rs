//! Maintenance operation database operations.

use anyhow::Result;
use sqlx::Row;
use tracing::debug;

use super::records::MaintenanceOperation;
use super::Database;

impl Database {
    pub async fn store_maintenance_operation(
        &self,
        operation: &MaintenanceOperation,
    ) -> Result<()> {
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
                debug!("Maintenance operation stored: {}", operation.id);
                Ok(())
            }
            Err(e) => {
                tracing::error!(
                    "Failed to store maintenance operation {}: {}",
                    operation.id,
                    e
                );
                Err(e.into())
            }
        }
    }

    pub async fn get_maintenance_operation_by_id(
        &self,
        operation_id: &str,
    ) -> Result<Option<MaintenanceOperation>> {
        debug!("Querying maintenance operation by ID: {}", operation_id);

        let row = sqlx::query(
            r#"
            SELECT id, operation_type, target_name, status, started_at,
                   completed_at, error_message, details
            FROM maintenance_operations
            WHERE id = ?
            "#,
        )
        .bind(operation_id)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = row {
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
            debug!("Found maintenance operation: {}", operation_id);
            Ok(Some(operation))
        } else {
            debug!("No maintenance operation found with ID: {}", operation_id);
            Ok(None)
        }
    }
}
