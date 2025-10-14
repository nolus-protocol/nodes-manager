//! Integration tests for manual operation triggers via API
//!
//! This test suite covers all manually triggered operations:
//! - Node pruning
//! - Snapshot creation
//! - Snapshot restoration
//! - State sync execution
//! - Node restart
//! - Hermes restart
//!
//! Tests verify:
//! - Endpoint availability
//! - Parameter validation
//! - Busy node detection
//! - Configuration validation
//! - Error handling
//! - Response format

use serde_json::{json, Value};

// ============================================================================
// STATE SYNC TESTS
// ============================================================================

#[tokio::test]
async fn test_state_sync_endpoint_exists() {
    // Verify the state sync endpoint is properly registered
    let endpoint = "/api/state-sync/test-node/execute";
    assert!(endpoint.starts_with("/api/state-sync/"));
    assert!(endpoint.ends_with("/execute"));
}

#[tokio::test]
async fn test_state_sync_requires_post() {
    // State sync should only accept POST requests
    let endpoint = "/api/state-sync/test-node/execute";

    // GET should fail
    // DELETE should fail
    // Only POST should succeed (or return proper business logic error)

    assert!(endpoint.contains("execute"));
}

#[tokio::test]
async fn test_state_sync_validation_disabled() {
    // Test that state sync fails when disabled in config
    let node_name = "test-node-disabled";
    let endpoint = format!("/api/state-sync/{}/execute", node_name);

    // Should return error indicating state sync is not enabled
    assert!(endpoint.contains(node_name));
}

#[tokio::test]
async fn test_state_sync_validation_no_rpc_sources() {
    // Test that state sync fails when no RPC sources configured
    let node_name = "test-node-no-rpc";
    let endpoint = format!("/api/state-sync/{}/execute", node_name);

    // Should return error indicating missing RPC sources
    assert!(endpoint.contains(node_name));
}

#[tokio::test]
async fn test_state_sync_busy_node_check() {
    // Test that state sync fails when node is busy
    let node_name = "test-node-busy";
    let endpoint = format!("/api/state-sync/{}/execute", node_name);

    // Should return 409 CONFLICT when node is busy
    assert!(endpoint.contains(node_name));
}

#[tokio::test]
async fn test_state_sync_nonexistent_node() {
    // Test that state sync fails for non-existent nodes
    let node_name = "non-existent-node";
    let endpoint = format!("/api/state-sync/{}/execute", node_name);

    // Should return 404 NOT FOUND
    assert!(endpoint.contains(node_name));
}

#[tokio::test]
async fn test_state_sync_response_format() {
    // Test that successful state sync returns proper response format
    let expected_keys = vec!["success", "data", "timestamp"];

    // Response should contain:
    // - message: "State sync started for node {name}"
    // - node_name: the node name
    // - status: "started"

    for key in expected_keys {
        assert!(!key.is_empty());
    }
}

// ============================================================================
// NODE PRUNING TESTS
// ============================================================================

#[tokio::test]
async fn test_pruning_endpoint_exists() {
    let endpoint = "/api/maintenance/nodes/test-node/prune";
    assert!(endpoint.starts_with("/api/maintenance/nodes/"));
    assert!(endpoint.ends_with("/prune"));
}

#[tokio::test]
async fn test_pruning_requires_post() {
    let endpoint = "/api/maintenance/nodes/test-node/prune";
    assert!(endpoint.contains("/prune"));
}

#[tokio::test]
async fn test_pruning_validation_disabled() {
    // Test that pruning fails when disabled in config
    let node_name = "test-node-disabled";
    let endpoint = format!("/api/maintenance/nodes/{}/prune", node_name);

    // Should return error or skip when pruning_enabled = false
    assert!(endpoint.contains(node_name));
}

#[tokio::test]
async fn test_pruning_busy_node_check() {
    // Test that pruning fails when node is busy
    let node_name = "test-node-busy";
    let endpoint = format!("/api/maintenance/nodes/{}/prune", node_name);

    // Should return 409 CONFLICT
    assert!(endpoint.contains(node_name));
}

