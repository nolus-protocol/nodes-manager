//! Integration tests for operation tracking
//!
//! These tests verify that operations are tracked correctly
//! and concurrent operations are prevented.

mod common;

use common::fixtures::*;
use manager::operation_tracker::SimpleOperationTracker;

#[tokio::test]
async fn test_prevents_concurrent_operations_on_same_target() {
    let tracker = SimpleOperationTracker::new();

    // Start first operation
    let result1 = tracker
        .try_start_operation(nodes::NODE_1, operations::PRUNING, None)
        .await;
    assert!(result1.is_ok(), "First operation should start successfully");

    // Try to start second operation on same target (should fail)
    let result2 = tracker
        .try_start_operation(nodes::NODE_1, operations::SNAPSHOT_CREATE, None)
        .await;
    assert!(
        result2.is_err(),
        "Second operation should fail - target is busy"
    );

    let error_msg = result2.unwrap_err().to_string();
    assert!(
        error_msg.contains("busy"),
        "Error should indicate target is busy, got: {}",
        error_msg
    );
}

#[tokio::test]
async fn test_allows_operations_on_different_targets() {
    let tracker = SimpleOperationTracker::new();

    // Start operations on different targets
    assert!(tracker
        .try_start_operation(nodes::NODE_1, operations::PRUNING, None)
        .await
        .is_ok());
    assert!(tracker
        .try_start_operation(nodes::NODE_2, operations::SNAPSHOT_CREATE, None)
        .await
        .is_ok());
    assert!(tracker
        .try_start_operation(nodes::NODE_3, operations::STATE_SYNC, None)
        .await
        .is_ok());

    // All should be busy
    assert!(tracker.is_busy(nodes::NODE_1).await);
    assert!(tracker.is_busy(nodes::NODE_2).await);
    assert!(tracker.is_busy(nodes::NODE_3).await);
}

#[tokio::test]
async fn test_finish_operation_allows_new_operation() {
    let tracker = SimpleOperationTracker::new();

    // Start and finish operation
    tracker
        .try_start_operation(nodes::NODE_1, operations::PRUNING, None)
        .await
        .unwrap();
    tracker.finish_operation(nodes::NODE_1).await;

    // Should not be busy anymore
    assert!(!tracker.is_busy(nodes::NODE_1).await);

    // Should be able to start new operation
    let result = tracker
        .try_start_operation(nodes::NODE_1, operations::SNAPSHOT_CREATE, None)
        .await;
    assert!(
        result.is_ok(),
        "Should be able to start operation after previous finished"
    );
}

#[tokio::test]
async fn test_cancel_operation() {
    let tracker = SimpleOperationTracker::new();

    // Start operation
    tracker
        .try_start_operation(nodes::NODE_1, operations::PRUNING, None)
        .await
        .unwrap();

    // Cancel it
    let cancel_result = tracker.cancel_operation(nodes::NODE_1).await;
    assert!(
        cancel_result.is_ok(),
        "Should be able to cancel active operation"
    );

    // Should not be busy anymore
    assert!(!tracker.is_busy(nodes::NODE_1).await);

    // Trying to cancel again should fail
    let cancel_again = tracker.cancel_operation(nodes::NODE_1).await;
    assert!(
        cancel_again.is_err(),
        "Canceling non-existent operation should fail"
    );
}

#[tokio::test]
async fn test_get_operation_status() {
    let tracker = SimpleOperationTracker::new();

    // Initially no operations
    let status = tracker.get_operation_status().await;
    assert_eq!(status.total_active, 0);

    // Start some operations
    tracker
        .try_start_operation(
            nodes::NODE_1,
            operations::PRUNING,
            Some("user1".to_string()),
        )
        .await
        .unwrap();
    tracker
        .try_start_operation(
            nodes::NODE_2,
            operations::SNAPSHOT_CREATE,
            Some("user2".to_string()),
        )
        .await
        .unwrap();

    // Check status
    let status = tracker.get_operation_status().await;
    assert_eq!(status.total_active, 2);
    assert!(status.busy_nodes.contains_key(nodes::NODE_1));
    assert!(status.busy_nodes.contains_key(nodes::NODE_2));

    // Verify operation details
    let op1 = status.busy_nodes.get(nodes::NODE_1).unwrap();
    assert_eq!(op1.operation_type, operations::PRUNING);
    assert_eq!(op1.user_info, Some("user1".to_string()));
}

#[tokio::test]
async fn test_cleanup_old_operations() {
    let tracker = SimpleOperationTracker::new();

    // Start operation
    tracker
        .try_start_operation(nodes::NODE_1, operations::PRUNING, None)
        .await
        .unwrap();

    // Cleanup with 0 hours (should remove all)
    let cleaned = tracker.cleanup_old_operations(0).await;
    assert_eq!(cleaned, 1, "Should have cleaned up 1 operation");

    // Should not be busy anymore
    assert!(!tracker.is_busy(nodes::NODE_1).await);
}
