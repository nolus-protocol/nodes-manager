// File: agent/src/operations/snapshots.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, logs, systemctl};
use crate::types::{SnapshotInfo, SnapshotRequest};

pub async fn execute_full_snapshot_sequence(request: &SnapshotRequest) -> Result<SnapshotInfo> {
    info!("Starting snapshot creation for: {}", request.node_name);

    // Generate timestamp and directory name using the node_name from request
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let snapshot_dirname = format!("{}_{}", request.node_name, timestamp);
    let snapshot_path = format!("{}/{}", request.backup_path, snapshot_dirname);
    let validator_backup_path = format!("{}/priv_validator_state.json", snapshot_path);

    // Step 1: Create backup directory and snapshot directory
    commands::create_directory(&request.backup_path).await?;
    commands::create_directory(&snapshot_path).await?;

    // Step 2: Stop the node service
    systemctl::stop_service(&request.service_name).await?;

    // Step 3: Truncate logs (if configured)
    if let Some(log_path) = &request.log_path {
        logs::truncate_log_path(log_path).await?;
    }

    // Step 4: Backup validator state to snapshot directory
    let validator_source = format!("{}/data/priv_validator_state.json", request.deploy_path);
    commands::copy_file_if_exists(&validator_source, &validator_backup_path).await?;

    // Step 5: Copy data and wasm directories to snapshot directory
    info!("Copying blockchain data to snapshot directory...");
    commands::copy_directories_to_snapshot(&request.deploy_path, &snapshot_path, &["data", "wasm"]).await?;

    // Step 6: Get directory size and verify snapshot
    let size_bytes = commands::get_directory_size(&snapshot_path).await?;
    if size_bytes < 1024 {
        return Err(anyhow::anyhow!(
            "Snapshot directory is too small ({} bytes), likely empty or incomplete",
            size_bytes
        ));
    }

    // Step 7: Start the node service
    systemctl::start_service(&request.service_name).await?;

    // Step 8: Verify service is running
    let status = systemctl::get_service_status(&request.service_name).await?;
    if status != "active" {
        return Err(anyhow::anyhow!(
            "Service {} failed to start properly after snapshot (status: {})",
            request.service_name, status
        ));
    }

    info!("Snapshot directory created successfully: {} ({:.1} MB)",
          snapshot_dirname, size_bytes as f64 / 1024.0 / 1024.0);

    Ok(SnapshotInfo {
        filename: snapshot_dirname,
        size_bytes,
        path: snapshot_path,
    })
}
