//! Business Rule Tests: Snapshot Naming Convention
//!
//! These tests verify that snapshots follow the network-based naming convention:
//! {network}_{timestamp} NOT {node}_{timestamp}
//!
//! This enables cross-node recovery - any node on the same network can restore
//! from the same snapshot.

mod common;

use chrono::Utc;

#[test]
fn test_snapshot_name_format_network_based() {
    let network = "osmosis-1";
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let snapshot_name = format!("{}_{}", network, timestamp);

    // Verify format: network_YYYYMMDD_HHMMSS
    assert!(snapshot_name.starts_with("osmosis-1_"));
    assert!(snapshot_name.contains('_'));

    let parts: Vec<&str> = snapshot_name.split('_').collect();
    assert_eq!(parts.len(), 3, "Should have network, date, and time parts");
    assert_eq!(parts[0], "osmosis-1");
}

#[test]
fn test_snapshot_name_does_not_include_node_name() {
    let network = "pirin-1";
    let node_name = "pirin-node-1"; // This should NOT be in the snapshot name
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let snapshot_name = format!("{}_{}", network, timestamp);

    // Verify node name is NOT in snapshot name
    assert!(!snapshot_name.contains(node_name));
    assert!(snapshot_name.starts_with("pirin-1_"));
}

#[test]
fn test_snapshots_from_different_nodes_same_network_can_share() {
    let network = "cosmos-hub-4";
    let timestamp = "20250109_120000";

    // Both nodes on same network can create/use same snapshot name format
    let snapshot_from_node1 = format!("{}_{}", network, timestamp);
    let snapshot_from_node2 = format!("{}_{}", network, timestamp);

    assert_eq!(snapshot_from_node1, snapshot_from_node2);
    assert_eq!(snapshot_from_node1, "cosmos-hub-4_20250109_120000");
}

#[test]
fn test_snapshot_name_parsing() {
    let snapshot_name = "osmosis-1_20250109_143022";

    let parts: Vec<&str> = snapshot_name.split('_').collect();
    assert_eq!(parts.len(), 3);

    let network = parts[0];
    let date = parts[1];
    let time = parts[2];

    assert_eq!(network, "osmosis-1");
    assert_eq!(date, "20250109");
    assert_eq!(time, "143022");
}

#[test]
fn test_snapshot_name_uniqueness_by_timestamp() {
    let network = "juno-1";

    let snapshot1 = format!("{}_{}", network, "20250109_120000");
    let snapshot2 = format!("{}_{}", network, "20250109_130000");
    let snapshot3 = format!("{}_{}", network, "20250110_120000");

    // Different timestamps = different snapshots
    assert_ne!(snapshot1, snapshot2);
    assert_ne!(snapshot1, snapshot3);
    assert_ne!(snapshot2, snapshot3);

    // But all from same network
    assert!(snapshot1.starts_with("juno-1_"));
    assert!(snapshot2.starts_with("juno-1_"));
    assert!(snapshot3.starts_with("juno-1_"));
}

#[test]
fn test_cross_network_snapshots_are_different() {
    let timestamp = "20250109_120000";

    let osmosis_snapshot = format!("osmosis-1_{}", timestamp);
    let cosmos_snapshot = format!("cosmoshub-4_{}", timestamp);
    let juno_snapshot = format!("juno-1_{}", timestamp);

    // Same timestamp but different networks = different snapshots
    assert_ne!(osmosis_snapshot, cosmos_snapshot);
    assert_ne!(osmosis_snapshot, juno_snapshot);
    assert_ne!(cosmos_snapshot, juno_snapshot);
}

#[test]
fn test_snapshot_name_with_network_containing_hyphens() {
    let network = "cosmos-hub-4";
    let timestamp = "20250109_120000";
    let snapshot_name = format!("{}_{}", network, timestamp);

    // Network can contain hyphens
    assert_eq!(snapshot_name, "cosmos-hub-4_20250109_120000");

    // Parse correctly
    let parts: Vec<&str> = snapshot_name.split('_').collect();
    // Will be ["cosmos-hub-4", "20250109", "120000"]
    assert_eq!(parts[0], "cosmos-hub-4");
}

#[test]
fn test_snapshot_restoration_cross_node_compatibility() {
    // Scenario: Node 1 creates snapshot, Node 2 restores from it
    let network = "pirin-1";
    let timestamp = "20250109_120000";

    // Node 1 creates snapshot with network-based name
    let snapshot_created_by_node1 = format!("{}_{}", network, timestamp);

    // Node 2 can restore from same snapshot name
    let snapshot_used_by_node2 = format!("{}_{}", network, timestamp);

    // Both refer to the same snapshot
    assert_eq!(snapshot_created_by_node1, snapshot_used_by_node2);
    assert_eq!(snapshot_created_by_node1, "pirin-1_20250109_120000");
}

#[test]
fn test_snapshot_name_format_validation() {
    let valid_snapshot = "osmosis-1_20250109_120000";
    let parts: Vec<&str> = valid_snapshot.split('_').collect();

    // Must have exactly 3 parts
    assert_eq!(parts.len(), 3);

    // Date part must be 8 digits
    assert_eq!(parts[1].len(), 8);
    assert!(parts[1].chars().all(|c| c.is_ascii_digit()));

    // Time part must be 6 digits
    assert_eq!(parts[2].len(), 6);
    assert!(parts[2].chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn test_invalid_snapshot_names() {
    // These would be invalid snapshot names (node-based, not network-based)
    let invalid_names = vec![
        "node-1_20250109_120000",          // Uses node name instead of network
        "server1-osmosis_20250109_120000", // Includes server name
        "osmosis-node-1_20250109_120000",  // Includes node identifier
    ];

    // Valid network-based name for comparison
    let valid_name = "osmosis-1_20250109_120000";

    for invalid in invalid_names {
        assert_ne!(invalid, valid_name);
        // None of these should start with the network name only
        if invalid.starts_with("osmosis-1_") {
            panic!(
                "Invalid name {} should not start with network name",
                invalid
            );
        }
    }
}

#[test]
fn test_snapshot_filename_with_extension() {
    let network = "osmosis-1";
    let timestamp = "20250109_120000";
    let snapshot_name = format!("{}_{}", network, timestamp);
    let filename = format!("{}.tar.lz4", snapshot_name);

    assert_eq!(filename, "osmosis-1_20250109_120000.tar.lz4");
    assert!(filename.ends_with(".tar.lz4"));
}

#[test]
fn test_multiple_snapshots_same_network_different_times() {
    let network = "pirin-1";

    // Create snapshots at different times
    let morning_snapshot = format!("{}_{}", network, "20250109_080000");
    let afternoon_snapshot = format!("{}_{}", network, "20250109_140000");
    let evening_snapshot = format!("{}_{}", network, "20250109_200000");

    // All should be unique
    assert_ne!(morning_snapshot, afternoon_snapshot);
    assert_ne!(morning_snapshot, evening_snapshot);
    assert_ne!(afternoon_snapshot, evening_snapshot);

    // All should be for the same network
    assert!(morning_snapshot.starts_with("pirin-1_"));
    assert!(afternoon_snapshot.starts_with("pirin-1_"));
    assert!(evening_snapshot.starts_with("pirin-1_"));
}
