// File: agent/src/operations/pruning.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, logs, systemctl};
use crate::types::PruningRequest;

pub async fn execute_full_pruning_sequence(request: &PruningRequest) -> Result<String> {
    info!(
        "Starting FULL pruning sequence for service: {}",
        request.service_name
    );
    info!(
        "Deploy path: {}, keep_blocks: {}, keep_versions: {}",
        request.deploy_path, request.keep_blocks, request.keep_versions
    );

    let mut operation_log = Vec::new();

    // Step 1: Stop the node service
    info!("Step 1: Stopping service {}", request.service_name);
    systemctl::stop_service(&request.service_name).await?;
    operation_log.push(format!("✓ Stopped service: {}", request.service_name));

    // Step 2: Truncate logs (if configured)
    if let Some(log_path) = &request.log_path {
        info!("Step 2: Truncating logs at: {}", log_path);
        logs::truncate_log_path(log_path).await?;
        operation_log.push(format!("✓ Truncated logs: {}", log_path));
    } else {
        info!("Step 2: No log path configured, skipping log truncation");
        operation_log.push("• Skipped log truncation (not configured)".to_string());
    }

    // Step 3: Execute pruning
    info!("Step 3: Executing cosmos-pruner");
    let pruning_output = commands::execute_cosmos_pruner(
        &request.deploy_path,
        request.keep_blocks,
        request.keep_versions,
    )
    .await?;
    operation_log.push("✓ Completed cosmos-pruner execution".to_string());

    // Step 4: Start the node service
    info!("Step 4: Starting service {}", request.service_name);
    systemctl::start_service(&request.service_name).await?;
    operation_log.push(format!("✓ Started service: {}", request.service_name));

    // Step 5: Verify service is running
    let status = systemctl::get_service_status(&request.service_name).await?;
    if status != "active" {
        return Err(anyhow::anyhow!(
            "Service {} failed to start properly after pruning (status: {})",
            request.service_name,
            status
        ));
    }
    operation_log.push(format!("✓ Verified service is running: {}", status));

    info!(
        "Full pruning sequence completed successfully for: {}",
        request.service_name
    );

    // Return comprehensive operation summary
    let summary = format!(
        "=== PRUNING OPERATION COMPLETED ===\n\
        Service: {}\n\
        Deploy Path: {}\n\
        Blocks Kept: {}\n\
        Versions Kept: {}\n\
        \n\
        Operation Steps:\n\
        {}\n\
        \n\
        Pruning Output:\n\
        {}",
        request.service_name,
        request.deploy_path,
        request.keep_blocks,
        request.keep_versions,
        operation_log.join("\n"),
        pruning_output.trim()
    );

    Ok(summary)
}
