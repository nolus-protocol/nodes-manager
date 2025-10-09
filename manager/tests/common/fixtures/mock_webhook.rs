//! Mock webhook server for testing alert delivery
//!
//! This simulates a webhook endpoint that receives alerts,
//! allowing tests to verify alerts are sent correctly.

use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, Request, ResponseTemplate,
};

/// Captured webhook request
#[derive(Debug, Clone)]
pub struct WebhookRequest {
    pub body: Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Mock webhook server that captures alert requests
pub struct MockWebhookServer {
    pub server: MockServer,
    pub base_url: String,
    captured_requests: Arc<Mutex<Vec<WebhookRequest>>>,
}

impl MockWebhookServer {
    /// Create a new mock webhook server
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        let base_url = server.uri();
        let captured_requests = Arc::new(Mutex::new(Vec::new()));

        Self {
            server,
            base_url,
            captured_requests,
        }
    }

    /// Mock successful webhook delivery
    pub async fn mock_success(&self) {
        let requests = self.captured_requests.clone();

        Mock::given(method("POST"))
            .and(path("/webhook"))
            .respond_with(move |req: &Request| {
                // Capture the request
                if let Ok(body) = req.body_json::<Value>() {
                    let webhook_req = WebhookRequest {
                        body,
                        timestamp: chrono::Utc::now(),
                    };
                    tokio::spawn({
                        let requests = requests.clone();
                        async move {
                            requests.lock().await.push(webhook_req);
                        }
                    });
                }
                ResponseTemplate::new(200)
            })
            .mount(&self.server)
            .await;
    }

    /// Mock webhook failure
    pub async fn mock_failure(&self, status_code: u16) {
        Mock::given(method("POST"))
            .and(path("/webhook"))
            .respond_with(ResponseTemplate::new(status_code))
            .mount(&self.server)
            .await;
    }

    /// Mock webhook timeout
    pub async fn mock_timeout(&self) {
        Mock::given(method("POST"))
            .and(path("/webhook"))
            .respond_with(ResponseTemplate::new(408))
            .mount(&self.server)
            .await;
    }

    /// Get all captured webhook requests
    pub async fn get_captured_requests(&self) -> Vec<WebhookRequest> {
        self.captured_requests.lock().await.clone()
    }

    /// Get the number of webhook requests received
    pub async fn request_count(&self) -> usize {
        self.captured_requests.lock().await.len()
    }

    /// Clear captured requests
    pub async fn clear(&self) {
        self.captured_requests.lock().await.clear();
    }

    /// Get the webhook URL
    pub fn webhook_url(&self) -> String {
        format!("{}/webhook", self.base_url)
    }

    /// Verify that a webhook was sent with specific content
    pub async fn assert_webhook_sent_with(
        &self,
        expected_field: &str,
        expected_value: &str,
    ) -> bool {
        let requests = self.get_captured_requests().await;
        requests.iter().any(|req| {
            req.body
                .get(expected_field)
                .and_then(|v| v.as_str())
                .map(|v| v == expected_value)
                .unwrap_or(false)
        })
    }

    /// Verify specific alert was sent
    pub async fn assert_alert_sent(&self, node_name: &str, message_contains: &str) -> bool {
        let requests = self.get_captured_requests().await;
        requests.iter().any(|req| {
            let node_matches = req
                .body
                .get("node_name")
                .and_then(|v| v.as_str())
                .map(|v| v == node_name)
                .unwrap_or(false);

            let message_matches = req
                .body
                .get("message")
                .and_then(|v| v.as_str())
                .map(|v| v.contains(message_contains))
                .unwrap_or(false);

            node_matches && message_matches
        })
    }
}
