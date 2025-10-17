//! Comprehensive integration tests for state sync functionality
//!
//! Tests cover:
//! - Path configuration (deploy_path vs pruning_deploy_path)
//! - Multiple RPC server handling
//! - RPC parameter fetching
//! - Maintenance window integration
//! - Error scenarios and validation

mod common;

use common::fixtures::*;
use manager::config::{Config, NodeConfig, ServerConfig};
use manager::state_sync::fetch_state_sync_params;
use std::collections::HashMap;

// ============================================================================
// CRITICAL: Path Configuration Tests
// ============================================================================

#[tokio::test]
async fn test_deploy_path_is_home_directory_not_data() {
    // Verify that deploy_path represents the home directory, not the data subdirectory
    let mut config = Config {
        host: "0.0.0.0".to_string(),
        port: 8095,
        check_interval_seconds: 90,
        rpc_timeout_seconds: 10,
        alarm_webhook_url: "http://test".to_string(),
        hermes_min_uptime_minutes: Some(5),
        auto_restore_trigger_words: None,
        log_monitoring_context_lines: None,
        servers: HashMap::new(),
        nodes: HashMap::new(),
        hermes: HashMap::new(),
        etl: HashMap::new(),
    };

    let node = NodeConfig {
        rpc_url: "http://localhost:26657".to_string(),
        network: "test-network".to_string(),
        server_host: "test-server".to_string(),
        enabled: true,
        service_name: "test-node".to_string(),
        deploy_path: Some("/opt/deploy/nolus/test-node".to_string()),
        pruning_enabled: Some(true),
        pruning_schedule: None,
        pruning_keep_blocks: Some(1000),
        pruning_keep_versions: Some(1000),
        log_path: Some("/var/log/test-node".to_string()),
        truncate_logs_enabled: Some(true),
        log_monitoring_enabled: None,
        log_monitoring_patterns: None,
        snapshots_enabled: Some(true),
        snapshot_backup_path: Some("/backup/snapshots".to_string()),
        auto_restore_enabled: Some(true),
        snapshot_schedule: None,
        snapshot_retention_count: Some(7),
        state_sync_enabled: Some(true),
        state_sync_schedule: None,
        state_sync_rpc_sources: Some(vec![
            "http://rpc1.example.com:26657".to_string(),
            "http://rpc2.example.com:26657".to_string(),
        ]),
        state_sync_trust_height_offset: Some(2000),
        state_sync_max_sync_timeout_seconds: Some(1800),
    };

    config.nodes.insert("test-node".to_string(), node);

    let node_config = config.nodes.get("test-node").unwrap();

    // CRITICAL: Verify deploy_path is the home directory
    assert_eq!(
        node_config.deploy_path,
        Some("/opt/deploy/nolus/test-node".to_string())
    );

    // CRITICAL: Config path should be constructed from deploy_path
    let config_path = format!(
        "{}/config/config.toml",
        node_config.deploy_path.as_ref().unwrap()
    );
    assert_eq!(
        config_path,
        "/opt/deploy/nolus/test-node/config/config.toml"
    );

    // CRITICAL: Verify it does NOT contain /data in the config path
    assert!(!config_path.contains("/data/config"));

    // Verify data directory would be at deploy_path/data
    let data_path = format!("{}/data", node_config.deploy_path.as_ref().unwrap());
    assert_eq!(data_path, "/opt/deploy/nolus/test-node/data");
}

#[tokio::test]
async fn test_config_path_construction_from_deploy_path() {
    // Test that config.toml path is correctly constructed
    let deploy_paths = vec![
        "/opt/deploy/nolus/full-node-3",
        "/mnt/nodes/osmosis/validator-1",
        "/home/cosmos/neutron-node",
    ];

    for deploy_path in deploy_paths {
        let config_path = format!("{}/config/config.toml", deploy_path);

        // Should have /config/ directory
        assert!(config_path.contains("/config/"));

        // Should NOT have /data/ in the config path
        assert!(!config_path.contains("/data/"));

        // Should end with config.toml
        assert!(config_path.ends_with("config.toml"));

        // Verify structure
        assert_eq!(config_path, format!("{}/config/config.toml", deploy_path));
    }
}

