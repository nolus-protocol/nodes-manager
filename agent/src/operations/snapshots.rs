// File: agent/src/operations/snapshots.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, logs, systemctl};
use crate::types::{SnapshotInfo, SnapshotRequest};

pub async fn execute_full_snapshot_sequence(request: &SnapshotRequest) -> Result<SnapshotInfo> {
    info!("Starting snapshot sequence for: {}", request.node_name);
    info!("Deploy path: {}, Backup path: {}", request.deploy_path, request.backup_path);
    info!("Using gzip compression for reliable operation");

    // Generate timestamp and filenames
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let snapshot_filename = format!("{}_{}.tar.gz", request.node_name, timestamp);
    let snapshot_path = format!("{}/{}", request.backup_path, snapshot_filename);
    let validator_backup_path = format!("{}/validator_state_backup_{}.json", request.backup_path, timestamp);

    // Step 1: Create backup directory
    info!("Step 1: Creating backup directory: {}", request.backup_path);
    commands::create_directory(&request.backup_path).await?;

    // Step 2: Stop the node service
    info!("Step 2: Stopping service {}", request.service_name);
    systemctl::stop_service(&request.service_name).await?;

    // Step 3: Truncate logs (if configured)
    if let Some(log_path) = &request.log_path {
        info!("Step 3: Truncating logs at: {}", log_path);
        logs::truncate_log_path(log_path).await?;
    } else {
        info!("Step 3: No log path configured, skipping log truncation");
    }

    // Step 4: Backup validator state
    info!("Step 4: Backing up validator state");
    let validator_source = format!("{}/data/priv_validator_state.json", request.deploy_path);
    commands::copy_file_if_exists(&validator_source, &validator_backup_path).await?;

    // Step 5: Create gzip compressed snapshot - REMOVED: No pruning step, direct snapshot creation
    info!("Step 5: Creating gzip compressed snapshot");
    info!("Starting gzip compression of data and wasm directories...");
    commands::create_gzip_archive(
        &request.deploy_path,
        &snapshot_path,
        &["data", "wasm"]
    ).await?;
    info!("Gzip compression completed successfully!");

    // Step 6: Get file size
    info!("Step 6: Getting snapshot file size");
    let size_bytes = commands::get_file_size(&snapshot_path).await?;

    // Step 7: Start the node service
    info!("Step 7: Starting service {}", request.service_name);
    systemctl::start_service(&request.service_name).await?;

    // Step 8: Verify service is running
    let status = systemctl::get_service_status(&request.service_name).await?;
    if status != "active" {
        return Err(anyhow::anyhow!(
            "Service {} failed to start properly after snapshot (status: {})",
            request.service_name, status
        ));
    }

    info!("Snapshot sequence completed successfully!");
    info!("Snapshot created: {} ({} bytes)", snapshot_filename, size_bytes);
    info!("Validator backup: {}", validator_backup_path);
    info!("Note: Pruning is handled separately - use the pruning operation if needed");

    Ok(SnapshotInfo {
        filename: snapshot_filename,
        size_bytes,
        path: snapshot_path,
    })
}
