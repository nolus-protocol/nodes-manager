// File: agent/src/operations/restore.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, logs, systemctl};
use crate::types::RestoreRequest;

pub async fn execute_full_restore_sequence(request: &RestoreRequest) -> Result<String> {
    info!(
        "Starting snapshot restore for node: {} from network snapshot",
        request.node_name
    );

    // Step 1: Verify snapshot directory exists
    let dir_check_command = format!("test -d '{}'", request.snapshot_dir);
    commands::execute_shell_command(&dir_check_command)
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "Snapshot directory does not exist: {}",
                request.snapshot_dir
            )
        })?;

    // Step 2: MANDATORY - Verify both data and wasm directories exist in snapshot
    let data_check_command = format!("test -d '{}/data'", request.snapshot_dir);
    commands::execute_shell_command(&data_check_command)
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "CRITICAL: data directory missing from snapshot: {}/data",
                request.snapshot_dir
            )
        })?;

    let wasm_check_command = format!("test -d '{}/wasm'", request.snapshot_dir);
    commands::execute_shell_command(&wasm_check_command)
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "CRITICAL: wasm directory missing from snapshot: {}/wasm",
                request.snapshot_dir
            )
        })?;

    info!("✓ Verified both data and wasm directories exist in snapshot");

    // Step 3: Get directory sizes for logging
    let data_size_command = format!("du -sb '{}/data' | cut -f1", request.snapshot_dir);
    let data_size = commands::execute_shell_command(&data_size_command)
        .await
        .unwrap_or_default()
        .trim()
        .parse::<u64>()
        .unwrap_or(0);

    let wasm_size_command = format!("du -sb '{}/wasm' | cut -f1", request.snapshot_dir);
    let wasm_size = commands::execute_shell_command(&wasm_size_command)
        .await
        .unwrap_or_default()
        .trim()
        .parse::<u64>()
        .unwrap_or(0);

    info!(
        "Snapshot data size: {:.1} MB, wasm size: {:.1} MB",
        data_size as f64 / 1024.0 / 1024.0,
        wasm_size as f64 / 1024.0 / 1024.0
    );

    // Step 4: Stop the node service
    systemctl::stop_service(&request.service_name).await?;
    info!("✓ Node service stopped");

    // Step 5: Backup CURRENT validator state (to preserve individual node's signing state)
    let current_validator_path = format!("{}/data/priv_validator_state.json", request.deploy_path);
    let validator_backup_path = format!("{}/priv_validator_state_backup.json", request.deploy_path);

    info!("Backing up current validator state to preserve individual signing information");
    commands::backup_current_validator_state(&current_validator_path, &validator_backup_path)
        .await?;
    info!("✓ Current validator state backed up");

    // Step 6: Truncate logs (if configured)
    if let Some(log_path) = &request.log_path {
        logs::truncate_log_path(log_path).await?;
        info!("✓ Logs truncated");
    }

    // Step 7: Delete existing data and wasm directories
    let data_dir = format!("{}/data", request.deploy_path);
    let wasm_dir = format!("{}/wasm", request.deploy_path);

    let data_exists_cmd = format!("test -d '{}'", data_dir);
    if commands::execute_shell_command(&data_exists_cmd)
        .await
        .is_ok()
    {
        commands::delete_directory(&data_dir).await?;
        info!("✓ Existing data directory deleted");
    }

    let wasm_exists_cmd = format!("test -d '{}'", wasm_dir);
    if commands::execute_shell_command(&wasm_exists_cmd)
        .await
        .is_ok()
    {
        commands::delete_directory(&wasm_dir).await?;
        info!("✓ Existing wasm directory deleted");
    }

    // Step 8: MANDATORY - Copy BOTH data and wasm directories from network snapshot (includes snapshot's validator state)
    info!("Copying network snapshot data and wasm directories (including snapshot's validator state)...");
    commands::copy_snapshot_directories_mandatory(&request.snapshot_dir, &request.deploy_path)
        .await?;
    info!("✓ Both data and wasm directories copied successfully from snapshot");

    // Step 9: MANDATORY - Verify both directories were copied successfully
    let verify_data_cmd = format!("test -d '{}/data'", request.deploy_path);
    commands::execute_shell_command(&verify_data_cmd)
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "CRITICAL: data directory not found after copy to {}/data",
                request.deploy_path
            )
        })?;

    let verify_wasm_cmd = format!("test -d '{}/wasm'", request.deploy_path);
    commands::execute_shell_command(&verify_wasm_cmd)
        .await
        .map_err(|_| {
            anyhow::anyhow!(
                "CRITICAL: wasm directory not found after copy to {}/wasm",
                request.deploy_path
            )
        })?;

    info!("✓ Verified both data and wasm directories exist after copy");

    // Step 10: Overwrite snapshot's validator state with CURRENT node's validator state (preserving individual signing state)
    info!("Overwriting snapshot's validator state with current node's individual validator state");
    commands::restore_current_validator_state(&validator_backup_path, &current_validator_path)
        .await?;
    info!("✓ Current validator state restored, overwriting snapshot's validator state to prevent double-signing");

    // Step 11: Set proper ownership/permissions
    let chown_cmd = format!(
        "chown -R $(stat -c '%U:%G' '{}') '{}/data'",
        request.deploy_path, request.deploy_path
    );
    commands::execute_shell_command(&chown_cmd).await?;

    let wasm_chown_cmd = format!(
        "chown -R $(stat -c '%U:%G' '{}') '{}/wasm'",
        request.deploy_path, request.deploy_path
    );
    commands::execute_shell_command(&wasm_chown_cmd).await?;
    info!("✓ Permissions set for both data and wasm directories");

    // Step 12: Clean up validator backup
    commands::remove_file_if_exists(&validator_backup_path).await?;
    info!("✓ Validator backup file cleaned up");

    // Step 13: Start the node service
    systemctl::start_service(&request.service_name).await?;
    info!("✓ Node service started");

    // Step 14: Verify service is running
    let status = systemctl::get_service_status(&request.service_name).await?;
    if status != "active" {
        return Err(anyhow::anyhow!(
            "Service {} failed to start properly after restore (status: {})",
            request.service_name,
            status
        ));
    }
    info!("✓ Service verified as active");

    info!("Network snapshot restore completed successfully for node: {} (individual validator state preserved by overwriting snapshot's validator state)", request.node_name);

    Ok(format!("Network snapshot restore completed for {} (individual validator state preserved, snapshot's validator state overwritten)", request.node_name))
}
