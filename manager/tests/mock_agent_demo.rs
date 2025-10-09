//! Integration tests demonstrating mock agent usage
//!
//! This shows how mocks work in practice - these are FAKE HTTP servers
//! that simulate agent responses without needing a real agent running.

mod common;

use common::fixtures::*;
use reqwest::Client;
use serde_json::Value;

#[tokio::test]
async fn test_mock_agent_pruning() {
    // Start a MOCK agent server (not real!)
    let mock = MockAgentServer::start().await;
    let job_id = random_job_id();

    // Configure mock to respond to pruning request
    mock.mock_pruning_success(&job_id).await;

    // Make HTTP request to mock server
    let client = Client::new();
    let response = client
        .post(format!("{}/pruning/execute", mock.base_url))
        .json(&serde_json::json!({
            "node_name": "test-node",
            "data_dir": "/data"
        }))
        .send()
        .await
        .unwrap();

    // Verify response
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["job_id"], job_id);
    assert_eq!(body["status"], "started");
}

#[tokio::test]
async fn test_mock_agent_operation_status() {
    // Start mock agent
    let mock = MockAgentServer::start().await;
    let job_id = random_job_id();

    // Configure mock to return "running" status
    mock.mock_operation_running(&job_id).await;

    // Query operation status
    let client = Client::new();
    let response = client
        .get(format!("{}/operation/status/{}", mock.base_url, job_id))
        .send()
        .await
        .unwrap();

    // Verify response
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["status"], "running");
    assert_eq!(body["progress"], 50);
}

#[tokio::test]
async fn test_mock_agent_completed_operation() {
    // Start mock agent
    let mock = MockAgentServer::start().await;
    let job_id = random_job_id();

    // Configure mock to return "completed" status
    mock.mock_operation_completed(&job_id).await;

    // Query operation status
    let client = Client::new();
    let response = client
        .get(format!("{}/operation/status/{}", mock.base_url, job_id))
        .send()
        .await
        .unwrap();

    // Verify response
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["status"], "completed");
    assert_eq!(body["result"], "success");
}

#[tokio::test]
async fn test_mock_agent_failed_operation() {
    // Start mock agent
    let mock = MockAgentServer::start().await;
    let job_id = random_job_id();

    // Configure mock to return "failed" status
    mock.mock_operation_failed(&job_id, "Disk full").await;

    // Query operation status
    let client = Client::new();
    let response = client
        .get(format!("{}/operation/status/{}", mock.base_url, job_id))
        .send()
        .await
        .unwrap();

    // Verify response
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["status"], "failed");
    assert_eq!(body["error"], "Disk full");
}

#[tokio::test]
async fn test_mock_agent_error_response() {
    // Start mock agent
    let mock = MockAgentServer::start().await;

    // Configure mock to return error
    mock.mock_error("/pruning/execute", 500, "Internal server error")
        .await;

    // Make request
    let client = Client::new();
    let response = client
        .post(format!("{}/pruning/execute", mock.base_url))
        .json(&serde_json::json!({}))
        .send()
        .await
        .unwrap();

    // Verify error response
    assert_eq!(response.status(), 500);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["error"], "Internal server error");
}

#[tokio::test]
async fn test_mock_agent_snapshot_create() {
    let mock = MockAgentServer::start().await;
    let job_id = random_job_id();

    mock.mock_snapshot_create_success(&job_id).await;

    let client = Client::new();
    let response = client
        .post(format!("{}/snapshot/create", mock.base_url))
        .json(&serde_json::json!({
            "network": "osmosis-1",
            "data_dir": "/data"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["job_id"], job_id);
}

#[tokio::test]
async fn test_mock_agent_snapshot_restore() {
    let mock = MockAgentServer::start().await;
    let job_id = random_job_id();

    mock.mock_snapshot_restore_success(&job_id).await;

    let client = Client::new();
    let response = client
        .post(format!("{}/snapshot/restore", mock.base_url))
        .json(&serde_json::json!({
            "snapshot_name": "osmosis-1_20250101_120000",
            "data_dir": "/data"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["job_id"], job_id);
}
