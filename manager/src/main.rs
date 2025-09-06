// File: manager/src/main.rs
use anyhow::Result;
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::{EnvFilter, fmt};

mod config;
mod database;
mod health;
mod http;
mod maintenance_tracker;
mod operation_tracker;
mod scheduler;
mod snapshot;
mod web;
mod services;

use config::ConfigManager;
use database::Database;
use health::HealthMonitor;
use http::HttpAgentManager;
use maintenance_tracker::MaintenanceTracker;
use operation_tracker::SimpleOperationTracker;
use scheduler::MaintenanceScheduler;
use snapshot::SnapshotManager;
use services::AlertService;
use web::start_web_server;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging with reduced verbosity
    let env_filter = EnvFilter::from_default_env()
        .add_directive("manager=info".parse()?)
        .add_directive("tower_http=warn".parse()?)
        .add_directive("tokio_cron_scheduler=warn".parse()?)
        .add_directive("hyper=warn".parse()?)
        .add_directive("reqwest=warn".parse()?)
        .add_directive("sqlx=warn".parse()?);

    fmt()
        .with_env_filter(env_filter)
        .init();

    info!("Starting Blockchain Infrastructure Manager");

    // Load configuration
    let config_manager = ConfigManager::new("config".to_string()).await?;
    let config = config_manager.get_current_config();
    info!("Configuration loaded: {} nodes, {} hermes instances, {} servers",
          config.nodes.len(), config.hermes.len(), config.servers.len());

    // Initialize database
    let database = Arc::new(Database::new("data/nodes.db").await?);
    info!("Database initialized");

    // Initialize operation tracker
    let operation_tracker = Arc::new(SimpleOperationTracker::new());
    info!("Operation tracker initialized");

    // Initialize maintenance tracker
    let maintenance_tracker = Arc::new(MaintenanceTracker::new());
    info!("Maintenance tracker initialized");

    // Initialize centralized AlertService
    let alert_service = Arc::new(AlertService::new(config.alarm_webhook_url.clone()));
    info!("Centralized alert service initialized");

    // Initialize HTTP agent manager with operation tracking AND maintenance tracking
    let http_manager = Arc::new(HttpAgentManager::new(
        config.clone(),
        operation_tracker.clone(),
        maintenance_tracker.clone()
    ));
    info!("HTTP agent manager initialized");

    // Initialize snapshot manager WITH AlertService
    let snapshot_manager = Arc::new(SnapshotManager::new(
        config.clone(),
        http_manager.clone(),
        maintenance_tracker.clone(),
        alert_service.clone(),
    ));
    info!("Snapshot manager initialized with centralized alerting");

    // Initialize health monitor WITH AlertService (for auto-restore and health alerts)
    let health_monitor = Arc::new(HealthMonitor::new(
        config.clone(),
        database.clone(),
        maintenance_tracker.clone(),
        snapshot_manager.clone(),
        alert_service.clone(),
    ));
    info!("Health monitor initialized with centralized alerting and auto-restore capability");

    // Initialize scheduler
    let scheduler = Arc::new(MaintenanceScheduler::new(
        database.clone(),
        http_manager.clone(),
        config.clone(),
        snapshot_manager.clone(),
    ).await?);
    info!("Maintenance scheduler initialized");

    // Start scheduler
    scheduler.start().await?;
    info!("Scheduler started");

    // Start periodic health monitoring with configurable interval
    let health_monitor_clone = health_monitor.clone();
    let check_interval = config.check_interval_seconds;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(check_interval));
        loop {
            interval.tick().await;
            if let Err(e) = health_monitor_clone.check_all_nodes().await {
                warn!("Health monitoring error: {}", e);
            }
        }
    });

    // Start periodic operation cleanup (every hour)
    let operation_tracker_clone = operation_tracker.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600)); // 1 hour
        loop {
            interval.tick().await;
            let cleaned = operation_tracker_clone.cleanup_old_operations(24).await; // 24 hours
            if cleaned > 0 {
                warn!("Cleaned up {} stuck operations older than 24 hours", cleaned);
            }
        }
    });

    // Start periodic maintenance cleanup (every 6 hours)
    let maintenance_tracker_clone = maintenance_tracker.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(21600)); // 6 hours
        loop {
            interval.tick().await;
            let cleaned = maintenance_tracker_clone.cleanup_expired_maintenance(48).await; // 48 hours max
            if cleaned > 0 {
                warn!("Cleaned up {} expired maintenance windows older than 48 hours", cleaned);
            }
        }
    });

    info!("Background tasks started with {}s health check interval (including auto-restore monitoring and centralized alerting)", check_interval);

    // Start web server
    start_web_server(
        config,
        database,
        health_monitor,
        http_manager,
        Arc::new(config_manager),
        snapshot_manager,
        operation_tracker,
    ).await?;

    Ok(())
}
