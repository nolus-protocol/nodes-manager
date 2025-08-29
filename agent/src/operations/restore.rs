// File: agent/src/operations/restore.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, logs, systemctl};
use crate::types::RestoreRequest;

pub async fn execute_full_restore_sequence(request: &RestoreRequest) -> Result<String> {
    info!("Starting snapshot restore for: {}", request.node_name);

    // Step 1: Verify snapshot file exists
    let file_check_command = format!("test -f '{}'", request.snapshot_file);
    commands::execute_shell_command(&file_check_command).await
        .map_err(|_| anyhow::anyhow!("Snapshot file does not exist: {}", request.snapshot_file))?;

    let file_size = commands::get_file_size(&request.snapshot_file).await?;

    // Step 2: Stop the node service
    systemctl::stop_service(&request.service_name).await?;

    // Step 3: Truncate logs (if configured)
    if let Some(log_path) = &request.log_path {
        logs::truncate_log_path(log_path).await?;
    }

    // Step 4: Delete existing data and wasm directories
    let data_dir = format!("{}/data", request.deploy_path);
    let wasm_dir = format!("{}/wasm", request.deploy_path);

    let data_exists_cmd = format!("test -d '{}'", data_dir);
    if commands::execute_shell_command(&data_exists_cmd).await.is_ok() {
        commands::delete_directory(&data_dir).await?;
    }

    let wasm_exists_cmd = format!("test -d '{}'", wasm_dir);
    if commands::execute_shell_command(&wasm_exists_cmd).await.is_ok() {
        commands::delete_directory(&wasm_dir).await?;
    }

    // Step 5: Extract snapshot using LZ4
    info!("Extracting LZ4 snapshot ({:.1} MB)...", file_size as f64 / 1024.0 / 1024.0);
    commands::extract_lz4_archive(&request.snapshot_file, &request.deploy_path).await?;

    // Step 6: Verify extraction results
    let verify_data_cmd = format!("test -d '{}/data'", request.deploy_path);
    if commands::execute_shell_command(&verify_data_cmd).await.is_err() {
        return Err(anyhow::anyhow!("Data directory not found after extraction"));
    }

    // Step 7: Restore validator state (if available)
    if let Some(validator_backup_file) = &request.validator_backup_file {
        let validator_destination = format!("{}/data/priv_validator_state.json", request.deploy_path);
        commands::copy_file_if_exists(validator_backup_file, &validator_destination).await?;
    }

    // Step 8: Set proper ownership/permissions
    let chown_cmd = format!("chown -R $(stat -c '%U:%G' '{}') '{}/data'",
                           request.deploy_path, request.deploy_path);
    commands::execute_shell_command(&chown_cmd).await?;

    let wasm_chown_cmd = format!("if [ -d '{}/wasm' ]; then chown -R $(stat -c '%U:%G' '{}') '{}/wasm'; fi",
                                request.deploy_path, request.deploy_path, request.deploy_path);
    commands::execute_shell_command(&wasm_chown_cmd).await?;

    // Step 9: Start the node service
    systemctl::start_service(&request.service_name).await?;

    // Step 10: Verify service is running
    let status = systemctl::get_service_status(&request.service_name).await?;
    if status != "active" {
        return Err(anyhow::anyhow!(
            "Service {} failed to start properly after restore (status: {})",
            request.service_name, status
        ));
    }

    info!("Snapshot restore completed successfully for: {}", request.node_name);

    Ok(format!("Snapshot restore completed for {}", request.node_name))
}
