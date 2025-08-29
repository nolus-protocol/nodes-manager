// File: agent/src/operations/restore.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, logs, systemctl};
use crate::types::RestoreRequest;

pub async fn execute_full_restore_sequence(request: &RestoreRequest) -> Result<String> {
    info!("Starting FAST COPY restore for: {}", request.node_name);

    // Step 1: Verify backup directory exists
    if !commands::directory_exists(&request.snapshot_file).await {
        return Err(anyhow::anyhow!("Backup directory does not exist: {}", request.snapshot_file));
    }

    let backup_size = commands::get_directory_size(&request.snapshot_file).await.unwrap_or(0);
    info!("Restoring from backup directory: {} ({:.1} MB)",
          request.snapshot_file, backup_size as f64 / 1024.0 / 1024.0);

    // Step 2: Stop the node service
    info!("Stopping service: {}", request.service_name);
    systemctl::stop_service(&request.service_name).await?;

    // Step 3: Truncate logs (if configured)
    if let Some(log_path) = &request.log_path {
        info!("Truncating logs: {}", log_path);
        logs::truncate_log_path(log_path).await?;
    }

    // Step 4: Delete existing data and wasm directories
    let data_dir = format!("{}/data", request.deploy_path);
    let wasm_dir = format!("{}/wasm", request.deploy_path);

    if commands::directory_exists(&data_dir).await {
        info!("Deleting existing data directory...");
        commands::delete_directory(&data_dir).await?;
        info!("Existing data directory deleted");
    }

    if commands::directory_exists(&wasm_dir).await {
        info!("Deleting existing wasm directory...");
        commands::delete_directory(&wasm_dir).await?;
        info!("Existing wasm directory deleted");
    }

    // Step 5: FAST COPY data directory from backup
    let backup_data_dir = format!("{}/data", request.snapshot_file);
    let backup_wasm_dir = format!("{}/wasm", request.snapshot_file);

    if !commands::directory_exists(&backup_data_dir).await {
        return Err(anyhow::anyhow!("Backup data directory not found: {}", backup_data_dir));
    }

    info!("Starting FAST COPY restore of data directory...");
    let start_time = std::time::Instant::now();

    // Copy data directory back
    commands::copy_directory(&backup_data_dir, &data_dir).await?;

    let data_restore_time = start_time.elapsed();
    info!("Data directory restored in {:.1}s", data_restore_time.as_secs_f64());

    // Copy wasm directory back if it exists in backup
    if commands::directory_exists(&backup_wasm_dir).await {
        info!("Restoring wasm directory...");
        commands::copy_directory(&backup_wasm_dir, &wasm_dir).await?;
        info!("Wasm directory restored");
    } else {
        info!("No wasm directory in backup, skipping");
    }

    let total_restore_time = start_time.elapsed();
    info!("FAST COPY restore completed in {:.1}s", total_restore_time.as_secs_f64());

    // Step 6: Restore validator state if provided
    if let Some(validator_backup_file) = &request.validator_backup_file {
        let validator_destination = format!("{}/data/priv_validator_state.json", request.deploy_path);
        commands::copy_file_if_exists(validator_backup_file, &validator_destination).await?;
        info!("Validator state restored");
    } else {
        // Try to restore from the standard location in backup
        let validator_backup_default = format!("{}/priv_validator_state.json", request.snapshot_file);
        if std::path::Path::new(&validator_backup_default).exists() {
            let validator_destination = format!("{}/data/priv_validator_state.json", request.deploy_path);
            commands::copy_file_if_exists(&validator_backup_default, &validator_destination).await?;
            info!("Validator state restored from backup directory");
        }
    }

    // Step 7: Set proper ownership/permissions
    info!("Setting proper ownership and permissions...");
    let chown_cmd = format!("chown -R $(stat -c '%U:%G' '{}') '{}/data'",
                           request.deploy_path, request.deploy_path);
    commands::execute_shell_command(&chown_cmd).await?;

    let wasm_chown_cmd = format!("if [ -d '{}/wasm' ]; then chown -R $(stat -c '%U:%G' '{}') '{}/wasm'; fi",
                                request.deploy_path, request.deploy_path, request.deploy_path);
    commands::execute_shell_command(&wasm_chown_cmd).await?;

    // Step 8: Start the node service
    info!("Starting service: {}", request.service_name);
    systemctl::start_service(&request.service_name).await?;

    // Step 9: Verify service is running
    let status = systemctl::get_service_status(&request.service_name).await?;
    if status != "active" {
        return Err(anyhow::anyhow!(
            "Service {} failed to start properly after restore (status: {})",
            request.service_name, status
        ));
    }

    info!("✅ Service {} restarted successfully after FAST COPY restore", request.service_name);

    // Step 10: Verify restore results
    let restored_data_size = commands::get_directory_size(&data_dir).await.unwrap_or(0);
    info!("Restored data directory size: {:.1} MB", restored_data_size as f64 / 1024.0 / 1024.0);

    let success_message = format!(
        "✅ FAST COPY restore completed for {} in {:.1}s - {:.1} MB restored",
        request.node_name,
        total_restore_time.as_secs_f64(),
        restored_data_size as f64 / 1024.0 / 1024.0
    );

    info!("{}", success_message);
    Ok(success_message)
}