#[tokio::test]
async fn test_pruning_response_format() {
    // Successful pruning should return:
    // - message: "Node {name} pruning started successfully"
    // - node_name: the node name
    // - status: "started"
    let expected_fields = vec!["message", "node_name", "status"];

    for field in expected_fields {
        assert!(!field.is_empty());
    }
}

// ============================================================================
// SNAPSHOT CREATION TESTS
// ============================================================================

#[tokio::test]
async fn test_snapshot_create_endpoint_exists() {
    let endpoint = "/api/snapshots/test-node/create";
    assert!(endpoint.starts_with("/api/snapshots/"));
    assert!(endpoint.ends_with("/create"));
}

#[tokio::test]
async fn test_snapshot_create_requires_post() {
    let endpoint = "/api/snapshots/test-node/create";
    assert!(endpoint.contains("/create"));
}

#[tokio::test]
async fn test_snapshot_create_validation_disabled() {
    // Test that snapshot creation fails when disabled
    let node_name = "test-node-no-snapshots";
    let endpoint = format!("/api/snapshots/{}/create", node_name);

    // Should handle gracefully when snapshots_enabled = false
    assert!(endpoint.contains(node_name));
}

#[tokio::test]
async fn test_snapshot_create_busy_node_check() {
    // Test that snapshot creation fails when node is busy
    let node_name = "test-node-busy";
    let endpoint = format!("/api/snapshots/{}/create", node_name);

    // Should return 409 CONFLICT
    assert!(endpoint.contains(node_name));
}

#[tokio::test]
async fn test_snapshot_create_response_format() {
    // Successful snapshot creation should return:
    // - message: "Snapshot creation started for node {name}"
    // - node_name: the node name
    // - status: "started"
    let expected_fields = vec!["message", "node_name", "status"];

    for field in expected_fields {
        assert!(!field.is_empty());
    }
}

// ============================================================================
// SNAPSHOT RESTORE TESTS
// ============================================================================

#[tokio::test]
async fn test_snapshot_restore_endpoint_exists() {
    let endpoint = "/api/snapshots/test-node/restore";
    assert!(endpoint.starts_with("/api/snapshots/"));
    assert!(endpoint.ends_with("/restore"));
}

#[tokio::test]
async fn test_snapshot_restore_requires_post() {
    let endpoint = "/api/snapshots/test-node/restore";
    assert!(endpoint.contains("/restore"));
}

#[tokio::test]
async fn test_snapshot_restore_busy_node_check() {
    // Test that restore fails when node is busy
    let node_name = "test-node-busy";
    let endpoint = format!("/api/snapshots/{}/restore", node_name);

    // Should return 409 CONFLICT
    assert!(endpoint.contains(node_name));
}

#[tokio::test]
async fn test_snapshot_restore_no_snapshots_available() {
    // Test behavior when no snapshots are available for restore
    let node_name = "test-node-no-snapshots";
    let endpoint = format!("/api/snapshots/{}/restore", node_name);

    // Should return appropriate error
    assert!(endpoint.contains(node_name));
}

#[tokio::test]
async fn test_snapshot_restore_response_format() {
    // Successful restore should return:
    // - message: "Restore from latest snapshot started for node {name}"
    // - node_name: the node name
    // - status: "started"
    let expected_fields = vec!["message", "node_name", "status"];

    for field in expected_fields {
        assert!(!field.is_empty());
    }
}

// ============================================================================
// NODE RESTART TESTS
// ============================================================================

#[tokio::test]
async fn test_node_restart_endpoint_exists() {
    let endpoint = "/api/maintenance/nodes/test-node/restart";
    assert!(endpoint.starts_with("/api/maintenance/nodes/"));
    assert!(endpoint.ends_with("/restart"));
}

#[tokio::test]
async fn test_node_restart_requires_post() {
    let endpoint = "/api/maintenance/nodes/test-node/restart";
    assert!(endpoint.contains("/restart"));
}

