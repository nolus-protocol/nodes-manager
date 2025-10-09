// File: agent/src/main.rs
use anyhow::Result;
use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    response::Json as ResponseJson,
    routing::{get, post},
    Router,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

mod operations;
mod services;
mod types;

use operations::{pruning, restore, snapshots, state_sync};
use services::{commands, job_manager::JobManager, systemctl};
use types::*;

#[derive(Clone)]
pub struct AppState {
    pub api_key: String,
    pub busy_nodes: Arc<RwLock<HashMap<String, BusyState>>>,
    pub job_manager: JobManager,
}

#[derive(Clone, Debug)]
pub struct BusyState {
    operation_type: String,
    started_at: chrono::DateTime<chrono::Utc>,
}

impl AppState {
    async fn try_start_operation(
        &self,
        node_name: &str,
        operation_type: &str,
    ) -> Result<(), String> {
        let mut busy = self.busy_nodes.write().await;

        if let Some(existing) = busy.get(node_name) {
            let duration = chrono::Utc::now().signed_duration_since(existing.started_at);
            return Err(format!(
                "Node {} is busy with {} (started {}m ago)",
                node_name,
                existing.operation_type,
                duration.num_minutes()
            ));
        }

        busy.insert(
            node_name.to_string(),
            BusyState {
                operation_type: operation_type.to_string(),
                started_at: chrono::Utc::now(),
            },
        );

        info!("Node {} marked as busy with {}", node_name, operation_type);
        Ok(())
    }

    async fn finish_operation(&self, node_name: &str) {
        let mut busy = self.busy_nodes.write().await;
        if let Some(state) = busy.remove(node_name) {
            let duration = chrono::Utc::now().signed_duration_since(state.started_at);
            info!(
                "Node {} operation {} completed after {}m",
                node_name,
                state.operation_type,
                duration.num_minutes()
            );
        }
    }

    async fn cleanup_old_operations(&self, max_hours: i64) -> u32 {
        let mut busy = self.busy_nodes.write().await;
        let cutoff = chrono::Utc::now() - chrono::Duration::hours(max_hours);
        let initial_count = busy.len();

        busy.retain(|node_name, state| {
            let should_keep = state.started_at > cutoff;
            if !should_keep {
                warn!(
                    "Cleaned up stuck operation on {} ({})",
                    node_name, state.operation_type
                );
            }
            should_keep
        });

        let cleaned = initial_count - busy.len();
        if cleaned > 0 {
            warn!(
                "Cleaned up {} stuck operations older than {}h",
                cleaned, max_hours
            );
        }
        cleaned as u32
    }

    async fn get_busy_status(&self) -> HashMap<String, String> {
        let busy = self.busy_nodes.read().await;
        busy.iter()
            .map(|(node, state)| (node.clone(), state.operation_type.clone()))
            .collect()
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting Blockchain Server Agent on 0.0.0.0:8745 with async job support");

    let api_key =
        std::env::var("AGENT_API_KEY").unwrap_or_else(|_| "default-development-key".to_string());

    if api_key == "default-development-key" {
        warn!("Using default development API key - set AGENT_API_KEY environment variable for production");
    }

    let job_manager = JobManager::new();
    let app_state = AppState {
        api_key,
        busy_nodes: Arc::new(RwLock::new(HashMap::new())),
        job_manager: job_manager.clone(),
    };

    let cleanup_state = app_state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            cleanup_state.cleanup_old_operations(24).await;
            cleanup_state.job_manager.cleanup_old_jobs(48).await;
        }
    });

    let app = Router::new()
        .route("/command/execute", post(execute_command))
        .route("/service/status", post(get_service_status))
        .route("/service/start", post(start_service))
        .route("/service/stop", post(stop_service))
        .route("/service/uptime", post(get_service_uptime))
        .route("/logs/truncate", post(truncate_logs))
        .route("/logs/delete-all", post(delete_all_files_in_directory))
        .route("/pruning/execute", post(execute_pruning_async))
        .route("/snapshot/create", post(create_snapshot_async))
        .route("/snapshot/restore", post(restore_snapshot_async))
        .route("/snapshot/check-triggers", post(check_restore_triggers))
        .route("/state-sync/execute", post(execute_state_sync_async)) // NEW: State sync endpoint
        .route("/operation/status/:job_id", get(get_job_status))
        .route("/status/busy", post(get_busy_status))
        .route("/status/cleanup", post(cleanup_operations))
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

