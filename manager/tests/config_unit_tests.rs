//! Unit tests for configuration parsing and validation
//!
//! These tests verify that configuration files are parsed correctly
//! and validation rules are enforced.

mod common;

use std::fs;
use tempfile::TempDir;

#[test]
fn test_parse_main_config() {
    let temp_dir = TempDir::new().unwrap();
    let config_dir = temp_dir.path().join("config");
    fs::create_dir(&config_dir).unwrap();

    let main_toml = r#"
host = "0.0.0.0"
port = 8080
check_interval_seconds = 90
rpc_timeout_seconds = 10
alarm_webhook_url = "https://example.com/webhook"
hermes_min_uptime_minutes = 5
auto_restore_trigger_words = ["consensus failure", "state sync"]
log_monitoring_context_lines = 10
    "#;

    fs::write(config_dir.join("main.toml"), main_toml).unwrap();

    let config: manager::config::Config = toml::from_str(main_toml).unwrap();

    assert_eq!(config.host, "0.0.0.0");
    assert_eq!(config.port, 8080);
    assert_eq!(config.check_interval_seconds, 90);
    assert_eq!(config.rpc_timeout_seconds, 10);
    assert_eq!(config.alarm_webhook_url, "https://example.com/webhook");
    assert_eq!(config.hermes_min_uptime_minutes, Some(5));
    assert_eq!(
        config.auto_restore_trigger_words,
        Some(vec![
            "consensus failure".to_string(),
            "state sync".to_string()
        ])
    );
    assert_eq!(config.log_monitoring_context_lines, Some(10));
}

#[test]
fn test_parse_server_config() {
    let server_toml = r#"
[server]
host = "192.168.1.100"
agent_port = 8745
api_key = "test-key"
request_timeout_seconds = 300
max_concurrent_requests = 5

[nodes.dummy]
rpc_url = "http://localhost:26657"
server_host = "192.168.1.100"
enabled = true
    "#;

    let server_config: manager::config::ServerConfigFile = toml::from_str(server_toml).unwrap();

    assert_eq!(server_config.server.host, "192.168.1.100");
    assert_eq!(server_config.server.agent_port, 8745);
    assert_eq!(server_config.server.api_key, "test-key");
    assert_eq!(server_config.server.request_timeout_seconds, 300);
    assert_eq!(server_config.server.max_concurrent_requests, Some(5));
}

#[test]
fn test_parse_node_config() {
    let server_toml = r#"
[server]
host = "localhost"
agent_port = 8745
api_key = "key"

[nodes.osmosis-1]
rpc_url = "http://localhost:26657"
network = "osmosis-1"
server_host = "localhost"
enabled = true
pruning_enabled = true
pruning_schedule = "0 0 2 * * *"
pruning_keep_blocks = 100000
pruning_deploy_path = "/opt/osmosis/data"
pruning_service_name = "osmosisd"
snapshots_enabled = true
snapshot_schedule = "0 0 3 * * *"
snapshot_backup_path = "/backup/snapshots"
auto_restore_enabled = true
    "#;

    let config: manager::config::ServerConfigFile = toml::from_str(server_toml).unwrap();

    assert!(config.nodes.contains_key("osmosis-1"));
    let node = config.nodes.get("osmosis-1").unwrap();

    assert_eq!(node.rpc_url, "http://localhost:26657");
    assert_eq!(node.network, "osmosis-1");
    assert_eq!(node.server_host, "localhost");
    assert!(node.enabled);
    assert_eq!(node.pruning_enabled, Some(true));
    assert_eq!(node.pruning_schedule, Some("0 0 2 * * *".to_string()));
    assert_eq!(node.pruning_keep_blocks, Some(100000));
    assert_eq!(
        node.pruning_deploy_path,
        Some("/opt/osmosis/data".to_string())
    );
    assert_eq!(node.pruning_service_name, Some("osmosisd".to_string()));
    assert_eq!(node.snapshots_enabled, Some(true));
    assert_eq!(node.snapshot_schedule, Some("0 0 3 * * *".to_string()));
    assert_eq!(
        node.snapshot_backup_path,
        Some("/backup/snapshots".to_string())
    );
    assert_eq!(node.auto_restore_enabled, Some(true));
}