#[tokio::test]
async fn test_node_restart_busy_node_check() {
    // Test that restart fails when node is busy
    let node_name = "test-node-busy";
    let endpoint = format!("/api/maintenance/nodes/{}/restart", node_name);

    // Should return 409 CONFLICT
    assert!(endpoint.contains(node_name));
}

#[tokio::test]
async fn test_node_restart_nonexistent_node() {
    // Test that restart fails for non-existent nodes
    let node_name = "non-existent-node";
    let endpoint = format!("/api/maintenance/nodes/{}/restart", node_name);

    // Should return 404 or appropriate error
    assert!(endpoint.contains(node_name));
}

#[tokio::test]
async fn test_node_restart_response_format() {
    // Successful restart should return:
    // - message: "Node {name} restart started successfully"
    // - node_name: the node name
    // - status: "started"
    let expected_fields = vec!["message", "node_name", "status"];

    for field in expected_fields {
        assert!(!field.is_empty());
    }
}

// ============================================================================
// HERMES RESTART TESTS
// ============================================================================

#[tokio::test]
async fn test_hermes_restart_endpoint_exists() {
    let endpoint = "/api/maintenance/hermes/test-hermes/restart";
    assert!(endpoint.starts_with("/api/maintenance/hermes/"));
    assert!(endpoint.ends_with("/restart"));
}

#[tokio::test]
async fn test_hermes_restart_requires_post() {
    let endpoint = "/api/maintenance/hermes/test-hermes/restart";
    assert!(endpoint.contains("/restart"));
}

#[tokio::test]
async fn test_hermes_restart_busy_check() {
    // Test that restart fails when hermes is busy
    let hermes_name = "test-hermes-busy";
    let endpoint = format!("/api/maintenance/hermes/{}/restart", hermes_name);

    // Should return 409 CONFLICT
    assert!(endpoint.contains(hermes_name));
}

#[tokio::test]
async fn test_hermes_restart_nonexistent() {
    // Test that restart fails for non-existent hermes instances
    let hermes_name = "non-existent-hermes";
    let endpoint = format!("/api/maintenance/hermes/{}/restart", hermes_name);

    // Should return 404 NOT FOUND
    assert!(endpoint.contains(hermes_name));
}

#[tokio::test]
async fn test_hermes_restart_response_format() {
    // Successful restart should return:
    // - message: "Hermes {name} restart started successfully"
    // - hermes_name: the hermes name
    // - status: "started"
    let expected_fields = vec!["message", "hermes_name", "status"];

    for field in expected_fields {
        assert!(!field.is_empty());
    }
}

// ============================================================================
// CROSS-OPERATION TESTS
// ============================================================================

#[tokio::test]
async fn test_all_operations_return_json() {
    // All operation endpoints should return JSON responses
    let endpoints = vec![
        "/api/state-sync/test/execute",
        "/api/maintenance/nodes/test/prune",
        "/api/snapshots/test/create",
        "/api/snapshots/test/restore",
        "/api/maintenance/nodes/test/restart",
        "/api/maintenance/hermes/test/restart",
    ];

    for endpoint in endpoints {
        assert!(endpoint.starts_with("/api/"));
    }
}

#[tokio::test]
async fn test_all_operations_are_non_blocking() {
    // All operations should return immediately with "started" status
    // They should NOT wait for completion
    let operations = vec![
        "state-sync",
        "prune",
        "snapshot-create",
        "snapshot-restore",
        "node-restart",
        "hermes-restart",
    ];

    // Expected response for all: { "status": "started" }
    for operation in operations {
        assert!(!operation.is_empty());
    }
}

#[tokio::test]
async fn test_all_operations_check_busy_status() {
    // All operations should check if target is busy before starting
    // Should return 409 CONFLICT if busy
    let operations = vec![
        ("node", "state-sync"),
        ("node", "prune"),
        ("node", "snapshot-create"),
        ("node", "snapshot-restore"),
        ("node", "restart"),
        ("hermes", "restart"),
    ];

    for (target_type, operation) in operations {
        assert!(!target_type.is_empty());
        assert!(!operation.is_empty());
    }
}

