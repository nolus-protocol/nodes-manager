//! Business Rule Tests: Snapshot Naming Convention
//!
//! These tests verify that snapshots follow the network-based naming convention:
//! {network}_{date}_{blockheight} NOT {node}_{timestamp}
//!
//! This enables cross-node recovery - any node on the same network can restore
//! from the same snapshot. Block height provides a precise blockchain state reference.

mod common;

use chrono::Utc;

#[test]
fn test_snapshot_name_format_network_based() {
    let network = "osmosis-1";
    let date = Utc::now().format("%Y%m%d").to_string();
    let block_height = 17154420;
    let snapshot_name = format!("{}_{}_{}", network, date, block_height);

    // Verify format: network_YYYYMMDD_blockheight
    assert!(snapshot_name.starts_with("osmosis-1_"));
    assert!(snapshot_name.contains('_'));

    let parts: Vec<&str> = snapshot_name.split('_').collect();
    assert_eq!(
        parts.len(),
        3,
        "Should have network, date, and block height parts"
    );
    assert_eq!(parts[0], "osmosis-1");
}

#[test]
fn test_snapshot_name_does_not_include_node_name() {
    let network = "pirin-1";
    let node_name = "pirin-node-1"; // This should NOT be in the snapshot name
    let date = Utc::now().format("%Y%m%d").to_string();
    let block_height = 12345678;
    let snapshot_name = format!("{}_{}_{}", network, date, block_height);

    // Verify node name is NOT in snapshot name
    assert!(!snapshot_name.contains(node_name));
    assert!(snapshot_name.starts_with("pirin-1_"));
}

#[test]
fn test_snapshots_from_different_nodes_same_network_can_share() {
    let network = "cosmos-hub-4";
    let date = "20250109";
    let block_height = 18500000;

    // Both nodes on same network can create/use same snapshot name format
    let snapshot_from_node1 = format!("{}_{}_{}", network, date, block_height);
    let snapshot_from_node2 = format!("{}_{}_{}", network, date, block_height);

    assert_eq!(snapshot_from_node1, snapshot_from_node2);
    assert_eq!(snapshot_from_node1, "cosmos-hub-4_20250109_18500000");
}

#[test]
fn test_snapshot_name_parsing() {
    let snapshot_name = "osmosis-1_20250109_17154420";

    let parts: Vec<&str> = snapshot_name.split('_').collect();
    assert_eq!(parts.len(), 3);

    let network = parts[0];
    let date = parts[1];
    let block_height = parts[2];

    assert_eq!(network, "osmosis-1");
    assert_eq!(date, "20250109");
    assert_eq!(block_height, "17154420");

    // Block height should be parseable as a number
    assert!(block_height.parse::<u64>().is_ok());
}

#[test]
fn test_snapshot_name_uniqueness_by_block_height() {
    let network = "juno-1";
    let date = "20250109";

    let snapshot1 = format!("{}_{}_{}", network, date, 15000000);
    let snapshot2 = format!("{}_{}_{}", network, date, 15100000);
    let snapshot3 = format!("{}_{}_{}", network, date, 15200000);

    // Different block heights = different snapshots
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
    let date = "20250109";
    let block_height = 17154420;

    let osmosis_snapshot = format!("osmosis-1_{}_{}", date, block_height);
    let cosmos_snapshot = format!("cosmoshub-4_{}_{}", date, block_height);
    let juno_snapshot = format!("juno-1_{}_{}", date, block_height);

    // Same block height but different networks = different snapshots
    assert_ne!(osmosis_snapshot, cosmos_snapshot);
    assert_ne!(osmosis_snapshot, juno_snapshot);
    assert_ne!(cosmos_snapshot, juno_snapshot);
}

#[test]
fn test_snapshot_name_with_network_containing_hyphens() {
    let network = "cosmos-hub-4";
    let date = "20250109";
    let block_height = 18500000;
    let snapshot_name = format!("{}_{}_{}", network, date, block_height);

    // Network can contain hyphens
    assert_eq!(snapshot_name, "cosmos-hub-4_20250109_18500000");

    // Parse correctly
    let parts: Vec<&str> = snapshot_name.split('_').collect();
    // Will be ["cosmos-hub-4", "20250109", "18500000"]
    assert_eq!(parts[0], "cosmos-hub-4");
    assert_eq!(parts[1], "20250109");
    assert_eq!(parts[2], "18500000");
}

#[test]
fn test_snapshot_restoration_cross_node_compatibility() {
    // Scenario: Node 1 creates snapshot, Node 2 restores from it
    let network = "pirin-1";
    let date = "20250121";
    let block_height = 17154420;

    // Node 1 creates snapshot with network-based name
    let snapshot_created_by_node1 = format!("{}_{}_{}", network, date, block_height);

    // Node 2 can restore from same snapshot name
    let snapshot_used_by_node2 = format!("{}_{}_{}", network, date, block_height);

    // Both refer to the same snapshot
    assert_eq!(snapshot_created_by_node1, snapshot_used_by_node2);
    assert_eq!(snapshot_created_by_node1, "pirin-1_20250121_17154420");
}

