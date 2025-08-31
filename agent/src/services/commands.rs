// File: agent/src/services/commands.rs
use anyhow::{anyhow, Result};
use tokio::process::Command as AsyncCommand;
use tracing::{debug, info, warn, error};

pub async fn execute_shell_command(command: &str) -> Result<String> {
    debug!("Executing command: {}", command);

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        let error_msg = if !stderr.is_empty() { stderr } else { stdout };
        Err(anyhow!("Command failed: {}", error_msg))
    }
}

pub async fn execute_cosmos_pruner(deploy_path: &str, keep_blocks: u64, keep_versions: u64) -> Result<String> {
    info!("Starting cosmos-pruner: prune '{}' --blocks={} --versions={}", deploy_path, keep_blocks, keep_versions);

    // Use direct command execution - no shell wrapper
    let mut command = AsyncCommand::new("cosmos-pruner");
    command
        .arg("prune")
        .arg(deploy_path)
        .arg("--blocks")
        .arg(keep_blocks.to_string())
        .arg("--versions")
        .arg(keep_versions.to_string());

    info!("Executing cosmos-pruner process - waiting for completion...");

    let output = command.output().await?;

    // GUARANTEED: Every process has an exit code - capture it immediately
    let exit_code = output.status.code().unwrap_or(-1);
    let success = output.status.success();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // LOG: Always log completion and exit code for debugging
    info!("cosmos-pruner process completed with exit code: {} (success: {})", exit_code, success);
    info!("cosmos-pruner stdout length: {} bytes", stdout.len());
    if !stderr.is_empty() {
        info!("cosmos-pruner stderr: {}", stderr.trim());
    }

    // CONTINUE WORKFLOW: Regardless of exit code, return appropriate result
    if success {
        info!("cosmos-pruner completed successfully (exit code 0)");
        Ok(format!("cosmos-pruner completed successfully (exit code: {})\nOutput: {}", exit_code, stdout.trim()))
    } else {
        // Process completed with non-zero exit code - this is still completion!
        error!("cosmos-pruner failed with exit code: {}", exit_code);
        Err(anyhow!(
            "cosmos-pruner failed (exit code: {})\nStdout: {}\nStderr: {}",
            exit_code, stdout.trim(), stderr.trim()
        ))
    }
}

// NEW: LZ4 compression function for background execution
pub async fn create_lz4_compressed_snapshot(backup_path: &str, snapshot_dirname: &str) {
    let lz4_filename = format!("{}.tar.lz4", snapshot_dirname);
    let lz4_path = format!("{}/{}", backup_path, lz4_filename);

    let command = format!(
        "tar -cf - -C '{}' '{}' | lz4 -z -c > '{}'",
        backup_path, snapshot_dirname, lz4_path
    );

    info!("Starting background LZ4 compression: {}", lz4_filename);
    debug!("LZ4 command: {}", command);

    match AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await
    {
        Ok(output) => {
            let exit_code = output.status.code().unwrap_or(-1);
            let success = output.status.success();

            if success {
                info!("Background LZ4 compression completed successfully: {} (exit code: {})", lz4_filename, exit_code);
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("Background LZ4 compression failed: {} (exit code: {}, stderr: {})", lz4_filename, exit_code, stderr.trim());
            }
        }
        Err(e) => {
            error!("Background LZ4 compression error: {} - {}", lz4_filename, e);
        }
    }
}

pub async fn create_directory(path: &str) -> Result<()> {
    let command = format!("mkdir -p '{}'", path);
    execute_shell_command(&command).await?;
    Ok(())
}

pub async fn delete_directory(path: &str) -> Result<()> {
    info!("Deleting directory: {}", path);
    let command = format!("rm -rf '{}'", path);
    execute_shell_command(&command).await?;
    info!("Directory deleted successfully: {}", path);
    Ok(())
}

pub async fn get_file_size(file_path: &str) -> Result<u64> {
    let command = format!("stat -c%s '{}'", file_path);
    let output = execute_shell_command(&command).await?;

    output.trim().parse::<u64>()
        .map_err(|e| anyhow!("Failed to parse file size: {}", e))
}

pub async fn copy_file_if_exists(source: &str, destination: &str) -> Result<()> {
    let command = format!(
        "if [ -f '{}' ]; then cp '{}' '{}' && echo 'copied'; else echo 'not found'; fi",
        source, source, destination
    );

    let output = execute_shell_command(&command).await?;
    debug!("Copy result: {}", output.trim());
    Ok(())
}

