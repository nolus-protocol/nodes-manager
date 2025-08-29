// File: agent/src/operations/restore.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, logs, systemctl};
use crate::types::RestoreRequest;

pub async fn execute_full_restore_sequence(request: &RestoreRequest) -> Result<String> {
    info!("Starting FULL snapshot restore sequence for: {}", request.node_name);
    info!("Deploy path: {}, Snapshot file: {}", request.deploy_path, request.snapshot_file);

    let mut operation_log = Vec::new();

    // Step 1: Stop the node service
    info!("Step 1: Stopping service {}", request.service_name);
    systemctl::stop_service(&request.service_name).await?;
    operation_log.push(format!("✓ Stopped service: {}", request.service_name));

    // Step 2: Truncate logs (if configured)
    if let Some(log_path) = &request.log_path {
        info!("Step 2: Truncating logs at: {}", log_path);
        logs::truncate_log_path(log_path).await?;
        operation_log.push(format!("✓ Truncated logs: {}", log_path));
    } else {
        info!("Step 2: No log path configured, skipping log truncation");
        operation_log.push("• Skipped log truncation (not configured)".to_string());
    }

    // Step 3: Delete existing data and wasm directories
    info!("Step 3: Deleting existing data and wasm directories");
    let data_dir = format!("{}/data", request.deploy_path);
    let wasm_dir = format!("{}/wasm", request.deploy_path);

    commands::delete_directory(&data_dir).await?;
    commands::delete_directory(&wasm_dir).await?;
    operation_log.push("✓ Deleted existing data and wasm directories".to_string());

    // Step 4: Extract snapshot - CHANGED: now uses reliable gzip extraction
    info!("Step 4: Extracting gzip snapshot to restore data and wasm directories");
    info!("Starting gzip decompression and extraction...");
    commands::extract_gzip_archive(&request.snapshot_file, &request.deploy_path).await?;
    info!("Gzip extraction completed successfully!");
    operation_log.push("✓ Extracted gzip snapshot archive".to_string());

    // Step 5: Restore validator state (if available)
    if let Some(validator_backup_file) = &request.validator_backup_file {
        info!("Step 5: Restoring validator state from: {}", validator_backup_file);
        let validator_destination = format!("{}/data/priv_validator_state.json", request.deploy_path);
        commands::copy_file_if_exists(validator_backup_file, &validator_destination).await?;
        operation_log.push("✓ Restored validator state".to_string());
    } else {
        info!("Step 5: No validator backup file provided, skipping validator restore");
        operation_log.push("• Skipped validator restore (not available)".to_string());
    }

    // Step 6: Set proper ownership/permissions
    info!("Step 6: Setting proper ownership and permissions");
    let chown_cmd = format!("chown -R $(stat -c '%U:%G' '{}') '{}/data' '{}/wasm'",
                           request.deploy_path, request.deploy_path, request.deploy_path);
    commands::execute_shell_command(&chown_cmd).await?;
    operation_log.push("✓ Set proper ownership and permissions".to_string());

    // Step 7: Start the node service
    info!("Step 7: Starting service {}", request.service_name);
    systemctl::start_service(&request.service_name).await?;
    operation_log.push(format!("✓ Started service: {}", request.service_name));

    // Step 8: Verify service is running
    let status = systemctl::get_service_status(&request.service_name).await?;
    if status != "active" {
        return Err(anyhow::anyhow!(
            "Service {} failed to start properly after restore (status: {})",
            request.service_name, status
        ));
    }
    operation_log.push(format!("✓ Verified service is running: {}", status));

    info!("Full snapshot restore sequence completed successfully for: {}", request.node_name);

    // Return comprehensive operation summary
    let summary = format!(
        "=== SNAPSHOT RESTORE COMPLETED ===\n\
        Node: {}\n\
        Deploy Path: {}\n\
        Snapshot File: {}\n\
        Validator Backup: {}\n\
        Compression: gzip\n\
        \n\
        Operation Steps:\n\
        {}\n\
        \n\
        Restore completed successfully - node should be syncing from restored state.",
        request.node_name,
        request.deploy_path,
        request.snapshot_file,
        request.validator_backup_file.as_deref().unwrap_or("Not available"),
        operation_log.join("\n")
    );

    Ok(summary)
}
