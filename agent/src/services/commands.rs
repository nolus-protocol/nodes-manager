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
    let command = format!(
        "cosmos-pruner prune '{}' --blocks={} --versions={} && echo 'PRUNING_SUCCESS'",
        deploy_path, keep_blocks, keep_versions
    );

    info!("Executing cosmos-pruner: {}", command);

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if stdout.contains("PRUNING_SUCCESS") {
        info!("Cosmos-pruner completed successfully");
        Ok(format!("Cosmos-pruner completed successfully\nOutput: {}", stdout.trim()))
    } else {
        Err(anyhow!("Cosmos-pruner failed or did not complete properly\nStdout: {}\nStderr: {}", stdout.trim(), stderr.trim()))
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

// NEW: Function to copy data and wasm directories from snapshot directory
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

pub async fn create_gzip_archive(source_dir: &str, target_file: &str, directories: &[&str]) -> Result<()> {
    let dirs = directories.join(" ");

    // Ensure parent directory exists
    if let Some(parent_dir) = std::path::Path::new(target_file).parent() {
        if let Some(parent_str) = parent_dir.to_str() {
            create_directory(parent_str).await?;
        }
    }

    // Use tar with -C flag to avoid cd command and complex shell logic
    let command = format!(
        "tar -czf '{}' -C '{}' {} && echo 'ARCHIVE_SUCCESS'",
        target_file, source_dir, dirs
    );

    info!("Creating gzip archive with tar -czf: {}", command);

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if stdout.contains("ARCHIVE_SUCCESS") {
        info!("Gzip archive created successfully");

        // Verify archive was created and has reasonable size
        match get_file_size(target_file).await {
            Ok(size) => {
                if size > 1024 {
                    info!("Archive verified: {} bytes", size);
                    Ok(())
                } else {
                    Err(anyhow!("Archive file too small ({} bytes), likely corrupt or empty", size))
                }
            },
            Err(e) => {
                Err(anyhow!("Archive file not found or inaccessible after creation: {}", e))
            }
        }
    } else {
        Err(anyhow!("Archive creation failed or did not complete properly\nStdout: {}\nStderr: {}", stdout.trim(), stderr.trim()))
    }
}

pub async fn extract_gzip_archive(archive_file: &str, target_dir: &str) -> Result<()> {
    // Verify archive file exists
    let verify_command = format!("test -f '{}'", archive_file);
    execute_shell_command(&verify_command).await
        .map_err(|_| anyhow!("Archive file does not exist: {}", archive_file))?;

    // Create target directory
    create_directory(target_dir).await?;

    // Use tar with -C flag to avoid cd command
    let command = format!(
        "tar -xzf '{}' -C '{}' && echo 'EXTRACT_SUCCESS'",
        archive_file, target_dir
    );

    info!("Extracting gzip archive with tar -xzf: {}", command);

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if stdout.contains("EXTRACT_SUCCESS") {
        info!("Gzip archive extracted successfully");

        // Verify extraction results
        let verify_command = format!("test -d '{}/data'", target_dir);
        match execute_shell_command(&verify_command).await {
            Ok(_) => {
                info!("Extraction verified: data directory found");
                Ok(())
            },
            Err(_) => {
                info!("Warning: data directory not found after extraction, but extraction reported success");
                Ok(())
            }
        }
    } else {
        Err(anyhow!("Archive extraction failed or did not complete properly\nStdout: {}\nStderr: {}", stdout.trim(), stderr.trim()))
    }
}

// Keep old function names for backward compatibility
pub async fn create_lz4_archive(source_dir: &str, target_file: &str, directories: &[&str]) -> Result<()> {
    create_gzip_archive(source_dir, target_file, directories).await
}

pub async fn extract_lz4_archive(archive_file: &str, target_dir: &str) -> Result<()> {
    extract_gzip_archive(archive_file, target_dir).await
}

// NEW: Function to copy directories to snapshot directory for snapshot creation
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

// NEW: Function to get directory size
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
