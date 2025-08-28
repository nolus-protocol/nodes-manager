// File: agent/src/operations/snapshots.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, logs, systemctl};
use crate::types::{SnapshotInfo, SnapshotRequest};

pub async fn execute_full_snapshot_sequence(request: &SnapshotRequest) -> Result<SnapshotInfo> {
    info!("Starting FULL snapshot sequence for: {}", request.node_name);
    info!("Deploy path: {}, Backup path: {}", request.deploy_path, request.backup_path);

    // Generate timestamp and filenames
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let snapshot_filename = format!("{}_{}.tar.lz4", request.node_name, timestamp);
    let snapshot_path = format!("{}/{}", request.backup_path, snapshot_filename);
    let validator_backup_path = format!("{}/validator_state_backup_{}.json", request.backup_path, timestamp);

    // Step 1: Create backup directory
    info!("Step 1: Creating backup directory: {}", request.backup_path);
    commands::create_directory(&request.backup_path).await?;

    // Step 2: Stop the node service
    info!("Step 2: Stopping service {}", request.service_name);
    systemctl::stop_service(&request.service_name).await?;

    // Step 3: Truncate logs (if configured) - FIXED: Use truncate_log_path instead of truncate_log_file
    if let Some(log_path) = &request.log_path {
        info!("Step 3: Truncating logs at: {}", log_path);
        logs::truncate_log_path(log_path).await?;
    } else {
        info!("Step 3: No log path configured, skipping log truncation");
    }

    // Step 4: Execute cosmos-pruner before snapshot (with default values)
    info!("Step 4: Executing cosmos-pruner before snapshot");
    let _pruning_output = commands::execute_cosmos_pruner(&request.deploy_path, 50000, 100).await?;

    // Step 5: Backup validator state
    info!("Step 5: Backing up validator state");
    let validator_source = format!("{}/data/priv_validator_state.json", request.deploy_path);
    commands::copy_file_if_exists(&validator_source, &validator_backup_path).await?;

    // Step 6: Create LZ4 compressed snapshot
    info!("Step 6: Creating LZ4 compressed snapshot (this may take a long time)");
    commands::create_lz4_archive(
        &request.deploy_path,
        &snapshot_path,
        &["data", "wasm"]
    ).await?;

    // Step 7: Get file size
    info!("Step 7: Getting snapshot file size");
    let size_bytes = commands::get_file_size(&snapshot_path).await?;

    // Step 8: Start the node service
    info!("Step 8: Starting service {}", request.service_name);
    systemctl::start_service(&request.service_name).await?;

    // Step 9: Verify service is running
    let status = systemctl::get_service_status(&request.service_name).await?;
    if status != "active" {
        return Err(anyhow::anyhow!(
            "Service {} failed to start properly after snapshot (status: {})",
            request.service_name, status
        ));
    }

    info!("Full snapshot sequence completed successfully!");
    info!("Snapshot created: {} ({} bytes)", snapshot_filename, size_bytes);
    info!("Validator backup: {}", validator_backup_path);

    Ok(SnapshotInfo {
        filename: snapshot_filename,
        size_bytes,
        path: snapshot_path,
    })
}
