//! Admin CRUD endpoints for configuration management.
//!
//! This module provides endpoints for managing servers, nodes, hermes instances,
//! and global settings through the API.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{error, info};

use super::common::{ApiResponse, ApiResult};
use crate::web::AppState;

// ============================================================================
// Server CRUD
// ============================================================================

#[derive(Deserialize)]
pub struct CreateServerRequest {
    pub name: String,
    pub host: String,
    #[serde(default = "default_agent_port")]
    pub agent_port: u16,
    pub api_key_ref: String,
    #[serde(default = "default_request_timeout")]
    pub request_timeout_seconds: u64,
}

fn default_agent_port() -> u16 {
    8745
}

fn default_request_timeout() -> u64 {
    300
}

#[derive(Deserialize)]
pub struct UpdateServerRequest {
    pub name: Option<String>,
    pub host: Option<String>,
    pub agent_port: Option<u16>,
    pub api_key_ref: Option<String>,
    pub request_timeout_seconds: Option<u64>,
}

pub async fn get_all_servers(State(state): State<AppState>) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    match store.get_all_servers().await {
        Ok(servers) => {
            // Convert to JSON, masking api_key_ref
            let servers_json: Vec<Value> = servers
                .iter()
                .map(|s| {
                    json!({
                        "id": s.id,
                        "name": s.name,
                        "host": s.host,
                        "agent_port": s.agent_port,
                        "api_key_configured": !s.api_key_ref.is_empty(),
                        "request_timeout_seconds": s.request_timeout_seconds,
                        "created_at": s.created_at.to_rfc3339(),
                        "updated_at": s.updated_at.to_rfc3339()
                    })
                })
                .collect();
            Ok(Json(ApiResponse::success(json!(servers_json))))
        }
        Err(e) => {
            error!("Failed to get servers: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn get_server(Path(id): Path<String>, State(state): State<AppState>) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    match store.get_server(&id).await {
        Ok(Some(server)) => Ok(Json(ApiResponse::success(json!({
            "id": server.id,
            "name": server.name,
            "host": server.host,
            "agent_port": server.agent_port,
            "api_key_configured": !server.api_key_ref.is_empty(),
            "request_timeout_seconds": server.request_timeout_seconds,
            "created_at": server.created_at.to_rfc3339(),
            "updated_at": server.updated_at.to_rfc3339()
        })))),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error(format!("Server {} not found", id))),
        )),
        Err(e) => {
            error!("Failed to get server {}: {}", id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn create_server(
    State(state): State<AppState>,
    Json(req): Json<CreateServerRequest>,
) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    // Check if server name already exists
    match store.get_server_by_name(&req.name).await {
        Ok(Some(_)) => {
            return Err((
                StatusCode::CONFLICT,
                Json(ApiResponse::error(format!(
                    "Server with name '{}' already exists",
                    req.name
                ))),
            ));
        }
        Err(e) => {
            error!("Failed to check server existence: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ));
        }
        Ok(None) => {}
    }

    // Test connectivity to the agent
    let test_url = format!("http://{}:{}/status/busy", req.host, req.agent_port);
    info!("Testing connectivity to agent at {}", test_url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    if let Err(e) = client.get(&test_url).send().await {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(format!(
                "Cannot connect to agent at {}:{} - {}",
                req.host, req.agent_port, e
            ))),
        ));
    }

    match store
        .create_server(
            req.name,
            req.host,
            req.agent_port,
            req.api_key_ref,
            req.request_timeout_seconds,
        )
        .await
    {
        Ok(server) => {
            info!("Created server: {} ({})", server.name, server.id);
            Ok(Json(ApiResponse::success(json!({
                "id": server.id,
                "name": server.name,
                "message": "Server created successfully"
            }))))
        }
        Err(e) => {
            error!("Failed to create server: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn update_server(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateServerRequest>,
) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    let mut server = match store.get_server(&id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error(format!("Server {} not found", id))),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ));
        }
    };

    // Apply updates
    if let Some(name) = req.name {
        server.name = name;
    }
    if let Some(host) = req.host {
        server.host = host;
    }
    if let Some(port) = req.agent_port {
        server.agent_port = port as i64;
    }
    if let Some(api_key_ref) = req.api_key_ref {
        server.api_key_ref = api_key_ref;
    }
    if let Some(timeout) = req.request_timeout_seconds {
        server.request_timeout_seconds = timeout as i64;
    }

    match store.update_server(server).await {
        Ok(updated) => {
            info!("Updated server: {} ({})", updated.name, updated.id);
            Ok(Json(ApiResponse::success(json!({
                "id": updated.id,
                "name": updated.name,
                "message": "Server updated successfully"
            }))))
        }
        Err(e) => {
            error!("Failed to update server {}: {}", id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn delete_server(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    match store.delete_server(&id).await {
        Ok(true) => {
            info!("Deleted server: {}", id);
            Ok(Json(ApiResponse::success(json!({
                "message": "Server deleted successfully"
            }))))
        }
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error(format!("Server {} not found", id))),
        )),
        Err(e) => {
            error!("Failed to delete server {}: {}", id, e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

// ============================================================================
// Node CRUD
// ============================================================================

#[derive(Deserialize)]
pub struct CreateNodeRequest {
    pub name: String,
    pub server_id: String,
    pub network: String,
    pub rpc_url: String,
    pub service_name: String,
    #[serde(default)]
    pub enabled: bool,
    pub deploy_path: Option<String>,
    pub log_path: Option<String>,
    pub snapshot_backup_path: Option<String>,
    // Pruning
    #[serde(default)]
    pub pruning_enabled: bool,
    pub pruning_schedule: Option<String>,
    pub pruning_keep_blocks: Option<i64>,
    pub pruning_keep_versions: Option<i64>,
    // Snapshots
    #[serde(default)]
    pub snapshots_enabled: bool,
    pub snapshot_schedule: Option<String>,
    pub snapshot_retention_count: Option<i64>,
    #[serde(default)]
    pub auto_restore_enabled: bool,
    // State sync
    #[serde(default)]
    pub state_sync_enabled: bool,
    pub state_sync_schedule: Option<String>,
    pub state_sync_rpc_sources: Option<Vec<String>>,
    pub state_sync_trust_height_offset: Option<i64>,
    // Log monitoring
    #[serde(default)]
    pub log_monitoring_enabled: bool,
    pub log_monitoring_patterns: Option<Vec<String>>,
    #[serde(default)]
    pub truncate_logs_enabled: bool,
}

#[derive(Deserialize)]
pub struct UpdateNodeRequest {
    pub name: Option<String>,
    pub server_id: Option<String>,
    pub network: Option<String>,
    pub rpc_url: Option<String>,
    pub service_name: Option<String>,
    pub enabled: Option<bool>,
    pub deploy_path: Option<String>,
    pub log_path: Option<String>,
    pub snapshot_backup_path: Option<String>,
    pub pruning_enabled: Option<bool>,
    pub pruning_schedule: Option<String>,
    pub pruning_keep_blocks: Option<i64>,
    pub pruning_keep_versions: Option<i64>,
    pub snapshots_enabled: Option<bool>,
    pub snapshot_schedule: Option<String>,
    pub snapshot_retention_count: Option<i64>,
    pub auto_restore_enabled: Option<bool>,
    pub state_sync_enabled: Option<bool>,
    pub state_sync_schedule: Option<String>,
    pub state_sync_rpc_sources: Option<Vec<String>>,
    pub state_sync_trust_height_offset: Option<i64>,
    pub log_monitoring_enabled: Option<bool>,
    pub log_monitoring_patterns: Option<Vec<String>>,
    pub truncate_logs_enabled: Option<bool>,
}

#[derive(Deserialize)]
pub struct ToggleNodeRequest {
    pub enabled: bool,
}

pub async fn get_all_nodes_config(State(state): State<AppState>) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    match store.get_all_nodes().await {
        Ok(nodes) => Ok(Json(ApiResponse::success(json!(nodes)))),
        Err(e) => {
            error!("Failed to get nodes: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn get_node_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    match store.get_node(&id).await {
        Ok(Some(node)) => Ok(Json(ApiResponse::success(json!(node)))),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error(format!("Node {} not found", id))),
        )),
        Err(e) => {
            error!("Failed to get node {}: {}", id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn create_node(
    State(state): State<AppState>,
    Json(req): Json<CreateNodeRequest>,
) -> ApiResult<Value> {
    use crate::database::NodeRecord;
    use chrono::Utc;

    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    // Check if node name already exists
    match store.get_node_by_name(&req.name).await {
        Ok(Some(_)) => {
            return Err((
                StatusCode::CONFLICT,
                Json(ApiResponse::error(format!(
                    "Node with name '{}' already exists",
                    req.name
                ))),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ));
        }
        Ok(None) => {}
    }

    // Verify server exists
    match store.get_server(&req.server_id).await {
        Ok(None) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(format!(
                    "Server {} not found",
                    req.server_id
                ))),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ));
        }
        Ok(Some(_)) => {}
    }

    // Test RPC connectivity
    info!("Testing RPC connectivity to {}", req.rpc_url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let status_url = format!("{}/status", req.rpc_url.trim_end_matches('/'));
    if let Err(e) = client.get(&status_url).send().await {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::error(format!(
                "Cannot connect to RPC at {} - {}",
                req.rpc_url, e
            ))),
        ));
    }

    let now = Utc::now();
    let record = NodeRecord {
        id: String::new(), // Will be set by create_node
        name: req.name.clone(),
        server_id: req.server_id,
        network: req.network,
        rpc_url: req.rpc_url,
        enabled: req.enabled,
        service_name: req.service_name,
        deploy_path: req.deploy_path,
        log_path: req.log_path,
        snapshot_backup_path: req.snapshot_backup_path,
        pruning_enabled: req.pruning_enabled,
        pruning_schedule: req.pruning_schedule,
        pruning_keep_blocks: req.pruning_keep_blocks,
        pruning_keep_versions: req.pruning_keep_versions,
        snapshots_enabled: req.snapshots_enabled,
        snapshot_schedule: req.snapshot_schedule,
        snapshot_retention_count: req.snapshot_retention_count,
        auto_restore_enabled: req.auto_restore_enabled,
        state_sync_enabled: req.state_sync_enabled,
        state_sync_schedule: req.state_sync_schedule,
        state_sync_rpc_sources: req
            .state_sync_rpc_sources
            .map(|v| serde_json::to_string(&v).unwrap_or_default()),
        state_sync_trust_height_offset: req.state_sync_trust_height_offset,
        state_sync_max_sync_timeout_seconds: None,
        log_monitoring_enabled: req.log_monitoring_enabled,
        log_monitoring_patterns: req
            .log_monitoring_patterns
            .map(|v| serde_json::to_string(&v).unwrap_or_default()),
        truncate_logs_enabled: req.truncate_logs_enabled,
        created_at: now,
        updated_at: now,
    };

    match store.create_node(record).await {
        Ok(node) => {
            info!("Created node: {} ({})", node.name, node.id);

            // Reload config manager and scheduler
            if let Err(e) = state.config_manager.reload_from_database().await {
                error!("Failed to reload config after node creation: {}", e);
            }
            if let Err(e) = reload_scheduler_config(&state).await {
                error!("Failed to reload scheduler after node creation: {}", e);
            }

            Ok(Json(ApiResponse::success(json!({
                "id": node.id,
                "name": node.name,
                "message": "Node created successfully"
            }))))
        }
        Err(e) => {
            error!("Failed to create node: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn update_node(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateNodeRequest>,
) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    let mut node = match store.get_node(&id).await {
        Ok(Some(n)) => n,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error(format!("Node {} not found", id))),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ));
        }
    };

    // Apply updates
    if let Some(v) = req.name {
        node.name = v;
    }
    if let Some(v) = req.server_id {
        node.server_id = v;
    }
    if let Some(v) = req.network {
        node.network = v;
    }
    if let Some(v) = req.rpc_url {
        node.rpc_url = v;
    }
    if let Some(v) = req.service_name {
        node.service_name = v;
    }
    if let Some(v) = req.enabled {
        node.enabled = v;
    }
    if let Some(v) = req.deploy_path {
        node.deploy_path = Some(v);
    }
    if let Some(v) = req.log_path {
        node.log_path = Some(v);
    }
    if let Some(v) = req.snapshot_backup_path {
        node.snapshot_backup_path = Some(v);
    }
    if let Some(v) = req.pruning_enabled {
        node.pruning_enabled = v;
    }
    if let Some(v) = req.pruning_schedule {
        node.pruning_schedule = Some(v);
    }
    if let Some(v) = req.pruning_keep_blocks {
        node.pruning_keep_blocks = Some(v);
    }
    if let Some(v) = req.pruning_keep_versions {
        node.pruning_keep_versions = Some(v);
    }
    if let Some(v) = req.snapshots_enabled {
        node.snapshots_enabled = v;
    }
    if let Some(v) = req.snapshot_schedule {
        node.snapshot_schedule = Some(v);
    }
    if let Some(v) = req.snapshot_retention_count {
        node.snapshot_retention_count = Some(v);
    }
    if let Some(v) = req.auto_restore_enabled {
        node.auto_restore_enabled = v;
    }
    if let Some(v) = req.state_sync_enabled {
        node.state_sync_enabled = v;
    }
    if let Some(v) = req.state_sync_schedule {
        node.state_sync_schedule = Some(v);
    }
    if let Some(v) = req.state_sync_rpc_sources {
        node.state_sync_rpc_sources = Some(serde_json::to_string(&v).unwrap_or_default());
    }
    if let Some(v) = req.state_sync_trust_height_offset {
        node.state_sync_trust_height_offset = Some(v);
    }
    if let Some(v) = req.log_monitoring_enabled {
        node.log_monitoring_enabled = v;
    }
    if let Some(v) = req.log_monitoring_patterns {
        node.log_monitoring_patterns = Some(serde_json::to_string(&v).unwrap_or_default());
    }
    if let Some(v) = req.truncate_logs_enabled {
        node.truncate_logs_enabled = v;
    }

    match store.update_node(node).await {
        Ok(updated) => {
            info!("Updated node: {} ({})", updated.name, updated.id);

            // Reload config manager and scheduler
            if let Err(e) = state.config_manager.reload_from_database().await {
                error!("Failed to reload config after node update: {}", e);
            }
            if let Err(e) = reload_scheduler_config(&state).await {
                error!("Failed to reload scheduler after node update: {}", e);
            }

            Ok(Json(ApiResponse::success(json!({
                "id": updated.id,
                "name": updated.name,
                "message": "Node updated successfully"
            }))))
        }
        Err(e) => {
            error!("Failed to update node {}: {}", id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn delete_node_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    match store.delete_node(&id).await {
        Ok(true) => {
            info!("Deleted node: {}", id);

            // Reload config manager and scheduler
            if let Err(e) = state.config_manager.reload_from_database().await {
                error!("Failed to reload config after node deletion: {}", e);
            }
            if let Err(e) = reload_scheduler_config(&state).await {
                error!("Failed to reload scheduler after node deletion: {}", e);
            }

            Ok(Json(ApiResponse::success(json!({
                "message": "Node deleted successfully"
            }))))
        }
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error(format!("Node {} not found", id))),
        )),
        Err(e) => {
            error!("Failed to delete node {}: {}", id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn toggle_node(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<ToggleNodeRequest>,
) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    match store.toggle_node(&id, req.enabled).await {
        Ok(Some(node)) => {
            info!("Toggled node {} to enabled={}", node.name, req.enabled);

            // Reload config manager and scheduler
            if let Err(e) = state.config_manager.reload_from_database().await {
                error!("Failed to reload config after node toggle: {}", e);
            }
            if let Err(e) = reload_scheduler_config(&state).await {
                error!("Failed to reload scheduler after node toggle: {}", e);
            }

            Ok(Json(ApiResponse::success(json!({
                "id": node.id,
                "name": node.name,
                "enabled": node.enabled,
                "message": format!("Node {} {}", node.name, if req.enabled { "enabled" } else { "disabled" })
            }))))
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error(format!("Node {} not found", id))),
        )),
        Err(e) => {
            error!("Failed to toggle node {}: {}", id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

// ============================================================================
// Hermes CRUD
// ============================================================================

#[derive(Deserialize)]
pub struct CreateHermesRequest {
    pub name: String,
    pub server_id: String,
    pub service_name: String,
    pub log_path: Option<String>,
    pub restart_schedule: Option<String>,
    pub dependent_nodes: Option<Vec<String>>,
    #[serde(default)]
    pub truncate_logs_enabled: bool,
}

#[derive(Deserialize)]
pub struct UpdateHermesRequest {
    pub name: Option<String>,
    pub server_id: Option<String>,
    pub service_name: Option<String>,
    pub log_path: Option<String>,
    pub restart_schedule: Option<String>,
    pub dependent_nodes: Option<Vec<String>>,
    pub truncate_logs_enabled: Option<bool>,
}

pub async fn get_all_hermes_config(State(state): State<AppState>) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    match store.get_all_hermes().await {
        Ok(hermes) => Ok(Json(ApiResponse::success(json!(hermes)))),
        Err(e) => {
            error!("Failed to get hermes: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn get_hermes_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    match store.get_hermes(&id).await {
        Ok(Some(hermes)) => Ok(Json(ApiResponse::success(json!(hermes)))),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error(format!("Hermes {} not found", id))),
        )),
        Err(e) => {
            error!("Failed to get hermes {}: {}", id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn create_hermes(
    State(state): State<AppState>,
    Json(req): Json<CreateHermesRequest>,
) -> ApiResult<Value> {
    use crate::database::HermesRecord;
    use chrono::Utc;

    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    // Check if hermes name already exists
    match store.get_hermes_by_name(&req.name).await {
        Ok(Some(_)) => {
            return Err((
                StatusCode::CONFLICT,
                Json(ApiResponse::error(format!(
                    "Hermes with name '{}' already exists",
                    req.name
                ))),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ));
        }
        Ok(None) => {}
    }

    // Verify server exists
    match store.get_server(&req.server_id).await {
        Ok(None) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::error(format!(
                    "Server {} not found",
                    req.server_id
                ))),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ));
        }
        Ok(Some(_)) => {}
    }

    let now = Utc::now();
    let record = HermesRecord {
        id: String::new(),
        name: req.name.clone(),
        server_id: req.server_id,
        service_name: req.service_name,
        log_path: req.log_path,
        restart_schedule: req.restart_schedule,
        dependent_nodes: req
            .dependent_nodes
            .map(|v| serde_json::to_string(&v).unwrap_or_default()),
        truncate_logs_enabled: req.truncate_logs_enabled,
        created_at: now,
        updated_at: now,
    };

    match store.create_hermes(record).await {
        Ok(hermes) => {
            info!("Created hermes: {} ({})", hermes.name, hermes.id);

            // Reload config manager and scheduler
            if let Err(e) = state.config_manager.reload_from_database().await {
                error!("Failed to reload config after hermes creation: {}", e);
            }
            if let Err(e) = reload_scheduler_config(&state).await {
                error!("Failed to reload scheduler after hermes creation: {}", e);
            }

            Ok(Json(ApiResponse::success(json!({
                "id": hermes.id,
                "name": hermes.name,
                "message": "Hermes created successfully"
            }))))
        }
        Err(e) => {
            error!("Failed to create hermes: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn update_hermes(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(req): Json<UpdateHermesRequest>,
) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    let mut hermes = match store.get_hermes(&id).await {
        Ok(Some(h)) => h,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ApiResponse::error(format!("Hermes {} not found", id))),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ));
        }
    };

    // Apply updates
    if let Some(v) = req.name {
        hermes.name = v;
    }
    if let Some(v) = req.server_id {
        hermes.server_id = v;
    }
    if let Some(v) = req.service_name {
        hermes.service_name = v;
    }
    if let Some(v) = req.log_path {
        hermes.log_path = Some(v);
    }
    if let Some(v) = req.restart_schedule {
        hermes.restart_schedule = Some(v);
    }
    if let Some(v) = req.dependent_nodes {
        hermes.dependent_nodes = Some(serde_json::to_string(&v).unwrap_or_default());
    }
    if let Some(v) = req.truncate_logs_enabled {
        hermes.truncate_logs_enabled = v;
    }

    match store.update_hermes(hermes).await {
        Ok(updated) => {
            info!("Updated hermes: {} ({})", updated.name, updated.id);

            // Reload config manager and scheduler
            if let Err(e) = state.config_manager.reload_from_database().await {
                error!("Failed to reload config after hermes update: {}", e);
            }
            if let Err(e) = reload_scheduler_config(&state).await {
                error!("Failed to reload scheduler after hermes update: {}", e);
            }

            Ok(Json(ApiResponse::success(json!({
                "id": updated.id,
                "name": updated.name,
                "message": "Hermes updated successfully"
            }))))
        }
        Err(e) => {
            error!("Failed to update hermes {}: {}", id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn delete_hermes_config(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    match store.delete_hermes(&id).await {
        Ok(true) => {
            info!("Deleted hermes: {}", id);

            // Reload config manager and scheduler
            if let Err(e) = state.config_manager.reload_from_database().await {
                error!("Failed to reload config after hermes deletion: {}", e);
            }
            if let Err(e) = reload_scheduler_config(&state).await {
                error!("Failed to reload scheduler after hermes deletion: {}", e);
            }

            Ok(Json(ApiResponse::success(json!({
                "message": "Hermes deleted successfully"
            }))))
        }
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            Json(ApiResponse::error(format!("Hermes {} not found", id))),
        )),
        Err(e) => {
            error!("Failed to delete hermes {}: {}", id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

// ============================================================================
// Global Settings
// ============================================================================

pub async fn get_global_settings(State(state): State<AppState>) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    match store.get_all_settings().await {
        Ok(settings) => Ok(Json(ApiResponse::success(json!(settings)))),
        Err(e) => {
            error!("Failed to get settings: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateSettingsRequest {
    pub settings: std::collections::HashMap<String, String>,
}

pub async fn update_global_settings(
    State(state): State<AppState>,
    Json(req): Json<UpdateSettingsRequest>,
) -> ApiResult<Value> {
    let store = match state.config_manager.get_store() {
        Some(store) => store,
        None => {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ApiResponse::error(
                    "Configuration store not available".to_string(),
                )),
            ));
        }
    };

    let mut updated_count = 0;
    for (key, value) in req.settings {
        if let Err(e) = store.set_setting(&key, &value).await {
            error!("Failed to update setting {}: {}", key, e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(format!(
                    "Failed to update setting {}: {}",
                    key, e
                ))),
            ));
        }
        updated_count += 1;
    }

    info!("Updated {} global settings", updated_count);
    Ok(Json(ApiResponse::success(json!({
        "message": format!("Updated {} settings", updated_count),
        "updated_count": updated_count
    }))))
}

// ============================================================================
// Import/Export & System
// ============================================================================

pub async fn import_config_from_toml(State(state): State<AppState>) -> ApiResult<Value> {
    match state.config_manager.reimport_from_toml().await {
        Ok(result) => {
            info!("Configuration imported from TOML files");

            // Reload scheduler with new configuration
            if let Err(e) = reload_scheduler_config(&state).await {
                error!("Failed to reload scheduler after import: {}", e);
            }

            Ok(Json(ApiResponse::success(json!({
                "message": "Configuration imported successfully",
                "servers_created": result.servers_created,
                "servers_updated": result.servers_updated,
                "nodes_created": result.nodes_created,
                "nodes_updated": result.nodes_updated,
                "nodes_skipped": result.nodes_skipped,
                "hermes_created": result.hermes_created,
                "hermes_updated": result.hermes_updated,
                "hermes_skipped": result.hermes_skipped
            }))))
        }
        Err(e) => {
            error!("Failed to import configuration: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::error(e.to_string())),
            ))
        }
    }
}

pub async fn get_config_source(State(state): State<AppState>) -> ApiResult<Value> {
    Ok(Json(ApiResponse::success(json!({
        "source": format!("{:?}", state.config_manager.get_source()),
        "store_available": state.config_manager.get_store().is_some()
    }))))
}

/// Helper function to reload the scheduler with updated configuration.
/// This should be called after any configuration change that affects schedules.
pub async fn reload_scheduler_config(state: &AppState) -> Result<(), String> {
    let new_config = state.config_manager.get_current_config().await;
    state
        .scheduler
        .reload_config(new_config)
        .await
        .map_err(|e| e.to_string())
}
