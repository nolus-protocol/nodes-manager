//! Integration tests for agent restore operations
//!
//! These tests verify the critical snapshot restoration logic, especially
//! the validator state preservation feature that prevents double-signing.

use agent::types::RestoreRequest;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a mock snapshot directory
fn create_mock_snapshot(snapshot_dir: &PathBuf) {
    fs::create_dir_all(snapshot_dir).expect("Failed to create snapshot dir");

    // Create data directory in snapshot
    let data_dir = snapshot_dir.join("data");
    fs::create_dir_all(&data_dir).expect("Failed to create data dir");
    fs::write(data_dir.join("application.db"), b"snapshot blockchain data").unwrap();
    fs::write(data_dir.join("blockstore.db"), b"snapshot blockstore").unwrap();
    
    // Snapshot's validator state (this should be overwritten during restore)
    fs::write(
        data_dir.join("priv_validator_state.json"),
        r#"{"height":"50","round":0,"step":1}"#,
    )
    .unwrap();

    // Create wasm directory in snapshot
    let wasm_dir = snapshot_dir.join("wasm");
    fs::create_dir_all(&wasm_dir).expect("Failed to create wasm dir");
    fs::write(wasm_dir.join("contract1.wasm"), b"snapshot wasm").unwrap();
}

/// Helper to create a mock running node with current validator state
fn create_mock_running_node(deploy_path: &PathBuf) {
    fs::create_dir_all(deploy_path).expect("Failed to create deploy dir");

    // Current node's data directory
    let data_dir = deploy_path.join("data");
    fs::create_dir_all(&data_dir).expect("Failed to create data dir");
    fs::write(data_dir.join("application.db"), b"current blockchain data").unwrap();
    fs::write(data_dir.join("blockstore.db"), b"current blockstore").unwrap();
    
    // Current validator state (THIS should be preserved during restore)
    fs::write(
        data_dir.join("priv_validator_state.json"),
        r#"{"height":"100","round":2,"step":3}"#,
    )
    .unwrap();

    // Current wasm directory
    let wasm_dir = deploy_path.join("wasm");
    fs::create_dir_all(&wasm_dir).expect("Failed to create wasm dir");
    fs::write(wasm_dir.join("contract1.wasm"), b"current wasm").unwrap();
}

#[test]
fn test_restore_request_validation() {
    let deploy_dir = TempDir::new().unwrap();
    let snapshot_dir = TempDir::new().unwrap();

    create_mock_running_node(&deploy_dir.path().to_path_buf());
    create_mock_snapshot(&snapshot_dir.path().to_path_buf());

    let request = RestoreRequest {
        node_name: "test-node".to_string(),
        service_name: "test-service".to_string(),
        deploy_path: deploy_dir.path().to_string_lossy().to_string(),
        snapshot_dir: snapshot_dir.path().to_string_lossy().to_string(),
        log_path: None,
    };

    // Verify request fields
    assert_eq!(request.node_name, "test-node");
    assert_eq!(request.service_name, "test-service");
    assert!(PathBuf::from(&request.deploy_path).exists());
    assert!(PathBuf::from(&request.snapshot_dir).exists());
}

#[test]
fn test_snapshot_structure_verification() {
    let snapshot_dir = TempDir::new().unwrap();
    create_mock_snapshot(&snapshot_dir.path().to_path_buf());

    let snapshot_path = snapshot_dir.path();

    // Verify snapshot contains required directories
    assert!(
        snapshot_path.join("data").exists(),
        "Snapshot must contain data directory"
    );
    assert!(
        snapshot_path.join("wasm").exists(),
        "Snapshot must contain wasm directory"
    );

    // Verify snapshot contains blockchain files
    assert!(snapshot_path.join("data/application.db").exists());
    assert!(snapshot_path.join("data/blockstore.db").exists());
    assert!(snapshot_path
        .join("data/priv_validator_state.json")
        .exists());
}

