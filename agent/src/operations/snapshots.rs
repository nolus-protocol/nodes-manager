// File: agent/src/operations/snapshots.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, logs, systemctl};
use crate::types::{SnapshotInfo, SnapshotRequest};

pub async fn execute_full_snapshot_sequence(request: &SnapshotRequest) -> Result<SnapshotInfo> {
    info!("Starting snapshot creation for: {}", request.node_name);

    // Generate timestamp and filenames using the node_name from request
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let snapshot_filename = format!("{}_{}.lz4", request.node_name, timestamp);
    let snapshot_path = format!("{}/{}", request.backup_path, snapshot_filename);
    let validator_backup_path = format!("{}/validator_state_backup_{}.json", request.backup_path, timestamp);

    // Step 1: Create backup directory
    commands::create_directory(&request.backup_path).await?;

    // Step 2: Stop the node service
    systemctl::stop_service(&request.service_name).await?;

    // Step 3: Truncate logs (if configured)
    if let Some(log_path) = &request.log_path {
        logs::truncate_log_path(log_path).await?;
    }

    // Step 4: Backup validator state
    let validator_source = format!("{}/data/priv_validator_state.json", request.deploy_path);
    commands::copy_file_if_exists(&validator_source, &validator_backup_path).await?;

    // Step 5: Create LZ4 compressed snapshot
    info!("Creating LZ4 compressed snapshot...");
    commands::create_lz4_archive(
        &request.deploy_path,
        &snapshot_path,
        &["data", "wasm"]
    ).await?;

    // Step 6: Get file size and verify snapshot
    let size_bytes = commands::get_file_size(&snapshot_path).await?;
    if size_bytes < 1024 {
        return Err(anyhow::anyhow!(
            "Snapshot file is too small ({} bytes), likely corrupt or empty",
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

    info!("Snapshot created successfully: {} ({:.1} MB)",
          snapshot_filename, size_bytes as f64 / 1024.0 / 1024.0);

    Ok(SnapshotInfo {
        filename: snapshot_filename,
        size_bytes,
        path: snapshot_path,
    })
}
