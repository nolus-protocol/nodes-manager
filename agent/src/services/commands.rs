// File: agent/src/services/commands.rs
use anyhow::{anyhow, Result};
use tokio::process::Command as AsyncCommand;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
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

    info!("Executing cosmos-pruner: {}", command);

    let mut child = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn cosmos-pruner: {}", e))?;

    info!("Cosmos-pruner process started, reading output...");

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    tokio::spawn(async move {
        while let Ok(Some(line)) = stdout_reader.next_line().await {
            info!("cosmos-pruner stdout: {}", line);
        }
    });

    tokio::spawn(async move {
        while let Ok(Some(line)) = stderr_reader.next_line().await {
            info!("cosmos-pruner stderr: {}", line);
        }
    });

    let status = child.wait().await
        .map_err(|e| anyhow!("Failed to wait for cosmos-pruner: {}", e))?;

    info!("Cosmos-pruner completed with status: {:?}", status);

    if status.success() {
        info!("Cosmos-pruner execution completed successfully");
        Ok("Cosmos-pruner completed successfully".to_string())
    } else {
        let error_msg = format!("Cosmos-pruner failed with exit code: {:?}", status.code());
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
    let command = format!(
        "cd '{}' && tar -cf - {} | lz4 -z -c > '{}'",
        source_dir, dirs, target_file
    );

    info!("Creating LZ4 archive with streaming: {}", command);
    info!("Source directory: {}", source_dir);
    info!("Target file: {}", target_file);
    info!("Directories to archive: {:?}", directories);

    let mut child = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn LZ4 archive creation: {}", e))?;

    info!("LZ4 archive creation process started, monitoring progress...");

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    tokio::spawn(async move {
        while let Ok(Some(line)) = stdout_reader.next_line().await {
            info!("lz4-archive stdout: {}", line);
        }
    });

    tokio::spawn(async move {
        while let Ok(Some(line)) = stderr_reader.next_line().await {
            info!("lz4-archive stderr: {}", line);
        }
    });

    let status = child.wait().await
        .map_err(|e| anyhow!("Failed to wait for LZ4 archive creation: {}", e))?;

    info!("LZ4 archive creation completed with status: {:?}", status);

    if status.success() {
        info!("LZ4 archive creation completed successfully: {}", target_file);
        Ok(())
    } else {
        let error_msg = format!("LZ4 archive creation failed with exit code: {:?}", status.code());
        Err(anyhow!(error_msg))
    }
}

pub async fn extract_lz4_archive(archive_file: &str, target_dir: &str) -> Result<()> {
    let command = format!(
        "cd '{}' && lz4 -dc '{}' | tar -xf -",
        target_dir, archive_file
    );

    info!("Extracting LZ4 archive with streaming: {}", command);
    info!("Archive file: {}", archive_file);
    info!("Target directory: {}", target_dir);

    let mut child = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow!("Failed to spawn LZ4 archive extraction: {}", e))?;

    info!("LZ4 archive extraction process started, monitoring progress...");

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    tokio::spawn(async move {
        while let Ok(Some(line)) = stdout_reader.next_line().await {
            info!("lz4-extract stdout: {}", line);
        }
    });

    tokio::spawn(async move {
        while let Ok(Some(line)) = stderr_reader.next_line().await {
            info!("lz4-extract stderr: {}", line);
        }
    });

    let status = child.wait().await
        .map_err(|e| anyhow!("Failed to wait for LZ4 archive extraction: {}", e))?;

    info!("LZ4 archive extraction completed with status: {:?}", status);

    if status.success() {
        info!("LZ4 archive extraction completed successfully to: {}", target_dir);
        Ok(())
    } else {
        let error_msg = format!("LZ4 archive extraction failed with exit code: {:?}", status.code());
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
