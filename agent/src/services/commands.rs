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
    let data_path = format!("{}/data", deploy_path);
    let command = format!(
        "cosmos-pruner prune '{}' --blocks={} --versions={}",
        data_path, keep_blocks, keep_versions
    );

    info!("Executing cosmos-pruner: {}", command);

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() {
        info!("Cosmos-pruner completed successfully");
        Ok(format!("Cosmos-pruner completed successfully\nOutput: {}", stdout.trim()))
    } else {
        error!("Cosmos-pruner failed - stdout: {}, stderr: {}", stdout.trim(), stderr.trim());
        Err(anyhow!("Cosmos-pruner failed\nStdout: {}\nStderr: {}", stdout.trim(), stderr.trim()))
    }
}

pub async fn create_directory(path: &str) -> Result<()> {
    let command = format!("mkdir -p '{}'", path);
    execute_shell_command(&command).await?;
    info!("Created directory: {}", path);
    Ok(())
}

pub async fn delete_directory(path: &str) -> Result<()> {
    info!("Deleting directory: {}", path);
    let command = format!("rm -rf '{}'", path);
    execute_shell_command(&command).await?;
    info!("Directory deleted successfully: {}", path);
    Ok(())
}

// NEW: Check if directory exists
pub async fn directory_exists(path: &str) -> bool {
    let command = format!("test -d '{}'", path);
    execute_shell_command(&command).await.is_ok()
}

// NEW: Fast directory copy (much more reliable than tar)
pub async fn copy_directory(source: &str, destination: &str) -> Result<()> {
    info!("Copying directory: {} -> {}", source, destination);

    // Ensure source exists
    if !directory_exists(source).await {
        return Err(anyhow!("Source directory does not exist: {}", source));
    }

    // Create parent directory of destination
    if let Some(parent) = std::path::Path::new(destination).parent() {
        if let Some(parent_str) = parent.to_str() {
            create_directory(parent_str).await?;
        }
    }

    // Use cp -r for reliable recursive copy
    let command = format!("cp -r '{}' '{}'", source, destination);

    info!("Executing copy command: {}", command);
    let start_time = std::time::Instant::now();

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await?;

    let copy_time = start_time.elapsed();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Directory copy failed: {}", stderr));
    }

    // Verify copy was successful
    if !directory_exists(destination).await {
        return Err(anyhow!("Copy operation completed but destination directory not found"));
    }

    info!("Directory copy completed in {:.1}s: {}", copy_time.as_secs_f64(), destination);
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

// LEGACY: Keep old gzip functions for backward compatibility (but they're not recommended)
pub async fn create_gzip_archive(source_dir: &str, target_file: &str, directories: &[&str]) -> Result<()> {
    warn!("⚠️  Using legacy gzip archive creation - consider using fast copy method instead");

    let dirs = directories.join(" ");

    info!("Creating gzip archive: source_dir={}, target_file={}, dirs={}", source_dir, target_file, dirs);

    // Ensure parent directory exists
    if let Some(parent_dir) = std::path::Path::new(target_file).parent() {
        if let Some(parent_str) = parent_dir.to_str() {
            create_directory(parent_str).await?;
        }
    }

    // Simplified tar command (removed the hanging && chain)
    let command = format!(
        "tar -czf '{}' -C '{}' {}",
        target_file, source_dir, dirs
    );

    info!("Executing tar command: {}", command);
    let start_time = std::time::Instant::now();

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await?;

    let execution_time = start_time.elapsed();
    info!("Tar command completed in {:.1}s", execution_time.as_secs_f64());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        error!("Tar command failed - stdout: {}, stderr: {}", stdout.trim(), stderr.trim());
        return Err(anyhow!("Archive creation failed\nStdout: {}\nStderr: {}", stdout.trim(), stderr.trim()));
    }

    // Verify archive was created
    if !std::path::Path::new(target_file).exists() {
        return Err(anyhow!("Archive file was not created: {}", target_file));
    }

    // Check archive size
    match get_file_size(target_file).await {
        Ok(size) => {
            if size < 1024 {
                return Err(anyhow!("Archive file too small ({} bytes), likely corrupt or empty", size));
            }
            info!("Archive verified successfully: {} ({:.1} MB)", target_file, size as f64 / 1024.0 / 1024.0);
        },
        Err(e) => {
            return Err(anyhow!("Failed to verify archive file: {}", e));
        }
    }

    info!("Gzip archive creation completed successfully: {}", target_file);
    Ok(())
}

