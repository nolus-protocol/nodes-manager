//! HTTP request handlers for the agent server

use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    response::Json as ResponseJson,
};
use std::sync::Arc;

use crate::middleware::ApiKeyAuth;
use crate::operations::{pruning, restore, snapshots, state_sync};
use crate::services::{commands, logs, systemctl};
use crate::types::*;
use crate::AppState;

// === Command handlers ===

pub async fn execute_command(
    _auth: ApiKeyAuth,
    Json(request): Json<CommandRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    match commands::execute_shell_command(&request.command).await {
        Ok(output) => Ok(ResponseJson(ApiResponse::success_with_output(output))),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}

// === Service handlers ===

pub async fn get_service_status(
    _auth: ApiKeyAuth,
    Json(request): Json<ServiceRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    match systemctl::get_service_status(&request.service_name).await {
        Ok(status) => Ok(ResponseJson(ApiResponse::success_with_status(status))),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}

pub async fn start_service(
    _auth: ApiKeyAuth,
    Json(request): Json<ServiceRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    match systemctl::start_service(&request.service_name).await {
        Ok(_) => Ok(ResponseJson(ApiResponse::success())),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}

pub async fn stop_service(
    _auth: ApiKeyAuth,
    Json(request): Json<ServiceRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    match systemctl::stop_service(&request.service_name).await {
        Ok(_) => Ok(ResponseJson(ApiResponse::success())),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}

pub async fn get_service_uptime(
    _auth: ApiKeyAuth,
    Json(request): Json<ServiceRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    match systemctl::get_service_uptime(&request.service_name).await {
        Ok(uptime_seconds) => Ok(ResponseJson(ApiResponse::success_with_uptime(
            uptime_seconds,
        ))),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}

// === Log handlers ===

pub async fn truncate_logs(
    _auth: ApiKeyAuth,
    State(state): State<Arc<AppState>>,
    Json(request): Json<LogTruncateRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if let Err(err) = state
        .try_start_operation(&request.service_name, "log_truncation")
        .await
    {
        return Ok(ResponseJson(ApiResponse::error(err)));
    }

    let result = logs::truncate_service_logs(&request.service_name, &request.log_path).await;
    state.finish_operation(&request.service_name).await;

    match result {
        Ok(_) => Ok(ResponseJson(ApiResponse::success())),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}

pub async fn delete_all_files_in_directory(
    _auth: ApiKeyAuth,
    Json(request): Json<LogDeleteAllRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    match logs::delete_all_files_in_directory(&request.log_path).await {
        Ok(_) => Ok(ResponseJson(ApiResponse::success())),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}

// === Async operation handlers ===

pub async fn execute_pruning_async(
    _auth: ApiKeyAuth,
    State(state): State<Arc<AppState>>,
    Json(request): Json<PruningRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    let service_name = request.service_name.clone();
    match state
        .execute_async_operation(&service_name, "pruning", move || async move {
            let output = pruning::execute_full_pruning_sequence(&request).await?;
            Ok(serde_json::json!({ "output": output, "operation": "pruning" }))
        })
        .await
    {
        Ok(job_id) => Ok(ResponseJson(ApiResponse::success_with_job(
            job_id,
            "started".to_string(),
        ))),
        Err(err) => Ok(ResponseJson(ApiResponse::error(err))),
    }
}

pub async fn create_snapshot_async(
    _auth: ApiKeyAuth,
    State(state): State<Arc<AppState>>,
    Json(request): Json<SnapshotRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    let node_name = request.node_name.clone();
    match state
        .execute_async_operation(&node_name, "snapshot_creation", move || async move {
            let snapshot_info = snapshots::execute_full_snapshot_sequence(&request).await?;

            // Spawn LZ4 compression in background
            let backup_path = request.backup_path.clone();
            let snapshot_dirname = snapshot_info.filename.clone();
            tokio::spawn(async move {
                commands::create_lz4_compressed_snapshot(&backup_path, &snapshot_dirname).await;
            });

            Ok(serde_json::json!({
                "filename": snapshot_info.filename,
                "size_bytes": snapshot_info.size_bytes,
                "path": snapshot_info.path,
                "compression": "directory",
                "operation": "snapshot_creation"
            }))
        })
        .await
    {
        Ok(job_id) => Ok(ResponseJson(ApiResponse::success_with_job(
            job_id,
            "started".to_string(),
        ))),
        Err(err) => Ok(ResponseJson(ApiResponse::error(err))),
    }
}

pub async fn restore_snapshot_async(
    _auth: ApiKeyAuth,
    State(state): State<Arc<AppState>>,
    Json(request): Json<RestoreRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    let node_name = request.node_name.clone();
    match state
        .execute_async_operation(&node_name, "snapshot_restore", move || async move {
            let output = restore::execute_full_restore_sequence(&request).await?;
            Ok(serde_json::json!({ "output": output, "operation": "snapshot_restore" }))
        })
        .await
    {
        Ok(job_id) => Ok(ResponseJson(ApiResponse::success_with_job(
            job_id,
            "started".to_string(),
        ))),
        Err(err) => Ok(ResponseJson(ApiResponse::error(err))),
    }
}

pub async fn execute_state_sync_async(
    _auth: ApiKeyAuth,
    State(state): State<Arc<AppState>>,
    Json(request): Json<StateSyncRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    let service_name = request.service_name.clone();
    match state
        .execute_async_operation(&service_name, "state_sync", move || async move {
            let output = state_sync::execute_state_sync_sequence(&request).await?;
            Ok(serde_json::json!({ "output": output, "operation": "state_sync" }))
        })
        .await
    {
        Ok(job_id) => Ok(ResponseJson(ApiResponse::success_with_job(
            job_id,
            "started".to_string(),
        ))),
        Err(err) => Ok(ResponseJson(ApiResponse::error(err))),
    }
}

// === Job status handlers ===

pub async fn get_job_status(
    _auth: ApiKeyAuth,
    State(state): State<Arc<AppState>>,
    Path(job_id): Path<String>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
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

pub async fn check_restore_triggers(
    _auth: ApiKeyAuth,
    Json(request): Json<serde_json::Value>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
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

// === Status handlers ===

pub async fn get_busy_status(
    _auth: ApiKeyAuth,
    State(state): State<Arc<AppState>>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
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

pub async fn cleanup_operations(
    _auth: ApiKeyAuth,
    State(state): State<Arc<AppState>>,
    Json(request): Json<serde_json::Value>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
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