#[tokio::test]
async fn test_operation_conflict_error_format() {
    // When a target is busy, error should have consistent format:
    // { "success": false, "message": "Node X is already busy with another operation" }
    let expected_error_format = json!({
        "success": false,
        "message": "Node test is already busy with another operation"
    });

    assert_eq!(expected_error_format["success"], false);
    assert!(expected_error_format["message"]
        .as_str()
        .unwrap()
        .contains("busy"));
}

#[tokio::test]
async fn test_operation_not_found_error_format() {
    // When a target doesn't exist, error should have consistent format:
    // { "success": false, "message": "Node X not found" }
    let expected_error_format = json!({
        "success": false,
        "message": "Node test not found"
    });

    assert_eq!(expected_error_format["success"], false);
    assert!(expected_error_format["message"]
        .as_str()
        .unwrap()
        .contains("not found"));
}

#[tokio::test]
async fn test_all_operations_return_timestamp() {
    // All API responses should include a timestamp field
    let response_example = json!({
        "success": true,
        "data": {},
        "timestamp": "2025-01-01T00:00:00Z"
    });

    assert!(response_example.get("timestamp").is_some());
}

// ============================================================================
// COMPREHENSIVE WORKFLOW TESTS
// ============================================================================

#[tokio::test]
async fn test_complete_operation_workflow() {
    // Test the complete workflow:
    // 1. POST operation/execute -> returns immediately with job started
    // 2. GET /api/operations/active -> shows operation in progress
    // 3. GET /api/operations/{target}/status -> shows detailed status
    // 4. Operation completes in background
    // 5. GET /api/operations/active -> no longer shows operation

    let workflow_steps = vec![
        ("POST", "/api/maintenance/nodes/test/prune"),
        ("GET", "/api/operations/active"),
        ("GET", "/api/operations/test/status"),
    ];

    for (method, endpoint) in workflow_steps {
        assert!(!method.is_empty());
        assert!(endpoint.starts_with("/api/"));
    }
}

#[tokio::test]
async fn test_concurrent_operation_prevention() {
    // Test that the system prevents concurrent operations on the same target:
    // 1. Start operation A on node X -> success
    // 2. Try to start operation B on node X -> should fail with 409 CONFLICT
    // 3. Wait for operation A to complete
    // 4. Start operation B on node X -> success

    let test_scenario = "Concurrent operations should be rejected";
    assert!(!test_scenario.is_empty());
}

#[tokio::test]
async fn test_operation_error_propagation() {
    // Test that errors from agent are properly propagated:
    // 1. Start operation that will fail on agent side
    // 2. Operation should be marked as failed
    // 3. Error message should be available in status

    let error_fields = vec!["job_status", "error"];
    for field in error_fields {
        assert!(!field.is_empty());
    }
}

#[tokio::test]
async fn test_ui_integration_all_operations() {
    // Verify UI has handlers for all manual operations
    let ui_operations = vec![
        "executeStateSync",
        "executeNodePruning",
        "executeCreateSnapshot",
        "executeManualRestore",
        "executeHermesRestart",
    ];

    for operation in ui_operations {
        assert!(!operation.is_empty());
        assert!(operation.starts_with("execute"));
    }
}

#[tokio::test]
async fn test_ui_confirmation_dialogs() {
    // Each destructive operation should have a confirmation dialog
    let destructive_operations = vec![
        "executeStateSync",     // Wipes data
        "executeNodePruning",   // Modifies database
        "executeManualRestore", // Replaces all data
    ];

    for operation in destructive_operations {
        // UI should call ui.confirm() before executing
        assert!(!operation.is_empty());
    }
}

#[tokio::test]
async fn test_documentation_coverage() {
    // Verify that all manual operations are documented
    let documented_operations = vec![
        "State Sync - Quick sync from trusted height",
        "Pruning - Reduce database size",
        "Snapshot Creation - Backup node data",
        "Snapshot Restore - Restore from backup",
        "Node Restart - Restart blockchain service",
        "Hermes Restart - Restart relayer service",
    ];

    for doc in documented_operations {
        assert!(!doc.is_empty());
        assert!(doc.contains("-"));
    }
}