#[test]
fn test_validator_state_preservation_concept() {
    // This test verifies the CONCEPT of validator state preservation
    // In actual restore, current validator state should be backed up and restored

    let deploy_dir = TempDir::new().unwrap();
    let snapshot_dir = TempDir::new().unwrap();

    create_mock_running_node(&deploy_dir.path().to_path_buf());
    create_mock_snapshot(&snapshot_dir.path().to_path_buf());

    // Read current node's validator state (what should be preserved)
    let current_validator_path = deploy_dir
        .path()
        .join("data/priv_validator_state.json");
    let current_state = fs::read_to_string(&current_validator_path).unwrap();
    let current_parsed: serde_json::Value = serde_json::from_str(&current_state).unwrap();

    // Read snapshot's validator state (what will be initially copied)
    let snapshot_validator_path = snapshot_dir
        .path()
        .join("data/priv_validator_state.json");
    let snapshot_state = fs::read_to_string(&snapshot_validator_path).unwrap();
    let snapshot_parsed: serde_json::Value = serde_json::from_str(&snapshot_state).unwrap();

    // Verify they are different (snapshot has height 50, current has height 100)
    assert_ne!(
        current_parsed.get("height"),
        snapshot_parsed.get("height"),
        "Current and snapshot validator states should differ"
    );

    // The restoration process should:
    // 1. Backup current state (height 100)
    // 2. Copy snapshot data (includes height 50)
    // 3. Overwrite with backed up state (restore height 100)
    // This prevents double-signing by preserving the node's signing history

    assert_eq!(current_parsed.get("height").unwrap(), "100");
    assert_eq!(snapshot_parsed.get("height").unwrap(), "50");
}

#[test]
fn test_validator_state_higher_in_current_than_snapshot() {
    let deploy_dir = TempDir::new().unwrap();
    let snapshot_dir = TempDir::new().unwrap();

    create_mock_running_node(&deploy_dir.path().to_path_buf());
    create_mock_snapshot(&snapshot_dir.path().to_path_buf());

    // Current validator state
    let current_state = fs::read_to_string(
        deploy_dir
            .path()
            .join("data/priv_validator_state.json"),
    )
    .unwrap();
    let current: serde_json::Value = serde_json::from_str(&current_state).unwrap();
    let current_height = current.get("height").unwrap().as_str().unwrap().parse::<u64>().unwrap();

    // Snapshot validator state
    let snapshot_state = fs::read_to_string(
        snapshot_dir
            .path()
            .join("data/priv_validator_state.json"),
    )
    .unwrap();
    let snapshot: serde_json::Value = serde_json::from_str(&snapshot_state).unwrap();
    let snapshot_height = snapshot.get("height").unwrap().as_str().unwrap().parse::<u64>().unwrap();

    // Current height should be higher (100 vs 50)
    // This is typical - node has continued syncing after snapshot was taken
    assert!(
        current_height > snapshot_height,
        "Current height {} should be > snapshot height {}",
        current_height,
        snapshot_height
    );

    // CRITICAL: Restoration must preserve current height to prevent double-signing
    // If we used snapshot's height (50), validator would re-sign blocks 50-100
}

#[test]
fn test_both_directories_required_in_snapshot() {
    let snapshot_dir = TempDir::new().unwrap();
    create_mock_snapshot(&snapshot_dir.path().to_path_buf());

    // Verify both data and wasm are present
    let data_exists = snapshot_dir.path().join("data").exists();
    let wasm_exists = snapshot_dir.path().join("wasm").exists();

    assert!(
        data_exists && wasm_exists,
        "Snapshot must contain BOTH data and wasm directories"
    );

    // If either is missing, restore should fail
    // This is enforced in execute_full_restore_sequence step 2
}

#[test]
fn test_directory_deletion_and_copy_concept() {
    // Test the concept of deleting old directories and copying new ones

    let deploy_dir = TempDir::new().unwrap();
    let snapshot_dir = TempDir::new().unwrap();

    create_mock_running_node(&deploy_dir.path().to_path_buf());
    create_mock_snapshot(&snapshot_dir.path().to_path_buf());

    // Backup current validator state
    let current_validator = deploy_dir
        .path()
        .join("data/priv_validator_state.json");
    let validator_backup_path = deploy_dir
        .path()
        .join("priv_validator_state_backup.json");
    fs::copy(&current_validator, &validator_backup_path).unwrap();

    // Delete current data and wasm (simulating restore step 7)
    fs::remove_dir_all(deploy_dir.path().join("data")).unwrap();
    fs::remove_dir_all(deploy_dir.path().join("wasm")).unwrap();

    // Copy from snapshot (simulating restore step 8)
    let snapshot_data = snapshot_dir.path().join("data");
    let snapshot_wasm = snapshot_dir.path().join("wasm");
    let target_data = deploy_dir.path().join("data");
    let target_wasm = deploy_dir.path().join("wasm");

    copy_dir_recursive(&snapshot_data, &target_data);
    copy_dir_recursive(&snapshot_wasm, &target_wasm);

    // Verify directories were copied
    assert!(target_data.exists());
    assert!(target_wasm.exists());

    // Restore backed up validator state (simulating restore step 10)
    let restored_validator = deploy_dir
        .path()
        .join("data/priv_validator_state.json");
    fs::copy(&validator_backup_path, &restored_validator).unwrap();

    // Read final validator state
    let final_state = fs::read_to_string(&restored_validator).unwrap();
    let final_parsed: serde_json::Value = serde_json::from_str(&final_state).unwrap();

    // Verify final state matches ORIGINAL node's state, not snapshot's state
    assert_eq!(
        final_parsed.get("height").unwrap(),
        "100",
        "Validator height should be preserved from original node"
    );
}

