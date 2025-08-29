// File: agent/src/operations/snapshots.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, logs, systemctl};
use crate::types::{SnapshotInfo, SnapshotRequest};

pub async fn execute_full_snapshot_sequence(request: &SnapshotRequest) -> Result<SnapshotInfo> {
    info!("Starting FAST COPY snapshot for: {}", request.node_name);

    // Generate timestamp and paths
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let snapshot_dir_name = format!("{}_{}", request.node_name, timestamp);
    let snapshot_backup_dir = format!("{}/{}", request.backup_path, snapshot_dir_name);
    let validator_backup_path = format!("{}/priv_validator_state.json", snapshot_backup_dir);

    // Step 1: Create backup directory structure
    commands::create_directory(&snapshot_backup_dir).await?;
    info!("Created backup directory: {}", snapshot_backup_dir);

    // Step 2: Stop the node service
    info!("Stopping service: {}", request.service_name);
    systemctl::stop_service(&request.service_name).await?;

    // Step 3: Truncate logs (if configured)
    if let Some(log_path) = &request.log_path {
        info!("Truncating logs: {}", log_path);
        logs::truncate_log_path(log_path).await?;
    }

    // Step 4: Backup validator state FIRST (small file)
    let validator_source = format!("{}/data/priv_validator_state.json", request.deploy_path);
    commands::copy_file_if_exists(&validator_source, &validator_backup_path).await?;

    // Step 5: FAST COPY data and wasm directories (THE CORE CHANGE)
    let data_source = format!("{}/data", request.deploy_path);
    let data_backup = format!("{}/data", snapshot_backup_dir);
    let wasm_source = format!("{}/wasm", request.deploy_path);
    let wasm_backup = format!("{}/wasm", snapshot_backup_dir);

    info!("Starting FAST COPY of data directory...");
    let start_time = std::time::Instant::now();

    // Copy data directory (this is fast and reliable)
    commands::copy_directory(&data_source, &data_backup).await?;

    let data_copy_time = start_time.elapsed();
    info!("Data directory copied in {:.1}s", data_copy_time.as_secs_f64());

    // Copy wasm directory if it exists
    if commands::directory_exists(&wasm_source).await {
        info!("Copying wasm directory...");
        commands::copy_directory(&wasm_source, &wasm_backup).await?;
        info!("Wasm directory copied");
    } else {
        info!("No wasm directory found, skipping");
    }

    let total_copy_time = start_time.elapsed();
    info!("FAST COPY completed in {:.1}s", total_copy_time.as_secs_f64());

    // Step 6: Get backup directory size
    let backup_size_bytes = commands::get_directory_size(&snapshot_backup_dir).await.unwrap_or(0);
    info!("Backup directory size: {:.1} MB", backup_size_bytes as f64 / 1024.0 / 1024.0);

    // Step 7: Start the node service IMMEDIATELY
    info!("Starting service: {}", request.service_name);
    systemctl::start_service(&request.service_name).await?;

    // Step 8: Verify service is running
    let status = systemctl::get_service_status(&request.service_name).await?;
    if status != "active" {
        return Err(anyhow::anyhow!(
            "Service {} failed to start properly after snapshot (status: {})",
            request.service_name, status
        ));
    }

    info!("✅ Service {} restarted successfully - NODE IS NOW ONLINE", request.service_name);

    // Step 9: Create snapshot info for response (node is already back online)
    let snapshot_info = SnapshotInfo {
        filename: snapshot_dir_name.clone(),
        size_bytes: backup_size_bytes,
        path: snapshot_backup_dir.clone(),
    };

    // Step 10: BACKGROUND COMPRESSION (fire-and-forget, non-critical)
    let backup_dir_for_compression = snapshot_backup_dir.clone();
    let node_name_for_compression = request.node_name.clone();
    let backup_path_for_compression = request.backup_path.clone();

    tokio::spawn(async move {
        info!("🔄 Starting BACKGROUND compression for {}", node_name_for_compression);

        match create_background_compression(
            &backup_dir_for_compression,
            &backup_path_for_compression,
            &snapshot_dir_name,
        ).await {
            Ok(compressed_file) => {
                info!("✅ Background compression completed: {}", compressed_file);
            }
            Err(e) => {
                // This is non-critical - log and continue
                info!("ℹ️  Background compression failed (non-critical): {}", e);
            }
        }
    });

    info!("✅ FAST COPY snapshot completed for {} - returning success immediately", request.node_name);
    Ok(snapshot_info)
}

// Background compression task (non-critical, fire-and-forget)
async fn create_background_compression(
    source_dir: &str,
    backup_path: &str,
    snapshot_name: &str,
) -> Result<String> {
    let compressed_file = format!("{}/{}.tar.gz", backup_path, snapshot_name);

    // Create compressed version in background
    let command = format!(
        "tar -czf '{}' -C '{}' .",
        compressed_file, source_dir
    );

    info!("Background compression command: {}", command);

    // Use a longer timeout for background compression (it's not critical)
    let start_time = std::time::Instant::now();

    match commands::execute_shell_command(&command).await {
        Ok(_) => {
            let compression_time = start_time.elapsed();

            // Verify compressed file was created
            if std::path::Path::new(&compressed_file).exists() {
                if let Ok(compressed_size) = commands::get_file_size(&compressed_file).await {
                    info!("Background compression successful: {:.1} MB in {:.1}s",
                          compressed_size as f64 / 1024.0 / 1024.0,
                          compression_time.as_secs_f64());

                    // Optionally remove the uncompressed directory to save space
                    if let Err(e) = commands::delete_directory(source_dir).await {
                        info!("Failed to cleanup uncompressed directory (non-critical): {}", e);
                    } else {
                        info!("Cleaned up uncompressed directory to save space");
                    }

                    Ok(compressed_file)
                } else {
                    Err(anyhow::anyhow!("Compressed file exists but size check failed"))
                }
            } else {
                Err(anyhow::anyhow!("Compressed file was not created"))
            }
        }
        Err(e) => {
            info!("Background compression failed, keeping uncompressed backup: {}", e);
            Err(e)
        }
    }
}