#[test]
fn test_parse_hermes_config() {
    let server_toml = r#"
[server]
host = "localhost"
agent_port = 8745
api_key = "key"

[nodes.dummy]
rpc_url = "http://localhost:26657"
server_host = "localhost"
enabled = true

[hermes.hermes-1]
server_host = "localhost"
service_name = "hermes"
restart_schedule = "0 0 4 * * *"
dependent_nodes = ["osmosis-1", "cosmos-hub"]
    "#;

    let config: manager::config::ServerConfigFile = toml::from_str(server_toml).unwrap();

    assert!(config.hermes.is_some());
    let hermes_map = config.hermes.unwrap();
    assert!(hermes_map.contains_key("hermes-1"));

    let hermes = hermes_map.get("hermes-1").unwrap();
    assert_eq!(hermes.service_name, "hermes");
    assert_eq!(hermes.restart_schedule, Some("0 0 4 * * *".to_string()));
    assert_eq!(
        hermes.dependent_nodes,
        Some(vec!["osmosis-1".to_string(), "cosmos-hub".to_string()])
    );
}

#[test]
fn test_parse_state_sync_config() {
    let server_toml = r#"
[server]
host = "localhost"
agent_port = 8745
api_key = "key"

[nodes.test-node]
rpc_url = "http://localhost:26657"
network = "test-network"
server_host = "localhost"
enabled = true
state_sync_enabled = true
state_sync_schedule = "0 0 5 * * *"
state_sync_rpc_sources = ["https://rpc1.example.com", "https://rpc2.example.com"]
state_sync_trust_height_offset = 2000
    "#;

    let config: manager::config::ServerConfigFile = toml::from_str(server_toml).unwrap();
    let node = config.nodes.get("test-node").unwrap();

    assert_eq!(node.state_sync_enabled, Some(true));
    assert_eq!(node.state_sync_schedule, Some("0 0 5 * * *".to_string()));
    assert_eq!(
        node.state_sync_rpc_sources,
        Some(vec![
            "https://rpc1.example.com".to_string(),
            "https://rpc2.example.com".to_string()
        ])
    );
    assert_eq!(node.state_sync_trust_height_offset, Some(2000));
}

#[test]
fn test_parse_node_defaults() {
    let server_toml = r#"
[server]
host = "localhost"
agent_port = 8745
api_key = "key"

[defaults]
base_deploy_path = "/opt/deploy"
base_log_path = "/var/log"
base_backup_path = "/home/backup/snapshots"

[nodes.test-node]
rpc_url = "http://localhost:26657"
network = "test-network"
server_host = "localhost"
enabled = true
    "#;

    let config: manager::config::ServerConfigFile = toml::from_str(server_toml).unwrap();

    assert!(config.defaults.is_some());
    let defaults = config.defaults.unwrap();

    assert_eq!(defaults.base_deploy_path, Some("/opt/deploy".to_string()));
    assert_eq!(defaults.base_log_path, Some("/var/log".to_string()));
    assert_eq!(
        defaults.base_backup_path,
        Some("/home/backup/snapshots".to_string())
    );
}

#[test]
fn test_parse_multiple_nodes() {
    let server_toml = r#"
[server]
host = "localhost"
agent_port = 8745
api_key = "key"

[nodes.node-1]
rpc_url = "http://localhost:26657"
network = "network-1"
server_host = "localhost"
enabled = true

[nodes.node-2]
rpc_url = "http://localhost:26658"
network = "network-2"
server_host = "localhost"
enabled = false

[nodes.node-3]
rpc_url = "http://localhost:26659"
network = "network-3"
server_host = "localhost"
enabled = true
    "#;

    let config: manager::config::ServerConfigFile = toml::from_str(server_toml).unwrap();

    assert_eq!(config.nodes.len(), 3);
    assert!(config.nodes.contains_key("node-1"));
    assert!(config.nodes.contains_key("node-2"));
    assert!(config.nodes.contains_key("node-3"));

    assert!(config.nodes.get("node-1").unwrap().enabled);
    assert!(!config.nodes.get("node-2").unwrap().enabled);
    assert!(config.nodes.get("node-3").unwrap().enabled);
}

