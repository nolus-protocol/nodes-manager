//! Unit tests for database operations
//!
//! These tests verify database CRUD operations work correctly
//! using in-memory SQLite for speed and isolation.

mod common;

use chrono::Utc;
use common::fixtures::*;
use sqlx::Row;

#[tokio::test]
async fn test_database_initialization() {
    let db = TestDatabase::new()
        .await
        .expect("Failed to create test database");
    let pool = db.pool();

    // Verify tables exist
    let result = sqlx::query("SELECT name FROM sqlite_master WHERE type='table'")
        .fetch_all(pool)
        .await
        .expect("Failed to query tables");

    let table_names: Vec<String> = result
        .iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();

    assert!(table_names.contains(&"node_health".to_string()));
    assert!(table_names.contains(&"maintenance_logs".to_string()));
}

#[tokio::test]
async fn test_insert_node_health_record() {
    let db = TestDatabase::new().await.unwrap();
    let pool = db.pool();

    let result = sqlx::query(
        r#"
        INSERT INTO node_health 
        (node_name, server_name, network, is_healthy, is_synced, latest_block_height, catching_up)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("test-node")
    .bind("test-server")
    .bind("osmosis-1")
    .bind(true)
    .bind(true)
    .bind(1000000i64)
    .bind(false)
    .execute(pool)
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().rows_affected(), 1);
}

#[tokio::test]
async fn test_query_node_health_records() {
    let db = TestDatabase::new().await.unwrap();
    let pool = db.pool();

    // Insert test data
    sqlx::query(
        "INSERT INTO node_health (node_name, server_name, network, is_healthy, is_synced, latest_block_height, catching_up)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind("node-1")
    .bind("server-1")
    .bind("osmosis-1")
    .bind(true)
    .bind(true)
    .bind(1000000i64)
    .bind(false)
    .execute(pool)
    .await
    .unwrap();

    // Query data
    let records = sqlx::query("SELECT * FROM node_health WHERE node_name = ?")
        .bind("node-1")
        .fetch_all(pool)
        .await
        .unwrap();

    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.get::<String, _>("node_name"), "node-1");
    assert_eq!(record.get::<String, _>("network"), "osmosis-1");
    assert!(record.get::<bool, _>("is_healthy"));
    assert!(record.get::<bool, _>("is_synced"));
}