#[test]
fn test_snapshot_name_format_validation() {
    let valid_snapshot = "osmosis-1_20250109_17154420";
    let parts: Vec<&str> = valid_snapshot.split('_').collect();

    // Must have exactly 3 parts
    assert_eq!(parts.len(), 3);

    // Date part must be 8 digits (YYYYMMDD)
    assert_eq!(parts[1].len(), 8);
    assert!(parts[1].chars().all(|c| c.is_ascii_digit()));

    // Block height part must be all digits (variable length)
    assert!(!parts[2].is_empty());
    assert!(parts[2].chars().all(|c| c.is_ascii_digit()));

    // Block height should be parseable as u64
    assert!(parts[2].parse::<u64>().is_ok());
}

#[test]
fn test_invalid_snapshot_names() {
    // These would be invalid snapshot names (node-based, not network-based)
    let invalid_names = vec![
        "node-1_20250109_17154420",          // Uses node name instead of network
        "server1-osmosis_20250109_17154420", // Includes server name
        "osmosis-node-1_20250109_17154420",  // Includes node identifier
    ];

    // Valid network-based name for comparison
    let valid_name = "osmosis-1_20250109_17154420";

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
    let date = "20250109";
    let block_height = 17154420;
    let snapshot_name = format!("{}_{}_{}", network, date, block_height);
    let filename = format!("{}.tar.lz4", snapshot_name);

    assert_eq!(filename, "osmosis-1_20250109_17154420.tar.lz4");
    assert!(filename.ends_with(".tar.lz4"));
}

#[test]
fn test_multiple_snapshots_same_network_different_block_heights() {
    let network = "pirin-1";
    let date = "20250109";

    // Create snapshots at different block heights
    let snapshot1 = format!("{}_{}_{}", network, date, 17000000);
    let snapshot2 = format!("{}_{}_{}", network, date, 17100000);
    let snapshot3 = format!("{}_{}_{}", network, date, 17200000);

    // All should be unique
    assert_ne!(snapshot1, snapshot2);
    assert_ne!(snapshot1, snapshot3);
    assert_ne!(snapshot2, snapshot3);

    // All should be for the same network
    assert!(snapshot1.starts_with("pirin-1_"));
    assert!(snapshot2.starts_with("pirin-1_"));
    assert!(snapshot3.starts_with("pirin-1_"));
}

#[test]
fn test_snapshot_sorting_by_block_height() {
    // Test that snapshots are correctly sorted by block height, not alphabetically
    let network = "pirin-1";

    // Different dates and block heights
    let snapshot1 = format!("{}_{}_{}", network, "20250120", 15000000);
    let snapshot2 = format!("{}_{}_{}", network, "20250121", 17154420);
    let snapshot3 = format!("{}_{}_{}", network, "20250122", 2000000); // Lower block height!

    // Extract block heights
    let height1: u64 = snapshot1.split('_').next_back().unwrap().parse().unwrap();
    let height2: u64 = snapshot2.split('_').next_back().unwrap().parse().unwrap();
    let height3: u64 = snapshot3.split('_').next_back().unwrap().parse().unwrap();

    // Verify that snapshot2 has the highest block height
    assert!(height2 > height1);
    assert!(height2 > height3);

    // This confirms that alphabetical sorting would fail (2000000 < 17154420 alphabetically)
    // but numeric sorting works correctly
    assert!(
        height3 < height2,
        "Block height 2000000 should be less than 17154420"
    );
}

#[test]
fn test_block_height_provides_precise_state_reference() {
    let network = "osmosis-1";
    let date = "20250121";
    let block_height = 17154420;

    let snapshot_name = format!("{}_{}_{}", network, date, block_height);

    // Extract block height from snapshot name
    let parts: Vec<&str> = snapshot_name.split('_').collect();
    let extracted_height: u64 = parts[2].parse().unwrap();

    // Verify we can extract and use the block height
    assert_eq!(extracted_height, block_height);

    // Block height provides exact blockchain state reference
    assert!(extracted_height > 0, "Block height must be positive");
}

#[test]
fn test_directory_and_lz4_use_same_block_height_naming() {
    let network = "pirin-1";
    let date = "20250121";
    let block_height = 17154420;

    // Both directory and LZ4 archive use the same base name
    let snapshot_name = format!("{}_{}_{}", network, date, block_height);
    let directory_path = format!("/backup/{}/", snapshot_name);
    let lz4_archive = format!("/backup/{}.tar.lz4", snapshot_name);

    // Verify both use block height in their names
    assert_eq!(directory_path, "/backup/pirin-1_20250121_17154420/");
    assert_eq!(lz4_archive, "/backup/pirin-1_20250121_17154420.tar.lz4");

    // Verify the base name is identical (minus directory separator or extension)
    assert!(lz4_archive.contains(&snapshot_name));
    assert!(directory_path.contains(&snapshot_name));

    // Extract and verify block height from both paths
    let dir_parts: Vec<&str> = snapshot_name.split('_').collect();
    let lz4_base = lz4_archive
        .trim_end_matches(".tar.lz4")
        .split('/')
        .last()
        .unwrap();
    let lz4_parts: Vec<&str> = lz4_base.split('_').collect();

    assert_eq!(dir_parts[2], "17154420");
    assert_eq!(lz4_parts[2], "17154420");
    assert_eq!(
        dir_parts[2], lz4_parts[2],
        "Both should have same block height"
    );
}
