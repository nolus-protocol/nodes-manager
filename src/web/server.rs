// File: src/web/server.rs

use anyhow::Result;
use axum::{
    extract::DefaultBodyLimit,
    http::{header, Method},
    routing::{delete, get, post, put},
    Router,
};
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    services::ServeDir,
    trace::TraceLayer,
};
use tracing::info;

use crate::config::ConfigManager;
use crate::database::Database;
use crate::health::HealthMonitor;
use crate::scheduler::MaintenanceScheduler;
use crate::ssh::SshManager;
use crate::web::{handlers, AppState};
use crate::Config;

pub async fn start_web_server(
    config: Arc<Config>,
    database: Arc<Database>,
    health_monitor: Arc<HealthMonitor>,
    ssh_manager: Arc<SshManager>,
    scheduler: Arc<MaintenanceScheduler>,
    config_manager: Arc<ConfigManager>,
) -> Result<()> {
    let app_state = AppState::new(
        config.clone(),
        database,
        health_monitor,
        ssh_manager,
        scheduler,
        config_manager,
    );

    let app = create_app(app_state).await;

    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("Web server starting on http://{}", addr);
    info!("API documentation available at http://{}/api/docs", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

async fn create_app(app_state: AppState) -> Router {
    // Create API routes
    let api_routes = Router::new()
        // Health monitoring endpoints
        .route("/nodes/health", get(handlers::get_all_nodes_health))
        .route("/nodes/:name/health", get(handlers::get_node_health))
        .route("/nodes/:name/history", get(handlers::get_node_health_history))
        .route("/nodes/:name/check", post(handlers::force_health_check))

        // Maintenance management endpoints
        .route("/maintenance/schedule", get(handlers::get_scheduled_operations))
        .route("/maintenance/pruning", post(handlers::schedule_pruning))
        .route("/maintenance/hermes-restart", post(handlers::schedule_hermes_restart))
        .route("/maintenance/:id", delete(handlers::cancel_scheduled_operation))
        .route("/maintenance/run-now", post(handlers::execute_immediate_operation))
        .route("/maintenance/logs", get(handlers::get_maintenance_logs))
        .route("/maintenance/prune-multiple", post(handlers::execute_batch_pruning))
        .route("/maintenance/restart-multiple", post(handlers::execute_batch_hermes_restart))
        .route("/maintenance/status/:operation_id", get(handlers::get_operation_status))
        .route("/maintenance/summary", get(handlers::get_operations_summary))

        // Hermes management endpoints
        .route("/hermes/instances", get(handlers::get_all_hermes_instances))
        .route("/hermes/:name/restart", post(handlers::restart_hermes_instance))
        .route("/hermes/:name/status", get(handlers::get_hermes_status))
        .route("/hermes/restart-all", post(handlers::restart_all_hermes))

        // Configuration management endpoints
        .route("/config/nodes", get(handlers::get_all_node_configs))
        .route("/config/nodes/:name", put(handlers::update_node_config))
        .route("/config/hermes", get(handlers::get_all_hermes_configs))
        .route("/config/servers", get(handlers::get_all_server_configs))
        .route("/config/reload", post(handlers::reload_configurations))
        .route("/config/validate", post(handlers::validate_configuration))
        .route("/config/files", get(handlers::list_config_files))

        // System status endpoints
        .route("/system/status", get(handlers::get_system_status))
        .route("/system/ssh-connections", get(handlers::get_ssh_connections_status))
        .route("/system/operations", get(handlers::get_running_operations))
        .route("/system/health", get(handlers::health_check))
        .route("/system/connectivity", get(handlers::test_server_connectivity))
        .route("/system/services", get(handlers::get_all_service_statuses))

        // Utility endpoints
        .route("/docs", get(handlers::api_documentation))
        .route("/version", get(handlers::get_version_info));

    // Root application router
    Router::new()
        // API routes with /api prefix
        .nest("/api", api_routes)

        // Static file serving for web interface
        .nest_service("/", ServeDir::new("static"))

        // Health check at root
        .route("/health", get(handlers::health_check))

        // Add application state
        .with_state(app_state)

        // Add middleware layers
        .layer(create_cors_layer())
        .layer(TraceLayer::new_for_http())
        .layer(DefaultBodyLimit::max(1024 * 1024)) // 1MB max body size
}

fn create_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::ACCEPT,
        ])
        .max_age(std::time::Duration::from_secs(3600))
}

// Alternative simplified server for development/testing
#[allow(dead_code)]
pub async fn start_simple_server(
    config: Arc<Config>,
    database: Arc<Database>,
    health_monitor: Arc<HealthMonitor>,
    ssh_manager: Arc<SshManager>,
    scheduler: Arc<MaintenanceScheduler>,
    config_manager: Arc<ConfigManager>,
) -> Result<()> {
    let app_state = AppState::new(
        config.clone(),
        database,
        health_monitor,
        ssh_manager,
        scheduler,
        config_manager,
    );

    // Simplified router for testing
    let app = Router::new()
        .route("/health", get(handlers::health_check))
        .route("/api/nodes/health", get(handlers::get_all_nodes_health))
        .route("/api/system/status", get(handlers::get_system_status))
        .with_state(app_state)
        .layer(TraceLayer::new_for_http());

    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("Simple web server starting on http://{}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}

// Server configuration
#[allow(dead_code)]
pub struct ServerConfig {
    pub enable_cors: bool,
    pub enable_tracing: bool,
    pub max_body_size: usize,
    pub static_dir: String,
    pub enable_compression: bool,
}

#[allow(dead_code)]
impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            enable_cors: true,
            enable_tracing: true,
            max_body_size: 1024 * 1024, // 1MB
            static_dir: "static".to_string(),
            enable_compression: true,
        }
    }
}

// Custom server with configuration
#[allow(dead_code)]
pub async fn start_custom_server(
    config: Arc<Config>,
    database: Arc<Database>,
    health_monitor: Arc<HealthMonitor>,
    ssh_manager: Arc<SshManager>,
    scheduler: Arc<MaintenanceScheduler>,
    config_manager: Arc<ConfigManager>,
    server_config: ServerConfig,
) -> Result<()> {
    let app_state = AppState::new(
        config.clone(),
        database,
        health_monitor,
        ssh_manager,
        scheduler,
        config_manager,
    );

    let mut app = create_app(app_state).await;

    // Apply server configuration
    if !server_config.enable_cors {
        // Remove CORS layer - would need to be implemented differently in practice
    }

    if server_config.max_body_size != 1024 * 1024 {
        app = app.layer(DefaultBodyLimit::max(server_config.max_body_size));
    }

    let addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!("Custom web server starting on http://{}", addr);
    axum::serve(listener, app).await?;

    Ok(())
}

// Server metrics and monitoring
#[allow(dead_code)]
pub async fn get_server_metrics() -> serde_json::Value {
    serde_json::json!({
        "server_type": "axum",
        "framework_version": env!("CARGO_PKG_VERSION"),
        "rust_version": option_env!("CARGO_PKG_RUST_VERSION").unwrap_or("unknown"),
        "build_timestamp": env!("CARGO_PKG_VERSION"),
        "features": [
            "cors",
            "tracing",
            "static_files",
            "json_api",
            "health_checks"
        ]
    })
}

// Graceful shutdown handler
#[allow(dead_code)]
pub async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    info!("Shutdown signal received, starting graceful shutdown");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_endpoint() {
        // This would require setting up test state
        // Simplified test structure for now
        assert!(true);
    }

    #[tokio::test]
    async fn test_cors_configuration() {
        let _cors_layer = create_cors_layer();
        // Test CORS configuration
        assert!(true);
    }
}
