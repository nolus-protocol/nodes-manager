// File: agent/src/main.rs
use anyhow::Result;
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::Json as ResponseJson,
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::process::Command as AsyncCommand;
use tracing::{debug, error, info, warn};

// Request structures
#[derive(Debug, Deserialize)]
struct CommandRequest {
    command: String,
}

#[derive(Debug, Deserialize)]
struct ServiceRequest {
    service_name: String,
}

#[derive(Debug, Deserialize)]
struct LogTruncateRequest {
    log_path: String,
    service_name: String,
}

#[derive(Debug, Deserialize)]
struct PruningRequest {
    deploy_path: String,
    keep_blocks: u64,
    keep_versions: u64,
}

#[derive(Debug, Deserialize)]
struct SnapshotRequest {
    node_name: String,
    deploy_path: String,
    backup_path: String,
}

// Response structure
#[derive(Debug, Serialize)]
struct ApiResponse<T> {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    uptime_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compression: Option<String>,
}

impl<T> ApiResponse<T> {
    fn success_with_data(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            output: None,
            error: None,
            status: None,
            uptime_seconds: None,
            filename: None,
            size_bytes: None,
            path: None,
            compression: None,
        }
    }

    fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            output: None,
            error: Some(message),
            status: None,
            uptime_seconds: None,
            filename: None,
            size_bytes: None,
            path: None,
            compression: None,
        }
    }
}

impl ApiResponse<()> {
    fn success() -> Self {
        Self {
            success: true,
            data: None,
            output: None,
            error: None,
            status: None,
            uptime_seconds: None,
            filename: None,
            size_bytes: None,
            path: None,
            compression: None,
        }
    }

    fn success_with_output(output: String) -> Self {
        Self {
            success: true,
            data: None,
            output: Some(output),
            error: None,
            status: None,
            uptime_seconds: None,
            filename: None,
            size_bytes: None,
            path: None,
            compression: None,
        }
    }

    fn success_with_status(status: String) -> Self {
        Self {
            success: true,
            data: None,
            output: None,
            error: None,
            status: Some(status),
            uptime_seconds: None,
            filename: None,
            size_bytes: None,
            path: None,
            compression: None,
        }
    }

    fn success_with_uptime(uptime_seconds: u64) -> Self {
        Self {
            success: true,
            data: None,
            output: None,
            error: None,
            status: None,
            uptime_seconds: Some(uptime_seconds),
            filename: None,
            size_bytes: None,
            path: None,
            compression: None,
        }
    }

    fn success_with_snapshot(filename: String, size_bytes: u64, path: String) -> Self {
        Self {
            success: true,
            data: None,
            output: None,
            error: None,
            status: None,
            uptime_seconds: None,
            filename: Some(filename),
            size_bytes: Some(size_bytes),
            path: Some(path),
            compression: Some("lz4".to_string()),
        }
    }
}

// Application state
#[derive(Clone)]
struct AppState {
    api_key: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting Blockchain Server Agent on 0.0.0.0:8745");

    let api_key = std::env::var("AGENT_API_KEY")
        .unwrap_or_else(|_| "default-development-key".to_string());

    if api_key == "default-development-key" {
        warn!("Using default development API key - set AGENT_API_KEY environment variable for production");
    }

    let app_state = AppState { api_key };

    let app = Router::new()
        .route("/command/execute", post(execute_command))
        .route("/service/status", post(get_service_status))
        .route("/service/start", post(start_service))
        .route("/service/stop", post(stop_service))
        .route("/service/uptime", post(get_service_uptime))
        .route("/logs/truncate", post(truncate_logs))
        .route("/pruning/execute", post(execute_pruning))
        .route("/snapshot/create", post(create_snapshot))
        .with_state(Arc::new(app_state));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8745").await?;
    info!("Server agent listening on 0.0.0.0:8745");

    axum::serve(listener, app).await?;
    Ok(())
}

fn validate_api_key(headers: &axum::http::HeaderMap, expected_key: &str) -> bool {
    if let Some(auth_header) = headers.get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                return token == expected_key;
            }
        }
    }
    false
}

// === SERVICE OPERATIONS - Simple synchronous execution ===

async fn execute_command(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<CommandRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    debug!("Executing command: {}", request.command);

    let output = AsyncCommand::new("sh")
        .arg("-c")
        .arg(&request.command)
        .output()
        .await;

    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout).to_string();
            let stderr = String::from_utf8_lossy(&result.stderr).to_string();

            if result.status.success() {
                Ok(ResponseJson(ApiResponse::success_with_output(stdout)))
            } else {
                let error_msg = if !stderr.is_empty() { stderr } else { stdout };
                Ok(ResponseJson(ApiResponse::error(format!("Command failed: {}", error_msg))))
            }
        }
        Err(e) => Ok(ResponseJson(ApiResponse::error(format!("Failed to execute command: {}", e)))),
    }
}