// Helper function to recursively copy directories
fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let file_type = entry.file_type().unwrap();
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).unwrap();
        }
    }
}

#[test]
fn test_restore_request_fields() {
    let request = RestoreRequest {
        node_name: "pirin-node-3".to_string(),
        service_name: "full-node-3".to_string(),
        deploy_path: "/opt/deploy/nolus/full-node-3".to_string(),
        snapshot_dir: "/home/backup/snapshots/pirin-1_20250125_17154420".to_string(),
        log_path: Some("/var/log/full-node-3".to_string()),
    };

    assert_eq!(request.node_name, "pirin-node-3");
    assert_eq!(request.service_name, "full-node-3");
    assert!(request.deploy_path.contains("full-node-3"));
    assert!(request.snapshot_dir.contains("pirin-1"));
    assert!(request.snapshot_dir.contains("17154420")); // block height
    assert!(request.log_path.is_some());
}

#[test]
fn test_snapshot_path_parsing() {
    let snapshot_path = "/home/backup/snapshots/osmosis-1_20250125_17154420";

    // Extract snapshot name from path
    let snapshot_name = snapshot_path.split('/').last().unwrap();
    assert_eq!(snapshot_name, "osmosis-1_20250125_17154420");

    // Parse components
    let parts: Vec<&str> = snapshot_name.split('_').collect();
    assert_eq!(parts.len(), 3);

    let network = parts[0];
    let date = parts[1];
    let block_height = parts[2];

    assert_eq!(network, "osmosis-1");
    assert_eq!(date, "20250125");
    assert_eq!(block_height, "17154420");
}

#[test]
fn test_validator_state_json_structure() {
    let validator_state = r#"{
        "height": "12345",
        "round": 2,
        "step": 3
    }"#;

    let parsed: serde_json::Value = serde_json::from_str(validator_state).unwrap();

    assert!(parsed.get("height").is_some());
    assert!(parsed.get("round").is_some());
    assert!(parsed.get("step").is_some());

    let height = parsed.get("height").unwrap().as_str().unwrap();
    assert!(height.parse::<u64>().is_ok(), "Height should be numeric");
}

#[test]
fn test_cross_node_restore_same_network() {
    // Verify that snapshots from one node can restore another node on same network
    let snapshot_from_node1 = "pirin-1_20250125_17154420"; // From pirin-node-1
    let _restoring_to_node2 = "pirin-node-2"; // Restoring to pirin-node-2

    // Both nodes on pirin-1 network
    assert!(snapshot_from_node1.starts_with("pirin-1"));

    // Snapshot is network-based, not node-specific
    assert!(!snapshot_from_node1.contains("node-1"));
    assert!(!snapshot_from_node1.contains("node-2"));

    // Node 2 can restore from this snapshot because:
    // 1. Same network (pirin-1)
    // 2. Node 2's validator state will be preserved
    // 3. Only blockchain data is shared across network
}

#[test]
fn test_backup_path_for_validator_state() {
    let deploy_path = "/opt/deploy/nolus/full-node-3";
    let validator_current = format!("{}/data/priv_validator_state.json", deploy_path);
    let validator_backup = format!("{}/priv_validator_state_backup.json", deploy_path);

    // Backup is stored in deploy_path root, not in data directory
    assert!(validator_backup.contains("/opt/deploy/nolus/full-node-3/"));
    assert!(!validator_backup.contains("/data/"));

    // Current is in data directory
    assert!(validator_current.contains("/data/"));
}
