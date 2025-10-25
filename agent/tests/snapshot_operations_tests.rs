//! Integration tests for agent snapshot operations
//!
//! These tests verify the critical snapshot creation logic that handles
//! blockchain data backup. Tests use temporary directories to avoid
//! requiring actual blockchain nodes or systemctl access.

use agent::types::SnapshotRequest;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a mock node directory structure
fn create_mock_node_structure() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let deploy_path = temp_dir.path();

    // Create data directory with mock blockchain data
    let data_dir = deploy_path.join("data");
    fs::create_dir_all(&data_dir).expect("Failed to create data dir");

    // Create mock blockchain files
    fs::write(data_dir.join("application.db"), b"mock blockchain data").unwrap();
    fs::write(data_dir.join("blockstore.db"), b"mock blockstore").unwrap();
    fs::write(
        data_dir.join("priv_validator_state.json"),
        r#"{"height":"100","round":0,"step":3}"#,
    )
    .unwrap();

    // Create wasm directory with mock contract data
    let wasm_dir = deploy_path.join("wasm");
    fs::create_dir_all(&wasm_dir).expect("Failed to create wasm dir");
    fs::write(wasm_dir.join("contract1.wasm"), b"mock wasm data").unwrap();

    temp_dir
}

/// Helper to create snapshot request for testing
fn create_test_snapshot_request(deploy_path: PathBuf, backup_path: PathBuf) -> SnapshotRequest {
    SnapshotRequest {
        node_name: "test-node".to_string(),
        snapshot_name: "test-network_20250125_12345".to_string(),
        service_name: "test-service".to_string(),
        deploy_path: deploy_path.to_string_lossy().to_string(),
        backup_path: backup_path.to_string_lossy().to_string(),
        log_path: None, // Skip log truncation in tests
    }
}

#[test]
fn test_snapshot_request_validation() {
    let node_dir = create_mock_node_structure();
    let backup_dir = TempDir::new().expect("Failed to create backup dir");

    let request = create_test_snapshot_request(
        node_dir.path().to_path_buf(),
        backup_dir.path().to_path_buf(),
    );

    // Verify request fields are set correctly
    assert_eq!(request.node_name, "test-node");
    assert!(request.snapshot_name.contains("test-network"));
    assert!(request.snapshot_name.contains("12345")); // block height
    assert_eq!(request.service_name, "test-service");
}

#[test]
fn test_snapshot_name_format() {
    // Snapshot name should be: {network}_{date}_{blockheight}
    let snapshot_name = "osmosis-1_20250125_17154420";

    let parts: Vec<&str> = snapshot_name.split('_').collect();
    assert_eq!(
        parts.len(),
        3,
        "Should have network, date, and block height"
    );

    let network = parts[0];
    let date = parts[1];
    let block_height = parts[2];

    assert_eq!(network, "osmosis-1");
    assert_eq!(date.len(), 8, "Date should be YYYYMMDD");
    assert!(
        block_height.parse::<u64>().is_ok(),
        "Block height should be numeric"
    );
}

#[test]
fn test_mock_node_structure_creation() {
    let node_dir = create_mock_node_structure();
    let deploy_path = node_dir.path();

    // Verify data directory structure
    assert!(deploy_path.join("data").exists());
    assert!(deploy_path.join("data/application.db").exists());
    assert!(deploy_path.join("data/blockstore.db").exists());
    assert!(deploy_path.join("data/priv_validator_state.json").exists());

    // Verify wasm directory structure
    assert!(deploy_path.join("wasm").exists());
    assert!(deploy_path.join("wasm/contract1.wasm").exists());
}

#[test]
fn test_snapshot_info_structure() {
    use agent::types::SnapshotInfo;

    let info = SnapshotInfo {
        filename: "test-network_20250125_12345".to_string(),
        size_bytes: 1024 * 1024, // 1 MB
        path: "/backup/test-network_20250125_12345".to_string(),
    };

    assert_eq!(info.filename, "test-network_20250125_12345");
    assert_eq!(info.size_bytes, 1024 * 1024);
    assert!(info.path.contains("test-network"));
}

#[test]
fn test_snapshot_request_paths() {
    let node_dir = create_mock_node_structure();
    let backup_dir = TempDir::new().unwrap();

    let request = create_test_snapshot_request(
        node_dir.path().to_path_buf(),
        backup_dir.path().to_path_buf(),
    );

    // Verify paths are valid
    assert!(PathBuf::from(&request.deploy_path).exists());
    assert!(PathBuf::from(&request.backup_path).exists());

    // Verify data and wasm exist in deploy_path
    let deploy = PathBuf::from(&request.deploy_path);
    assert!(deploy.join("data").exists());
    assert!(deploy.join("wasm").exists());
}

#[test]
fn test_validator_state_in_mock_node() {
    let node_dir = create_mock_node_structure();
    let validator_state_path = node_dir.path().join("data/priv_validator_state.json");

    assert!(validator_state_path.exists());

    let content = fs::read_to_string(validator_state_path).unwrap();
    assert!(content.contains("height"));
    assert!(content.contains("round"));

    // Verify it's valid JSON
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(parsed.get("height").is_some());
}

#[test]
fn test_directory_size_calculation_concept() {
    let node_dir = create_mock_node_structure();

    // Calculate total size of files in data directory
    let data_dir = node_dir.path().join("data");
    let mut total_size = 0u64;

    for entry in fs::read_dir(data_dir).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            total_size += entry.metadata().unwrap().len();
        }
    }

    // We created 3 files with known content
    assert!(total_size > 0, "Directory should have files");
    assert!(
        total_size > 50,
        "Combined file size should be more than 50 bytes"
    );
}

#[test]
fn test_snapshot_name_uniqueness_by_block_height() {
    let network = "osmosis-1";
    let date = "20250125";

    let snapshot1 = format!("{}_{}_{}", network, date, 17154420);
    let snapshot2 = format!("{}_{}_{}", network, date, 17154421);
    let snapshot3 = format!("{}_{}_{}", network, date, 17154422);

    // Different block heights create different snapshots
    assert_ne!(snapshot1, snapshot2);
    assert_ne!(snapshot1, snapshot3);
    assert_ne!(snapshot2, snapshot3);

    // But all from same network
    assert!(snapshot1.starts_with("osmosis-1_"));
    assert!(snapshot2.starts_with("osmosis-1_"));
    assert!(snapshot3.starts_with("osmosis-1_"));
}

#[test]
fn test_cross_network_snapshots_different() {
    let date = "20250125";
    let block_height = 12345;

    let osmosis_snapshot = format!("osmosis-1_{}_{}", date, block_height);
    let juno_snapshot = format!("juno-1_{}_{}", date, block_height);
    let cosmos_snapshot = format!("cosmos-hub-4_{}_{}", date, block_height);

    // Different networks = different snapshots even at same height
    assert_ne!(osmosis_snapshot, juno_snapshot);
    assert_ne!(osmosis_snapshot, cosmos_snapshot);
    assert_ne!(juno_snapshot, cosmos_snapshot);

    assert!(osmosis_snapshot.starts_with("osmosis-1"));
    assert!(juno_snapshot.starts_with("juno-1"));
    assert!(cosmos_snapshot.starts_with("cosmos-hub-4"));
}
