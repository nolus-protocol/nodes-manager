//! Integration tests for OperationExecutor
//!
//! These tests verify the core operation execution framework that all manual
//! and scheduled operations use. Critical for preventing bugs like operations
//! getting stuck or maintenance windows not being cleaned up.

mod common;

use common::fixtures::*;
use manager::config::ConfigManager;
use manager::database::Database;
use manager::services::{AlertService, OperationExecutor};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Helper to create a test OperationExecutor with all dependencies
async fn setup_test_executor() -> (Arc<OperationExecutor>, Arc<Database>, Arc<AlertService>) {
    // Create test database
    let db = Database::new(":memory:")
        .await
        .expect("Failed to create test database");
    let database = Arc::new(db);

    // Create test config
    let config_builder = TestConfigBuilder::new()
        .with_main_config(|main| main.alert_webhook("http://localhost:9999/webhook"))
        .with_server("test-server", |server| {
            server.add_node(|node| {
                node.name("test-node-1")
                    .rpc_url("http://localhost:26657")
                    .network("test-network")
            })
        });

    let test_config = config_builder.build();
    let config_manager = ConfigManager::new(test_config.config_dir().to_string_lossy().to_string())
        .await
        .expect("Failed to create config manager");
    let config = config_manager.get_current_config();

    // Create alert service (disabled for tests)
    let alert_service = Arc::new(AlertService::new("".to_string()));

    // Create OperationExecutor
    let executor = Arc::new(OperationExecutor::new(
        config.clone(),
        database.clone(),
        alert_service.clone(),
    ));

    (executor, database, alert_service)
}

#[tokio::test]
async fn test_operation_completes_in_background() {
    let (executor, database, _) = setup_test_executor().await;
    let start = Instant::now();

    // Execute a slow operation
    let op_id = executor
        .execute_async("test_operation", "test-node-1", || async {
            sleep(Duration::from_secs(2)).await;
            Ok(())
        })
        .await
        .expect("Operation should start successfully");

    // Verify it returns immediately (< 100ms)
    assert!(
        start.elapsed() < Duration::from_millis(100),
        "execute_async should return immediately, took: {:?}",
        start.elapsed()
    );

    // Verify operation was recorded in database with "started" status
    let ops = database
        .get_maintenance_operations(Some(10))
        .await
        .expect("Should fetch operations");
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].id, op_id);
    assert_eq!(ops[0].status, "started");
    assert_eq!(ops[0].operation_type, "test_operation");
    assert_eq!(ops[0].target_name, "test-node-1");

    // Wait for background task to complete
    sleep(Duration::from_secs(3)).await;

    // Verify operation was updated to "completed"
    let ops = database
        .get_maintenance_operations(Some(10))
        .await
        .expect("Should fetch operations");
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].status, "completed");
    assert!(ops[0].completed_at.is_some());
    assert!(ops[0].error_message.is_none());
}

#[tokio::test]
async fn test_operation_failure_recorded_correctly() {
    let (executor, database, _) = setup_test_executor().await;

    // Execute an operation that fails
    let op_id = executor
        .execute_async("failing_operation", "test-node-1", || async {
            sleep(Duration::from_millis(100)).await;
            Err(anyhow::anyhow!(
                "Test error: operation failed intentionally"
            ))
        })
        .await
        .expect("Operation should start successfully");

    // Wait for background task to complete
    sleep(Duration::from_millis(500)).await;

    // Verify operation was recorded with failure
    let ops = database
        .get_maintenance_operations(Some(10))
        .await
        .expect("Should fetch operations");
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].id, op_id);
    assert_eq!(ops[0].status, "failed");
    assert!(ops[0].completed_at.is_some());
    assert!(ops[0].error_message.is_some());

    let error_msg = ops[0].error_message.as_ref().unwrap();
    assert!(
        error_msg.contains("operation failed intentionally"),
        "Error message should contain failure reason, got: {}",
        error_msg
    );
}

#[tokio::test]
async fn test_multiple_concurrent_operations_execute_independently() {
    let (executor, database, _) = setup_test_executor().await;

    // Start three operations on different nodes simultaneously
    let op1 = executor
        .execute_async("operation1", "node-1", || async {
            sleep(Duration::from_millis(200)).await;
            Ok(())
        })
        .await
        .expect("Op 1 should start");

    let op2 = executor
        .execute_async("operation2", "node-2", || async {
            sleep(Duration::from_millis(300)).await;
            Ok(())
        })
        .await
        .expect("Op 2 should start");

    let op3 = executor
        .execute_async("operation3", "node-3", || async {
            sleep(Duration::from_millis(100)).await;
            Ok(())
        })
        .await
        .expect("Op 3 should start");

    // All should have unique operation IDs
    assert_ne!(op1, op2);
    assert_ne!(op2, op3);
    assert_ne!(op1, op3);

    // All should be recorded as started
    let ops = database
        .get_maintenance_operations(Some(10))
        .await
        .expect("Should fetch operations");
    assert_eq!(ops.len(), 3);
    assert!(ops.iter().all(|op| op.status == "started"));

    // Wait for all to complete
    sleep(Duration::from_millis(500)).await;

    // All should be completed
    let ops = database
        .get_maintenance_operations(Some(10))
        .await
        .expect("Should fetch operations");
    assert_eq!(ops.len(), 3);
    assert!(
        ops.iter().all(|op| op.status == "completed"),
        "All operations should complete successfully"
    );
}

