//! Mock HTTP agent server for testing
//!
//! This provides a fake agent that responds to all agent API endpoints
//! without requiring a real agent server running.

use serde_json::json;
use wiremock::{
    matchers::{method, path, path_regex},
    Mock, MockServer, ResponseTemplate,
};

/// Mock agent server that simulates agent HTTP responses
pub struct MockAgentServer {
    pub server: MockServer,
    pub base_url: String,
}

impl MockAgentServer {
    /// Create a new mock agent server
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        let base_url = server.uri();
        Self { server, base_url }
    }

    /// Mock successful pruning execution
    pub async fn mock_pruning_success(&self, job_id: &str) {
        Mock::given(method("POST"))
            .and(path("/pruning/execute"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "job_id": job_id,
                "status": "started"
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock successful snapshot creation
    pub async fn mock_snapshot_create_success(&self, job_id: &str) {
        Mock::given(method("POST"))
            .and(path("/snapshot/create"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "job_id": job_id,
                "status": "started"
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock successful snapshot restore
    pub async fn mock_snapshot_restore_success(&self, job_id: &str) {
        Mock::given(method("POST"))
            .and(path("/snapshot/restore"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "job_id": job_id,
                "status": "started"
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock successful state sync
    pub async fn mock_state_sync_success(&self, job_id: &str) {
        Mock::given(method("POST"))
            .and(path("/state-sync/execute"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "job_id": job_id,
                "status": "started"
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock operation status as running
    pub async fn mock_operation_running(&self, job_id: &str) {
        Mock::given(method("GET"))
            .and(path_regex(format!("/operation/status/{}", job_id)))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "job_id": job_id,
                "status": "running",
                "progress": 50
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock operation status as completed
    pub async fn mock_operation_completed(&self, job_id: &str) {
        Mock::given(method("GET"))
            .and(path_regex(format!("/operation/status/{}", job_id)))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "job_id": job_id,
                "status": "completed",
                "result": "success"
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock operation status as failed
    pub async fn mock_operation_failed(&self, job_id: &str, error: &str) {
        Mock::given(method("GET"))
            .and(path_regex(format!("/operation/status/{}", job_id)))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "job_id": job_id,
                "status": "failed",
                "error": error
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock agent timeout (no response)
    pub async fn mock_timeout(&self, endpoint: &str) {
        Mock::given(method("POST"))
            .and(path(endpoint))
            .respond_with(ResponseTemplate::new(408))
            .mount(&self.server)
            .await;
    }

    /// Mock agent error response
    pub async fn mock_error(&self, endpoint: &str, status_code: u16, error_msg: &str) {
        Mock::given(method("POST"))
            .and(path(endpoint))
            .respond_with(ResponseTemplate::new(status_code).set_body_json(json!({
                "error": error_msg
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock health check endpoint
    pub async fn mock_health_check(&self, healthy: bool) {
        let status = if healthy { "healthy" } else { "unhealthy" };
        Mock::given(method("GET"))
            .and(path("/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": status
            })))
            .mount(&self.server)
            .await;
    }
}
