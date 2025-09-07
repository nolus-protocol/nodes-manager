// File: manager/src/health/monitor.rs
use crate::config::{Config, NodeConfig, ServerConfig, EtlConfig};
use crate::database::{Database, HealthRecord};
use crate::maintenance_tracker::MaintenanceTracker;
use crate::snapshot::SnapshotManager;
use crate::services::alert_service::{AlertService, AlertType, AlertSeverity};
use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use futures::future::join_all;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub node_name: String,
    pub rpc_url: String,
    pub is_healthy: bool,
    pub error_message: Option<String>,
    pub last_check: DateTime<Utc>,
    pub block_height: Option<i64>,
    pub is_syncing: Option<bool>,
    pub is_catching_up: bool,
    pub validator_address: Option<String>,
    pub network: String,
    pub server_host: String,
    pub enabled: bool,
    pub in_maintenance: bool,
}

// NEW: ETL service health status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EtlHealthStatus {
    pub service_name: String,
    pub service_url: String,
    pub is_healthy: bool,
    pub error_message: Option<String>,
    pub last_check: DateTime<Utc>,
    pub response_time_ms: Option<u64>,
    pub status_code: Option<u16>,
    pub server_host: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: String,
    pub result: Option<StatusResult>,
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResult {
    pub node_info: NodeInfo,
    pub sync_info: SyncInfo,
    pub validator_info: ValidatorInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub network: String,
    pub moniker: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncInfo {
    pub latest_block_height: String,
    pub catching_up: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorInfo {
    pub address: String,
    pub voting_power: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

// Auto-restore cooldown tracking
#[derive(Debug, Clone)]
struct AutoRestoreCooldown {
    last_restore_attempt: DateTime<Utc>,
    restore_count: u32,
}

// Block height tracking for progression detection
#[derive(Debug, Clone)]
struct BlockHeightState {
    last_height: i64,
    last_updated: DateTime<Utc>,
    consecutive_no_progress: u32,
}

pub struct HealthMonitor {
    config: Arc<Config>,
    database: Arc<Database>,
    maintenance_tracker: Arc<MaintenanceTracker>,
    snapshot_manager: Arc<SnapshotManager>,
    alert_service: Arc<AlertService>,
    client: HttpClient,
    auto_restore_cooldowns: Arc<Mutex<HashMap<String, AutoRestoreCooldown>>>,
    block_height_states: Arc<Mutex<HashMap<String, BlockHeightState>>>,
    auto_restore_checked_states: Arc<Mutex<HashMap<String, bool>>>,
    etl_client: HttpClient, // NEW: Separate client for ETL health checks
}

impl HealthMonitor {
    pub fn new(
        config: Arc<Config>,
        database: Arc<Database>,
        maintenance_tracker: Arc<MaintenanceTracker>,
        snapshot_manager: Arc<SnapshotManager>,
        alert_service: Arc<AlertService>,
    ) -> Self {
        let client = HttpClient::builder()
            .timeout(Duration::from_secs(config.rpc_timeout_seconds))
            .build()
            .expect("Failed to create HTTP client");

        // NEW: ETL client with shorter timeout for quick health checks
        let etl_client = HttpClient::builder()
            .timeout(Duration::from_secs(10)) // 10 second timeout for ETL services
            .build()
            .expect("Failed to create ETL HTTP client");

        Self {
            config,
            database,
            maintenance_tracker,
            snapshot_manager,
            alert_service,
            client,
            auto_restore_cooldowns: Arc::new(Mutex::new(HashMap::new())),
            block_height_states: Arc::new(Mutex::new(HashMap::new())),
            auto_restore_checked_states: Arc::new(Mutex::new(HashMap::new())),
            etl_client,
        }
    }

    pub async fn check_all_nodes(&self) -> Result<Vec<HealthStatus>> {
        let mut health_statuses = Vec::new();
        let mut tasks = Vec::new();

        for (node_name, node_config) in &self.config.nodes {
            if !node_config.enabled {
                continue;
            }

            // Skip health checks entirely for nodes in maintenance
            if self.maintenance_tracker.is_in_maintenance(node_name).await {
                info!("Skipping health check for {} - node in maintenance mode", node_name);

                let maintenance_status = HealthStatus {
                    node_name: node_name.clone(),
                    rpc_url: node_config.rpc_url.clone(),
                    is_healthy: false,
                    error_message: Some("Node is in maintenance mode - health checks suspended".to_string()),
                    last_check: Utc::now(),
                    block_height: None,
                    is_syncing: None,
                    is_catching_up: false,
                    validator_address: None,
                    network: node_config.network.clone(),
                    server_host: node_config.server_host.clone(),
                    enabled: node_config.enabled,
                    in_maintenance: true,
                };

                health_statuses.push(maintenance_status);
                continue;
            }

            let task = {
                let node_name = node_name.clone();
                let node_config = node_config.clone();
                let monitor = self.clone();
                tokio::spawn(async move { monitor.check_node_health(&node_name, &node_config).await })
            };
            tasks.push(task);
        }

        let results = join_all(tasks).await;
        for result in results {
            match result {
                Ok(Ok(status)) => health_statuses.push(status),
                Ok(Err(e)) => error!("Health check failed: {}", e),
                Err(e) => error!("Health check task panicked: {}", e),
            }
        }

        // Store results in database and handle alerts
        for status in &health_statuses {
            if let Err(e) = self.store_health_record(status).await {
                error!("Failed to store health record for {}: {}", status.node_name, e);
            }

            // Send alerts using centralized AlertService
            if let Err(e) = self.handle_health_alerts(status).await {
                error!("Failed to handle health alerts for {}: {}", status.node_name, e);
            }
        }

        // Per-node log monitoring - only for healthy nodes NOT in maintenance
        let non_maintenance_statuses: Vec<_> = health_statuses.iter()
            .filter(|status| !status.in_maintenance)
            .collect();

        if !non_maintenance_statuses.is_empty() {
            if let Err(e) = self.monitor_logs_per_node(&non_maintenance_statuses).await {
                error!("Log monitoring failed: {}", e);
            }
        }

        // Auto-restore monitoring - only check UNHEALTHY nodes ONCE per unhealthy period
        let non_maintenance_statuses: Vec<_> = health_statuses.iter()
            .filter(|status| !status.in_maintenance)
            .cloned()
            .collect();

        if !non_maintenance_statuses.is_empty() {
            if let Err(e) = self.monitor_auto_restore_triggers(&non_maintenance_statuses).await {
                error!("Auto-restore monitoring failed: {}", e);
            }
        }

        Ok(health_statuses)
    }

    // NEW: Check all ETL services health
    pub async fn check_all_etl_services(&self) -> Result<Vec<EtlHealthStatus>> {
        let mut etl_statuses = Vec::new();
        let mut tasks = Vec::new();

        for (service_name, etl_config) in &self.config.etl {
            if !etl_config.enabled {
                continue;
            }

            let task = {
                let service_name = service_name.clone();
                let etl_config = etl_config.clone();
                let monitor = self.clone();
                tokio::spawn(async move { monitor.check_etl_service_health(&service_name, &etl_config).await })
            };
            tasks.push(task);
        }

        let results = join_all(tasks).await;
        for result in results {
            match result {
                Ok(Ok(status)) => etl_statuses.push(status),
                Ok(Err(e)) => error!("ETL health check failed: {}", e),
                Err(e) => error!("ETL health check task panicked: {}", e),
            }
        }

        // Store results in database and handle alerts
        for status in &etl_statuses {
            if let Err(e) = self.store_etl_health_record(status).await {
                error!("Failed to store ETL health record for {}: {}", status.service_name, e);
            }

            // Send alerts using centralized AlertService
            if let Err(e) = self.handle_etl_health_alerts(status).await {
                error!("Failed to handle ETL health alerts for {}: {}", status.service_name, e);
            }
        }

        Ok(etl_statuses)
    }

    // NEW: Check individual ETL service health
    pub async fn check_etl_service_health(&self, service_name: &str, etl_config: &EtlConfig) -> Result<EtlHealthStatus> {
        let endpoint = etl_config.endpoint.as_deref().unwrap_or("/health");
        let service_url = format!("https://{}:{}{}", etl_config.host, etl_config.port, endpoint);

        let mut status = EtlHealthStatus {
            service_name: service_name.to_string(),
            service_url: service_url.clone(),
            is_healthy: false,
            error_message: None,
            last_check: Utc::now(),
            response_time_ms: None,
            status_code: None,
            server_host: etl_config.server_host.clone(),
            enabled: etl_config.enabled,
        };

        let start_time = std::time::Instant::now();

        match self.fetch_etl_health(&service_url, etl_config.timeout_seconds.unwrap_or(10)).await {
            Ok(response_status) => {
                let response_time = start_time.elapsed().as_millis() as u64;
                status.response_time_ms = Some(response_time);
                status.status_code = Some(response_status);

                if response_status == 200 {
                    status.is_healthy = true;
                    debug!("ETL service {} is healthy ({}ms response)", service_name, response_time);
                } else {
                    status.error_message = Some(format!("HTTP status {}", response_status));
                    debug!("ETL service {} returned status {} ({}ms response)", service_name, response_status, response_time);
                }
            }
            Err(e) => {
                let response_time = start_time.elapsed().as_millis() as u64;
                status.response_time_ms = Some(response_time);
                status.error_message = Some(e.to_string());
                debug!("ETL service {} health check failed: {} ({}ms)", service_name, e, response_time);
            }
        }

        Ok(status)
    }

    // NEW: Fetch ETL service health via HTTP
    async fn fetch_etl_health(&self, url: &str, timeout_seconds: u64) -> Result<u16> {
        let response = timeout(
            Duration::from_secs(timeout_seconds),
            self.etl_client.get(url).send(),
        )
        .await
        .map_err(|_| anyhow!("ETL health check timeout"))?
        .map_err(|e| anyhow!("ETL HTTP request failed: {}", e))?;

        Ok(response.status().as_u16())
    }

    // NEW: Handle ETL health alerts
    async fn handle_etl_health_alerts(&self, status: &EtlHealthStatus) -> Result<()> {
        let details = Some(serde_json::json!({
            "service_url": status.service_url,
            "response_time_ms": status.response_time_ms,
            "status_code": status.status_code,
            "last_check": status.last_check.to_rfc3339()
        }));

        self.alert_service.send_progressive_alert(
            &status.service_name,
            &status.server_host,
            status.is_healthy,
            status.error_message.clone(),
            details,
        ).await
    }

    // NEW: Store ETL health record (reuse existing table with service type)
    async fn store_etl_health_record(&self, status: &EtlHealthStatus) -> Result<()> {
        let record = HealthRecord {
            node_name: format!("etl:{}", status.service_name),
            is_healthy: status.is_healthy,
            error_message: status.error_message.clone(),
            timestamp: status.last_check,
            block_height: status.status_code.map(|code| code as i64),
            is_syncing: None,
            is_catching_up: None,
            validator_address: Some(status.service_url.clone()),
        };

        self.database.store_health_record(&record).await
    }

    // NEW: Get ETL service health from database
    pub async fn get_etl_service_health(&self, service_name: &str) -> Result<Option<EtlHealthStatus>> {
        let etl_key = format!("etl:{}", service_name);
        let record = self.database.get_latest_health_record(&etl_key).await?;

        match record {
            Some(record) => {
                let etl_config = self.config.etl.get(service_name)
                    .ok_or_else(|| anyhow!("ETL service {} not found in configuration", service_name))?;

                let status = EtlHealthStatus {
                    service_name: service_name.to_string(),
                    service_url: record.validator_address.unwrap_or_default(),
                    is_healthy: record.is_healthy,
                    error_message: record.error_message,
                    last_check: record.timestamp,
                    response_time_ms: None, // Not stored in database
                    status_code: record.block_height.map(|h| h as u16),
                    server_host: etl_config.server_host.clone(),
                    enabled: etl_config.enabled,
                };

                Ok(Some(status))
            }
            None => Ok(None),
        }
    }

    // Handle health alerts using centralized AlertService
    async fn handle_health_alerts(&self, status: &HealthStatus) -> Result<()> {
        // Skip alerts for maintenance nodes
        if status.in_maintenance {
            return Ok(());
        }

        let details = Some(serde_json::json!({
            "rpc_url": status.rpc_url,
            "block_height": status.block_height,
            "is_catching_up": status.is_catching_up,
            "network": status.network,
            "last_check": status.last_check.to_rfc3339()
        }));

        self.alert_service.send_progressive_alert(
            &status.node_name,
            &status.server_host,
            status.is_healthy,
            status.error_message.clone(),
            details,
        ).await
    }

    // Auto-restore monitoring - only for UNHEALTHY nodes and only ONCE per unhealthy period
    async fn monitor_auto_restore_triggers(&self, health_statuses: &[HealthStatus]) -> Result<()> {
        let trigger_words = match &self.config.auto_restore_trigger_words {
            Some(words) if !words.is_empty() => words,
            _ => return Ok(()),
        };

        let unhealthy_nodes: Vec<_> = health_statuses.iter()
            .filter(|status| !status.is_healthy && status.enabled && !status.in_maintenance)
            .collect();

        if unhealthy_nodes.is_empty() {
            debug!("No unhealthy nodes to check for auto-restore triggers");
            return Ok(());
        }

        info!("Checking auto-restore triggers for {} unhealthy nodes", unhealthy_nodes.len());

        let mut tasks = Vec::new();

        for status in unhealthy_nodes {
            if self.has_already_checked_auto_restore(&status.node_name).await {
                debug!("Auto-restore triggers already checked for {} during current unhealthy state", status.node_name);
                continue;
            }

            let node_config = match self.config.nodes.get(&status.node_name) {
                Some(config) => config,
                None => continue,
            };

            if !node_config.auto_restore_enabled.unwrap_or(false) {
                continue;
            }

            let log_path = match &node_config.log_path {
                Some(path) => path,
                None => continue,
            };

            let node_name = status.node_name.clone();
            let server_host = status.server_host.clone();
            let log_path = log_path.clone();
            let trigger_words = trigger_words.clone();
            let monitor = self.clone();

            let task = tokio::spawn(async move {
                monitor.check_auto_restore_triggers_for_node(&node_name, &server_host, &log_path, &trigger_words).await
            });

            tasks.push(task);
        }

        let results = join_all(tasks).await;
        for result in results {
            if let Err(e) = result {
                error!("Auto-restore trigger check task failed: {}", e);
            }
        }

        Ok(())
    }

    async fn has_already_checked_auto_restore(&self, node_name: &str) -> bool {
        let checked_states = self.auto_restore_checked_states.lock().await;
        checked_states.get(node_name).copied().unwrap_or(false)
    }

    async fn mark_auto_restore_checked(&self, node_name: &str) {
        let mut checked_states = self.auto_restore_checked_states.lock().await;
        checked_states.insert(node_name.to_string(), true);
        debug!("Marked {} as checked for auto-restore triggers", node_name);
    }

    async fn check_auto_restore_triggers_for_node(
        &self,
        node_name: &str,
        server_host: &str,
        log_path: &str,
        trigger_words: &[String],
    ) -> Result<()> {
        if !self.is_auto_restore_allowed(node_name).await {
            debug!("Auto-restore for {} is in cooldown period", node_name);
            self.mark_auto_restore_checked(node_name).await;
            return Ok(());
        }

        let server_config = self.config.servers.get(server_host)
            .ok_or_else(|| anyhow!("Server {} not found", server_host))?;

        let log_file = format!("{}/out1.log", log_path);
        let command = format!(
            "tail -n 500 '{}' | grep -q -E '{}'",
            log_file,
            trigger_words.join("|")
        );

        self.mark_auto_restore_checked(node_name).await;

        match self.execute_log_command(server_config, &command).await {
            Ok(_) => {
                warn!("Auto-restore trigger words found in {} log file: {}", node_name, log_file);
                if let Err(e) = self.execute_auto_restore(node_name, trigger_words).await {
                    error!("Auto-restore failed for {}: {}", node_name, e);
                    // Send failure alert using AlertService
                    self.alert_service.send_immediate_alert(
                        AlertType::AutoRestore,
                        AlertSeverity::Critical,
                        node_name,
                        server_host,
                        format!("CRITICAL: Auto-restore failed for {} - manual intervention required", node_name),
                        Some(serde_json::json!({
                            "error_message": e.to_string(),
                            "trigger_words": trigger_words
                        })),
                    ).await?;
                } else {
                    info!("Auto-restore completed successfully for {}", node_name);
                }
            }
            Err(_) => {
                debug!("No auto-restore trigger words found for {}", node_name);
            }
        }

        Ok(())
    }

    async fn is_auto_restore_allowed(&self, node_name: &str) -> bool {
        let cooldowns = self.auto_restore_cooldowns.lock().await;
        let now = Utc::now();

        match cooldowns.get(node_name) {
            Some(cooldown) => {
                let hours_since_last = (now - cooldown.last_restore_attempt).num_hours();
                if hours_since_last >= 2 {
                    true
                } else {
                    debug!("Auto-restore for {} is in cooldown ({}h remaining)",
                          node_name, 2 - hours_since_last);
                    false
                }
            }
            None => true,
        }
    }

    async fn execute_auto_restore(&self, node_name: &str, trigger_words: &[String]) -> Result<()> {
        info!("Executing auto-restore for {} due to trigger words: {:?}", node_name, trigger_words);

        {
            let mut cooldowns = self.auto_restore_cooldowns.lock().await;
            let now = Utc::now();
            let cooldown = cooldowns.entry(node_name.to_string()).or_insert(AutoRestoreCooldown {
                last_restore_attempt: now,
                restore_count: 0,
            });
            cooldown.last_restore_attempt = now;
            cooldown.restore_count += 1;
        }

        // Send starting notification
        let server_host = self.get_server_for_node(node_name).await.unwrap_or_else(|| "unknown".to_string());
        self.alert_service.send_immediate_alert(
            AlertType::AutoRestore,
            AlertSeverity::Warning,
            node_name,
            &server_host,
            format!("Auto-restore STARTED for {} due to corruption indicators", node_name),
            Some(serde_json::json!({
                "trigger_words": trigger_words,
                "status": "starting"
            })),
        ).await?;

        match self.snapshot_manager.restore_from_snapshot(node_name).await {
            Ok(snapshot_info) => {
                info!("Auto-restore completed for {} using snapshot: {}", node_name, snapshot_info.filename);
                self.alert_service.send_immediate_alert(
                    AlertType::AutoRestore,
                    AlertSeverity::Info,
                    node_name,
                    &server_host,
                    format!("Auto-restore COMPLETED for {} - node should be syncing from restored state", node_name),
                    Some(serde_json::json!({
                        "trigger_words": trigger_words,
                        "status": "completed",
                        "snapshot_filename": snapshot_info.filename
                    })),
                ).await?;
                Ok(())
            }
            Err(e) => {
                error!("Auto-restore failed for {}: {}", node_name, e);
                Err(e)
            }
        }
    }

    async fn get_server_for_node(&self, node_name: &str) -> Option<String> {
        if let Some(dash_pos) = node_name.find('-') {
            let server_part = &node_name[..dash_pos];
            if self.config.servers.contains_key(server_part) {
                return Some(server_part.to_string());
            }
        }

        for (config_node_name, node_config) in &self.config.nodes {
            if config_node_name == node_name {
                return Some(node_config.server_host.clone());
            }
        }

        None
    }

    // Enhanced health check with block progression tracking
    pub async fn check_node_health(&self, node_name: &str, node_config: &NodeConfig) -> Result<HealthStatus> {
        let mut status = HealthStatus {
            node_name: node_name.to_string(),
            rpc_url: node_config.rpc_url.clone(),
            is_healthy: false,
            error_message: None,
            last_check: Utc::now(),
            block_height: None,
            is_syncing: None,
            is_catching_up: false,
            validator_address: None,
            network: node_config.network.clone(),
            server_host: node_config.server_host.clone(),
            enabled: node_config.enabled,
            in_maintenance: false,
        };

        match self.fetch_node_status(&node_config.rpc_url).await {
            Ok(rpc_response) => {
                if let Some(result) = rpc_response.result {
                    let current_height = result.sync_info.latest_block_height.parse::<i64>().unwrap_or(0);
                    let is_catching_up = result.sync_info.catching_up;

                    status.block_height = Some(current_height);
                    status.is_catching_up = is_catching_up;
                    status.is_syncing = Some(is_catching_up);
                    status.validator_address = Some(result.validator_info.address);

                    let block_progression_healthy = self.check_block_progression(node_name, current_height).await;

                    status.is_healthy = block_progression_healthy || is_catching_up;

                    if !status.is_healthy && !is_catching_up {
                        status.error_message = Some("Block height not progressing".to_string());
                    }

                } else if let Some(error) = rpc_response.error {
                    status.error_message = Some(format!("RPC Error: {}", error.message));
                } else {
                    status.error_message = Some("Unknown RPC response format".to_string());
                }
            }
            Err(e) => {
                status.error_message = Some(e.to_string());
            }
        }

        Ok(status)
    }

    async fn check_block_progression(&self, node_name: &str, current_height: i64) -> bool {
        let mut block_states = self.block_height_states.lock().await;
        let now = Utc::now();

        match block_states.get_mut(node_name) {
            None => {
                block_states.insert(node_name.to_string(), BlockHeightState {
                    last_height: current_height,
                    last_updated: now,
                    consecutive_no_progress: 0,
                });
                true
            }
            Some(state) => {
                let minutes_since_update = (now - state.last_updated).num_minutes();

                if minutes_since_update < 2 {
                    return true;
                }

                if current_height > state.last_height {
                    state.last_height = current_height;
                    state.last_updated = now;
                    state.consecutive_no_progress = 0;
                    true
                } else if current_height == state.last_height {
                    state.consecutive_no_progress += 1;
                    state.last_updated = now;
                    state.consecutive_no_progress < 3
                } else {
                    state.last_height = current_height;
                    state.last_updated = now;
                    state.consecutive_no_progress += 1;
                    false
                }
            }
        }
    }

    async fn fetch_node_status(&self, rpc_url: &str) -> Result<RpcResponse> {
        let request_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "status",
            "params": [],
            "id": Uuid::new_v4().to_string()
        });

        let response = timeout(
            Duration::from_secs(self.config.rpc_timeout_seconds),
            self.client.post(rpc_url).json(&request_body).send(),
        )
        .await
        .map_err(|_| anyhow!("RPC request timeout"))?
        .map_err(|e| anyhow!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!(
                "HTTP error {}: {}",
                response.status(),
                response.text().await.unwrap_or_default()
            ));
        }

        let rpc_response: RpcResponse = response
            .json()
            .await
            .map_err(|e| anyhow!("Failed to parse JSON response: {}", e))?;

        Ok(rpc_response)
    }

    pub async fn get_node_health(&self, node_name: &str) -> Result<Option<HealthStatus>> {
        let record = self.database.get_latest_health_record(node_name).await?;

        match record {
            Some(record) => {
                let node_config = self.config.nodes.get(node_name)
                    .ok_or_else(|| anyhow!("Node {} not found in configuration", node_name))?;

                let is_in_maintenance = self.maintenance_tracker
                    .is_in_maintenance(node_name)
                    .await;

                let status = HealthStatus {
                    node_name: record.node_name,
                    rpc_url: node_config.rpc_url.clone(),
                    is_healthy: record.is_healthy,
                    error_message: record.error_message,
                    last_check: record.timestamp,
                    block_height: record.block_height,
                    is_syncing: record.is_syncing.map(|s| s != 0),
                    is_catching_up: record.is_catching_up.unwrap_or(0) != 0,
                    validator_address: record.validator_address,
                    network: node_config.network.clone(),
                    server_host: node_config.server_host.clone(),
                    enabled: node_config.enabled,
                    in_maintenance: is_in_maintenance,
                };

                Ok(Some(status))
            }
            None => Ok(None),
        }
    }

    async fn store_health_record(&self, status: &HealthStatus) -> Result<()> {
        let record = HealthRecord {
            node_name: status.node_name.clone(),
            is_healthy: status.is_healthy,
            error_message: status.error_message.clone(),
            timestamp: status.last_check,
            block_height: status.block_height,
            is_syncing: status.is_syncing.map(|s| if s { 1 } else { 0 }),
            is_catching_up: Some(if status.is_catching_up { 1 } else { 0 }),
            validator_address: status.validator_address.clone(),
        };

        self.database.store_health_record(&record).await
    }

    // Per-node log monitoring
    async fn monitor_logs_per_node(&self, health_statuses: &[&HealthStatus]) -> Result<()> {
        let context_lines_value: i32 = self.config.log_monitoring_context_lines.unwrap_or(2);
        let mut tasks = Vec::new();

        for status in health_statuses {
            if !status.is_healthy {
                continue;
            }

            let node_config = match self.config.nodes.get(&status.node_name) {
                Some(config) => config,
                None => continue,
            };

            if !node_config.log_monitoring_enabled.unwrap_or(false) {
                debug!("Log monitoring disabled for node: {}", status.node_name);
                continue;
            }

            let patterns = match &node_config.log_monitoring_patterns {
                Some(patterns) if !patterns.is_empty() => patterns,
                _ => {
                    debug!("No log monitoring patterns configured for node: {}", status.node_name);
                    continue;
                }
            };

            let log_path = match &node_config.log_path {
                Some(path) => path,
                None => {
                    debug!("No log path configured for node: {}", status.node_name);
                    continue;
                }
            };

            let node_name = status.node_name.clone();
            let server_host = status.server_host.clone();
            let log_path = log_path.clone();
            let patterns = patterns.clone();
            let monitor = self.clone();
            let context_lines = context_lines_value;

            let task = tokio::spawn(async move {
                monitor.check_node_logs(&node_name, &server_host, &log_path, &patterns, context_lines).await
            });

            tasks.push(task);
        }

        if tasks.is_empty() {
            debug!("No nodes have log monitoring enabled");
            return Ok(());
        }

        info!("Running log monitoring for {} nodes with individual patterns", tasks.len());

        let results = join_all(tasks).await;
        for result in results {
            if let Err(e) = result {
                error!("Log monitoring task failed: {}", e);
            }
        }

        Ok(())
    }

    async fn check_node_logs(&self, node_name: &str, server_host: &str, log_path: &str, patterns: &[String], context_lines: i32) -> Result<()> {
        let server_config = self.config.servers.get(server_host)
            .ok_or_else(|| anyhow!("Server {} not found", server_host))?;

        let command = format!(
            "tail -n 1000 {}/out1.log | grep -n -A {} -B {} -E '{}'",
            log_path,
            context_lines,
            context_lines,
            patterns.join("|")
        );

        debug!("Checking log patterns for {}: {:?}", node_name, patterns);

        match self.execute_log_command(server_config, &command).await {
            Ok(output) => {
                if !output.trim().is_empty() {
                    info!("Log patterns detected for {}, sending alert", node_name);
                    // Send log pattern alert using AlertService
                    self.alert_service.send_immediate_alert(
                        AlertType::LogPattern,
                        AlertSeverity::Warning,
                        node_name,
                        server_host,
                        "Log pattern match detected".to_string(),
                        Some(serde_json::json!({
                            "log_path": log_path,
                            "log_output": output,
                            "patterns": patterns
                        })),
                    ).await?;
                } else {
                    debug!("No log patterns found for {}", node_name);
                }
            }
            Err(e) => {
                debug!("Log monitoring for {} failed: {}", node_name, e);
            }
        }

        Ok(())
    }

    async fn execute_log_command(&self, server_config: &ServerConfig, command: &str) -> Result<String> {
        let agent_url = format!("http://{}:{}/command/execute", server_config.host, server_config.agent_port);

        let payload = serde_json::json!({
            "command": command
        });

        let response = timeout(
            Duration::from_secs(server_config.request_timeout_seconds),
            self.client.post(&agent_url)
                .header("Authorization", format!("Bearer {}", server_config.api_key))
                .json(&payload)
                .send(),
        )
        .await
        .map_err(|_| anyhow!("Log command timeout"))?
        .map_err(|e| anyhow!("Log command request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(anyhow!("Log command returned status: {}", response.status()));
        }

        let result: serde_json::Value = response.json().await?;
        let output = result.get("output")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        Ok(output)
    }
}

impl Clone for HealthMonitor {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            database: self.database.clone(),
            maintenance_tracker: self.maintenance_tracker.clone(),
            snapshot_manager: self.snapshot_manager.clone(),
            alert_service: self.alert_service.clone(),
            client: self.client.clone(),
            auto_restore_cooldowns: self.auto_restore_cooldowns.clone(),
            block_height_states: self.block_height_states.clone(),
            auto_restore_checked_states: self.auto_restore_checked_states.clone(),
            etl_client: self.etl_client.clone(),
        }
    }
}
