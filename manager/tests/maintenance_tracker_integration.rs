//! Integration tests for maintenance tracking
//!
//! These tests verify that maintenance windows work correctly
//! and prevent concurrent operations.

mod common;

use common::fixtures::*;
use manager::maintenance_tracker::MaintenanceTracker;

#[tokio::test]
async fn test_prevents_concurrent_operations() {
    let tracker = MaintenanceTracker::new();

    // Start first operation
    let result1 = tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await;
    assert!(result1.is_ok(), "First operation should start successfully");

    // Try to start second operation on same node (should fail)
    let result2 = tracker
        .start_maintenance(
            nodes::NODE_1,
            operations::SNAPSHOT_CREATE,
            300,
            servers::SERVER_1,
        )
        .await;
    assert!(
        result2.is_err(),
        "Second operation should fail - node is busy"
    );
    assert!(
        result2
            .unwrap_err()
            .to_string()
            .contains("already in maintenance"),
        "Error should indicate node is in maintenance"
    );

    // Should still be in maintenance
    assert!(tracker.is_in_maintenance(nodes::NODE_1).await);

    // End maintenance
    tracker.end_maintenance(nodes::NODE_1).await.unwrap();

    // Should no longer be in maintenance
    assert!(!tracker.is_in_maintenance(nodes::NODE_1).await);

    // Should be able to start new operation now
    let result3 = tracker
        .start_maintenance(
            nodes::NODE_1,
            operations::SNAPSHOT_CREATE,
            300,
            servers::SERVER_1,
        )
        .await;
    assert!(
        result3.is_ok(),
        "Should be able to start operation after maintenance ended"
    );
}

#[tokio::test]
async fn test_allows_operations_on_different_nodes() {
    let tracker = MaintenanceTracker::new();

    // Start operations on different nodes
    let result1 = tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await;
    let result2 = tracker
        .start_maintenance(
            nodes::NODE_2,
            operations::SNAPSHOT_CREATE,
            300,
            servers::SERVER_2,
        )
        .await;
    let result3 = tracker
        .start_maintenance(
            nodes::NODE_3,
            operations::STATE_SYNC,
            300,
            servers::SERVER_1,
        )
        .await;

    assert!(result1.is_ok());
    assert!(result2.is_ok());
    assert!(result3.is_ok());

    // All should be in maintenance
    assert!(tracker.is_in_maintenance(nodes::NODE_1).await);
    assert!(tracker.is_in_maintenance(nodes::NODE_2).await);
    assert!(tracker.is_in_maintenance(nodes::NODE_3).await);
}

#[tokio::test]
async fn test_cleanup_expired_maintenance() {
    let tracker = MaintenanceTracker::new();

    // Start a maintenance window
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .unwrap();

    // Cleanup with 0 hours (should remove all)
    let cleaned = tracker.cleanup_expired_maintenance(0).await;
    assert_eq!(cleaned, 1, "Should have cleaned up 1 maintenance window");

    // Should no longer be in maintenance
    assert!(!tracker.is_in_maintenance(nodes::NODE_1).await);
}

#[tokio::test]
async fn test_cleanup_respects_max_duration() {
    let tracker = MaintenanceTracker::new();

    // Start maintenance windows
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .unwrap();

    // Cleanup with very high max duration (should not remove anything)
    let cleaned = tracker.cleanup_expired_maintenance(1000).await;
    assert_eq!(cleaned, 0, "Should not clean up recent maintenance windows");

    // Should still be in maintenance
    assert!(tracker.is_in_maintenance(nodes::NODE_1).await);
}
