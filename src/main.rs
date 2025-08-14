// File: src/main.rs

mod config;
mod database;
mod health;
mod maintenance_tracker;
mod scheduler;
mod ssh;
mod web;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, error};
use tracing_subscriber;

// Core data structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub check_interval_seconds: u64,
    pub rpc_timeout_seconds: u64,
    pub alarm_webhook_url: String,
    pub hermes_min_uptime_minutes: u64,
    pub servers: HashMap<String, ServerConfig>,
    pub nodes: HashMap<String, NodeConfig>,
    pub hermes: HashMap<String, HermesConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub ssh_key_path: String,
    pub ssh_username: String,
    pub max_concurrent_ssh: usize,
    pub ssh_timeout_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub rpc_url: String,
    pub network: String,
    pub server_host: String,
    pub enabled: bool,
    pub pruning_enabled: Option<bool>,
    pub pruning_schedule: Option<String>,
    pub pruning_keep_blocks: Option<u64>,
    pub pruning_keep_versions: Option<u64>,
    pub pruning_deploy_path: Option<String>,
    pub pruning_service_name: Option<String>,
    pub log_path: Option<String>,
    pub truncate_logs_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesConfig {
    pub server_host: String,
    pub service_name: String,
    pub log_path: String,
    pub restart_schedule: String,
    pub dependent_nodes: Vec<String>,
    pub truncate_logs_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHealth {
    pub node_name: String,
    pub status: HealthStatus,
    pub latest_block_height: Option<u64>,
    pub latest_block_time: Option<String>,
    pub catching_up: Option<bool>,
    pub last_check: DateTime<Utc>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Unhealthy,
    Unknown,
    Maintenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceOperation {
    pub id: String,
    pub operation_type: String,
    pub target_name: String,
    pub status: String,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmPayload {
    pub timestamp: DateTime<Utc>,
    pub alarm_type: String,
    pub severity: String,
    pub node_name: String,
    pub message: String,
    pub details: serde_json::Value,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            check_interval_seconds: 90,
            rpc_timeout_seconds: 10,
            alarm_webhook_url: "".to_string(),
            hermes_min_uptime_minutes: 5,
            servers: HashMap::new(),
            nodes: HashMap::new(),
            hermes: HashMap::new(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting Blockchain Nodes Manager with Maintenance Tracking");

    // Initialize database
    let db = Arc::new(database::init_database().await?);
    info!("Database initialized");

    // Load configuration
    let config_manager = config::ConfigManager::new("config".into());
    let config = Arc::new(config_manager.load_configs().await?);
    info!("Configuration loaded with {} servers, {} nodes, {} hermes instances",
          config.servers.len(), config.nodes.len(), config.hermes.len());

    // Initialize maintenance tracker
    let maintenance_tracker = Arc::new(maintenance_tracker::MaintenanceTracker::new());
    info!("Maintenance tracker initialized");

    // Initialize SSH manager with maintenance tracker
    let ssh_manager = Arc::new(ssh::SshManager::new(config.clone(), maintenance_tracker.clone()));
    info!("SSH manager initialized with maintenance integration");

    // Initialize health monitor with maintenance tracker
    let health_monitor = Arc::new(health::HealthMonitor::new(
        config.clone(),
        db.clone(),
        maintenance_tracker.clone(),
    ));
    info!("Health monitor initialized with maintenance awareness");

    // Initialize scheduler
    let scheduler = Arc::new(scheduler::MaintenanceScheduler::new(
        db.clone(),
        ssh_manager.clone(),
        config.clone(),
    ));
    info!("Maintenance scheduler initialized");

    // Start background tasks
    let health_task = {
        let monitor = health_monitor.clone();
        tokio::spawn(async move {
            if let Err(e) = monitor.start_monitoring().await {
                error!("Health monitoring error: {}", e);
            }
        })
    };

    let scheduler_task = {
        let sched = scheduler.clone();
        tokio::spawn(async move {
            if let Err(e) = sched.start_scheduler().await {
                error!("Scheduler error: {}", e);
            }
        })
    };

    // Start maintenance tracker cleanup task
    let maintenance_cleanup_task = {
        let tracker = maintenance_tracker.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1800)); // Every 30 minutes

            loop {
                interval.tick().await;

                // Cleanup maintenance windows that have been running too long (6 hours max)
                let cleaned = tracker.cleanup_expired_maintenance(6).await;
                if cleaned > 0 {
                    error!("Emergency cleaned up {} expired maintenance windows", cleaned);
                }
            }
        })
    };

    // Start web server with maintenance tracker
    let web_task = {
        let cfg = config.clone();
        let db_ref = db.clone();
        let monitor = health_monitor.clone();
        let ssh_mgr = ssh_manager.clone();
        let sched = scheduler.clone();
        let config_mgr = Arc::new(config_manager);
        let tracker = maintenance_tracker.clone();

        tokio::spawn(async move {
            if let Err(e) = web::start_web_server(
                cfg,
                db_ref,
                monitor,
                ssh_mgr,
                sched,
                config_mgr,
                tracker,
            ).await {
                error!("Web server error: {}", e);
            }
        })
    };

    info!("All services started successfully with maintenance tracking");
    info!("Web interface available at http://{}:{}", config.host, config.port);
    info!("Maintenance tracking enabled - nodes will show 'Maintenance' status during pruning");

    // Wait for all tasks
    tokio::try_join!(health_task, scheduler_task, maintenance_cleanup_task, web_task)?;

    Ok(())
}