async fn get_service_status(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<ServiceRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    debug!("Checking service status: {}", request.service_name);

    match AsyncCommand::new("systemctl")
        .arg("is-active")
        .arg(&request.service_name)
        .output()
        .await
    {
        Ok(result) => {
            let status = String::from_utf8_lossy(&result.stdout).trim().to_string();
            Ok(ResponseJson(ApiResponse::success_with_status(status)))
        }
        Err(e) => Ok(ResponseJson(ApiResponse::error(format!("Failed to check service status: {}", e)))),
    }
}

async fn start_service(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<ServiceRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    info!("Starting service: {}", request.service_name);

    match AsyncCommand::new("sudo")
        .arg("systemctl")
        .arg("start")
        .arg(&request.service_name)
        .output()
        .await
    {
        Ok(result) => {
            if result.status.success() {
                info!("Service {} started successfully", request.service_name);
                Ok(ResponseJson(ApiResponse::success()))
            } else {
                let error = String::from_utf8_lossy(&result.stderr);
                Ok(ResponseJson(ApiResponse::error(format!("Failed to start service: {}", error))))
            }
        }
        Err(e) => Ok(ResponseJson(ApiResponse::error(format!("Failed to start service: {}", e)))),
    }
}

async fn stop_service(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<ServiceRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    info!("Stopping service: {}", request.service_name);

    match AsyncCommand::new("sudo")
        .arg("systemctl")
        .arg("stop")
        .arg(&request.service_name)
        .output()
        .await
    {
        Ok(result) => {
            if result.status.success() {
                info!("Service {} stopped successfully", request.service_name);
                Ok(ResponseJson(ApiResponse::success()))
            } else {
                let error = String::from_utf8_lossy(&result.stderr);
                Ok(ResponseJson(ApiResponse::error(format!("Failed to stop service: {}", error))))
            }
        }
        Err(e) => Ok(ResponseJson(ApiResponse::error(format!("Failed to stop service: {}", e)))),
    }
}

async fn get_service_uptime(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<ServiceRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    debug!("Getting service uptime: {}", request.service_name);

    match AsyncCommand::new("systemctl")
        .arg("show")
        .arg(&request.service_name)
        .arg("--property=ActiveEnterTimestamp")
        .arg("--value")
        .output()
        .await
    {
        Ok(result) => {
            let timestamp_str = String::from_utf8_lossy(&result.stdout).trim().to_string();

            if timestamp_str.is_empty() || timestamp_str == "n/a" {
                Ok(ResponseJson(ApiResponse::success_with_uptime(0)))
            } else {
                // Parse timestamp and calculate uptime
                match AsyncCommand::new("date")
                    .arg("-d")
                    .arg(&timestamp_str)
                    .arg("+%s")
                    .output()
                    .await
                {
                    Ok(date_result) => {
                        let start_time_str = String::from_utf8_lossy(&date_result.stdout).trim().to_string();
                        if let Ok(start_time) = start_time_str.parse::<i64>() {
                            let now = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs() as i64;
                            let uptime_seconds = (now - start_time).max(0) as u64;
                            Ok(ResponseJson(ApiResponse::success_with_uptime(uptime_seconds)))
                        } else {
                            Ok(ResponseJson(ApiResponse::success_with_uptime(0)))
                        }
                    }
                    Err(_) => Ok(ResponseJson(ApiResponse::success_with_uptime(0))),
                }
            }
        }
        Err(e) => Ok(ResponseJson(ApiResponse::error(format!("Failed to get service uptime: {}", e)))),
    }
}

// === LONGER OPERATIONS - Execute synchronously until completion ===

async fn truncate_logs(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<LogTruncateRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    info!("Truncating logs at: {} for service: {}", request.log_path, request.service_name);

    // Step 1: Stop service
    info!("Stopping service: {}", request.service_name);
    let stop_result = AsyncCommand::new("sudo")
        .arg("systemctl")
        .arg("stop")
        .arg(&request.service_name)
        .output()
        .await;

    if let Err(e) = stop_result {
        return Ok(ResponseJson(ApiResponse::error(format!("Failed to stop service: {}", e))));
    }

    // Step 2: Truncate the log file
    info!("Truncating log file: {}", request.log_path);
    let truncate_result = AsyncCommand::new("sudo")
        .arg("truncate")
        .arg("-s")
        .arg("0")
        .arg(&request.log_path)
        .output()
        .await;

    if let Err(e) = truncate_result {
        // Try to start service even if truncate failed
        let _ = AsyncCommand::new("sudo")
            .arg("systemctl")
            .arg("start")
            .arg(&request.service_name)
            .output()
            .await;
        return Ok(ResponseJson(ApiResponse::error(format!("Failed to truncate logs: {}", e))));
    }

    // Step 3: Start service again
    info!("Starting service: {}", request.service_name);
    let start_result = AsyncCommand::new("sudo")
        .arg("systemctl")
        .arg("start")
        .arg(&request.service_name)
        .output()
        .await;

    match start_result {
        Ok(_) => {
            info!("Logs truncated successfully for {}", request.service_name);
            Ok(ResponseJson(ApiResponse::success()))
        }
        Err(e) => Ok(ResponseJson(ApiResponse::error(format!("Failed to restart service after log truncation: {}", e)))),
    }
}

