// File: agent/src/operations/restore.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, logs, systemctl};
use crate::types::RestoreRequest;

pub async fn execute_full_restore_sequence(request: &RestoreRequest) -> Result<String> {
    info!("Starting snapshot restore for node: {} from network snapshot", request.node_name);

    // Step 1: Verify snapshot directory exists
    let dir_check_command = format!("test -d '{}'", request.snapshot_dir);
    commands::execute_shell_command(&dir_check_command).await
        .map_err(|_| anyhow::anyhow!("Snapshot directory does not exist: {}", request.snapshot_dir))?;

    // Step 2: Get directory size for logging
    let size_command = format!("du -sb '{}' | cut -f1", request.snapshot_dir);
    let dir_size = commands::execute_shell_command(&size_command).await
        .unwrap_or_default()
        .trim()
        .parse::<u64>()
        .unwrap_or(0);

    // Step 3: Stop the node service
    systemctl::stop_service(&request.service_name).await?;

    // FIXED: Step 4: Backup CURRENT validator state (preserve latest signing state)
    let current_validator_path = format!("{}/data/priv_validator_state.json", request.deploy_path);
    let validator_backup_path = format!("{}/priv_validator_state_backup.json", request.deploy_path);

    info!("Backing up current validator state to preserve latest signing information");
    commands::backup_current_validator_state(&current_validator_path, &validator_backup_path).await?;

    // Step 5: Truncate logs (if configured)
    if let Some(log_path) = &request.log_path {
        logs::truncate_log_path(log_path).await?;
    }

    // Step 6: Delete existing data and wasm directories
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

    // Step 7: Copy data and wasm directories from network snapshot
    info!("Copying network snapshot data ({:.1} MB)...", dir_size as f64 / 1024.0 / 1024.0);
    commands::copy_snapshot_directories(&request.snapshot_dir, &request.deploy_path).await?;

    // Step 8: Verify copy results
    let verify_data_cmd = format!("test -d '{}/data'", request.deploy_path);
    if commands::execute_shell_command(&verify_data_cmd).await.is_err() {
        return Err(anyhow::anyhow!("Data directory not found after copy"));
    }

    // FIXED: Step 9: Restore the CURRENT node's validator state (not from snapshot)
    info!("Restoring current node's validator state (preserving latest signing information)");
    commands::restore_current_validator_state(&validator_backup_path, &current_validator_path).await?;

    // Step 10: Set proper ownership/permissions
    let chown_cmd = format!("chown -R $(stat -c '%U:%G' '{}') '{}/data'",
                           request.deploy_path, request.deploy_path);
    commands::execute_shell_command(&chown_cmd).await?;

    let wasm_chown_cmd = format!("if [ -d '{}/wasm' ]; then chown -R $(stat -c '%U:%G' '{}') '{}/wasm'; fi",
                                request.deploy_path, request.deploy_path, request.deploy_path);
    commands::execute_shell_command(&wasm_chown_cmd).await?;

    // Step 11: Clean up validator backup
    commands::remove_file_if_exists(&validator_backup_path).await?;

    // Step 12: Start the node service
    systemctl::start_service(&request.service_name).await?;

    // Step 13: Verify service is running
    let status = systemctl::get_service_status(&request.service_name).await?;
    if status != "active" {
        return Err(anyhow::anyhow!(
            "Service {} failed to start properly after restore (status: {})",
            request.service_name, status
        ));
    }

    info!("Network snapshot restore completed successfully for node: {} (validator state preserved)", request.node_name);

    Ok(format!("Network snapshot restore completed for {} (validator state preserved)", request.node_name))
}
