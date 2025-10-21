// File: agent/src/operations/snapshots.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, logs, systemctl};
use crate::types::{SnapshotInfo, SnapshotRequest};

pub async fn execute_full_snapshot_sequence(request: &SnapshotRequest) -> Result<SnapshotInfo> {
    info!(
        "Starting snapshot creation: {} (from node: {})",
        request.snapshot_name, request.node_name
    );

    // Use pre-built snapshot name from manager (includes network, date, and block height)
    let snapshot_dirname = request.snapshot_name.clone();
    let snapshot_path = format!("{}/{}", request.backup_path, snapshot_dirname);

    // Step 1: Verify source directories exist BEFORE starting snapshot
    let data_exists_cmd = format!("test -d '{}/data'", request.deploy_path);
    commands::execute_shell_command(&data_exists_cmd)
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "CRITICAL: Source data directory missing: {}/data",
                request.deploy_path
            )
        })?;

    let wasm_exists_cmd = format!("test -d '{}/wasm'", request.deploy_path);
    commands::execute_shell_command(&wasm_exists_cmd)
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "CRITICAL: Source wasm directory missing: {}/wasm",
                request.deploy_path
            )
        })?;

    info!("✓ Verified both source data and wasm directories exist");

    // Step 2: Create backup directory and snapshot directory
    commands::create_directory(&request.backup_path).await?;
    commands::create_directory(&snapshot_path).await?;
    info!("✓ Snapshot directories created");

    // Step 3: Stop the node service
    systemctl::stop_service(&request.service_name).await?;
    info!("✓ Node service stopped");

    // Step 4: Truncate logs (if configured)
    if let Some(log_path) = &request.log_path {
        logs::truncate_log_path(log_path).await?;
        info!("✓ Logs truncated");
    }

    // Step 5: MANDATORY - Copy BOTH data and wasm directories to snapshot directory (INCLUDING validator state)
    info!("Copying BOTH blockchain data and wasm directories to snapshot (INCLUDING validator state)...");
    commands::copy_directories_to_snapshot_mandatory(
        &request.deploy_path,
        &snapshot_path,
        &["data", "wasm"],
    )
    .await?;
    info!("✓ Both data and wasm directories copied to snapshot with validator state included");

    // Step 6: MANDATORY - Verify snapshot contains both directories
    let snapshot_data_check = format!("test -d '{}/data'", snapshot_path);
    commands::execute_shell_command(&snapshot_data_check)
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "CRITICAL: data directory missing from snapshot: {}/data",
                snapshot_path
            )
        })?;

    let snapshot_wasm_check = format!("test -d '{}/wasm'", snapshot_path);
    commands::execute_shell_command(&snapshot_wasm_check)
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "CRITICAL: wasm directory missing from snapshot: {}/wasm",
                snapshot_path
            )
        })?;

    info!("✓ Verified both data and wasm directories exist in snapshot");

    // Step 7: Verify validator state is included in snapshot
    let validator_in_snapshot = format!("{}/data/priv_validator_state.json", snapshot_path);
    let validator_check_cmd = format!("test -f '{}'", validator_in_snapshot);
    match commands::execute_shell_command(&validator_check_cmd).await {
        Ok(_) => info!("✓ Validator state included in snapshot for external system compatibility"),
        Err(_) => info!("! No validator state found in snapshot (node may not be a validator)"),
    }

    // Step 8: Get directory size and verify snapshot
    let size_bytes = commands::get_directory_size(&snapshot_path).await?;
    if size_bytes < 1024 {
        return Err(anyhow::anyhow!(
            "Snapshot directory is too small ({} bytes), likely empty or incomplete",
            size_bytes
        ));
    }
    info!(
        "✓ Snapshot size verified: {:.1} MB",
        size_bytes as f64 / 1024.0 / 1024.0
    );

    // Step 9: Start the node service
    systemctl::start_service(&request.service_name).await?;
    info!("✓ Node service started");

    // Step 10: Verify service is running
    let status = systemctl::get_service_status(&request.service_name).await?;
    if status != "active" {
        return Err(anyhow::anyhow!(
            "Service {} failed to start properly after snapshot (status: {})",
            request.service_name,
            status
        ));
    }
    info!("✓ Service verified as active");

    info!("Network snapshot created successfully: {} ({:.1} MB) - contains both data and wasm with validator state",
          snapshot_dirname, size_bytes as f64 / 1024.0 / 1024.0);

    Ok(SnapshotInfo {
        filename: snapshot_dirname,
        size_bytes,
        path: snapshot_path,
    })
}
