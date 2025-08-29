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
    // Ensure deploy path exists before starting
    let verify_path_cmd = format!("test -d '{}'", deploy_path);
    execute_shell_command(&verify_path_cmd).await
        .map_err(|_| anyhow!("Deploy path does not exist: {}", deploy_path))?;

    // Use explicit sync markers to ensure proper completion detection
    let command = format!(
        "set -e && cosmos-pruner prune '{}' --blocks={} --versions={} && echo 'PRUNING_SUCCESS'",
        deploy_path, keep_blocks, keep_versions
    );

    info!("Executing cosmos-pruner with sync verification: {}", command);

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() && stdout.contains("PRUNING_SUCCESS") {
        info!("Cosmos-pruner completed successfully and verified");
        Ok(format!("Cosmos-pruner completed successfully\nOutput: {}", stdout.trim()))
    } else {
        let error_msg = format!(
            "Cosmos-pruner failed - Exit code: {:?}, Success marker: {}\nStderr: {}\nStdout: {}",
            output.status.code(),
            stdout.contains("PRUNING_SUCCESS"),
            stderr.trim(),
            stdout.trim()
        );
        error!("{}", error_msg);
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
        "if [ -f '{}' ]; then cp '{}' '{}' && echo 'copied'; else echo 'not found'; fi",
        source, source, destination
    );

    let output = execute_shell_command(&command).await?;
    debug!("Copy result: {}", output.trim());
    Ok(())
}

pub async fn create_lz4_archive(source_dir: &str, target_file: &str, directories: &[&str]) -> Result<()> {
    let dirs = directories.join(" ");

    // Ensure parent directory exists
    if let Some(parent_dir) = std::path::Path::new(target_file).parent() {
        if let Some(parent_str) = parent_dir.to_str() {
            create_directory(parent_str).await?;
        }
    }

    // Verify source directories exist before starting
    for dir in directories {
        let verify_cmd = format!("test -d '{}/{}'", source_dir, dir);
        if execute_shell_command(&verify_cmd).await.is_err() {
            warn!("Source directory {}/{} does not exist, will be skipped", source_dir, dir);
        }
    }

    // Use explicit synchronization with error handling and success verification
    let command = format!(
        "set -e -o pipefail && cd '{}' && tar -cf - {} | lz4 -z -c > '{}' && echo 'ARCHIVE_SUCCESS'",
        source_dir, dirs, target_file
    );

    info!("Creating LZ4 archive with sync verification: cd '{}' && tar -cf - {} | lz4 -z -c > '{}'",
          source_dir, dirs, target_file);

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() && stdout.contains("ARCHIVE_SUCCESS") {
        info!("LZ4 archive created successfully and verified");

        // Double-check the archive was created and has reasonable size
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
        let error_msg = format!(
            "LZ4 archive creation failed - Exit code: {:?}, Success marker: {}\nStderr: {}\nStdout: {}",
            output.status.code(),
            stdout.contains("ARCHIVE_SUCCESS"),
            stderr.trim(),
            stdout.trim()
        );
        error!("{}", error_msg);
        Err(anyhow!(error_msg))
    }
}

pub async fn extract_lz4_archive(archive_file: &str, target_dir: &str) -> Result<()> {
    // Verify archive exists and is readable
    let verify_command = format!("test -r '{}'", archive_file);
    execute_shell_command(&verify_command).await
        .map_err(|_| anyhow!("Archive file does not exist or is not readable: {}", archive_file))?;

    // Get archive size for verification
    let archive_size = get_file_size(archive_file).await?;
    if archive_size < 1024 {
        return Err(anyhow!("Archive file too small ({} bytes), likely corrupt", archive_size));
    }

    create_directory(target_dir).await?;

    // Use explicit synchronization with error handling and success verification
    let command = format!(
        "set -e -o pipefail && cd '{}' && lz4 -dc '{}' | tar -xf - && echo 'EXTRACT_SUCCESS'",
        target_dir, archive_file
    );

    info!("Extracting LZ4 archive with sync verification: cd '{}' && lz4 -dc '{}' | tar -xf -",
          target_dir, archive_file);

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if output.status.success() && stdout.contains("EXTRACT_SUCCESS") {
        info!("LZ4 archive extracted successfully and verified");

        // Verify extraction by checking for expected content
        let verify_command = format!("find '{}' -type f | head -1", target_dir);
        match execute_shell_command(&verify_command).await {
            Ok(output) if !output.trim().is_empty() => {
                info!("Extraction verified: files found in target directory");
                Ok(())
            },
            _ => {
                warn!("Warning: extraction completed but no files found - archive may have been empty");
                Ok(()) // Don't fail - empty archives are technically valid
            }
        }
    } else {
        let error_msg = format!(
            "LZ4 archive extraction failed - Exit code: {:?}, Success marker: {}\nStderr: {}\nStdout: {}",
            output.status.code(),
            stdout.contains("EXTRACT_SUCCESS"),
            stderr.trim(),
            stdout.trim()
        );
        error!("{}", error_msg);
        Err(anyhow!(error_msg))
    }
}

pub async fn check_log_for_trigger_words(log_file: &str, trigger_words: &[String]) -> Result<bool> {
    if trigger_words.is_empty() {
        return Ok(false);
    }

    // Verify log file exists and is readable
    let verify_command = format!("test -r '{}'", log_file);
    execute_shell_command(&verify_command).await
        .map_err(|_| anyhow!("Log file does not exist or is not readable: {}", log_file))?;

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
