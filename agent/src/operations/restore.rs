// File: agent/src/operations/restore.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, logs, systemctl};
use crate::types::RestoreRequest;

pub async fn execute_full_restore_sequence(request: &RestoreRequest) -> Result<String> {
    info!("Starting FULL snapshot restore sequence for: {}", request.node_name);
    info!("Deploy path: {}, Snapshot file: {}", request.deploy_path, request.snapshot_file);

    let mut operation_log = Vec::new();

    // Step 1: Verify snapshot file exists
    info!("Step 1: Verifying snapshot file exists");
    let file_check_command = format!("test -f '{}'", request.snapshot_file);
    commands::execute_shell_command(&file_check_command).await
        .map_err(|_| anyhow::anyhow!("Snapshot file does not exist: {}", request.snapshot_file))?;

    let file_size = commands::get_file_size(&request.snapshot_file).await?;
    info!("Snapshot file verified: {} bytes ({:.2} MB)", file_size, file_size as f64 / 1024.0 / 1024.0);
    operation_log.push(format!("✓ Verified snapshot file: {:.2} MB", file_size as f64 / 1024.0 / 1024.0));

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

    // Step 4: Delete existing data and wasm directories
    info!("Step 4: Deleting existing data and wasm directories");
    let data_dir = format!("{}/data", request.deploy_path);
    let wasm_dir = format!("{}/wasm", request.deploy_path);

    // Only delete if they exist to avoid errors
    let data_exists_cmd = format!("test -d '{}'", data_dir);
    if commands::execute_shell_command(&data_exists_cmd).await.is_ok() {
        commands::delete_directory(&data_dir).await?;
        operation_log.push("✓ Deleted existing data directory".to_string());
    } else {
        operation_log.push("• Data directory didn't exist (skipped)".to_string());
    }

    let wasm_exists_cmd = format!("test -d '{}'", wasm_dir);
    if commands::execute_shell_command(&wasm_exists_cmd).await.is_ok() {
        commands::delete_directory(&wasm_dir).await?;
        operation_log.push("✓ Deleted existing wasm directory".to_string());
    } else {
        operation_log.push("• Wasm directory didn't exist (skipped)".to_string());
    }

    // Step 5: Extract snapshot using LZ4
    info!("Step 5: Extracting LZ4 snapshot to restore data and wasm directories");
    info!("Starting LZ4 decompression and extraction...");

    commands::extract_lz4_archive(&request.snapshot_file, &request.deploy_path).await?;

    info!("LZ4 extraction completed successfully!");
    operation_log.push("✓ Extracted LZ4 snapshot archive".to_string());

    // Step 6: Verify extraction results
    info!("Step 6: Verifying extraction results");
    let verify_data_cmd = format!("test -d '{}/data'", request.deploy_path);
    let verify_wasm_cmd = format!("test -d '{}/wasm'", request.deploy_path);

    if commands::execute_shell_command(&verify_data_cmd).await.is_ok() {
        operation_log.push("✓ Verified data directory was extracted".to_string());
    } else {
        return Err(anyhow::anyhow!("Data directory not found after extraction"));
    }

    if commands::execute_shell_command(&verify_wasm_cmd).await.is_ok() {
        operation_log.push("✓ Verified wasm directory was extracted".to_string());
    } else {
        operation_log.push("• Wasm directory not found (might not exist in snapshot)".to_string());
    }

    // Step 7: Restore validator state (if available)
    if let Some(validator_backup_file) = &request.validator_backup_file {
        info!("Step 7: Restoring validator state from: {}", validator_backup_file);
        let validator_destination = format!("{}/data/priv_validator_state.json", request.deploy_path);
        commands::copy_file_if_exists(validator_backup_file, &validator_destination).await?;
        operation_log.push("✓ Restored validator state".to_string());
    } else {
        info!("Step 7: No validator backup file provided, skipping validator restore");
        operation_log.push("• Skipped validator restore (not available)".to_string());
    }

    // Step 8: Set proper ownership/permissions
    info!("Step 8: Setting proper ownership and permissions");
    let chown_cmd = format!("chown -R $(stat -c '%U:%G' '{}') '{}/data'",
                           request.deploy_path, request.deploy_path);
    commands::execute_shell_command(&chown_cmd).await?;

    // Also fix wasm directory if it exists
    let wasm_chown_cmd = format!("if [ -d '{}/wasm' ]; then chown -R $(stat -c '%U:%G' '{}') '{}/wasm'; fi",
                                request.deploy_path, request.deploy_path, request.deploy_path);
    commands::execute_shell_command(&wasm_chown_cmd).await?;

    operation_log.push("✓ Set proper ownership and permissions".to_string());

    // Step 9: Start the node service
    info!("Step 9: Starting service {}", request.service_name);
    systemctl::start_service(&request.service_name).await?;
    operation_log.push(format!("✓ Started service: {}", request.service_name));

    // Step 10: Verify service is running
    info!("Step 10: Verifying service status");
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
        Snapshot Size: {:.2} MB\n\
        Validator Backup: {}\n\
        \n\
        Operation Steps:\n\
        {}\n\
        \n\
        Restore completed successfully - node should be syncing from restored state.",
        request.node_name,
        request.deploy_path,
        request.snapshot_file,
        file_size as f64 / 1024.0 / 1024.0,
        request.validator_backup_file.as_deref().unwrap_or("Not available"),
        operation_log.join("\n")
    );

    Ok(summary)
}
