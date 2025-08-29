// File: agent/src/services/commands.rs
use anyhow::{anyhow, Result};
use tokio::process::Command as AsyncCommand;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{debug, info, warn};

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

    info!("Executing cosmos-pruner: {}", command);

    // Use the simple .output() approach for cosmos-pruner to avoid pipe issues
    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await
        .map_err(|e| anyhow!("Failed to execute cosmos-pruner: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Log the output line by line
    for line in stdout.lines() {
        if !line.trim().is_empty() {
            info!("cosmos-pruner stdout: {}", line);
        }
    }
    for line in stderr.lines() {
        if !line.trim().is_empty() {
            info!("cosmos-pruner stderr: {}", line);
        }
    }

    info!("Cosmos-pruner completed with status: {:?}", output.status);

    if output.status.success() {
        info!("Cosmos-pruner execution completed successfully");
        Ok("Cosmos-pruner completed successfully".to_string())
    } else {
        let error_msg = format!("Cosmos-pruner failed with exit code: {:?}", output.status.code());
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

pub async fn create_lz4_archive(source_dir: &str, target_file: &str, directories: &[&str]) -> Result<()> {
    let dirs = directories.join(" ");

    // Simplified command without complex piping - let tar and lz4 run to completion
    let command = format!(
        "cd '{}' && tar -cf - {} | lz4 -z -c > '{}'",
        source_dir, dirs, target_file
    );

    info!("Creating LZ4 archive: {}", command);
    info!("Source directory: {}", source_dir);
    info!("Target file: {}", target_file);
    info!("Directories to archive: {:?}", directories);

    // Use .output() to avoid pipe handling issues
    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await
        .map_err(|e| anyhow!("Failed to execute LZ4 archive creation: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Log any output
    for line in stdout.lines() {
        if !line.trim().is_empty() {
            info!("lz4-archive stdout: {}", line);
        }
    }
    for line in stderr.lines() {
        if !line.trim().is_empty() {
            info!("lz4-archive stderr: {}", line);
        }
    }

    info!("LZ4 archive creation completed with status: {:?}", output.status);

    if output.status.success() {
        info!("LZ4 archive creation completed successfully: {}", target_file);
        Ok(())
    } else {
        let error_msg = format!("LZ4 archive creation failed with exit code: {:?}", output.status.code());
        Err(anyhow!(error_msg))
    }
}

pub async fn extract_lz4_archive(archive_file: &str, target_dir: &str) -> Result<()> {
    // Simplified command without complex piping
    let command = format!(
        "cd '{}' && lz4 -dc '{}' | tar -xf -",
        target_dir, archive_file
    );

    info!("Extracting LZ4 archive: {}", command);
    info!("Archive file: {}", archive_file);
    info!("Target directory: {}", target_dir);

    // Use .output() to avoid pipe handling issues
    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await
        .map_err(|e| anyhow!("Failed to execute LZ4 archive extraction: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Log any output
    for line in stdout.lines() {
        if !line.trim().is_empty() {
            info!("lz4-extract stdout: {}", line);
        }
    }
    for line in stderr.lines() {
        if !line.trim().is_empty() {
            info!("lz4-extract stderr: {}", line);
        }
    }

    info!("LZ4 archive extraction completed with status: {:?}", output.status);

    if output.status.success() {
        info!("LZ4 archive extraction completed successfully to: {}", target_dir);
        Ok(())
    } else {
        let error_msg = format!("LZ4 archive extraction failed with exit code: {:?}", output.status.code());
        Err(anyhow!(error_msg))
    }
}

// Keep this function for monitoring with progress updates during long operations
pub async fn create_lz4_archive_with_progress_monitoring(source_dir: &str, target_file: &str, directories: &[&str]) -> Result<()> {
    let dirs = directories.join(" ");

    // Create a script file to monitor progress
    let script_content = format!(
        r#"#!/bin/bash
cd '{}'
echo "Starting LZ4 compression..."
tar -cf - {} | lz4 -z -c > '{}'
echo "LZ4 compression completed with exit code: $?"
"#,
        source_dir, dirs, target_file
    );

    let script_path = format!("/tmp/lz4_archive_{}.sh", std::process::id());
    tokio::fs::write(&script_path, script_content).await?;

    let chmod_result = AsyncCommand::new("chmod")
        .arg("+x")
        .arg(&script_path)
        .output()
        .await?;

    if !chmod_result.status.success() {
        let _ = tokio::fs::remove_file(&script_path).await;
        return Err(anyhow!("Failed to make script executable"));
    }

    info!("Creating LZ4 archive with progress monitoring");
    info!("Source directory: {}", source_dir);
    info!("Target file: {}", target_file);
    info!("Directories to archive: {:?}", directories);

    // Execute the script and monitor progress
    let mut child = AsyncCommand::new("bash")
        .arg(&script_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn LZ4 script: {}", e))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    // Monitor output without blocking child.wait()
    let stdout_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            info!("lz4-script: {}", line);
        }
    });

    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            info!("lz4-script-err: {}", line);
        }
    });

    // Wait for the script to complete
    let status = child.wait().await.map_err(|e| anyhow!("Failed to wait for LZ4 script: {}", e))?;

    // Clean up
    let _ = tokio::fs::remove_file(&script_path).await;
    let _ = stdout_handle.await;
    let _ = stderr_handle.await;

    info!("LZ4 script completed with status: {:?}", status);

    if status.success() {
        info!("LZ4 archive creation completed successfully: {}", target_file);
        Ok(())
    } else {
        let error_msg = format!("LZ4 archive creation failed with exit code: {:?}", status.code());
        Err(anyhow!(error_msg))
    }
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