#[test]
fn test_parse_etl_config() {
    let server_toml = r#"
[server]
host = "localhost"
agent_port = 8745
api_key = "key"

[nodes.dummy]
rpc_url = "http://localhost:26657"
server_host = "localhost"
enabled = true

[etl.etl-service-1]
server_host = "localhost"
host = "localhost"
port = 8080
endpoint = "/health"
enabled = true
    "#;

    let config: manager::config::ServerConfigFile = toml::from_str(server_toml).unwrap();

    assert!(config.etl.is_some());
    let etl_map = config.etl.unwrap();
    assert!(etl_map.contains_key("etl-service-1"));

    let etl = etl_map.get("etl-service-1").unwrap();
    assert_eq!(etl.host, "localhost");
    assert_eq!(etl.port, 8080);
    assert_eq!(etl.endpoint, Some("/health".to_string()));
    assert!(etl.enabled);
}

#[test]
fn test_log_monitoring_config() {
    let server_toml = r#"
[server]
host = "localhost"
agent_port = 8745
api_key = "key"

[nodes.monitored-node]
rpc_url = "http://localhost:26657"
network = "test-network"
server_host = "localhost"
enabled = true
log_path = "/var/log/node"
log_monitoring_enabled = true
log_monitoring_patterns = ["ERROR", "FATAL", "consensus failure"]
    "#;

    let config: manager::config::ServerConfigFile = toml::from_str(server_toml).unwrap();
    let node = config.nodes.get("monitored-node").unwrap();

    assert_eq!(node.log_path, Some("/var/log/node".to_string()));
    assert_eq!(node.log_monitoring_enabled, Some(true));
    assert_eq!(
        node.log_monitoring_patterns,
        Some(vec![
            "ERROR".to_string(),
            "FATAL".to_string(),
            "consensus failure".to_string()
        ])
    );
}

#[test]
fn test_optional_fields_default_to_none() {
    let server_toml = r#"
[server]
host = "localhost"
agent_port = 8745
api_key = "key"

[nodes.minimal-node]
rpc_url = "http://localhost:26657"
network = "test-network"
server_host = "localhost"
enabled = true
    "#;

    let config: manager::config::ServerConfigFile = toml::from_str(server_toml).unwrap();
    let node = config.nodes.get("minimal-node").unwrap();

    // All optional fields should be None
    assert_eq!(node.pruning_enabled, None);
    assert_eq!(node.pruning_schedule, None);
    assert_eq!(node.pruning_keep_blocks, None);
    assert_eq!(node.snapshots_enabled, None);
    assert_eq!(node.snapshot_schedule, None);
    assert_eq!(node.state_sync_enabled, None);
    assert_eq!(node.log_monitoring_enabled, None);
}

#[test]
fn test_server_config_default_timeout() {
    let server_toml = r#"
[server]
host = "localhost"
agent_port = 8745
api_key = "key"

[nodes.dummy]
rpc_url = "http://localhost:26657"
server_host = "localhost"
enabled = true
    "#;

    let config: manager::config::ServerConfigFile = toml::from_str(server_toml).unwrap();

    // Should use default timeout of 300 seconds
    assert_eq!(config.server.request_timeout_seconds, 300);
}

#[test]
fn test_network_auto_detection() {
    let server_toml = r#"
[server]
host = "localhost"
agent_port = 8745
api_key = "key"

[nodes.auto-network-node]
rpc_url = "http://localhost:26657"
network = "auto"
server_host = "localhost"
enabled = true
    "#;

    let config: manager::config::ServerConfigFile = toml::from_str(server_toml).unwrap();
    let node = config.nodes.get("auto-network-node").unwrap();

    assert_eq!(node.network, "auto");
}

#[test]
fn test_empty_network_defaults_to_empty_string() {
    let server_toml = r#"
[server]
host = "localhost"
agent_port = 8745
api_key = "key"

[nodes.no-network-node]
rpc_url = "http://localhost:26657"
server_host = "localhost"
enabled = true
    "#;

    let config: manager::config::ServerConfigFile = toml::from_str(server_toml).unwrap();
    let node = config.nodes.get("no-network-node").unwrap();

    assert_eq!(node.network, "");
}