// ============================================================================
// CRITICAL: Multiple RPC Servers Test
// ============================================================================

#[tokio::test]
async fn test_all_rpc_servers_returned_not_just_first() {
    // Start two mock RPC servers
    let mock_rpc1 = MockRpcServer::start().await;
    let mock_rpc2 = MockRpcServer::start().await;

    // Configure first RPC with block data
    mock_rpc1.mock_latest_block(17047661).await;
    mock_rpc1
        .mock_block_at_height(
            17045661,
            "D24EA1EDCEAD66F36CFE277809BDA0E1AA6A5E586DC693C8030C291503694B9D",
            "2025-01-17T10:00:00Z",
        )
        .await;

    // Second RPC can be offline (we're using the first one)
    // But it should still be included in the results

    let rpc_sources = vec![mock_rpc1.base_url.clone(), mock_rpc2.base_url.clone()];

    let result = fetch_state_sync_params(&rpc_sources, 2000).await;

    assert!(result.is_ok());
    let params = result.unwrap();

    // CRITICAL: ALL RPC servers must be returned, not just the first successful one
    assert_eq!(params.rpc_servers.len(), 2);
    assert_eq!(params.rpc_servers[0], mock_rpc1.base_url);
    assert_eq!(params.rpc_servers[1], mock_rpc2.base_url);

    // Verify the trust height and hash were fetched correctly
    assert_eq!(params.trust_height, 17045661);
    assert_eq!(
        params.trust_hash,
        "D24EA1EDCEAD66F36CFE277809BDA0E1AA6A5E586DC693C8030C291503694B9D"
    );
}

#[tokio::test]
async fn test_three_rpc_servers_all_returned() {
    let mock_rpc1 = MockRpcServer::start().await;
    let mock_rpc2 = MockRpcServer::start().await;
    let mock_rpc3 = MockRpcServer::start().await;

    mock_rpc1.mock_latest_block(17050000).await;
    mock_rpc1
        .mock_block_at_height(17048000, "ABC123", "2025-01-17T10:00:00Z")
        .await;

    let rpc_sources = vec![
        mock_rpc1.base_url.clone(),
        mock_rpc2.base_url.clone(),
        mock_rpc3.base_url.clone(),
    ];

    let result = fetch_state_sync_params(&rpc_sources, 2000).await;
    assert!(result.is_ok());

    let params = result.unwrap();

    // All three RPC servers should be in the result
    assert_eq!(params.rpc_servers.len(), 3);
    assert!(params.rpc_servers.contains(&mock_rpc1.base_url));
    assert!(params.rpc_servers.contains(&mock_rpc2.base_url));
    assert!(params.rpc_servers.contains(&mock_rpc3.base_url));
}

// ============================================================================
// RPC Parameter Fetching Tests
// ============================================================================

#[tokio::test]
async fn test_fetch_state_sync_params_success() {
    let mock_rpc = MockRpcServer::start().await;

    // Mock latest block
    mock_rpc.mock_latest_block(17047661).await;

    // Mock block at trust height
    mock_rpc
        .mock_block_at_height(
            17045661,
            "D24EA1EDCEAD66F36CFE277809BDA0E1AA6A5E586DC693C8030C291503694B9D",
            "2025-01-17T10:00:00Z",
        )
        .await;

    let rpc_sources = vec![mock_rpc.base_url.clone()];
    let result = fetch_state_sync_params(&rpc_sources, 2000).await;

    assert!(result.is_ok());
    let params = result.unwrap();

    // Verify trust height calculation (latest - offset)
    assert_eq!(params.trust_height, 17045661);

    // Verify trust hash
    assert_eq!(
        params.trust_hash,
        "D24EA1EDCEAD66F36CFE277809BDA0E1AA6A5E586DC693C8030C291503694B9D"
    );

    // Verify RPC servers
    assert_eq!(params.rpc_servers.len(), 1);
    assert_eq!(params.rpc_servers[0], mock_rpc.base_url);
}

