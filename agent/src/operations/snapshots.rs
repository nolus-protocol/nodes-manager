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
    let snapshot_filename = format!("{}_{}.lz4", request.node_name, timestamp);
    let snapshot_path = format!("{}/{}", request.backup_path, snapshot_filename);
    let validator_backup_path = format!("{}/validator_state_backup_{}.json", request.backup_path, timestamp);

    let mut operation_log = Vec::new();

    // Step 1: Create backup directory
    info!("Step 1: Creating backup directory: {}", request.backup_path);
    commands::create_directory(&request.backup_path).await?;
    operation_log.push(format!("✓ Created backup directory: {}", request.backup_path));

    // Step 2: Stop the node service
    info!("Step 2: Stopping service {}", request.service_name);
    systemctl::stop_service(&request.service_name).await?;
    operation_log.push(format!("✓ Stopped service: {}", request.service_name));

    // Step 3: Truncate logs (if configured)
    if let Some(log_path) = &request.log_path {
        info!("Step 3: Truncating logs at: {}", log_path);
        logs::truncate_log_path(log_path).await?;
        operation_log.push(format!("✓ Truncated logs: {}", log_path));
    } else {
        info!("Step 3: No log path configured, skipping log truncation");
        operation_log.push("• Skipped log truncation (not configured)".to_string());
    }

    // Step 4: Backup validator state
    info!("Step 4: Backing up validator state");
    let validator_source = format!("{}/data/priv_validator_state.json", request.deploy_path);
    commands::copy_file_if_exists(&validator_source, &validator_backup_path).await?;
    operation_log.push("✓ Backed up validator state".to_string());

    // Step 5: Create LZ4 compressed snapshot
    info!("Step 5: Creating LZ4 compressed snapshot");
    info!("Starting LZ4 compression of data and wasm directories...");

    commands::create_lz4_archive(
        &request.deploy_path,
        &snapshot_path,
        &["data", "wasm"]
    ).await?;

    info!("LZ4 compression completed successfully!");
    operation_log.push("✓ Created LZ4 compressed snapshot".to_string());

    // Step 6: Get file size and verify snapshot
    info!("Step 6: Verifying snapshot file size");
    let size_bytes = commands::get_file_size(&snapshot_path).await?;

    if size_bytes < 1024 {
        return Err(anyhow::anyhow!(
            "Snapshot file is too small ({} bytes), likely corrupt or empty",
            size_bytes
        ));
    }

    info!("Snapshot verified: {} bytes ({:.2} MB)", size_bytes, size_bytes as f64 / 1024.0 / 1024.0);
    operation_log.push(format!("✓ Verified snapshot: {:.2} MB", size_bytes as f64 / 1024.0 / 1024.0));

    // Step 7: Start the node service
    info!("Step 7: Starting service {}", request.service_name);
    systemctl::start_service(&request.service_name).await?;
    operation_log.push(format!("✓ Started service: {}", request.service_name));

    // Step 8: Verify service is running
    let status = systemctl::get_service_status(&request.service_name).await?;
    if status != "active" {
        return Err(anyhow::anyhow!(
            "Service {} failed to start properly after snapshot (status: {})",
            request.service_name, status
        ));
    }
    operation_log.push(format!("✓ Verified service is running: {}", status));

    info!("Full snapshot sequence completed successfully!");
    info!("Snapshot created: {} ({} bytes)", snapshot_filename, size_bytes);
    info!("Validator backup: {}", validator_backup_path);

    // Log operation summary
    info!("=== SNAPSHOT OPERATION SUMMARY ===");
    for log_entry in &operation_log {
        info!("{}", log_entry);
    }

    Ok(SnapshotInfo {
        filename: snapshot_filename,
        size_bytes,
        path: snapshot_path,
    })
}
