// File: manager/src/main.rs
use anyhow::Result;
use std::sync::Arc;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, EnvFilter};

mod config;
mod constants;
mod database;
mod health;
mod http;
mod maintenance_tracker;
mod operation_tracker;
mod scheduler;
mod services;
mod snapshot;
mod state_sync;
mod web;

use config::ConfigManager;
use constants::cleanup;
use database::Database;
use health::HealthMonitor;
use http::HttpAgentManager;
use maintenance_tracker::MaintenanceTracker;
use operation_tracker::SimpleOperationTracker;
use scheduler::MaintenanceScheduler;
use services::{AlertService, HermesService, MaintenanceService, SnapshotService};
use snapshot::SnapshotManager;

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

    fmt().with_env_filter(env_filter).init();

    info!("Starting Blockchain Infrastructure Manager");

    // Load configuration
    let config_manager = ConfigManager::new("config".to_string()).await?;
    let config = config_manager.get_current_config();
    info!(
        "Configuration loaded: {} nodes, {} hermes instances, {} servers, {} ETL services",
        config.nodes.len(),
        config.hermes.len(),
        config.servers.len(),
        config.etl.len()
    );

    // Initialize database
    let database = Arc::new(Database::new("data/nodes.db").await?);
    info!("Database initialized");

    // Initialize operation tracker
    let operation_tracker = Arc::new(SimpleOperationTracker::new());
    info!("Operation tracker initialized");

    // Initialize maintenance tracker
    let maintenance_tracker = Arc::new(MaintenanceTracker::new());
    info!("Maintenance tracker initialized");

    // Initialize centralized AlertService with enhanced validation
    let alert_service = Arc::new(AlertService::new(config.alarm_webhook_url.clone()));

    // Validate alert service configuration
    if alert_service.is_enabled() {
        info!(
            "Alert service enabled with webhook: {}",
            alert_service.get_webhook_url()
        );

        // Test webhook connectivity on startup
        match alert_service.test_webhook().await {
            Ok(()) => info!("Alert webhook test successful!"),
            Err(e) => {
                error!("Alert webhook test failed: {}", e);
                warn!("Alerts may not work properly. Check your webhook URL and network connectivity.");
            }
        }
    } else {
        warn!("⚠️  ALERT SERVICE DISABLED ⚠️");
        warn!("No webhook URL configured in config/main.toml");
        warn!("Set 'alarm_webhook_url = \"your-webhook-url\"' to enable alerts");
    }

    // Initialize HTTP agent manager with operation tracking AND maintenance tracking
    let http_manager = Arc::new(HttpAgentManager::new(
        config.clone(),
        operation_tracker.clone(),
        maintenance_tracker.clone(),
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

    // Start periodic health monitoring with configurable interval (including ETL services)
    let health_monitor_clone = health_monitor.clone();
    let alert_service_clone = alert_service.clone();
    let check_interval = config.check_interval_seconds;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(check_interval));
        let mut check_count = 0u64;

        loop {
            interval.tick().await;
            check_count += 1;

            // Log periodic health check status for debugging
            if check_count.is_multiple_of(10) {
                info!(
                    "Health monitoring cycle #{} - Alert service enabled: {}",
                    check_count,
                    alert_service_clone.is_enabled()
                );
            }

            // Check blockchain nodes
            if let Err(e) = health_monitor_clone.check_all_nodes().await {
                warn!("Node health monitoring error: {}", e);
            }

            // NEW: Check ETL services
            if let Err(e) = health_monitor_clone.check_all_etl_services().await {
                warn!("ETL health monitoring error: {}", e);
            }
        }
    });

    // Start periodic operation cleanup (configurable interval)
    let operation_tracker_clone = operation_tracker.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            cleanup::CLEANUP_INTERVAL_SECONDS,
        ));
        loop {
            interval.tick().await;
            let cleaned = operation_tracker_clone
                .cleanup_old_operations(cleanup::OPERATION_CLEANUP_HOURS)
                .await;
            if cleaned > 0 {
                warn!(
                    "Cleaned up {} stuck operations older than {} hours",
                    cleaned,
                    cleanup::OPERATION_CLEANUP_HOURS
                );
            }
        }
    });

    // Start periodic maintenance cleanup (configurable interval)
    let maintenance_tracker_clone = maintenance_tracker.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            cleanup::CLEANUP_INTERVAL_SECONDS * 6,
        ));
        loop {
            interval.tick().await;
            let cleaned = maintenance_tracker_clone
                .cleanup_expired_maintenance(cleanup::MAINTENANCE_CLEANUP_HOURS as u32)
                .await;
            if cleaned > 0 {
                warn!(
                    "Cleaned up {} expired maintenance windows older than {} hours",
                    cleaned,
                    cleanup::MAINTENANCE_CLEANUP_HOURS
                );
            }
        }
    });

    info!("Background tasks started with {}s health check interval (including nodes, ETL services, auto-restore monitoring and centralized alerting)", check_interval);

    // Additional startup alert validation
    if alert_service.is_enabled() {
        info!(
            "✅ Alert system ready - alerts will be sent to: {}",
            alert_service.get_webhook_url()
        );
    } else {
        error!("❌ Alert system NOT configured - no alerts will be sent!");
        error!("Add this to your config/main.toml file:");
        error!("alarm_webhook_url = \"https://n8n-hooks.kostovster.io/webhook/nodes\"");
    }

    // Initialize business logic services with AlertService integration
    let hermes_service = Arc::new(HermesService::new(
        config.clone(),
        http_manager.clone(),
        alert_service.clone(),
    ));
    info!("HermesService initialized with alert integration");

    let maintenance_service = Arc::new(MaintenanceService::new(
        config.clone(),
        database.clone(),
        http_manager.clone(),
        alert_service.clone(),
    ));
    info!("MaintenanceService initialized with alert integration");

    let snapshot_service_v2 = Arc::new(SnapshotService::new(
        config.clone(),
        snapshot_manager.clone(),
    ));
    info!("SnapshotService initialized");

    // Initialize and start scheduler with service layer integration
    let scheduler = Arc::new(
        MaintenanceScheduler::new(
            config.clone(),
            maintenance_service.clone(),
            hermes_service.clone(),
        )
        .await?,
    );
    info!("Maintenance scheduler initialized with service layer");

    // Start scheduler
    scheduler.start().await?;
    info!("Scheduler started with AlertService integration");

    // Start web server
    start_web_server(
        config,
        database,
        health_monitor,
        http_manager,
        Arc::new(config_manager),
        snapshot_manager,
        operation_tracker,
        hermes_service,
        maintenance_service,
        snapshot_service_v2,
    )
    .await?;

    Ok(())
}