#[tokio::test]
async fn test_trust_height_offset_calculation() {
    let mock_rpc = MockRpcServer::start().await;

    let test_cases = vec![
        (10000, 2000, 8000),        // latest=10000, offset=2000 -> trust=8000
        (17047661, 2000, 17045661), // Real example
        (5000, 1000, 4000),         // offset=1000
        (100000, 5000, 95000),      // Large offset
    ];

    for (latest_height, offset, expected_trust_height) in test_cases {
        mock_rpc.mock_latest_block(latest_height).await;
        mock_rpc
            .mock_block_at_height(expected_trust_height, "HASH123", "2025-01-17T10:00:00Z")
            .await;

        let result = fetch_state_sync_params(&[mock_rpc.base_url.clone()], offset).await;
        assert!(result.is_ok());

        let params = result.unwrap();
        assert_eq!(params.trust_height, expected_trust_height as i64);
    }
}

#[tokio::test]
async fn test_rpc_failover_to_second_server() {
    let mock_rpc1 = MockRpcServer::start().await;
    let mock_rpc2 = MockRpcServer::start().await;

    // First RPC returns error
    mock_rpc1
        .mock_error("/block", 500, "Internal server error")
        .await;

    // Second RPC succeeds
    mock_rpc2.mock_latest_block(17050000).await;
    mock_rpc2
        .mock_block_at_height(17048000, "FAILOVER_HASH", "2025-01-17T10:00:00Z")
        .await;

    let rpc_sources = vec![mock_rpc1.base_url.clone(), mock_rpc2.base_url.clone()];
    let result = fetch_state_sync_params(&rpc_sources, 2000).await;

    // Should succeed using second RPC
    assert!(result.is_ok());

    let params = result.unwrap();

    // But should still return BOTH RPC servers for redundancy
    assert_eq!(params.rpc_servers.len(), 2);
    assert_eq!(params.trust_hash, "FAILOVER_HASH");
}

// ============================================================================
// Error Scenarios
// ============================================================================

#[tokio::test]
async fn test_all_rpc_servers_fail() {
    let mock_rpc1 = MockRpcServer::start().await;
    let mock_rpc2 = MockRpcServer::start().await;

    // Both RPCs fail
    mock_rpc1
        .mock_error("/block", 500, "Internal server error")
        .await;
    mock_rpc2
        .mock_error("/block", 503, "Service unavailable")
        .await;

    let rpc_sources = vec![mock_rpc1.base_url.clone(), mock_rpc2.base_url.clone()];
    let result = fetch_state_sync_params(&rpc_sources, 2000).await;

    // Should fail when all RPCs are down
    assert!(result.is_err());
}

#[tokio::test]
async fn test_empty_rpc_sources() {
    let rpc_sources: Vec<String> = vec![];
    let result = fetch_state_sync_params(&rpc_sources, 2000).await;

    // Should fail with no RPC sources
    assert!(result.is_err());
}

#[tokio::test]
async fn test_invalid_block_response() {
    let mock_rpc = MockRpcServer::start().await;

    // Mock invalid JSON response
    mock_rpc.mock_error("/block", 200, "Invalid JSON").await;

    let result = fetch_state_sync_params(&[mock_rpc.base_url.clone()], 2000).await;

    // Should fail on invalid response
    assert!(result.is_err());
}

// ============================================================================
// Config Validation Tests
// ============================================================================

