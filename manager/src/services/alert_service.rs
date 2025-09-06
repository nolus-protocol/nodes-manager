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
use tracing::{debug, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertType {
    NodeHealth,
    AutoRestore,
    Snapshot,
    Hermes,
    LogPattern,
    Maintenance,
}

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
}

impl AlertService {
    pub fn new(webhook_url: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to create HTTP client for AlertService");

        Self {
            webhook_url,
            client,
            alert_states: Arc::new(Mutex::new(HashMap::new())),
            previous_health_states: Arc::new(Mutex::new(HashMap::new())),
        }
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
                (Some(true), false) | (None, false) => true,  // Became unhealthy
                (Some(false), true) => {
                    // Became healthy - check if we should send recovery
                    return self.send_recovery_alert_if_needed(node_name, server_host, details).await;
                }
                (Some(false), false) => true,  // Still unhealthy
                _ => false,  // Still healthy or no change
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
                    if alert_state.consecutive_failures >= 3 {
                        alert_state.alert_count = 1;
                        alert_state.last_alert_sent = now;
                        alert_state.has_sent_alert = true;
                        info!("Node {} unhealthy for 3 consecutive checks - sending first alert", node_name);
                        true
                    } else {
                        info!("Node {} unhealthy check {}/3 - no alert sent yet",
                              node_name, alert_state.consecutive_failures);
                        false
                    }
                } else {
                    // Already sent first alert, use progressive timing
                    let hours_since_first = (now - alert_state.first_alert_time).num_hours();
                    let hours_since_last = (now - alert_state.last_alert_sent).num_hours();

                    let should_alert = match alert_state.alert_count {
                        1 => hours_since_first >= 3,
                        2 => hours_since_first >= 6,
                        3 => hours_since_first >= 12,
                        _ => hours_since_last >= 24,
                    };

                    if should_alert {
                        alert_state.last_alert_sent = now;
                        alert_state.alert_count += 1;
                        alert_state.has_sent_alert = true;
                        true
                    } else {
                        false
                    }
                }
            }
        };

        drop(alert_states);

        if should_send_alert {
            let payload = AlertPayload {
                timestamp: now,
                alert_type: AlertType::NodeHealth,
                severity: AlertSeverity::Critical,
                node_name: node_name.to_string(),
                message: error_message.unwrap_or_else(|| "Node is unhealthy".to_string()),
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

    /// Clear alert state for a specific node (when manually resolved)
    pub async fn clear_alert_state(&self, node_name: &str) {
        let mut alert_states = self.alert_states.lock().await;
        let mut previous_states = self.previous_health_states.lock().await;

        alert_states.remove(node_name);
        previous_states.remove(node_name);

        debug!("Cleared alert state for node: {}", node_name);
    }

    /// Private method to send webhook
    async fn send_webhook(&self, payload: &AlertPayload) -> Result<()> {
        if self.webhook_url.is_empty() {
            debug!("No webhook URL configured, skipping alert");
            return Ok(());
        }

        match timeout(
            Duration::from_secs(10),
            self.client.post(&self.webhook_url)
                .json(payload)
                .send(),
        )
        .await
        {
            Ok(Ok(response)) => {
                if response.status().is_success() {
                    info!("Alert sent successfully for {}: {:?}", payload.node_name, payload.alert_type);
                } else {
                    warn!("Alert webhook returned status: {} for {}", response.status(), payload.node_name);
                }
            }
            Ok(Err(e)) => {
                warn!("Failed to send alert for {}: {}", payload.node_name, e);
            }
            Err(_) => {
                warn!("Alert webhook timeout for {}", payload.node_name);
            }
        }

        Ok(())
    }
}

impl Clone for AlertService {
    fn clone(&self) -> Self {
        Self {
            webhook_url: self.webhook_url.clone(),
            client: self.client.clone(),
            alert_states: self.alert_states.clone(),
            previous_health_states: self.previous_health_states.clone(),
        }
    }
}
