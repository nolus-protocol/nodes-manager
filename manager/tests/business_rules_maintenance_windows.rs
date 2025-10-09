//! Business Rule Tests: Maintenance Window Respect
//!
//! These tests verify that:
//! - Health checks don't trigger alerts during maintenance windows
//! - Operations respect existing maintenance windows
//! - Maintenance windows have automatic cleanup after max duration

mod common;

use common::fixtures::*;
use manager::maintenance_tracker::MaintenanceTracker;

#[tokio::test]
async fn test_node_in_maintenance_should_skip_alerts() {
    let tracker = MaintenanceTracker::new();

    // Start maintenance
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .unwrap();

    // Node is in maintenance
    let in_maintenance = tracker.is_in_maintenance(nodes::NODE_1).await;
    assert!(in_maintenance, "Node should be in maintenance");

    // Health check logic should check this before sending alerts
    // This test verifies the check is available
    if in_maintenance {
        // Skip alert - this is the correct behavior
        println!("Skipping alert because node is in maintenance");
    } else {
        panic!("Should have detected maintenance window");
    }
}

#[tokio::test]
async fn test_maintenance_window_blocks_concurrent_operations() {
    let tracker = MaintenanceTracker::new();

    // Start maintenance
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .unwrap();

    // Try to start another operation during maintenance
    let result = tracker
        .start_maintenance(
            nodes::NODE_1,
            operations::SNAPSHOT_CREATE,
            300,
            servers::SERVER_1,
        )
        .await;

    assert!(
        result.is_err(),
        "Should not allow operation during maintenance"
    );
}

#[tokio::test]
async fn test_maintenance_window_automatic_cleanup() {
    let tracker = MaintenanceTracker::new();

    // Start maintenance
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .unwrap();

    // Cleanup with 0 hours (aggressive cleanup)
    let cleaned = tracker.cleanup_expired_maintenance(0).await;
    assert_eq!(cleaned, 1, "Should clean up the maintenance window");

    // Node should no longer be in maintenance
    assert!(!tracker.is_in_maintenance(nodes::NODE_1).await);
}

#[tokio::test]
async fn test_maintenance_window_cleanup_respects_duration() {
    let tracker = MaintenanceTracker::new();

    // Start maintenance
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .unwrap();

    // Cleanup with high max duration (should not clean up recent windows)
    let cleaned = tracker.cleanup_expired_maintenance(1000).await;
    assert_eq!(cleaned, 0, "Should not clean up recent maintenance");

    // Node should still be in maintenance
    assert!(tracker.is_in_maintenance(nodes::NODE_1).await);
}

#[tokio::test]
async fn test_multiple_nodes_in_maintenance() {
    let tracker = MaintenanceTracker::new();

    // Start maintenance on multiple nodes
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .unwrap();

    tracker
        .start_maintenance(
            nodes::NODE_2,
            operations::SNAPSHOT_CREATE,
            300,
            servers::SERVER_1,
        )
        .await
        .unwrap();

    tracker
        .start_maintenance(
            nodes::NODE_3,
            operations::STATE_SYNC,
            300,
            servers::SERVER_1,
        )
        .await
        .unwrap();

    // All should be in maintenance
    assert!(tracker.is_in_maintenance(nodes::NODE_1).await);
    assert!(tracker.is_in_maintenance(nodes::NODE_2).await);
    assert!(tracker.is_in_maintenance(nodes::NODE_3).await);

    // End maintenance on one node
    tracker.end_maintenance(nodes::NODE_2).await.unwrap();

    // Others should still be in maintenance
    assert!(tracker.is_in_maintenance(nodes::NODE_1).await);
    assert!(!tracker.is_in_maintenance(nodes::NODE_2).await);
    assert!(tracker.is_in_maintenance(nodes::NODE_3).await);
}

#[tokio::test]
async fn test_maintenance_end_allows_new_operations() {
    let tracker = MaintenanceTracker::new();

    // Start and end maintenance
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .unwrap();

    tracker.end_maintenance(nodes::NODE_1).await.unwrap();

    // Should now allow new operations
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
        "New operation should be allowed after maintenance ends"
    );
}

#[tokio::test]
async fn test_ending_non_existent_maintenance_is_safe() {
    let tracker = MaintenanceTracker::new();

    // Try to end maintenance that was never started
    let result = tracker.end_maintenance("non-existent-node").await;

    // Should not error, just log warning
    assert!(
        result.is_ok(),
        "Ending non-existent maintenance should be safe"
    );
}

