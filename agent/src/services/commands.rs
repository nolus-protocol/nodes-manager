// File: agent/src/services/commands.rs
use anyhow::{anyhow, Result};
use tokio::process::Command as AsyncCommand;
use std::process::Stdio;
use tracing::{debug, info, warn};

// ========================================================================
// WARNING: DO NOT MODIFY THE LZ4 FUNCTIONS BELOW (create_lz4_archive, extract_lz4_archive)
//
// These functions use the working approach:
// - Pipeline commands (tar | lz4)
// - .status() instead of .output()
// - Stdio::null() for all streams
//
// This combination works reliably. Any changes to this approach will cause
// workflow hanging issues. If modifications are needed, test thoroughly
// on a separate branch first.
// ========================================================================

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

// ========================================================================
// FIXED: Apply same working approach as LZ4 functions
// Use .status() + Stdio::null() to prevent stream capture blocking
// ========================================================================
pub async fn execute_cosmos_pruner(deploy_path: &str, keep_blocks: u64, keep_versions: u64) -> Result<String> {
    let command = format!(
        "cosmos-pruner prune {} --blocks={} --versions={}",
        deploy_path, keep_blocks, keep_versions
    );

    info!("Executing cosmos-pruner: {}", command);

    // FIXED: Use same approach as working LZ4 functions
    let status = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()  // Use .status() NOT .output()
        .await?;

    if status.success() {
        info!("Cosmos-pruner completed successfully");
        Ok("Cosmos-pruner completed successfully".to_string())
    } else {
        let error_msg = "Cosmos-pruner execution failed";
        Err(anyhow!(error_msg))
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
        "if [ -f '{}' ]; then cp '{}' '{}'; echo 'copied'; else echo 'not found'; fi",
        source, source, destination
    );

    let output = execute_shell_command(&command).await?;
    debug!("Copy result: {}", output.trim());
    Ok(())
}

// ========================================================================
// WORKING LZ4 ARCHIVE FUNCTION - DO NOT MODIFY
// Uses pipeline approach with .status() + Stdio::null() - this works!
// ========================================================================
pub async fn create_lz4_archive(source_dir: &str, target_file: &str, directories: &[&str]) -> Result<()> {
    let dirs = directories.join(" ");

    // Create the target directory if it doesn't exist
    if let Some(parent_dir) = std::path::Path::new(target_file).parent() {
        if let Some(parent_str) = parent_dir.to_str() {
            create_directory(parent_str).await?;
        }
    }

    let command = format!(
        "cd '{}' && tar -cf - {} | lz4 -z -c > '{}'",
        source_dir, dirs, target_file
    );

    info!("Creating LZ4 archive: cd '{}' && tar -cf - {} | lz4 -z -c > '{}'",
          source_dir, dirs, target_file);

    // CRITICAL: This approach works - do not change to .output()
    let status = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()  // Use .status() NOT .output()
        .await?;

    if status.success() {
        info!("LZ4 archive created successfully: {}", target_file);

        // Verify the file was actually created and has reasonable size
        match get_file_size(target_file).await {
            Ok(size) => {
                if size > 1024 { // At least 1KB
                    info!("Archive verified: {} bytes", size);
                    Ok(())
                } else {
                    Err(anyhow!("Archive file too small ({} bytes), likely corrupt", size))
                }
            },
            Err(e) => {
                Err(anyhow!("Archive file not found or inaccessible after creation: {}", e))
            }
        }
    } else {
        Err(anyhow!("LZ4 archive creation failed"))
    }
}

// ========================================================================
// WORKING LZ4 EXTRACT FUNCTION - DO NOT MODIFY
// Uses pipeline approach with .status() + Stdio::null() - this works!
// ========================================================================
pub async fn extract_lz4_archive(archive_file: &str, target_dir: &str) -> Result<()> {
    // Verify archive file exists first
    let verify_command = format!("test -f '{}'", archive_file);
    execute_shell_command(&verify_command).await
        .map_err(|_| anyhow!("Archive file does not exist: {}", archive_file))?;

    // Create target directory
    create_directory(target_dir).await?;

    let command = format!(
        "cd '{}' && lz4 -dc '{}' | tar -xf -",
        target_dir, archive_file
    );

    info!("Extracting LZ4 archive: cd '{}' && lz4 -dc '{}' | tar -xf -",
          target_dir, archive_file);

    // CRITICAL: This approach works - do not change to .output()
    let status = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()  // Use .status() NOT .output()
        .await?;

    if status.success() {
        info!("LZ4 archive extracted successfully to: {}", target_dir);

        // Verify extraction by checking if data directory exists
        let verify_command = format!("test -d '{}/data'", target_dir);
        match execute_shell_command(&verify_command).await {
            Ok(_) => {
                info!("Extraction verified: data directory found");
                Ok(())
            },
            Err(_) => {
                info!("Warning: data directory not found after extraction, but extraction reported success");
                Ok(()) // Still consider it successful since the main command succeeded
            }
        }
    } else {
        Err(anyhow!("LZ4 archive extraction failed"))
    }
}

// ========================================================================
// END OF CRITICAL WORKING FUNCTIONS
// ========================================================================

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

    // grep returns 0 if found, 1 if not found
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