#[tokio::test]
async fn test_operation_id_uniqueness() {
    let (executor, _, _) = setup_test_executor().await;

    // Start multiple operations quickly
    let mut operation_ids = Vec::new();
    for i in 0..10 {
        let op_id = executor
            .execute_async("test_op", &format!("node-{}", i), || async { Ok(()) })
            .await
            .expect("Operation should start");
        operation_ids.push(op_id);
    }

    // Verify all IDs are unique
    let unique_count = operation_ids
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len();
    assert_eq!(
        unique_count,
        operation_ids.len(),
        "All operation IDs should be unique"
    );
}

#[tokio::test]
async fn test_operation_records_correct_metadata() {
    let (executor, database, _) = setup_test_executor().await;

    let operation_type = "snapshot_creation";
    let target_name = "production-node";

    executor
        .execute_async(operation_type, target_name, || async {
            sleep(Duration::from_millis(100)).await;
            Ok(())
        })
        .await
        .expect("Operation should start");

    sleep(Duration::from_millis(200)).await;

    let ops = database
        .get_maintenance_operations(Some(10))
        .await
        .expect("Should fetch operations");
    assert_eq!(ops.len(), 1);

    let op = &ops[0];
    assert_eq!(op.operation_type, operation_type);
    assert_eq!(op.target_name, target_name);
    assert!(op.started_at.timestamp() > 0);
    assert!(op.completed_at.is_some());
    assert!(op.completed_at.unwrap().timestamp() >= op.started_at.timestamp());
}

#[tokio::test]
async fn test_operation_with_very_fast_completion() {
    let (executor, database, _) = setup_test_executor().await;

    // Operation that completes immediately
    let op_id = executor
        .execute_async("instant_operation", "test-node", || async { Ok(()) })
        .await
        .expect("Operation should start");

    // Even fast operations should be recorded
    sleep(Duration::from_millis(100)).await;

    let ops = database
        .get_maintenance_operations(Some(10))
        .await
        .expect("Should fetch operations");
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].id, op_id);
    assert_eq!(ops[0].status, "completed");
}

#[tokio::test]
async fn test_error_message_preserved_in_database() {
    let (executor, database, _) = setup_test_executor().await;

    let error_msg = "Network timeout after 30 seconds".to_string();
    let error_msg_clone = error_msg.clone();

    executor
        .execute_async("network_operation", "test-node", move || async move {
            Err(anyhow::anyhow!("{}", error_msg_clone))
        })
        .await
        .expect("Operation should start");

    sleep(Duration::from_millis(200)).await;

    let ops = database
        .get_maintenance_operations(Some(10))
        .await
        .expect("Should fetch operations");
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].status, "failed");

    let stored_error = ops[0].error_message.as_ref().unwrap();
    assert!(
        stored_error.contains(&error_msg),
        "Error message should be preserved. Expected '{}', got '{}'",
        error_msg,
        stored_error
    );
}

#[tokio::test]
async fn test_mixed_success_and_failure_operations() {
    let (executor, database, _) = setup_test_executor().await;

    // Start mix of successful and failing operations
    executor
        .execute_async("success_op_1", "node-1", || async { Ok(()) })
        .await
        .expect("Op should start");

    executor
        .execute_async("fail_op_1", "node-2", || async {
            Err(anyhow::anyhow!("Error 1"))
        })
        .await
        .expect("Op should start");

    executor
        .execute_async("success_op_2", "node-3", || async { Ok(()) })
        .await
        .expect("Op should start");

    executor
        .execute_async("fail_op_2", "node-4", || async {
            Err(anyhow::anyhow!("Error 2"))
        })
        .await
        .expect("Op should start");

    sleep(Duration::from_millis(200)).await;

    let ops = database
        .get_maintenance_operations(Some(10))
        .await
        .expect("Should fetch operations");
    assert_eq!(ops.len(), 4);

    let completed = ops.iter().filter(|op| op.status == "completed").count();
    let failed = ops.iter().filter(|op| op.status == "failed").count();

    assert_eq!(completed, 2, "Should have 2 successful operations");
    assert_eq!(failed, 2, "Should have 2 failed operations");
}

#[tokio::test]
async fn test_operations_with_same_type_different_targets() {
    let (executor, database, _) = setup_test_executor().await;

    // Start same operation type on different targets
    let op1 = executor
        .execute_async("pruning", "node-1", || async { Ok(()) })
        .await
        .expect("Op 1 should start");

    let op2 = executor
        .execute_async("pruning", "node-2", || async { Ok(()) })
        .await
        .expect("Op 2 should start");

    let op3 = executor
        .execute_async("pruning", "node-3", || async { Ok(()) })
        .await
        .expect("Op 3 should start");

    sleep(Duration::from_millis(200)).await;

    let ops = database
        .get_maintenance_operations(Some(10))
        .await
        .expect("Should fetch operations");
    assert_eq!(ops.len(), 3);

    // All should have same operation_type but different targets
    assert!(ops.iter().all(|op| op.operation_type == "pruning"));
    assert_eq!(
        ops.iter()
            .map(|op| op.target_name.as_str())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        3,
        "Should have 3 different target names"
    );

    // All should have unique operation IDs
    assert_ne!(op1, op2);
    assert_ne!(op2, op3);
    assert_ne!(op1, op3);
}
