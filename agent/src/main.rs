//! Blockchain Server Agent
//!
//! A lightweight HTTP agent deployed on each blockchain server to execute operations locally.

use anyhow::Result;
use axum::{
    routing::{get, post},
    Router,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

mod handlers;
mod middleware;
mod operations;
mod services;
pub mod types;

use services::job_manager::JobManager;

/// Application state shared across all handlers
#[derive(Clone)]
pub struct AppState {
    pub api_key: String,
    pub busy_nodes: Arc<RwLock<HashMap<String, BusyState>>>,
    pub job_manager: JobManager,
}

/// Tracks busy state for a node operation
#[derive(Clone, Debug)]
pub struct BusyState {
    operation_type: String,
    started_at: chrono::DateTime<chrono::Utc>,
}

impl AppState {
    pub async fn try_start_operation(
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

    pub async fn finish_operation(&self, node_name: &str) {
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

    pub async fn cleanup_old_operations(&self, max_hours: i64) -> u32 {
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

    pub async fn get_busy_status(&self) -> HashMap<String, String> {
        let busy = self.busy_nodes.read().await;
        busy.iter()
            .map(|(node, state)| (node.clone(), state.operation_type.clone()))
            .collect()
    }

    /// Execute an async operation with standard lifecycle handling.
    /// Returns the job_id for tracking, or an error message if the node is busy.
    pub async fn execute_async_operation<F, Fut>(
        self: &Arc<Self>,
        target_name: &str,
        operation_type: &str,
        operation: F,
    ) -> Result<String, String>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<serde_json::Value, anyhow::Error>> + Send,
    {
        self.try_start_operation(target_name, operation_type)
            .await?;

        let job_id = self
            .job_manager
            .create_job(operation_type, target_name)
            .await;

        let state = self.clone();
        let job_id_clone = job_id.clone();
        let target_name = target_name.to_string();
        let operation_type = operation_type.to_string();

        tokio::spawn(async move {
            let result = operation().await;

            match result {
                Ok(result_json) => {
                    state
                        .job_manager
                        .complete_job(&job_id_clone, result_json)
                        .await;
                }
                Err(e) => {
                    error!("{} failed for {}: {}", operation_type, target_name, e);
                    state
                        .job_manager
                        .fail_job(&job_id_clone, e.to_string())
                        .await;
                }
            }

            state.finish_operation(&target_name).await;
        });

        Ok(job_id)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    info!("Starting Blockchain Server Agent on 0.0.0.0:8745");

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

    // Spawn background cleanup task
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
        // Command execution
        .route("/command/execute", post(handlers::execute_command))
        // Service management
        .route("/service/status", post(handlers::get_service_status))
        .route("/service/start", post(handlers::start_service))
        .route("/service/stop", post(handlers::stop_service))
        .route("/service/uptime", post(handlers::get_service_uptime))
        // Log management
        .route("/logs/truncate", post(handlers::truncate_logs))
        .route(
            "/logs/delete-all",
            post(handlers::delete_all_files_in_directory),
        )
        // Async operations
        .route("/pruning/execute", post(handlers::execute_pruning_async))
        .route("/snapshot/create", post(handlers::create_snapshot_async))
        .route("/snapshot/restore", post(handlers::restore_snapshot_async))
        .route(
            "/snapshot/check-triggers",
            post(handlers::check_restore_triggers),
        )
        .route(
            "/state-sync/execute",
            post(handlers::execute_state_sync_async),
        )
        // Status and job management
        .route("/operation/status/{job_id}", get(handlers::get_job_status))
        .route("/status/busy", post(handlers::get_busy_status))
        .route("/status/cleanup", post(handlers::cleanup_operations))
        .with_state(Arc::new(app_state));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8745").await?;
    info!("Server agent listening on 0.0.0.0:8745");

    axum::serve(listener, app).await?;
    Ok(())
}