// FIXED: NEW - Remove file if it exists (for cleaning validator state from snapshots)
pub async fn remove_file_if_exists(file_path: &str) -> Result<()> {
    let command = format!(
        "if [ -f '{}' ]; then rm '{}' && echo 'removed'; else echo 'not found'; fi",
        file_path, file_path
    );

    let output = execute_shell_command(&command).await?;
    debug!("Remove result for {}: {}", file_path, output.trim());
    Ok(())
}

// FIXED: NEW - Backup current validator state before restore
pub async fn backup_current_validator_state(source: &str, backup_path: &str) -> Result<()> {
    info!("Backing up current validator state from {} to {}", source, backup_path);

    let command = format!(
        "if [ -f '{}' ]; then cp '{}' '{}' && echo 'validator_state_backed_up'; else echo 'validator_state_not_found'; fi",
        source, source, backup_path
    );

    let output = execute_shell_command(&command).await?;
    if output.contains("validator_state_backed_up") {
        info!("Current validator state backed up successfully");
    } else {
        info!("No current validator state found - will create default after restore");
    }
    Ok(())
}

// FIXED: NEW - Restore current validator state after snapshot restore
pub async fn restore_current_validator_state(backup_path: &str, destination: &str) -> Result<()> {
    info!("Restoring current validator state from {} to {}", backup_path, destination);

    let command = format!(
        "if [ -f '{}' ]; then cp '{}' '{}' && echo 'validator_state_restored'; else echo 'validator_backup_not_found'; fi",
        backup_path, backup_path, destination
    );

    let output = execute_shell_command(&command).await?;
    if output.contains("validator_state_restored") {
        info!("Current validator state restored successfully - signing state preserved");
    } else {
        warn!("No validator state backup found - node will start with default validator state");
    }
    Ok(())
}

pub async fn copy_snapshot_directories(snapshot_dir: &str, target_dir: &str) -> Result<()> {
    info!("Copying snapshot directories from {} to {}", snapshot_dir, target_dir);

    // Copy data directory
    let data_copy_cmd = format!(
        "if [ -d '{}/data' ]; then cp -r '{}/data' '{}/' && echo 'data_copied'; else echo 'data_not_found'; fi",
        snapshot_dir, snapshot_dir, target_dir
    );

    let data_result = execute_shell_command(&data_copy_cmd).await?;
    if !data_result.contains("data_copied") {
        return Err(anyhow!("Failed to copy data directory from snapshot"));
    }

    // Copy wasm directory (if exists)
    let wasm_copy_cmd = format!(
        "if [ -d '{}/wasm' ]; then cp -r '{}/wasm' '{}/' && echo 'wasm_copied'; else echo 'wasm_not_found'; fi",
        snapshot_dir, snapshot_dir, target_dir
    );

    let wasm_result = execute_shell_command(&wasm_copy_cmd).await?;
    debug!("Wasm copy result: {}", wasm_result.trim());

    info!("Snapshot directories copied successfully");
    Ok(())
}

pub async fn copy_directories_to_snapshot(source_dir: &str, snapshot_dir: &str, directories: &[&str]) -> Result<()> {
    info!("Copying directories {:?} from {} to snapshot {}", directories, source_dir, snapshot_dir);

    for dir in directories {
        let source_path = format!("{}/{}", source_dir, dir);
        let target_path = format!("{}/{}", snapshot_dir, dir);

        let copy_cmd = format!(
            "if [ -d '{}' ]; then cp -r '{}' '{}' && echo '{}_copied'; else echo '{}_not_found'; fi",
            source_path, source_path, target_path, dir, dir
        );

        let result = execute_shell_command(&copy_cmd).await?;
        if result.contains(&format!("{}_copied", dir)) {
            info!("Successfully copied {} directory to snapshot", dir);
        } else {
            warn!("Directory {} not found in source, skipping", dir);
        }
    }

    info!("Directory copying to snapshot completed");
    Ok(())
}

pub async fn get_directory_size(dir_path: &str) -> Result<u64> {
    let command = format!("du -sb '{}' | cut -f1", dir_path);
    let output = execute_shell_command(&command).await?;

    output.trim().parse::<u64>()
        .map_err(|e| anyhow!("Failed to parse directory size: {}", e))
}

pub async fn check_log_for_trigger_words(log_file: &str, trigger_words: &[String]) -> Result<bool> {
    if trigger_words.is_empty() {
        return Ok(false);
    }

    let pattern = trigger_words.join("|");
    let command = format!(
        "tail -n 1000 '{}' | grep -q -E '{}'",
        log_file, pattern
    );

    debug!("Checking log for trigger words: {}", command);

    match execute_shell_command(&command).await {
        Ok(_) => {
            info!("Trigger words found in log file: {}", log_file);
            Ok(true)
        }
        Err(_) => {
            debug!("No trigger words found in log file: {}", log_file);
            Ok(false)
        }
    }
}
