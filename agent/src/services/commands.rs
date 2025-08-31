// File: agent/src/services/commands.rs
use anyhow::{anyhow, Result};
use tokio::process::Command as AsyncCommand;
use tokio::io::{AsyncReadExt, BufReader, AsyncBufReadExt};
use std::process::Stdio;
use tracing::{debug, info, warn, error};

pub async fn execute_shell_command(command: &str) -> Result<String> {
    debug!("Executing shell command with manual process management: {}", command);

    // FIXED: Use manual process management to avoid tokio signal handling bugs
    let mut child = AsyncCommand::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn command: {}", e))?;

    // Take stdout and stderr for manual reading
    let stdout = child.stdout.take()
        .ok_or_else(|| anyhow!("Failed to capture stdout"))?;
    let stderr = child.stderr.take()
        .ok_or_else(|| anyhow!("Failed to capture stderr"))?;

    // Read streams concurrently
    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut output = String::new();
        let mut buffer = Vec::new();

        match reader.read_to_end(&mut buffer).await {
            Ok(_) => String::from_utf8_lossy(&buffer).to_string(),
            Err(_) => output,
        }
    });

    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut error_output = String::new();
        let mut buffer = Vec::new();

        match reader.read_to_end(&mut buffer).await {
            Ok(_) => String::from_utf8_lossy(&buffer).to_string(),
            Err(_) => error_output,
        }
    });

    // Wait for process completion using polling
    let exit_status = wait_for_process_with_polling(&mut child).await?;

    // Collect output
    let stdout_result = stdout_task.await
        .map_err(|e| anyhow!("Failed to read stdout: {}", e))?;
    let stderr_result = stderr_task.await
        .map_err(|e| anyhow!("Failed to read stderr: {}", e))?;

    if exit_status.success() {
        Ok(stdout_result)
    } else {
        let error_msg = if !stderr_result.is_empty() {
            stderr_result
        } else {
            stdout_result
        };
        Err(anyhow!("Command failed: {}", error_msg))
    }
}

pub async fn execute_cosmos_pruner(deploy_path: &str, keep_blocks: u64, keep_versions: u64) -> Result<String> {
    info!("Starting cosmos-pruner with manual process management: prune '{}' --blocks={} --versions={}", deploy_path, keep_blocks, keep_versions);

    // FIXED: Use manual process management to avoid tokio signal handling bugs
    let mut command = AsyncCommand::new("cosmos-pruner");
    command
        .arg("prune")
        .arg(deploy_path)
        .arg("--blocks")
        .arg(keep_blocks.to_string())
        .arg("--versions")
        .arg(keep_versions.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    info!("Spawning cosmos-pruner process manually...");

    // Step 1: Spawn the process (not .output().await which hangs)
    let mut child = command.spawn()
        .map_err(|e| anyhow!("Failed to spawn cosmos-pruner: {}", e))?;

    // Step 2: Take stdout and stderr for manual reading
    let stdout = child.stdout.take()
        .ok_or_else(|| anyhow!("Failed to capture stdout"))?;
    let stderr = child.stderr.take()
        .ok_or_else(|| anyhow!("Failed to capture stderr"))?;

    // Step 3: Read streams concurrently while process runs
    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout);
        let mut output = String::new();
        let mut buffer = String::new();

        while let Ok(bytes_read) = reader.read_line(&mut buffer).await {
            if bytes_read == 0 { break; } // EOF
            output.push_str(&buffer);
            // Log progress lines for debugging
            if buffer.contains("Pruning") || buffer.contains("Deleted") || buffer.contains("blocks") {
                info!("cosmos-pruner progress: {}", buffer.trim());
            }
            buffer.clear();
        }
        output
    });

    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr);
        let mut error_output = String::new();
        let mut buffer = String::new();

        while let Ok(bytes_read) = reader.read_line(&mut buffer).await {
            if bytes_read == 0 { break; } // EOF
            error_output.push_str(&buffer);
            // Log error lines immediately
            if !buffer.trim().is_empty() {
                warn!("cosmos-pruner stderr: {}", buffer.trim());
            }
            buffer.clear();
        }
        error_output
    });

    // Step 4: Wait for process completion using polling (avoids signal handling bugs)
    let exit_status = wait_for_process_with_polling(&mut child).await?;

    // Step 5: Collect all output
    let stdout_result = stdout_task.await
        .map_err(|e| anyhow!("Failed to read stdout: {}", e))?;
    let stderr_result = stderr_task.await
        .map_err(|e| anyhow!("Failed to read stderr: {}", e))?;

    let exit_code = exit_status.code().unwrap_or(-1);
    let success = exit_status.success();

    info!("cosmos-pruner completed with exit code: {} (success: {})", exit_code, success);
    info!("cosmos-pruner stdout length: {} bytes", stdout_result.len());

    if !stderr_result.is_empty() {
        info!("cosmos-pruner stderr: {}", stderr_result.trim());
    }

    if success {
        info!("cosmos-pruner completed successfully");
        Ok(format!("cosmos-pruner completed successfully (exit code: {})\nOutput: {}", exit_code, stdout_result.trim()))
    } else {
        error!("cosmos-pruner failed with exit code: {}", exit_code);
        Err(anyhow!(
            "cosmos-pruner failed (exit code: {})\nStdout: {}\nStderr: {}",
            exit_code, stdout_result.trim(), stderr_result.trim()
        ))
    }
}

/// Robust process waiting using polling to avoid tokio signal handling bugs
async fn wait_for_process_with_polling(child: &mut tokio::process::Child) -> Result<std::process::ExitStatus> {
    info!("Waiting for process completion using polling method...");

    let mut poll_count = 0;

    loop {
        // Use try_wait() instead of wait().await to avoid signal handling bugs
        match child.try_wait() {
            Ok(Some(status)) => {
                info!("Process completed after {} polls ({} seconds)", poll_count, poll_count);
                return Ok(status);
            }
            Ok(None) => {
                // Process still running, continue polling
                poll_count += 1;

                // Log progress every 5 minutes (300 polls)
                if poll_count % 300 == 0 {
                    info!("Process still running... ({} minutes elapsed)", poll_count / 60);
                }

                // Yield and wait 1 second before next poll
                tokio::task::yield_now().await;
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
            Err(e) => {
                error!("Error checking process status: {}", e);
                return Err(anyhow!("Failed to check process status: {}", e));
            }
        }
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
