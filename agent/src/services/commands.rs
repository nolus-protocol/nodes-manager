// File: agent/src/services/commands.rs
use anyhow::{anyhow, Result};
use tokio::process::Command as AsyncCommand;
use tokio::io::{AsyncBufReadExt, BufReader};
use std::process::Stdio;
use std::time::Duration;
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

    // Spawn cosmos-pruner with proper stream handling
    let mut command = AsyncCommand::new("cosmos-pruner");
    command
        .arg("prune")
        .arg(deploy_path)
        .arg("--blocks")
        .arg(keep_blocks.to_string())
        .arg("--versions")
        .arg(keep_versions.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true); // Ensure cleanup

    info!("Executing cosmos-pruner process with stream monitoring...");

    let mut child = command.spawn()
        .map_err(|e| anyhow!("Failed to spawn cosmos-pruner: {}", e))?;

    // Take streams for proper draining
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // Spawn task to continuously drain stdout
    let stdout_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        while let Ok(bytes_read) = reader.read_line(&mut line).await {
            if bytes_read == 0 { break; }
            info!("cosmos-pruner stdout: {}", line.trim());
            line.clear();
        }
    });

    // Spawn task to continuously drain stderr
    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        while let Ok(bytes_read) = reader.read_line(&mut line).await {
            if bytes_read == 0 { break; }
            info!("cosmos-pruner stderr: {}", line.trim());
            line.clear();
        }
    });

    // Monitor process completion with better detection
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                info!("cosmos-pruner process detected as completed");
                break status;
            }
            Ok(None) => {
                // Process still running, log progress and continue
                debug!("cosmos-pruner still running...");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Err(e) => {
                return Err(anyhow!("Error checking cosmos-pruner status: {}", e));
            }
        }
    };

    // Wait for stream tasks to complete
    let _ = tokio::try_join!(stdout_handle, stderr_handle);

    let exit_code = status.code().unwrap_or(-1);
    let success = status.success();

    info!("cosmos-pruner process completed with exit code: {} (success: {})", exit_code, success);

    // IMPORTANT: Always return success regardless of exit code
    // The workflow must continue no matter what cosmos-pruner returns
    Ok(format!("cosmos-pruner completed with exit code: {} (success: {})", exit_code, success))
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

// NEW - Remove file if it exists (for cleaning validator state from snapshots)
pub async fn remove_file_if_exists(file_path: &str) -> Result<()> {
    let command = format!(
        "if [ -f '{}' ]; then rm '{}' && echo 'removed'; else echo 'not found'; fi",
        file_path, file_path
    );

    let output = execute_shell_command(&command).await?;
    debug!("Remove result for {}: {}", file_path, output.trim());
    Ok(())
}

// NEW - Backup current validator state before restore
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

// NEW - Restore current validator state after snapshot restore
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

// Monitor copy progress by checking destination size
async fn monitor_copy_progress(source_path: &str, target_path: &str, operation_name: &str) {
    let mut last_size = 0u64;
    let mut stall_count = 0;

    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;

        // Check if target exists yet
        let target_exists_cmd = format!("test -d '{}'", target_path);
        if execute_shell_command(&target_exists_cmd).await.is_err() {
            debug!("{}: Target directory not created yet", operation_name);
            continue;
        }

        let source_size = get_directory_size(source_path).await.unwrap_or(0);
        let current_size = get_directory_size(target_path).await.unwrap_or(0);

        if source_size > 0 {
            let progress = (current_size * 100) / source_size;
            info!("{}: {}% complete ({:.1} MB / {:.1} MB)",
                  operation_name,
                  progress,
                  current_size as f64 / 1024.0 / 1024.0,
                  source_size as f64 / 1024.0 / 1024.0);

            // Check for completion
            if current_size >= source_size {
                info!("{}: Copy operation completed", operation_name);
                break;
            }

            // Check for stalled progress
            if current_size == last_size {
                stall_count += 1;
                if stall_count > 10 { // 5 minutes of no progress
                    warn!("{}: No progress detected for 5 minutes - operation may be stalled", operation_name);
                    stall_count = 0; // Reset to avoid spam
                }
            } else {
                stall_count = 0;
            }
            last_size = current_size;
        }
    }
}

