//! Business Rule Tests: Mutual Exclusion
//!
//! These tests verify that only one operation can run per node at a time.
//! This is a critical business rule to prevent data corruption and conflicts.

mod common;

use common::fixtures::*;
use manager::{maintenance_tracker::MaintenanceTracker, operation_tracker::SimpleOperationTracker};

#[tokio::test]
async fn test_mutual_exclusion_maintenance_tracker() {
    let tracker = MaintenanceTracker::new();

    // Start first operation
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .expect("First operation should start");

    // Try to start second operation (should fail)
    let result = tracker
        .start_maintenance(
            nodes::NODE_1,
            operations::SNAPSHOT_CREATE,
            300,
            servers::SERVER_1,
        )
        .await;

    assert!(result.is_err(), "Second operation should be rejected");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("already in maintenance"),
        "Error should indicate node is busy"
    );

    // Verify first operation is still active
    assert!(tracker.is_in_maintenance(nodes::NODE_1).await);
}

#[tokio::test]
async fn test_mutual_exclusion_operation_tracker() {
    let tracker = SimpleOperationTracker::new();

    // Start first operation
    tracker
        .try_start_operation(nodes::NODE_1, operations::PRUNING, None)
        .await
        .expect("First operation should start");

    // Try to start second operation (should fail)
    let result = tracker
        .try_start_operation(nodes::NODE_1, operations::SNAPSHOT_CREATE, None)
        .await;

    assert!(result.is_err(), "Second operation should be rejected");
    assert!(
        result.unwrap_err().to_string().contains("busy"),
        "Error should indicate target is busy"
    );
}

#[tokio::test]
async fn test_different_operation_types_still_blocked() {
    let tracker = MaintenanceTracker::new();

    // Start pruning
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .unwrap();

    // Try different operation types - all should fail
    let snapshot = tracker
        .start_maintenance(
            nodes::NODE_1,
            operations::SNAPSHOT_CREATE,
            300,
            servers::SERVER_1,
        )
        .await;
    assert!(snapshot.is_err());

    let restore = tracker
        .start_maintenance(
            nodes::NODE_1,
            operations::SNAPSHOT_RESTORE,
            300,
            servers::SERVER_1,
        )
        .await;
    assert!(restore.is_err());

    let state_sync = tracker
        .start_maintenance(
            nodes::NODE_1,
            operations::STATE_SYNC,
            300,
            servers::SERVER_1,
        )
        .await;
    assert!(state_sync.is_err());

    let restart = tracker
        .start_maintenance(nodes::NODE_1, operations::RESTART, 300, servers::SERVER_1)
        .await;
    assert!(restart.is_err());
}

#[tokio::test]
async fn test_parallel_operations_on_different_nodes_allowed() {
    let tracker = MaintenanceTracker::new();

    // Start operations on different nodes - all should succeed
    let result1 = tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await;
    let result2 = tracker
        .start_maintenance(
            nodes::NODE_2,
            operations::SNAPSHOT_CREATE,
            300,
            servers::SERVER_1,
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

    assert!(result1.is_ok(), "Operation on node-1 should succeed");
    assert!(result2.is_ok(), "Operation on node-2 should succeed");
    assert!(result3.is_ok(), "Operation on node-3 should succeed");

    // All nodes should be in maintenance
    assert!(tracker.is_in_maintenance(nodes::NODE_1).await);
    assert!(tracker.is_in_maintenance(nodes::NODE_2).await);
    assert!(tracker.is_in_maintenance(nodes::NODE_3).await);
}

#[tokio::test]
async fn test_operation_allowed_after_completion() {
    let tracker = MaintenanceTracker::new();

    // Start and complete first operation
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .unwrap();

    tracker.end_maintenance(nodes::NODE_1).await.unwrap();

    // New operation should now be allowed
    let result = tracker
        .start_maintenance(
            nodes::NODE_1,
            operations::SNAPSHOT_CREATE,
            300,
            servers::SERVER_1,
        )
        .await;

    assert!(
        result.is_ok(),
        "New operation should be allowed after completion"
    );
}

#[tokio::test]
async fn test_same_operation_type_still_blocked() {
    let tracker = MaintenanceTracker::new();

    // Start pruning
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .unwrap();

    // Try to start another pruning operation (should fail)
    let result = tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await;

    assert!(
        result.is_err(),
        "Same operation type should also be blocked"
    );
}

#[tokio::test]
async fn test_operation_on_same_network_different_nodes_allowed() {
    let tracker = MaintenanceTracker::new();

    // Both nodes on osmosis-1 network
    tracker
        .start_maintenance(
            "osmosis-node-1",
            operations::PRUNING,
            300,
            servers::SERVER_1,
        )
        .await
        .unwrap();

    let result = tracker
        .start_maintenance(
            "osmosis-node-2",
            operations::PRUNING,
            300,
            servers::SERVER_1,
        )
        .await;

    assert!(
        result.is_ok(),
        "Different nodes on same network should be allowed"
    );
}

#[tokio::test]
async fn test_concurrent_operations_with_user_tracking() {
    let tracker = SimpleOperationTracker::new();

    // User 1 starts operation
    tracker
        .try_start_operation(
            nodes::NODE_1,
            operations::PRUNING,
            Some("user1".to_string()),
        )
        .await
        .unwrap();

    // User 2 tries to start operation on same node (should fail)
    let result = tracker
        .try_start_operation(
            nodes::NODE_1,
            operations::SNAPSHOT_CREATE,
            Some("user2".to_string()),
        )
        .await;

    assert!(result.is_err(), "Second user should be blocked");

    // Verify first user's operation is still active
    let status = tracker.get_operation_status().await;
    assert_eq!(status.total_active, 1);
    let active_op = status.busy_nodes.get(nodes::NODE_1).unwrap();
    assert_eq!(active_op.user_info, Some("user1".to_string()));
}

#[tokio::test]
async fn test_emergency_cleanup_allows_new_operations() {
    let tracker = MaintenanceTracker::new();

    // Start operation
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .unwrap();

    // Emergency cleanup (0 hours = clean all)
    tracker.cleanup_expired_maintenance(0).await;

    // New operation should now be allowed
    let result = tracker
        .start_maintenance(
            nodes::NODE_1,
            operations::SNAPSHOT_CREATE,
            300,
            servers::SERVER_1,
        )
        .await;

    assert!(result.is_ok(), "Operation should be allowed after cleanup");
}