pub async fn extract_gzip_archive(archive_file: &str, target_dir: &str) -> Result<()> {
    warn!("⚠️  Using legacy gzip archive extraction - consider using fast copy method instead");

    info!("Extracting gzip archive: {} to {}", archive_file, target_dir);

    // Verify archive file exists
    if !std::path::Path::new(archive_file).exists() {
        return Err(anyhow!("Archive file does not exist: {}", archive_file));
    }

    // Get archive size for logging
    let archive_size = get_file_size(archive_file).await?;
    info!("Archive size: {:.1} MB", archive_size as f64 / 1024.0 / 1024.0);

    // Create target directory
    create_directory(target_dir).await?;

    // Simplified extraction command
    let command = format!(
        "tar -xzf '{}' -C '{}'",
        archive_file, target_dir
    );

    info!("Executing extraction command: {}", command);
    let start_time = std::time::Instant::now();

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await?;

    let execution_time = start_time.elapsed();
    info!("Extraction completed in {:.1}s", execution_time.as_secs_f64());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        error!("Extraction failed - stdout: {}, stderr: {}", stdout.trim(), stderr.trim());
        return Err(anyhow!("Archive extraction failed\nStdout: {}\nStderr: {}", stdout.trim(), stderr.trim()));
    }

    // Verify extraction results
    info!("Extraction command completed, verifying results...");

    let verify_data_cmd = format!("test -d '{}/data'", target_dir);
    let data_exists = execute_shell_command(&verify_data_cmd).await.is_ok();

    info!("Extraction verification: data_dir={}", data_exists);

    if !data_exists {
        return Err(anyhow!("Data directory not found after extraction: {}/data", target_dir));
    }

    info!("Gzip archive extracted successfully to: {}", target_dir);
    Ok(())
}

// Keep old function names for backward compatibility
pub async fn create_lz4_archive(source_dir: &str, target_file: &str, directories: &[&str]) -> Result<()> {
    warn!("create_lz4_archive called - redirecting to create_gzip_archive");
    create_gzip_archive(source_dir, target_file, directories).await
}

pub async fn extract_lz4_archive(archive_file: &str, target_dir: &str) -> Result<()> {
    warn!("extract_lz4_archive called - redirecting to extract_gzip_archive");
    extract_gzip_archive(archive_file, target_dir).await
}

pub async fn check_log_for_trigger_words(log_file: &str, trigger_words: &[String]) -> Result<bool> {
    if trigger_words.is_empty() {
        return Ok(false);
    }

    // Check if log file exists first
    let file_check = format!("test -f '{}'", log_file);
    if execute_shell_command(&file_check).await.is_err() {
        warn!("Log file does not exist: {}", log_file);
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

// Get directory size (useful for monitoring disk usage)
pub async fn get_directory_size(dir_path: &str) -> Result<u64> {
    let command = format!("du -sb '{}' | cut -f1", dir_path);
    let output = execute_shell_command(&command).await?;

    output.trim().parse::<u64>()
        .map_err(|e| anyhow!("Failed to parse directory size: {}", e))
}

// Check available disk space
pub async fn get_available_disk_space(path: &str) -> Result<u64> {
    let command = format!("df '{}' | tail -1 | awk '{{print $4}}'", path);
    let output = execute_shell_command(&command).await?;

    // df returns available space in KB, convert to bytes
    let kb = output.trim().parse::<u64>()
        .map_err(|e| anyhow!("Failed to parse disk space: {}", e))?;

    Ok(kb * 1024)
}
