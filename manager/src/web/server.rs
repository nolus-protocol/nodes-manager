// File: manager/src/web/server.rs
use crate::config::{Config, ConfigManager};
use crate::database::Database;
use crate::health::HealthMonitor;
use crate::http::HttpAgentManager;
use crate::operation_tracker::SimpleOperationTracker;
use crate::services::{HermesService, OperationExecutor, SnapshotService, StateSyncService};
use crate::snapshot::SnapshotManager;
use crate::web::{handlers, AppState};
use anyhow::Result;
use axum::{
    routing::{delete, get, post},
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

#[allow(clippy::too_many_arguments)]
pub async fn start_web_server(
    config: Arc<Config>,
    database: Arc<Database>,
    health_monitor: Arc<HealthMonitor>,
    http_manager: Arc<HttpAgentManager>,
    config_manager: Arc<ConfigManager>,
    snapshot_manager: Arc<SnapshotManager>,
    operation_tracker: Arc<SimpleOperationTracker>,
    operation_executor: Arc<OperationExecutor>,
    hermes_service: Arc<HermesService>,
    maintenance_service: Arc<crate::services::MaintenanceService>,
    snapshot_service_v2: Arc<SnapshotService>,
    state_sync_service: Arc<StateSyncService>,
) -> Result<()> {
    let state = AppState::new(
        config.clone(),
        database,
        health_monitor,
        http_manager,
        config_manager,
        snapshot_manager,
        operation_tracker,
        operation_executor,
        hermes_service,
        maintenance_service,
        snapshot_service_v2,
        state_sync_service,
    );

    if state.config.host == "0.0.0.0" && state.config.port == 8095 {
        start_simple_server(state).await
    } else {
        start_custom_server(state).await
    }
}

async fn start_simple_server(state: AppState) -> Result<()> {
    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8095").await?;
    tracing::info!("Server running on http://0.0.0.0:8095");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn start_custom_server(state: AppState) -> Result<()> {
    let app = create_router(state.clone());
    let addr = format!("{}:{}", state.config.host, state.config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Server running on http://{}", addr);
    axum::serve(listener, app).await?;
    Ok(())
}

fn create_router(state: AppState) -> Router {
    Router::new()
        // === HEALTH MONITORING ROUTES ===
        .route("/api/health/nodes", get(handlers::get_all_nodes_health))
        .route(
            "/api/health/nodes/refresh",
            post(handlers::refresh_all_nodes_health),
        )
        .route(
            "/api/health/nodes/{node_name}",
            get(handlers::get_node_health),
        )
        .route("/api/health/hermes", get(handlers::get_all_hermes_health))
        .route(
            "/api/health/hermes/refresh",
            post(handlers::refresh_all_hermes_health),
        )
        .route(
            "/api/health/hermes/{hermes_name}",
            get(handlers::get_hermes_health),
        )
        // === CONFIGURATION ROUTES ===
        .route("/api/config/nodes", get(handlers::get_all_node_configs))
        .route("/api/config/hermes", get(handlers::get_all_hermes_configs))
        // === MANUAL OPERATION ROUTES (WITH OPERATION TRACKING) ===
        .route(
            "/api/maintenance/nodes/{node_name}/restart",
            post(handlers::execute_manual_node_restart),
        )
        .route(
            "/api/maintenance/nodes/{node_name}/prune",
            post(handlers::execute_manual_node_pruning),
        )
        .route(
            "/api/maintenance/hermes/{hermes_name}/restart",
            post(handlers::execute_manual_hermes_restart),
        )
        // === SNAPSHOT MANAGEMENT ROUTES ===
        .route(
            "/api/snapshots/{node_name}/create",
            post(handlers::create_snapshot),
        )
        .route(
            "/api/snapshots/{node_name}/list",
            get(handlers::list_snapshots),
        )
        .route(
            "/api/snapshots/{node_name}/stats",
            get(handlers::get_snapshot_stats),
        )
        .route(
            "/api/snapshots/{node_name}/{filename}",
            delete(handlers::delete_snapshot),
        )
        .route(
            "/api/snapshots/{node_name}/cleanup",
            post(handlers::cleanup_old_snapshots),
        )
        // === NEW: MANUAL RESTORE ROUTES ===
        .route(
            "/api/snapshots/{node_name}/restore",
            post(handlers::execute_manual_restore_from_latest),
        )
        .route(
            "/api/snapshots/{node_name}/check-triggers",
            get(handlers::check_auto_restore_triggers),
        )
        .route(
            "/api/snapshots/{node_name}/auto-restore-status",
            get(handlers::get_auto_restore_status),
        )
        // === STATE SYNC ROUTES ===
        .route(
            "/api/state-sync/{node_name}/execute",
            post(handlers::execute_manual_state_sync),
        )
        // === OPERATION MANAGEMENT ROUTES ===
        .route(
            "/api/operations/active",
            get(handlers::get_active_operations),
        )
        .route(
            "/api/operations/{target_name}/cancel",
            post(handlers::cancel_operation),
        )
        .route(
            "/api/operations/{target_name}/status",
            get(handlers::check_target_status),
        )
        .route(
            "/api/operations/emergency-cleanup",
            post(handlers::emergency_cleanup_operations),
        )
        // === MAINTENANCE SCHEDULE ROUTES (SIMPLIFIED STUB) ===
        .route(
            "/api/maintenance/schedule",
            get(handlers::get_maintenance_schedule),
        )
        // === STATIC FILES ===
        .nest_service("/assets", ServeDir::new("ui/dist/assets"))
        // Add middleware
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
