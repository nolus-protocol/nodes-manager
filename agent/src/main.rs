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
use tracing::{info, warn};

mod operations;
mod services;
mod types;

use operations::{pruning, snapshots, restore};
use services::{commands, systemctl};
use types::*;

// Application state
#[derive(Clone)]
pub struct AppState {
    pub api_key: String,
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
        .route("/snapshot/restore", post(restore_snapshot)) // NEW: Restore endpoint
        .route("/snapshot/check-triggers", post(check_restore_triggers)) // NEW: Check triggers endpoint
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

// === BASIC COMMAND OPERATIONS ===

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

// === SERVICE OPERATIONS ===

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
        Ok(uptime_seconds) => Ok(ResponseJson(ApiResponse::success_with_uptime(uptime_seconds))),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}

// === LOG OPERATIONS ===

async fn truncate_logs(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<LogTruncateRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match services::logs::truncate_service_logs(&request.service_name, &request.log_path).await {
        Ok(_) => Ok(ResponseJson(ApiResponse::success())),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}

// === COMPLEX OPERATIONS ===

async fn execute_pruning(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<PruningRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match pruning::execute_full_pruning_sequence(&request).await {
        Ok(output) => Ok(ResponseJson(ApiResponse::success_with_output(output))),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
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

    match snapshots::execute_full_snapshot_sequence(&request).await {
        Ok(snapshot_info) => Ok(ResponseJson(ApiResponse::success_with_snapshot(
            snapshot_info.filename,
            snapshot_info.size_bytes,
            snapshot_info.path,
        ))),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}

// === NEW: RESTORE OPERATIONS ===

async fn restore_snapshot(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<RestoreRequest>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    match restore::execute_full_restore_sequence(&request).await {
        Ok(output) => Ok(ResponseJson(ApiResponse::success_with_output(output))),
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}

// NEW: Check for auto-restore trigger words
async fn check_restore_triggers(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(request): Json<serde_json::Value>,
) -> Result<ResponseJson<ApiResponse<()>>, StatusCode> {
    if !validate_api_key(&headers, &state.api_key) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let log_file = request.get("log_file")
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let trigger_words: Vec<String> = request.get("trigger_words")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    match commands::check_log_for_trigger_words(log_file, &trigger_words).await {
        Ok(found) => {
            let response = serde_json::json!({
                "triggers_found": found,
                "log_file": log_file,
                "trigger_words": trigger_words
            });
            Ok(ResponseJson(ApiResponse::success_with_output(response.to_string())))
        }
        Err(e) => Ok(ResponseJson(ApiResponse::error(e.to_string()))),
    }
}
