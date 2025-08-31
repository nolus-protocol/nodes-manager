// File: agent/src/operations/snapshots.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, logs, systemctl};
use crate::types::{SnapshotInfo, SnapshotRequest};

pub async fn execute_full_snapshot_sequence(request: &SnapshotRequest) -> Result<SnapshotInfo> {
    info!("Starting snapshot creation for network: {} (from node: {})", request.network, request.node_name);

    // FIXED: Generate snapshot directory name using NETWORK instead of node_name
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let snapshot_dirname = format!("{}_{}", request.network, timestamp);
    let snapshot_path = format!("{}/{}", request.backup_path, snapshot_dirname);

    // Step 1: Create backup directory and snapshot directory
    commands::create_directory(&request.backup_path).await?;
    commands::create_directory(&snapshot_path).await?;

    // Step 2: Stop the node service
    systemctl::stop_service(&request.service_name).await?;

    // Step 3: Truncate logs (if configured)
    if let Some(log_path) = &request.log_path {
        logs::truncate_log_path(log_path).await?;
    }

    // FIXED: Skip validator state backup - we don't want validator state in snapshots
    info!("Skipping validator state backup (will be preserved on individual nodes during restore)");

    // Step 4: Copy ONLY data and wasm directories to snapshot directory (no validator state)
    info!("Copying blockchain data to snapshot directory (excluding validator state)...");
    commands::copy_directories_to_snapshot(&request.deploy_path, &snapshot_path, &["data", "wasm"]).await?;

    // FIXED: Remove any validator state that might have been copied
    let validator_in_snapshot = format!("{}/data/priv_validator_state.json", snapshot_path);
    commands::remove_file_if_exists(&validator_in_snapshot).await?;

    // Step 5: Get directory size and verify snapshot
    let size_bytes = commands::get_directory_size(&snapshot_path).await?;
    if size_bytes < 1024 {
        return Err(anyhow::anyhow!(
            "Snapshot directory is too small ({} bytes), likely empty or incomplete",
            size_bytes
        ));
    }

    // Step 6: Start the node service
    systemctl::start_service(&request.service_name).await?;

    // Step 7: Verify service is running
    let status = systemctl::get_service_status(&request.service_name).await?;
    if status != "active" {
        return Err(anyhow::anyhow!(
            "Service {} failed to start properly after snapshot (status: {})",
            request.service_name, status
        ));
    }

    info!("Network snapshot created successfully: {} ({:.1} MB) - can be used by any node on {} network",
          snapshot_dirname, size_bytes as f64 / 1024.0 / 1024.0, request.network);

    Ok(SnapshotInfo {
        filename: snapshot_dirname,
        size_bytes,
        path: snapshot_path,
    })
}
