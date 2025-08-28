// File: agent/src/services/commands.rs
use anyhow::{anyhow, Result};
use tokio::process::Command as AsyncCommand;
use tracing::{debug, info};

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
        "cosmos-pruner prune {} --blocks={} --versions={}",
        deploy_path, keep_blocks, keep_versions
    );

    debug!("Executing cosmos-pruner: {}", command);

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        Ok(stdout)
    } else {
        let error_msg = if !stderr.is_empty() { stderr } else { stdout };
        Err(anyhow!("Cosmos pruner failed: {}", error_msg))
    }
}

pub async fn create_directory(path: &str) -> Result<()> {
    let command = format!("mkdir -p '{}'", path);
    execute_shell_command(&command).await?;
    Ok(())
}

// NEW: Delete directory function for restore operations
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

pub async fn create_lz4_archive(source_dir: &str, target_file: &str, directories: &[&str]) -> Result<()> {
    let dirs = directories.join(" ");
    let command = format!(
        "cd '{}' && tar -cf - {} | lz4 -z -c > '{}'",
        source_dir, dirs, target_file
    );

    debug!("Creating LZ4 archive: {}", command);

    let output = execute_shell_command(&command).await?;
    debug!("Archive creation output: {}", output);
    Ok(())
}

// NEW: Extract LZ4 archive function for restore operations
pub async fn extract_lz4_archive(archive_file: &str, target_dir: &str) -> Result<()> {
    info!("Extracting LZ4 archive: {} to: {}", archive_file, target_dir);

    let command = format!(
        "cd '{}' && lz4 -dc '{}' | tar -xf -",
        target_dir, archive_file
    );

    debug!("Extraction command: {}", command);

    let output = execute_shell_command(&command).await?;
    debug!("Extraction output: {}", output);

    info!("LZ4 archive extracted successfully");
    Ok(())
}

// NEW: Find latest snapshot file
pub async fn find_latest_snapshot(backup_dir: &str, network: &str) -> Result<Option<String>> {
    let command = format!(
        "find '{}' -name '{}_*.lz4' -o -name '{}_*.tar.gz' | xargs -r stat -c '%Y %n' | sort -nr | head -1 | cut -d' ' -f2-",
        backup_dir, network, network
    );

    debug!("Finding latest snapshot: {}", command);

    let output = execute_shell_command(&command).await?;
    let latest_file = output.trim();

    if latest_file.is_empty() {
        Ok(None)
    } else {
        Ok(Some(latest_file.to_string()))
    }
}

// NEW: Find validator backup file for a given snapshot
pub async fn find_validator_backup_for_snapshot(backup_dir: &str, snapshot_filename: &str) -> Result<Option<String>> {
    // Extract timestamp from snapshot filename
    let timestamp = extract_timestamp_from_snapshot_filename(snapshot_filename)?;
    let validator_backup_path = format!("{}/validator_state_backup_{}.json", backup_dir, timestamp);

    // Check if the validator backup file exists
    let check_command = format!("test -f '{}'", validator_backup_path);

    match execute_shell_command(&check_command).await {
        Ok(_) => Ok(Some(validator_backup_path)),
        Err(_) => Ok(None),
    }
}

// Helper function to extract timestamp from snapshot filename
fn extract_timestamp_from_snapshot_filename(filename: &str) -> Result<String> {
    let basename = filename.split('/').last().unwrap_or(filename);

    // Handle different formats: network_YYYYMMDD_HHMMSS.lz4 or network_YYYYMMDD_HHMMSS.tar.gz
    if let Some(timestamp_part) = basename.split('_').nth(1) {
        if let Some(timestamp_with_ext) = basename.split('_').nth(2) {
            let timestamp = timestamp_with_ext
                .strip_suffix(".lz4")
                .or_else(|| timestamp_with_ext.strip_suffix(".tar.gz"))
                .unwrap_or(timestamp_with_ext);

            Ok(format!("{}_{}", timestamp_part, timestamp))
        } else {
            Err(anyhow!("Could not extract timestamp from snapshot filename: {}", filename))
        }
    } else {
        Err(anyhow!("Invalid snapshot filename format: {}", filename))
    }
}

// NEW: Check if file contains trigger words (for auto-restore)
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