// Copy function that REQUIRES both data and wasm directories for restore with progress monitoring
pub async fn copy_snapshot_directories_mandatory(snapshot_dir: &str, target_dir: &str) -> Result<()> {
    info!("Copying of both data and wasm directories from {} to {}", snapshot_dir, target_dir);

    // Copy data directory with rsync and progress monitoring
    let data_source = format!("{}/data", snapshot_dir);
    let data_target = format!("{}/data", target_dir);

    info!("Starting data directory copy with progress monitoring...");

    // Start progress monitoring in background
    let data_source_clone = data_source.clone();
    let data_target_clone = data_target.clone();
    let data_monitor = tokio::spawn(async move {
        monitor_copy_progress(&data_source_clone, &data_target_clone, "Data copy").await;
    });

    let data_copy_cmd = format!(
        "if [ -d '{}' ]; then rsync -av '{}/' '{}' && echo 'data_copied'; else echo 'data_not_found'; fi",
        data_source, data_source, data_target
    );

    let data_result = execute_shell_command(&data_copy_cmd).await?;
    data_monitor.abort(); // Stop monitoring

    if !data_result.contains("data_copied") {
        return Err(anyhow!("CRITICAL: Failed to copy data directory from snapshot {}", data_source));
    }
    info!("Data directory copied successfully");

    // Copy wasm directory with rsync and progress monitoring
    let wasm_source = format!("{}/wasm", snapshot_dir);
    let wasm_target = format!("{}/wasm", target_dir);

    info!("Starting wasm directory copy with progress monitoring...");

    // Start progress monitoring in background
    let wasm_source_clone = wasm_source.clone();
    let wasm_target_clone = wasm_target.clone();
    let wasm_monitor = tokio::spawn(async move {
        monitor_copy_progress(&wasm_source_clone, &wasm_target_clone, "Wasm copy").await;
    });

    let wasm_copy_cmd = format!(
        "if [ -d '{}' ]; then rsync -av '{}/' '{}' && echo 'wasm_copied'; else echo 'wasm_not_found'; fi",
        wasm_source, wasm_source, wasm_target
    );

    let wasm_result = execute_shell_command(&wasm_copy_cmd).await?;
    wasm_monitor.abort(); // Stop monitoring

    if !wasm_result.contains("wasm_copied") {
        return Err(anyhow!("CRITICAL: Failed to copy wasm directory from snapshot {}", wasm_source));
    }
    info!("Wasm directory copied successfully");

    info!("Copy completed - both data and wasm directories copied successfully");
    Ok(())
}

// NEW: copy function for snapshot creation that REQUIRES both data and wasm directories with progress monitoring
pub async fn copy_directories_to_snapshot_mandatory(source_dir: &str, snapshot_dir: &str, directories: &[&str]) -> Result<()> {
    info!("Copying directories {:?} from {} to snapshot {}", directories, source_dir, snapshot_dir);

    for dir in directories {
        let source_path = format!("{}/{}", source_dir, dir);
        let target_path = format!("{}/{}", snapshot_dir, dir);

        // Check if source directory exists BEFORE copying
        let source_exists_cmd = format!("test -d '{}'", source_path);
        execute_shell_command(&source_exists_cmd).await
            .map_err(|_| anyhow!("CRITICAL: Source {} directory missing at: {}", dir, source_path))?;

        info!("Starting {} directory copy with progress monitoring...", dir);

        // Start progress monitoring in background
        let source_clone = source_path.clone();
        let target_clone = target_path.clone();
        let dir_name = dir.to_string();
        let monitor_handle = tokio::spawn(async move {
            monitor_copy_progress(&source_clone, &target_clone, &format!("{} copy", dir_name)).await;
        });

        // Use rsync instead of cp for better progress and reliability
        let copy_cmd = format!(
            "rsync -av '{}/' '{}'",
            source_path, target_path
        );

        execute_shell_command(&copy_cmd).await
            .map_err(|e| anyhow!("CRITICAL: Failed to copy {} directory from {} to {}: {}", dir, source_path, target_path, e))?;

        // Stop monitoring
        monitor_handle.abort();

        // Verify the copy was successful
        let target_exists_cmd = format!("test -d '{}'", target_path);
        execute_shell_command(&target_exists_cmd).await
            .map_err(|_| anyhow!("CRITICAL: {} directory not found after copy at: {}", dir, target_path))?;

        info!("Successfully copied {} directory to snapshot", dir);
    }

    info!("Directory copying to snapshot completed successfully");
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
            info!("Auto-restore trigger words found in log: {}", log_file);
            Ok(true)
        }
        Err(_) => {
            debug!("No trigger words found in log: {}", log_file);
            Ok(false)
        }
    }
}