#[tokio::test]
async fn test_insert_maintenance_log() {
    let db = TestDatabase::new().await.unwrap();
    let pool = db.pool();

    let started_at = Utc::now();

    let result = sqlx::query(
        r#"
        INSERT INTO maintenance_logs
        (node_name, server_name, operation_type, started_at, status)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind("test-node")
    .bind("test-server")
    .bind("pruning")
    .bind(started_at)
    .bind("in_progress")
    .execute(pool)
    .await;

    assert!(result.is_ok());
    assert_eq!(result.unwrap().rows_affected(), 1);
}

#[tokio::test]
async fn test_update_maintenance_log_completion() {
    let db = TestDatabase::new().await.unwrap();
    let pool = db.pool();

    let started_at = Utc::now();

    // Insert maintenance log
    let insert_result = sqlx::query(
        "INSERT INTO maintenance_logs (node_name, server_name, operation_type, started_at, status)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("test-node")
    .bind("test-server")
    .bind("snapshot")
    .bind(started_at)
    .bind("in_progress")
    .execute(pool)
    .await
    .unwrap();

    let log_id = insert_result.last_insert_rowid();

    // Update to completed
    let completed_at = Utc::now();
    let update_result = sqlx::query(
        "UPDATE maintenance_logs SET status = ?, completed_at = ?, duration_minutes = ? WHERE id = ?"
    )
    .bind("completed")
    .bind(completed_at)
    .bind(15i32)
    .bind(log_id)
    .execute(pool)
    .await
    .unwrap();

    assert_eq!(update_result.rows_affected(), 1);

    // Verify update
    let record = sqlx::query("SELECT * FROM maintenance_logs WHERE id = ?")
        .bind(log_id)
        .fetch_one(pool)
        .await
        .unwrap();

    assert_eq!(record.get::<String, _>("status"), "completed");
    assert_eq!(record.get::<i32, _>("duration_minutes"), 15);
}

#[tokio::test]
async fn test_query_health_history_by_node() {
    let db = TestDatabase::new().await.unwrap();
    let pool = db.pool();

    // Insert multiple health records for same node
    for i in 0..5 {
        sqlx::query(
            "INSERT INTO node_health (node_name, server_name, network, is_healthy, is_synced, latest_block_height, catching_up)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind("node-1")
        .bind("server-1")
        .bind("osmosis-1")
        .bind(true)
        .bind(true)
        .bind((1000000 + i * 100) as i64)
        .bind(false)
        .execute(pool)
        .await
        .unwrap();
    }

    // Query health history
    let records =
        sqlx::query("SELECT * FROM node_health WHERE node_name = ? ORDER BY checked_at DESC")
            .bind("node-1")
            .fetch_all(pool)
            .await
            .unwrap();

    assert_eq!(records.len(), 5);
}

#[tokio::test]
async fn test_query_maintenance_logs_by_server() {
    let db = TestDatabase::new().await.unwrap();
    let pool = db.pool();

    let started_at = Utc::now();

    // Insert logs for different servers
    sqlx::query(
        "INSERT INTO maintenance_logs (node_name, server_name, operation_type, started_at, status)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("node-1")
    .bind("server-1")
    .bind("pruning")
    .bind(started_at)
    .bind("completed")
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO maintenance_logs (node_name, server_name, operation_type, started_at, status)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("node-2")
    .bind("server-2")
    .bind("snapshot")
    .bind(started_at)
    .bind("completed")
    .execute(pool)
    .await
    .unwrap();

    // Query logs for server-1
    let records = sqlx::query("SELECT * FROM maintenance_logs WHERE server_name = ?")
        .bind("server-1")
        .fetch_all(pool)
        .await
        .unwrap();

    assert_eq!(records.len(), 1);
    assert_eq!(records[0].get::<String, _>("node_name"), "node-1");
}

#[tokio::test]
async fn test_delete_old_health_records() {
    let db = TestDatabase::new().await.unwrap();
    let pool = db.pool();

    // Insert health records
    for i in 0..10 {
        sqlx::query(
            "INSERT INTO node_health (node_name, server_name, network, is_healthy, is_synced, latest_block_height, catching_up)
             VALUES (?, ?, ?, ?, ?, ?, ?)"
        )
        .bind(format!("node-{}", i))
        .bind("server-1")
        .bind("osmosis-1")
        .bind(true)
        .bind(true)
        .bind(1000000i64)
        .bind(false)
        .execute(pool)
        .await
        .unwrap();
    }

    // Delete some records
    let delete_result = sqlx::query("DELETE FROM node_health WHERE node_name LIKE 'node-%'")
        .execute(pool)
        .await
        .unwrap();

    assert_eq!(delete_result.rows_affected(), 10);
}

#[tokio::test]
async fn test_clear_database() {
    let db = TestDatabase::new().await.unwrap();
    let pool = db.pool();

    // Insert data
    sqlx::query(
        "INSERT INTO node_health (node_name, server_name, network, is_healthy, is_synced, latest_block_height, catching_up)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind("node-1")
    .bind("server-1")
    .bind("osmosis-1")
    .bind(true)
    .bind(true)
    .bind(1000000i64)
    .bind(false)
    .execute(pool)
    .await
    .unwrap();

    // Clear database
    db.clear().await.unwrap();

    // Verify empty
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM node_health")
        .fetch_one(pool)
        .await
        .unwrap();

    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_health_record_with_error_message() {
    let db = TestDatabase::new().await.unwrap();
    let pool = db.pool();

    let result = sqlx::query(
        r#"
        INSERT INTO node_health 
        (node_name, server_name, network, is_healthy, is_synced, latest_block_height, catching_up, error_message)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("failing-node")
    .bind("server-1")
    .bind("osmosis-1")
    .bind(false)
    .bind(false)
    .bind(0i64)
    .bind(false)
    .bind("Connection timeout")
    .execute(pool)
    .await
    .unwrap();

    assert_eq!(result.rows_affected(), 1);

    // Query and verify error message
    let record = sqlx::query("SELECT * FROM node_health WHERE node_name = ?")
        .bind("failing-node")
        .fetch_one(pool)
        .await
        .unwrap();

    assert_eq!(
        record.get::<String, _>("error_message"),
        "Connection timeout"
    );
}

#[tokio::test]
async fn test_maintenance_log_with_error() {
    let db = TestDatabase::new().await.unwrap();
    let pool = db.pool();

    let started_at = Utc::now();

    let result = sqlx::query(
        r#"
        INSERT INTO maintenance_logs
        (node_name, server_name, operation_type, started_at, status, error_message)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("test-node")
    .bind("test-server")
    .bind("snapshot")
    .bind(started_at)
    .bind("failed")
    .bind("Disk full")
    .execute(pool)
    .await
    .unwrap();

    assert_eq!(result.rows_affected(), 1);

    // Query and verify
    let record = sqlx::query("SELECT * FROM maintenance_logs WHERE node_name = ?")
        .bind("test-node")
        .fetch_one(pool)
        .await
        .unwrap();

    assert_eq!(record.get::<String, _>("status"), "failed");
    assert_eq!(record.get::<String, _>("error_message"), "Disk full");
}
