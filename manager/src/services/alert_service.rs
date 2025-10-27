// File: manager/src/services/alert_service.rs
use anyhow::Result;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

use crate::constants::alerts;

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertType {
    NodeHealth,
    AutoRestore,
    Snapshot,
    Hermes,
    LogPattern,
    Maintenance,
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertSeverity {
    Critical,
    Warning,
    Info,
    Recovery,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlertPayload {
    pub timestamp: DateTime<Utc>,
    pub alert_type: AlertType,
    pub severity: AlertSeverity,
    pub node_name: String,
    pub message: String,
    pub server_host: String,
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
struct AlertState {
    first_alert_time: DateTime<Utc>,
    last_alert_sent: DateTime<Utc>,
    alert_count: u32,
    consecutive_failures: u32,
    has_sent_alert: bool,
}

pub struct AlertService {
    webhook_url: String,
    client: Client,
    alert_states: Arc<Mutex<HashMap<String, AlertState>>>,
    previous_health_states: Arc<Mutex<HashMap<String, bool>>>,
    is_enabled: bool,
}

impl AlertService {
    pub fn new(webhook_url: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client for AlertService");

        let is_enabled = !webhook_url.trim().is_empty();

        if is_enabled {
            info!("AlertService initialized with webhook URL: {}", webhook_url);
        } else {
            warn!("AlertService initialized WITHOUT webhook URL - alerts will be disabled!");
            warn!("To enable alerts, set 'alarm_webhook_url' in your configuration file");
        }

        Self {
            webhook_url: webhook_url.trim().to_string(),
            client,
            alert_states: Arc::new(Mutex::new(HashMap::new())),
            previous_health_states: Arc::new(Mutex::new(HashMap::new())),
            is_enabled,
        }
    }

    /// Check if alert service is properly configured
    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    /// Get webhook URL for debugging
    pub fn get_webhook_url(&self) -> &str {
        &self.webhook_url
    }

    /// Send progressive alerts for ongoing failures with rate limiting
    pub async fn send_progressive_alert(
        &self,
        node_name: &str,
        server_host: &str,
        is_healthy: bool,
        error_message: Option<String>,
        details: Option<serde_json::Value>,
    ) -> Result<()> {
        // Check if we should process this health state change
        let should_process = {
            let mut previous_states = self.previous_health_states.lock().await;
            let previous_health = previous_states.get(node_name).copied();
            previous_states.insert(node_name.to_string(), is_healthy);

            match (previous_health, is_healthy) {
                (Some(true), false) | (None, false) => true, // Became unhealthy
                (Some(false), true) => {
                    // Became healthy - check if we should send recovery
                    return self
                        .send_recovery_alert_if_needed(node_name, server_host, details)
                        .await;
                }
                (Some(false), false) => true, // Still unhealthy
                _ => false,                   // Still healthy or no change
            }
        };

        if !should_process {
            return Ok(());
        }

        let mut alert_states = self.alert_states.lock().await;
        let now = Utc::now();

        let should_send_alert = match alert_states.get_mut(node_name) {
            None => {
                // First time seeing this node as unhealthy
                let alert_state = AlertState {
                    first_alert_time: now,
                    last_alert_sent: DateTime::<Utc>::MIN_UTC,
                    alert_count: 0,
                    consecutive_failures: 1,
                    has_sent_alert: false,
                };
                alert_states.insert(node_name.to_string(), alert_state);
                info!("Node {} unhealthy check 1/3 - no alert sent yet", node_name);
                false
            }

            Some(alert_state) => {
                alert_state.consecutive_failures += 1;

                if alert_state.alert_count == 0 {
                    // Haven't sent first alert yet
                    if alert_state.consecutive_failures >= alerts::FIRST_ALERT_AFTER_CHECKS {
                        alert_state.alert_count = 1;
                        alert_state.last_alert_sent = now;
                        alert_state.has_sent_alert = true;
                        info!(
                            "Node {} unhealthy for {} consecutive checks - sending first alert",
                            node_name,
                            alerts::FIRST_ALERT_AFTER_CHECKS
                        );
                        true
                    } else {
                        info!(
                            "Node {} unhealthy check {}/{} - no alert sent yet",
                            node_name,
                            alert_state.consecutive_failures,
                            alerts::FIRST_ALERT_AFTER_CHECKS
                        );
                        false
                    }
                } else {
                    // Already sent at least one alert
                    let time_since_last = now.signed_duration_since(alert_state.last_alert_sent);
                    let hours_since_last = time_since_last.num_hours();

                    let should_send = match alert_state.alert_count {
                        1 => hours_since_last >= alerts::SECOND_ALERT_INTERVAL_HOURS,
                        2 => hours_since_last >= alerts::THIRD_ALERT_INTERVAL_HOURS,
                        3 => hours_since_last >= alerts::FOURTH_ALERT_INTERVAL_HOURS,
                        4 => hours_since_last >= alerts::SUBSEQUENT_ALERT_INTERVAL_HOURS,
                        _ => hours_since_last >= alerts::SUBSEQUENT_ALERT_INTERVAL_HOURS,
                    };

                    if should_send {
                        alert_state.alert_count += 1;
                        alert_state.last_alert_sent = now;
                        let total_hours = now
                            .signed_duration_since(alert_state.first_alert_time)
                            .num_hours();
                        info!(
                            "Sending follow-up alert #{} for {} (unhealthy for {} hours)",
                            alert_state.alert_count, node_name, total_hours
                        );
                        true
                    } else {
                        debug!(
                            "Node {} still unhealthy but not yet time for next alert",
                            node_name
                        );
                        false
                    }
                }
            }
        };

        if should_send_alert {
            let message = error_message.unwrap_or_else(|| "Node health check failed".to_string());
            let payload = AlertPayload {
                timestamp: now,
                alert_type: AlertType::NodeHealth,
                severity: AlertSeverity::Critical,
                node_name: node_name.to_string(),
                message,
                server_host: server_host.to_string(),
                details,
            };

            self.send_webhook(&payload).await?;
        }

        Ok(())
    }

    /// Send immediate alerts for events that need instant notification
    pub async fn send_immediate_alert(
        &self,
        alert_type: AlertType,
        severity: AlertSeverity,
        node_name: &str,
        server_host: &str,
        message: String,
        details: Option<serde_json::Value>,
    ) -> Result<()> {
        let payload = AlertPayload {
            timestamp: Utc::now(),
            alert_type,
            severity,
            node_name: node_name.to_string(),
            message,
            server_host: server_host.to_string(),
            details,
        };

        self.send_webhook(&payload).await
    }

    // =========================================================================
    // HIGH-LEVEL ALERT METHODS - Business Logic Layer
    // =========================================================================
    // These methods encapsulate alerting decisions so services don't need to
    // know alert types, severities, or when to send alerts.

    /// Alert for maintenance operation failure (pruning, snapshot creation, restart, state sync)
    /// Only sends alerts for SCHEDULED operations (not manual API calls)
    pub async fn alert_operation_failed(
        &self,
        operation_type: &str,
        target_name: &str,
        server_host: &str,
        error: &str,
        is_scheduled: bool,
    ) -> Result<()> {
        // Only alert for scheduled operations
        if !is_scheduled {
            return Ok(());
        }

        self.send_immediate_alert(
            AlertType::Maintenance,
            AlertSeverity::Critical,
            target_name,
            server_host,
            format!("Scheduled {} failed for {}: {}", operation_type, target_name, error),
            Some(serde_json::json!({
                "operation_type": operation_type,
                "error_message": error,
                "scheduled": true
            })),
        )
        .await
    }

    /// Alert for Hermes restart failure
    pub async fn alert_hermes_failed(
        &self,
        hermes_name: &str,
        server_host: &str,
        error: &str,
    ) -> Result<()> {
        self.send_immediate_alert(
            AlertType::Hermes,
            AlertSeverity::Critical,
            hermes_name,
            server_host,
            format!("Hermes restart failed for {}: {}", hermes_name, error),
            Some(serde_json::json!({
                "operation_type": "hermes_restart",
                "error_message": error
            })),
        )
        .await
    }

    /// Alert for state sync failure
    pub async fn alert_state_sync_failed(
        &self,
        node_name: &str,
        server_host: &str,
        error: &str,
    ) -> Result<()> {
        self.send_immediate_alert(
            AlertType::Maintenance,
            AlertSeverity::Critical,
            node_name,
            server_host,
            format!("State sync failed for {}: {}", node_name, error),
            Some(serde_json::json!({
                "operation_type": "state_sync",
                "error_message": error
            })),
        )
        .await
    }

    /// Alert for snapshot restore failure
    pub async fn alert_snapshot_restore_failed(
        &self,
        node_name: &str,
        server_host: &str,
        error: &str,
    ) -> Result<()> {
        self.send_immediate_alert(
            AlertType::Snapshot,
            AlertSeverity::Critical,
            node_name,
            server_host,
            format!("Snapshot restore failed for {}: {}", node_name, error),
            Some(serde_json::json!({
                "operation_type": "snapshot_restore",
                "error_message": error
            })),
        )
        .await
    }

    /// Alert when auto-restore starts
    pub async fn alert_auto_restore_started(
        &self,
        node_name: &str,
        server_host: &str,
        trigger_words: &[String],
    ) -> Result<()> {
        self.send_immediate_alert(
            AlertType::AutoRestore,
            AlertSeverity::Warning,
            node_name,
            server_host,
            format!("Auto-restore STARTED for {} due to corruption indicators", node_name),
            Some(serde_json::json!({
                "trigger_words": trigger_words,
                "status": "starting"
            })),
        )
        .await
    }

    /// Alert when auto-restore completes successfully
    pub async fn alert_auto_restore_completed(
        &self,
        node_name: &str,
        server_host: &str,
        snapshot_filename: &str,
        trigger_words: &[String],
    ) -> Result<()> {
        self.send_immediate_alert(
            AlertType::AutoRestore,
            AlertSeverity::Info,
            node_name,
            server_host,
            format!("Auto-restore COMPLETED for {} - node should be syncing from restored state", node_name),
            Some(serde_json::json!({
                "trigger_words": trigger_words,
                "status": "completed",
                "snapshot_filename": snapshot_filename
            })),
        )
        .await
    }

    /// Alert when auto-restore fails (requires manual intervention)
    pub async fn alert_auto_restore_failed(
        &self,
        node_name: &str,
        server_host: &str,
        error: &str,
        trigger_words: &[String],
    ) -> Result<()> {
        self.send_immediate_alert(
            AlertType::AutoRestore,
            AlertSeverity::Critical,
            node_name,
            server_host,
            format!("CRITICAL: Auto-restore failed for {} - manual intervention required", node_name),
            Some(serde_json::json!({
                "error_message": error,
                "trigger_words": trigger_words
            })),
        )
        .await
    }

    /// Alert for log pattern matches (per-node monitoring)
    pub async fn alert_log_pattern_match(
        &self,
        node_name: &str,
        server_host: &str,
        log_path: &str,
        log_output: &str,
        patterns: &[String],
    ) -> Result<()> {
        self.send_immediate_alert(
            AlertType::LogPattern,
            AlertSeverity::Warning,
            node_name,
            server_host,
            "Log pattern match detected".to_string(),
            Some(serde_json::json!({
                "log_path": log_path,
                "log_output": log_output,
                "patterns": patterns
            })),
        )
        .await
    }

    /// Send recovery alerts when services recover from failure
    async fn send_recovery_alert_if_needed(
        &self,
        node_name: &str,
        server_host: &str,
        details: Option<serde_json::Value>,
    ) -> Result<()> {
        let should_send_recovery = {
            let mut alert_states = self.alert_states.lock().await;
            if let Some(alert_state) = alert_states.remove(node_name) {
                alert_state.has_sent_alert
            } else {
                false
            }
        };

        if should_send_recovery {
            let payload = AlertPayload {
                timestamp: Utc::now(),
                alert_type: AlertType::NodeHealth,
                severity: AlertSeverity::Recovery,
                node_name: node_name.to_string(),
                message: "Node has recovered and is now healthy".to_string(),
                server_host: server_host.to_string(),
                details,
            };

            self.send_webhook(&payload).await?;
            info!("Recovery notification sent for node: {}", node_name);
        } else {
            debug!("No recovery notification needed for {} - no alerts were sent during unhealthy period", node_name);
        }

        Ok(())
    }

    /// Private method to send webhook
    async fn send_webhook(&self, payload: &AlertPayload) -> Result<()> {
        if !self.is_enabled {
            warn!(
                "Alert service disabled - webhook URL not configured. Alert would be: {} - {}",
                payload.node_name, payload.message
            );
            warn!("Set 'alarm_webhook_url' in config/main.toml to enable alerts");
            return Ok(());
        }

        info!(
            "Sending alert for {}: {:?} to {}",
            payload.node_name, payload.alert_type, self.webhook_url
        );

        match timeout(
            Duration::from_secs(10),
            self.client
                .post(&self.webhook_url)
                .header("Content-Type", "application/json")
                .json(payload)
                .send(),
        )
        .await
        {
            Ok(Ok(response)) => {
                if response.status().is_success() {
                    info!(
                        "Alert sent successfully for {}: {:?} (status: {})",
                        payload.node_name,
                        payload.alert_type,
                        response.status()
                    );
                } else {
                    let status = response.status();
                    let body = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "Failed to read response body".to_string());
                    error!(
                        "Alert webhook returned status: {} for {} - Response: {}",
                        status, payload.node_name, body
                    );
                }
            }
            Ok(Err(e)) => {
                error!(
                    "Failed to send alert for {}: {} - Webhook URL: {}",
                    payload.node_name, e, self.webhook_url
                );
            }
            Err(_) => {
                error!(
                    "Alert webhook timeout for {} - URL: {}",
                    payload.node_name, self.webhook_url
                );
            }
        }

        Ok(())
    }

    /// Test webhook connectivity
    pub async fn test_webhook(&self) -> Result<()> {
        if !self.is_enabled {
            return Err(anyhow::anyhow!(
                "Alert service is disabled - no webhook URL configured"
            ));
        }

        let test_payload = AlertPayload {
            timestamp: Utc::now(),
            alert_type: AlertType::Maintenance,
            severity: AlertSeverity::Info,
            node_name: "test-node".to_string(),
            message: "Test alert from AlertService".to_string(),
            server_host: "test-server".to_string(),
            details: Some(serde_json::json!({"test": true})),
        };

        info!("Testing webhook connectivity to: {}", self.webhook_url);
        self.send_webhook(&test_payload).await
    }
}

impl Clone for AlertService {
    fn clone(&self) -> Self {
        Self {
            webhook_url: self.webhook_url.clone(),
            client: self.client.clone(),
            alert_states: self.alert_states.clone(),
            previous_health_states: self.previous_health_states.clone(),
            is_enabled: self.is_enabled,
        }
    }
}