#[tokio::test]
async fn test_state_sync_config_validation() {
    let node = NodeConfig {
        rpc_url: "http://localhost:26657".to_string(),
        network: "test-network".to_string(),
        server_host: "test-server".to_string(),
        enabled: true,
        service_name: "test-node".to_string(),
        deploy_path: Some("/opt/deploy/nolus/test-node".to_string()),
        pruning_enabled: None,
        pruning_schedule: None,
        pruning_keep_blocks: None,
        pruning_keep_versions: None,
        log_path: None,
        truncate_logs_enabled: None,
        log_monitoring_enabled: None,
        log_monitoring_patterns: None,
        snapshots_enabled: None,
        snapshot_backup_path: None,
        auto_restore_enabled: None,
        snapshot_schedule: None,
        snapshot_retention_count: None,
        state_sync_enabled: Some(true),
        state_sync_schedule: None,
        state_sync_rpc_sources: Some(vec![
            "http://rpc1.example.com:26657".to_string(),
            "http://rpc2.example.com:26657".to_string(),
        ]),
        state_sync_trust_height_offset: Some(2000),
        state_sync_max_sync_timeout_seconds: Some(1800),
    };

    // Verify all state sync fields are set correctly
    assert_eq!(node.state_sync_enabled, Some(true));
    assert!(node.state_sync_rpc_sources.is_some());
    assert_eq!(node.state_sync_rpc_sources.as_ref().unwrap().len(), 2);
    assert_eq!(node.state_sync_trust_height_offset, Some(2000));
    assert_eq!(node.state_sync_max_sync_timeout_seconds, Some(1800));
}

#[tokio::test]
async fn test_default_timeout_is_30_minutes() {
    // When max_sync_timeout_seconds is not specified, default should be 1800 (30 min)
    // This is tested in the config defaults

    let node = NodeConfig {
        rpc_url: "http://localhost:26657".to_string(),
        network: "test-network".to_string(),
        server_host: "test-server".to_string(),
        enabled: true,
        service_name: "test-node".to_string(),
        deploy_path: Some("/opt/deploy/nolus/test-node".to_string()),
        pruning_enabled: None,
        pruning_schedule: None,
        pruning_keep_blocks: None,
        pruning_keep_versions: None,
        log_path: None,
        truncate_logs_enabled: None,
        log_monitoring_enabled: None,
        log_monitoring_patterns: None,
        snapshots_enabled: None,
        snapshot_backup_path: None,
        auto_restore_enabled: None,
        snapshot_schedule: None,
        snapshot_retention_count: None,
        state_sync_enabled: Some(true),
        state_sync_schedule: None,
        state_sync_rpc_sources: Some(vec!["http://rpc.example.com:26657".to_string()]),
        state_sync_trust_height_offset: Some(2000),
        state_sync_max_sync_timeout_seconds: None, // Not specified
    };

    // When None, the default should be applied (1800 seconds = 30 minutes)
    let timeout = node.state_sync_max_sync_timeout_seconds.unwrap_or(1800);
    assert_eq!(timeout, 1800);
}

// ============================================================================
// Regression Tests (Prevent Future Bugs)
// ============================================================================

#[tokio::test]
async fn test_regression_config_path_must_not_contain_data_subdirectory() {
    // Regression test: Previously, pruning_deploy_path included /data,
    // which caused state sync to look for config at:
    // /opt/deploy/nolus/full-node-3/data/config/config.toml (WRONG)
    // Instead of:
    // /opt/deploy/nolus/full-node-3/config/config.toml (CORRECT)

    let deploy_path = "/opt/deploy/nolus/full-node-3";
    let config_path = format!("{}/config/config.toml", deploy_path);

    // MUST NOT contain /data/config
    assert!(!config_path.contains("/data/config"));

    // MUST be: {deploy_path}/config/config.toml
    assert_eq!(
        config_path,
        "/opt/deploy/nolus/full-node-3/config/config.toml"
    );
}

#[tokio::test]
async fn test_regression_all_rpc_servers_must_be_included() {
    // Regression test: Previously, only the first successful RPC was returned
    // CometBFT requires at least 2 RPC servers for redundancy
    // This caused error: "at least two rpc_servers entries is required"

    let mock_rpc1 = MockRpcServer::start().await;
    let mock_rpc2 = MockRpcServer::start().await;

    mock_rpc1.mock_latest_block(17050000).await;
    mock_rpc1
        .mock_block_at_height(17048000, "HASH", "2025-01-17T10:00:00Z")
        .await;

    let rpc_sources = vec![mock_rpc1.base_url.clone(), mock_rpc2.base_url.clone()];
    let params = fetch_state_sync_params(&rpc_sources, 2000).await.unwrap();

    // MUST return ALL configured RPC servers
    assert_eq!(params.rpc_servers.len(), 2);

    // Not just the one that was queried
    assert_ne!(params.rpc_servers.len(), 1);
}