#[tokio::test]
async fn test_maintenance_prevents_scheduled_operations() {
    let tracker = MaintenanceTracker::new();

    // Simulate scheduled operation checking maintenance
    let node = nodes::NODE_1;

    // No maintenance - scheduled op can proceed
    if !tracker.is_in_maintenance(node).await {
        println!("Scheduled operation can proceed");
    }

    // Start manual maintenance
    tracker
        .start_maintenance(node, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .unwrap();

    // Scheduled operation should now be skipped
    if tracker.is_in_maintenance(node).await {
        println!("Skipping scheduled operation - node in maintenance");
    } else {
        panic!("Should detect maintenance window");
    }
}

#[tokio::test]
async fn test_node_name_is_globally_unique() {
    let tracker = MaintenanceTracker::new();

    // Start maintenance on node-1 from server-1
    tracker
        .start_maintenance("node-1", operations::PRUNING, 300, "server-1")
        .await
        .unwrap();

    // Try to start maintenance on same node name from different server
    // This should fail because node names are globally unique
    let result = tracker
        .start_maintenance("node-1", operations::SNAPSHOT_CREATE, 300, "server-2")
        .await;

    assert!(
        result.is_err(),
        "Same node name should be blocked regardless of server"
    );
}

#[tokio::test]
async fn test_estimated_duration_is_tracked() {
    let tracker = MaintenanceTracker::new();

    // Start maintenance with specific duration
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 120, servers::SERVER_1)
        .await
        .unwrap();

    // The duration is tracked (verification would require accessing internal state)
    // This test verifies the API accepts the duration parameter
    assert!(tracker.is_in_maintenance(nodes::NODE_1).await);
}

#[tokio::test]
async fn test_cleanup_removes_multiple_expired_windows() {
    let tracker = MaintenanceTracker::new();

    // Start maintenance on multiple nodes
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .unwrap();

    tracker
        .start_maintenance(
            nodes::NODE_2,
            operations::SNAPSHOT_CREATE,
            300,
            servers::SERVER_1,
        )
        .await
        .unwrap();

    tracker
        .start_maintenance(
            nodes::NODE_3,
            operations::STATE_SYNC,
            300,
            servers::SERVER_1,
        )
        .await
        .unwrap();

    // Cleanup all
    let cleaned = tracker.cleanup_expired_maintenance(0).await;
    assert_eq!(cleaned, 3, "Should clean up all 3 maintenance windows");

    // None should be in maintenance
    assert!(!tracker.is_in_maintenance(nodes::NODE_1).await);
    assert!(!tracker.is_in_maintenance(nodes::NODE_2).await);
    assert!(!tracker.is_in_maintenance(nodes::NODE_3).await);
}

#[tokio::test]
async fn test_maintenance_window_does_not_affect_other_nodes() {
    let tracker = MaintenanceTracker::new();

    // Start maintenance on NODE_1
    tracker
        .start_maintenance(nodes::NODE_1, operations::PRUNING, 300, servers::SERVER_1)
        .await
        .unwrap();

    // NODE_2 and NODE_3 should not be affected
    assert!(!tracker.is_in_maintenance(nodes::NODE_2).await);
    assert!(!tracker.is_in_maintenance(nodes::NODE_3).await);

    // Should be able to start operations on other nodes
    let result2 = tracker
        .start_maintenance(
            nodes::NODE_2,
            operations::SNAPSHOT_CREATE,
            300,
            servers::SERVER_1,
        )
        .await;

    assert!(result2.is_ok(), "Other nodes should not be affected");
}

#[tokio::test]
async fn test_long_running_operation_duration() {
    let tracker = MaintenanceTracker::new();

    // Start maintenance with very long duration (24 hours = 1440 minutes)
    tracker
        .start_maintenance(nodes::NODE_1, "snapshot_create", 1440, servers::SERVER_1)
        .await
        .unwrap();

    assert!(tracker.is_in_maintenance(nodes::NODE_1).await);

    // Cleanup with threshold less than 24 hours should not remove it
    let cleaned = tracker.cleanup_expired_maintenance(12).await;
    assert_eq!(cleaned, 0, "Should not clean up windows under 12 hours old");

    assert!(tracker.is_in_maintenance(nodes::NODE_1).await);
}