async fn execute_command(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<CommandRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match commands::execute_shell_command(&request.command).await {
        Ok(output) => Ok(ResponseJson(ApiResponse::success_with_output(output))),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
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

    match systemctl::get_service_status(&request.service_name).await {
        Ok(status) => Ok(ResponseJson(ApiResponse::success_with_status(status))),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
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

    match systemctl::start_service(&request.service_name).await {
        Ok(_) => Ok(ResponseJson(ApiResponse::success())),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
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

    match systemctl::stop_service(&request.service_name).await {
        Ok(_) => Ok(ResponseJson(ApiResponse::success())),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
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

    match systemctl::get_service_uptime(&request.service_name).await {
        Ok(uptime_seconds) => Ok(ResponseJson(ApiResponse::success_with_uptime(
            uptime_seconds,
        ))),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}

async fn truncate_logs(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<LogTruncateRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if let Err(err) = state
        .try_start_operation(&request.service_name, "log_truncation")
        .await
    {
        return Ok(ResponseJson(ApiResponse::error(err)));
    }

    let result =
        services::logs::truncate_service_logs(&request.service_name, &request.log_path).await;
    state.finish_operation(&request.service_name).await;

    match result {
        Ok(_) => Ok(ResponseJson(ApiResponse::success())),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}

async fn delete_all_files_in_directory(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<LogDeleteAllRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match services::logs::delete_all_files_in_directory(&request.log_path).await {
        Ok(_) => Ok(ResponseJson(ApiResponse::success())),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}

async fn execute_pruning_async(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<PruningRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if let Err(err) = state
        .try_start_operation(&request.service_name, "pruning")
        .await
    {
        return Ok(ResponseJson(ApiResponse::error(err)));
    }

    let job_id = state
        .job_manager
        .create_job("pruning", &request.service_name)
        .await;

    let state_clone = state.clone();
    let request_clone = request.clone();
    let job_id_clone = job_id.clone();

    tokio::spawn(async move {
        let result = pruning::execute_full_pruning_sequence(&request_clone).await;

        match result {
            Ok(output) => {
                let result_json = serde_json::json!({
                    "output": output,
                    "operation": "pruning"
                });
                state_clone
                    .job_manager
                    .complete_job(&job_id_clone, result_json)
                    .await;
            }
            Err(e) => {
                error!("Pruning failed for {}: {}", request_clone.service_name, e);
                state_clone
                    .job_manager
                    .fail_job(&job_id_clone, e.to_string())
                    .await;
            }
        }

        state_clone
            .finish_operation(&request_clone.service_name)
            .await;
    });

    Ok(ResponseJson(ApiResponse::success_with_job(
        job_id,
        "started".to_string(),
    )))
}

async fn create_snapshot_async(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<SnapshotRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if let Err(err) = state
        .try_start_operation(&request.node_name, "snapshot_creation")
        .await
    {
        return Ok(ResponseJson(ApiResponse::error(err)));
    }

    let job_id = state
        .job_manager
        .create_job("snapshot_creation", &request.node_name)
        .await;

    let state_clone = state.clone();
    let request_clone = request.clone();
    let job_id_clone = job_id.clone();

    tokio::spawn(async move {
        let result = snapshots::execute_full_snapshot_sequence(&request_clone).await;

        match result {
            Ok(snapshot_info) => {
                let backup_path = request_clone.backup_path.clone();
                let snapshot_dirname = snapshot_info.filename.clone();
                tokio::spawn(async move {
                    commands::create_lz4_compressed_snapshot(&backup_path, &snapshot_dirname).await;
                });

                let result_json = serde_json::json!({
                    "filename": snapshot_info.filename,
                    "size_bytes": snapshot_info.size_bytes,
                    "path": snapshot_info.path,
                    "compression": "directory",
                    "operation": "snapshot_creation"
                });
                state_clone
                    .job_manager
                    .complete_job(&job_id_clone, result_json)
                    .await;
            }
            Err(e) => {
                error!(
                    "Snapshot creation failed for {}: {}",
                    request_clone.node_name, e
                );
                state_clone
                    .job_manager
                    .fail_job(&job_id_clone, e.to_string())
                    .await;
            }
        }

        state_clone.finish_operation(&request_clone.node_name).await;
    });

    Ok(ResponseJson(ApiResponse::success_with_job(
        job_id,
        "started".to_string(),
    )))
}

async fn restore_snapshot_async(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<RestoreRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if let Err(err) = state
        .try_start_operation(&request.node_name, "snapshot_restore")
        .await
    {
        return Ok(ResponseJson(ApiResponse::error(err)));
    }

    let job_id = state
        .job_manager
        .create_job("snapshot_restore", &request.node_name)
        .await;

    let state_clone = state.clone();
    let request_clone = request.clone();
    let job_id_clone = job_id.clone();

    tokio::spawn(async move {
        let result = restore::execute_full_restore_sequence(&request_clone).await;

        match result {
            Ok(output) => {
                let result_json = serde_json::json!({
                    "output": output,
                    "operation": "snapshot_restore"
                });
                state_clone
                    .job_manager
                    .complete_job(&job_id_clone, result_json)
                    .await;
            }
            Err(e) => {
                error!(
                    "Snapshot restore failed for {}: {}",
                    request_clone.node_name, e
                );
                state_clone
                    .job_manager
                    .fail_job(&job_id_clone, e.to_string())
                    .await;
            }
        }

        state_clone.finish_operation(&request_clone.node_name).await;
    });

    Ok(ResponseJson(ApiResponse::success_with_job(
        job_id,
        "started".to_string(),
    )))
}

// NEW: State sync async endpoint
async fn execute_state_sync_async(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<StateSyncRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    if let Err(err) = state
        .try_start_operation(&request.service_name, "state_sync")
        .await
    {
        return Ok(ResponseJson(ApiResponse::error(err)));
    }

    let job_id = state
        .job_manager
        .create_job("state_sync", &request.service_name)
        .await;

    let state_clone = state.clone();
    let request_clone = request.clone();
    let job_id_clone = job_id.clone();

    tokio::spawn(async move {
        let result = state_sync::execute_state_sync_sequence(&request_clone).await;

        match result {
            Ok(output) => {
                let result_json = serde_json::json!({
                    "output": output,
                    "operation": "state_sync"
                });
                state_clone
                    .job_manager
                    .complete_job(&job_id_clone, result_json)
                    .await;
            }
            Err(e) => {
                error!(
                    "State sync failed for {}: {}",
                    request_clone.service_name, e
                );
                state_clone
                    .job_manager
                    .fail_job(&job_id_clone, e.to_string())
                    .await;
            }
        }

        state_clone
            .finish_operation(&request_clone.service_name)
            .await;
    });

    Ok(ResponseJson(ApiResponse::success_with_job(
        job_id,
        "started".to_string(),
    )))
}

async fn get_job_status(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Path(job_id): Path<String>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match state.job_manager.get_job_status(&job_id).await {
        Some(job_info) => {
            let mut response = ApiResponse::success();
            response.job_id = Some(job_info.job_id);
            response.job_status = Some(format!("{:?}", job_info.status));

            match job_info.status {
                JobStatus::Completed => {
                    if let Some(result) = job_info.result {
                        response.output = Some(result.to_string());
                    }
                }
                JobStatus::Failed => {
                    if let Some(error) = job_info.error_message {
                        response.error = Some(error);
                        response.success = false;
                    }
                }
                JobStatus::Running => {
                    response.output = Some("Operation still running".to_string());
                }
            }

            Ok(ResponseJson(response))
        }
        None => Ok(ResponseJson(ApiResponse::error(format!(
            "Job {} not found",
            job_id
        )))),
    }
}

async fn check_restore_triggers(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<serde_json::Value>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let log_file = request
        .get("log_file")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let trigger_words: Vec<String> = request
        .get("trigger_words")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    match commands::check_log_for_trigger_words(log_file, &trigger_words).await {
        Ok(found) => {
            let response = serde_json::json!({
                "triggers_found": found,
                "log_file": log_file,
                "trigger_words": trigger_words
            });
            Ok(ResponseJson(ApiResponse::success_with_output(
                response.to_string(),
            )))
        }
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}

async fn get_busy_status(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(_request): Json<serde_json::Value>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let busy_status = state.get_busy_status().await;
    let running_jobs = state.job_manager.get_running_jobs().await;

    let response = serde_json::json!({
        "busy_nodes": busy_status,
        "total_busy": busy_status.len(),
        "running_jobs": running_jobs.len()
    });

    Ok(ResponseJson(ApiResponse::success_with_output(
        response.to_string(),
    )))
}

async fn cleanup_operations(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<serde_json::Value>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let max_hours = request
        .get("max_hours")
        .and_then(|v| v.as_i64())
        .unwrap_or(12);

    let cleaned_operations = state.cleanup_old_operations(max_hours).await;
    let cleaned_jobs = state.job_manager.cleanup_old_jobs(max_hours).await;

    let response = serde_json::json!({
        "cleaned_operations": cleaned_operations,
        "cleaned_jobs": cleaned_jobs,
        "max_hours": max_hours
    });

    Ok(ResponseJson(ApiResponse::success_with_output(
        response.to_string(),
    )))
}