async fn execute_pruning(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<PruningRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    info!("Starting pruning: deploy_path={}, keep_blocks={}, keep_versions={}",
          request.deploy_path, request.keep_blocks, request.keep_versions);

    // FIXED: Use correct cosmos-pruner command syntax
    let pruning_command = format!(
        "cosmos-pruner prune {} --blocks={} --versions={}",
        request.deploy_path, request.keep_blocks, request.keep_versions
    );

    info!("Executing pruning command: {}", pruning_command);

    match AsyncCommand::new("sh")
        .arg("-c")
        .arg(&pruning_command)
        .output()
        .await
    {
        Ok(result) => {
            if result.status.success() {
                let output = String::from_utf8_lossy(&result.stdout);
                info!("Pruning completed successfully");
                Ok(ResponseJson(ApiResponse::success_with_output(output.to_string())))
            } else {
                let error = String::from_utf8_lossy(&result.stderr);
                error!("Pruning failed: {}", error);
                Ok(ResponseJson(ApiResponse::error(format!("Pruning failed: {}", error))))
            }
        }
        Err(e) => {
            error!("Failed to execute pruning: {}", e);
            Ok(ResponseJson(ApiResponse::error(format!("Failed to execute pruning: {}", e))))
        }
    }
}

async fn create_snapshot(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<SnapshotRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    info!("Starting snapshot creation for {} - executing synchronously", request.node_name);
    info!("Deploy path: {}, Backup path: {}", request.deploy_path, request.backup_path);

    // Generate snapshot filename with timestamp
    let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
    let snapshot_filename = format!("{}_{}.tar.lz4", request.node_name, timestamp);
    let snapshot_path = format!("{}/{}", request.backup_path, snapshot_filename);

    // Step 1: Create backup directory if it doesn't exist
    info!("Creating backup directory: {}", request.backup_path);
    let mkdir_result = AsyncCommand::new("mkdir")
        .arg("-p")
        .arg(&request.backup_path)
        .output()
        .await;

    if let Err(e) = mkdir_result {
        return Ok(ResponseJson(ApiResponse::error(format!("Failed to create backup directory: {}", e))));
    }

    // Step 2: Backup validator state first (if it exists)
    let validator_backup_path = format!("{}/validator_state_backup_{}.json", request.backup_path, timestamp);
    let backup_validator_cmd = format!(
        "if [ -f '{}/data/priv_validator_state.json' ]; then cp '{}/data/priv_validator_state.json' '{}'; fi",
        request.deploy_path, request.deploy_path, validator_backup_path
    );

    info!("Backing up validator state");
    if let Err(e) = AsyncCommand::new("sh").arg("-c").arg(&backup_validator_cmd).output().await {
        warn!("Could not backup validator state: {}", e);
        // Continue anyway - this is not critical
    } else {
        info!("Validator state backed up to: {}", validator_backup_path);
    }

    // Step 3: Create LZ4-compressed snapshot - THIS IS THE LONG OPERATION
    info!("Creating LZ4-compressed snapshot (this will take a while - manager is waiting)");
    info!("Target file: {}", snapshot_path);

    let snapshot_cmd = format!(
        "cd {} && tar -cf - data wasm | lz4 -z -c > {}",
        request.deploy_path, snapshot_path
    );

    info!("Executing snapshot command: {}", snapshot_cmd);

    match AsyncCommand::new("sh")
        .arg("-c")
        .arg(&snapshot_cmd)
        .output()
        .await
    {
        Ok(result) => {
            if result.status.success() {
                info!("Snapshot archive creation completed");

                // Step 4: Get file size
                let size_cmd = format!("stat -c%s '{}'", snapshot_path);
                let size_bytes = match AsyncCommand::new("sh").arg("-c").arg(&size_cmd).output().await {
                    Ok(size_result) => {
                        if size_result.status.success() {
                            let size_str = String::from_utf8_lossy(&size_result.stdout).trim().to_string();
                            size_str.parse::<u64>().unwrap_or(0)
                        } else {
                            0
                        }
                    }
                    Err(_) => 0,
                };

                info!("Snapshot created successfully: {} ({} bytes)", snapshot_filename, size_bytes);
                info!("Full path: {}", snapshot_path);

                Ok(ResponseJson(ApiResponse::success_with_snapshot(
                    snapshot_filename,
                    size_bytes,
                    snapshot_path,
                )))
            } else {
                let error = String::from_utf8_lossy(&result.stderr);
                error!("Snapshot creation failed: {}", error);

                // Clean up failed snapshot file
                let _ = AsyncCommand::new("rm").arg("-f").arg(&snapshot_path).output().await;

                Ok(ResponseJson(ApiResponse::error(format!("Snapshot creation failed: {}", error))))
            }
        }
        Err(e) => {
            error!("Failed to create snapshot: {}", e);

            // Clean up failed snapshot file
            let _ = AsyncCommand::new("rm").arg("-f").arg(&snapshot_path).output().await;

            Ok(ResponseJson(ApiResponse::error(format!("Failed to create snapshot: {}", e))))
        }
    }
}
