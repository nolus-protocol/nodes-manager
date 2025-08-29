// File: agent/src/operations/snapshots.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, logs, systemctl};
use crate::types::{SnapshotInfo, SnapshotRequest};

pub async fn execute_full_snapshot_sequence(request: &SnapshotRequest) -> Result<SnapshotInfo> {
    info!("Starting optimized snapshot creation for: {}", request.node_name);

    // Generate timestamp and paths using the node_name from request
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let snapshot_filename = format!("{}_{}.tar.gz", request.node_name, timestamp);
    let compressed_path = format!("{}/{}", request.backup_path, snapshot_filename);
    let backup_dir = format!("{}/{}_{}", request.backup_path, request.node_name, timestamp); // UNIFIED: Removed _backup suffix
    let validator_backup_path = format!("{}/validator_state_backup_{}.json", request.backup_path, timestamp);

    // Step 1: Create backup directory
    commands::create_directory(&request.backup_path).await?;

    // Step 2: Stop the node service
    info!("Stopping service for snapshot: {}", request.service_name);
    systemctl::stop_service(&request.service_name).await?;

    // Step 3: Truncate logs (if configured)
    if let Some(log_path) = &request.log_path {
        info!("Truncating logs at: {}", log_path);
        logs::truncate_log_path(log_path).await?;
    }

    // Step 4: Backup validator state
    let validator_source = format!("{}/data/priv_validator_state.json", request.deploy_path);
    commands::copy_file_if_exists(&validator_source, &validator_backup_path).await?;

    // Step 5: OPTIMIZED - Copy data and wasm folders to backup directory (fast operation)
    info!("Copying blockchain data folders to backup directory...");
    commands::copy_snapshot_folders(
        &request.deploy_path,
        &backup_dir,
        &["data", "wasm"]
    ).await?;

    // Step 6: Get directory size for reporting
    let size_bytes = commands::get_directory_size(&backup_dir).await.unwrap_or(0);
    if size_bytes < 1024 {
        return Err(anyhow::anyhow!(
            "Backup directory is too small ({} bytes), likely empty or copy failed",
            size_bytes
        ));
    }

    // Step 7: OPTIMIZED - Start the node service immediately after copying (no waiting for compression)
    info!("Starting service immediately after copying: {}", request.service_name);
    systemctl::start_service(&request.service_name).await?;

    // Step 8: Verify service is running
    let status = systemctl::get_service_status(&request.service_name).await?;
    if status != "active" {
        return Err(anyhow::anyhow!(
            "Service {} failed to start properly after snapshot copy (status: {})",
            request.service_name, status
        ));
    }

    info!("Optimized snapshot completed for {}: copied {:.1} MB, service restarted successfully",
          request.node_name, size_bytes as f64 / 1024.0 / 1024.0);

    // CRITICAL: Create success response BEFORE starting background compression
    let snapshot_result = SnapshotInfo {
        filename: snapshot_filename,
        size_bytes,
        path: compressed_path.clone(), // Clone so we can use it later for background compression
    };

    // Step 9: IMPORTANT - Start background gzip AFTER manager receives success confirmation
    // This is fire-and-forget - we don't track the compression result
    info!("Starting fire-and-forget background compression: {} -> {}", backup_dir, compressed_path);
    commands::compress_directory_background(&backup_dir, &compressed_path).await;

    // Return success to manager IMMEDIATELY - gzip runs independently in background
    Ok(snapshot_result)
}
