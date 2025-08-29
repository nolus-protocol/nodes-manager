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
        "if cosmos-pruner prune '{}' --blocks={} --versions={}; then echo 'PRUNING_SUCCESS'; else echo 'PRUNING_FAILED'; fi",
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
    } else if stdout.contains("PRUNING_FAILED") {
        Err(anyhow!("Cosmos-pruner failed\nStderr: {}", stderr))
    } else {
        Err(anyhow!("Pruning operation completed with unknown status\nStdout: {}\nStderr: {}", stdout, stderr))
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

pub async fn create_gzip_archive(source_dir: &str, target_file: &str, directories: &[&str]) -> Result<()> {
    let dirs = directories.join(" ");

    if let Some(parent_dir) = std::path::Path::new(target_file).parent() {
        if let Some(parent_str) = parent_dir.to_str() {
            create_directory(parent_str).await?;
        }
    }

    // Use tar with built-in gzip compression and explicit error handling
    let command = format!(
        "(cd '{}' || (echo 'ARCHIVE_CD_FAIL'; exit 1)) && ((tar -czf '{}' {} && echo 'ARCHIVE_SUCCESS') || echo 'ARCHIVE_FAILED')",
        source_dir, target_file, dirs
    );

    info!("Creating gzip archive: {}", command);

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if stdout.contains("ARCHIVE_SUCCESS") {
        info!("Archive created successfully");
        Ok(())
    } else if stdout.contains("ARCHIVE_CD_FAIL") {
        Err(anyhow!("Failed to change to source directory: {}", source_dir))
    } else if stdout.contains("ARCHIVE_FAILED") {
        Err(anyhow!("Archive creation failed\nStderr: {}", stderr))
    } else {
        Err(anyhow!("Archive operation completed with unknown status\nStdout: {}\nStderr: {}", stdout, stderr))
    }
}

pub async fn extract_gzip_archive(archive_file: &str, target_dir: &str) -> Result<()> {
    create_directory(target_dir).await?;

    // Use tar with built-in gzip decompression and explicit error handling
    let command = format!(
        "(cd '{}' || (echo 'EXTRACT_CD_FAIL'; exit 1)) && ((tar -xzf '{}' && echo 'EXTRACT_SUCCESS') || echo 'EXTRACT_FAILED')",
        target_dir, archive_file
    );

    info!("Extracting gzip archive: {}", command);

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if stdout.contains("EXTRACT_SUCCESS") {
        info!("Archive extracted successfully");
        Ok(())
    } else if stdout.contains("EXTRACT_CD_FAIL") {
        Err(anyhow!("Failed to change to target directory: {}", target_dir))
    } else if stdout.contains("EXTRACT_FAILED") {
        Err(anyhow!("Archive extraction failed\nStderr: {}", stderr))
    } else {
        Err(anyhow!("Extract operation completed with unknown status\nStdout: {}\nStderr: {}", stdout, stderr))
    }
}

// Keep old function names for backward compatibility
pub async fn create_lz4_archive(source_dir: &str, target_file: &str, directories: &[&str]) -> Result<()> {
    create_gzip_archive(source_dir, target_file, directories).await
}

pub async fn extract_lz4_archive(archive_file: &str, target_dir: &str) -> Result<()> {
    extract_gzip_archive(archive_file, target_dir).await
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
