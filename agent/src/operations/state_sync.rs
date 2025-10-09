// File: agent/src/operations/state_sync.rs
use anyhow::Result;
use tracing::info;

use crate::services::{commands, config_editor, logs, systemctl};
use crate::types::StateSyncRequest;

pub async fn execute_state_sync_sequence(request: &StateSyncRequest) -> Result<String> {
    info!(
        "🔄 Starting state sync sequence for service: {}",
        request.service_name
    );
    info!("RPC servers: {:?}", request.rpc_servers);
    info!(
        "Trust height: {}, hash: {}",
        request.trust_height, request.trust_hash
    );

    let mut operation_log = Vec::new();

    // Step 1: Stop the node service - FAIL FAST
    info!("Step 1: Stopping service {}", request.service_name);
    systemctl::stop_service(&request.service_name).await?;
    operation_log.push(format!("✓ Stopped service: {}", request.service_name));

    // Step 2: Truncate logs (if configured) - FAIL FAST
    if let Some(log_path) = &request.log_path {
        info!("Step 2: Truncating logs at: {}", log_path);
        logs::truncate_log_path(log_path).await?;
        operation_log.push(format!("✓ Truncated logs: {}", log_path));
    } else {
        info!("Step 2: No log path configured, skipping log truncation");
        operation_log.push("• Skipped log truncation (not configured)".to_string());
    }

    // Step 3: Update config.toml with state sync parameters - FAIL FAST
    info!("Step 3: Updating config.toml with state sync parameters");
    config_editor::enable_state_sync(
        &request.config_path,
        &request.rpc_servers,
        request.trust_height,
        &request.trust_hash,
    )
    .await?;
    operation_log.push("✓ Updated config.toml with state sync parameters".to_string());

    // Step 4: Execute unsafe-reset-all - FAIL FAST
    info!("Step 4: Executing unsafe-reset-all");
    let reset_cmd = format!(
        "{} tendermint unsafe-reset-all --home {} --keep-addr-book",
        request.daemon_binary, request.home_dir
    );
    commands::execute_shell_command(&reset_cmd).await?;
    operation_log.push("✓ Chain state reset (unsafe-reset-all)".to_string());

    // Step 5: Clean WASM cache (Strategy A: preserve blobs, delete cache only) - FAIL FAST
    info!("Step 5: Cleaning WASM cache");
    let wasm_cache = format!("{}/wasm/cache", request.home_dir);

    // Check if wasm/cache exists before trying to delete
    let cache_exists_cmd = format!("test -d '{}'", wasm_cache);
    if commands::execute_shell_command(&cache_exists_cmd)
        .await
        .is_ok()
    {
        let remove_cache_cmd = format!("rm -rf '{}'", wasm_cache);
        commands::execute_shell_command(&remove_cache_cmd).await?;
        operation_log.push("✓ WASM cache cleaned".to_string());
    } else {
        operation_log.push("• WASM cache not found, skipping".to_string());
    }

    // Step 6: Start the node service - FAIL FAST
    info!("Step 6: Starting service {}", request.service_name);
    systemctl::start_service(&request.service_name).await?;
    operation_log.push(format!("✓ Started service: {}", request.service_name));

    // Step 7: Wait for state sync to complete - TIMEOUT = FAIL
    info!(
        "Step 7: Waiting for state sync to complete (timeout: {}s)",
        request.timeout_seconds
    );
    wait_for_sync_completion(
        &request.daemon_binary,
        &request.home_dir,
        request.timeout_seconds,
    )
    .await?;
    operation_log.push("✓ State sync completed".to_string());

    // Step 8: Disable state sync in config - FAIL FAST
    info!("Step 8: Disabling state sync in config");
    config_editor::disable_state_sync(&request.config_path).await?;
    operation_log.push("✓ State sync disabled in config".to_string());

    // Step 9: Restart service to apply config changes
    info!("Step 9: Restarting service to apply config");
    systemctl::stop_service(&request.service_name).await?;
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    systemctl::start_service(&request.service_name).await?;
    operation_log.push("✓ Service restarted with state sync disabled".to_string());

    // Step 10: Verify service is running
    let status = systemctl::get_service_status(&request.service_name).await?;
    if status != "active" {
        return Err(anyhow::anyhow!(
            "Service {} failed to start properly after state sync (status: {})",
            request.service_name,
            status
        ));
    }
    operation_log.push(format!("✓ Verified service is running: {}", status));

    info!(
        "✓ State sync sequence completed successfully for: {}",
        request.service_name
    );

    // Return comprehensive operation summary
    let summary = format!(
        "=== STATE SYNC OPERATION COMPLETED ===\n\
        Service: {}\n\
        Home Dir: {}\n\
        Trust Height: {}\n\
        Trust Hash: {}\n\
        RPC Servers: {:?}\n\
        \n\
        Operation Steps:\n\
        {}\n",
        request.service_name,
        request.home_dir,
        request.trust_height,
        request.trust_hash,
        request.rpc_servers,
        operation_log.join("\n"),
    );

    Ok(summary)
}

/// Wait for state sync to complete by monitoring sync status - FAIL ON TIMEOUT
async fn wait_for_sync_completion(
    daemon_binary: &str,
    home_dir: &str,
    timeout_seconds: u64,
) -> Result<()> {
    use tokio::time::{sleep, timeout, Duration};

    let check_status_cmd = format!(
        "{} status --home {} 2>&1 | grep -o '\"catching_up\":[^,]*' | cut -d':' -f2",
        daemon_binary, home_dir
    );

    info!(
        "Monitoring sync status with timeout of {}s",
        timeout_seconds
    );

    let sync_future = async {
        let mut check_count = 0;
        loop {
            check_count += 1;

            // Wait 10 seconds between checks
            sleep(Duration::from_secs(10)).await;

            info!("Checking sync status (check #{})", check_count);

            match commands::execute_shell_command(&check_status_cmd).await {
                Ok(output) => {
                    let catching_up = output.trim();
                    info!("Sync status: catching_up = {}", catching_up);

                    if catching_up == "false" {
                        info!("✓ Node finished syncing!");
                        return Ok::<(), anyhow::Error>(());
                    } else {
                        info!("Node still syncing (catching_up = true)...");
                    }
                }
                Err(e) => {
                    // Node might not be ready yet, continue waiting
                    info!("Status check failed (node might still be starting): {}", e);
                }
            }
        }
    };

    // Apply timeout - FAIL FAST if timeout exceeded
    match timeout(Duration::from_secs(timeout_seconds), sync_future).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(anyhow::anyhow!(
            "State sync timeout after {}s - node did not complete syncing",
            timeout_seconds
        )),
    }
}
