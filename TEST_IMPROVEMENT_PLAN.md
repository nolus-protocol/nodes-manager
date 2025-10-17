# State Sync Test Coverage Analysis & Improvement Plan

## Current Test Coverage

### ✅ What's Tested:
1. **Config Parsing** (`config_unit_tests.rs`)
   - ✅ State sync enabled flag
   - ✅ State sync schedule parsing
   - ✅ RPC sources array
   - ✅ Trust height offset

2. **Maintenance Tracking** (`business_rules_maintenance_windows.rs`, `business_rules_mutual_exclusion.rs`)
   - ✅ State sync creates maintenance window
   - ✅ State sync respects mutual exclusion
   - ✅ Multiple nodes can run state sync simultaneously

3. **Mock Infrastructure** (`mock_agent.rs`, `mock_rpc.rs`)
   - ✅ Mock agent state sync endpoint
   - ✅ Mock RPC block queries
   - ✅ Mock operation status polling

### ❌ What's NOT Tested (CRITICAL GAPS):

1. **End-to-End State Sync Flow**
   - ❌ Full web handler → HTTP agent manager → agent flow
   - ❌ RPC parameter fetching
   - ❌ Job polling and completion
   - ❌ Error handling and recovery

2. **Path Configuration**
   - ❌ deploy_path is correctly used (not pruning_deploy_path)
   - ❌ Config path construction: `{deploy_path}/config/config.toml`
   - ❌ Home directory path correctness

3. **RPC Server Handling**
   - ❌ Multiple RPC servers are included (not just first one)
   - ❌ RPC failover logic
   - ❌ Trust height/hash fetching

4. **Maintenance Integration**
   - ❌ Web handler triggers maintenance tracking
   - ❌ Health checks are suppressed during state sync
   - ❌ UI shows "in maintenance" status

5. **Error Scenarios**
   - ❌ State sync disabled in config
   - ❌ No RPC sources configured
   - ❌ Node already busy
   - ❌ RPC connection failures
   - ❌ Agent failures
   - ❌ Timeout handling

6. **Placeholder Tests** (`manual_operations_integration.rs`)
   - ❌ All state sync tests are empty placeholders!

## Improvement Plan

### Priority 1: Critical Integration Tests

#### Test 1: End-to-End State Sync Success
```rust
#[tokio::test]
async fn test_state_sync_complete_flow() {
    // Setup
    let mock_agent = MockAgentServer::start().await;
    let mock_rpc = MockRpcServer::start().await;
    let config = test_config_with_state_sync();
    
    // Mock RPC responses
    mock_rpc.mock_latest_block(17047661).await;
    mock_rpc.mock_block_at_height(17045661, "D24EA1ED...").await;
    
    // Mock agent responses
    let job_id = "state_sync_node_123";
    mock_agent.mock_state_sync_success(job_id).await;
    mock_agent.mock_operation_completed(job_id).await;
    
    // Execute state sync via HTTP agent manager
    let result = http_manager.execute_state_sync("test-node").await;
    
    // Verify
    assert!(result.is_ok());
    // Verify maintenance window was created
    // Verify RPC was called with correct params
    // Verify agent received ALL RPC servers
}
```

#### Test 2: Path Configuration Correctness
```rust
#[tokio::test]
async fn test_state_sync_uses_correct_paths() {
    let config = test_config();
    let node = &config.nodes["test-node"];
    
    // Verify deploy_path is used (not pruning_deploy_path)
    assert_eq!(node.deploy_path, Some("/opt/deploy/nolus/test-node".to_string()));
    
    // Verify config path construction
    let config_path = format!("{}/config/config.toml", node.deploy_path.unwrap());
    assert_eq!(config_path, "/opt/deploy/nolus/test-node/config/config.toml");
    
    // Verify NOT using data subdirectory
    assert!(!config_path.contains("/data/config"));
}
```

#### Test 3: Multiple RPC Servers
```rust
#[tokio::test]
async fn test_state_sync_includes_all_rpc_servers() {
    let rpc_sources = vec![
        "http://rpc1.example.com:26657",
        "http://rpc2.example.com:26657",
    ];
    
    let params = fetch_state_sync_params(&rpc_sources, 2000).await.unwrap();
    
    // Verify ALL RPC servers are returned (not just first one)
    assert_eq!(params.rpc_servers.len(), 2);
    assert_eq!(params.rpc_servers[0], "http://rpc1.example.com:26657");
    assert_eq!(params.rpc_servers[1], "http://rpc2.example.com:26657");
}
```

#### Test 4: Maintenance Tracking Integration
```rust
#[tokio::test]
async fn test_state_sync_creates_maintenance_window() {
    let app = setup_test_app().await;
    let node_name = "test-node";
    
    // Execute state sync
    let response = app
        .post(&format!("/api/state-sync/{}/execute", node_name))
        .await;
    
    assert_eq!(response.status(), 200);
    
    // Verify maintenance window exists
    let maintenance = app.maintenance_tracker.get_maintenance(node_name).await;
    assert!(maintenance.is_some());
    assert_eq!(maintenance.unwrap().operation_type, "state_sync");
    
    // Verify node shows as "in maintenance" in health check
    let health = app.health_service.get_node_health(node_name).await;
    assert_eq!(health.status, "in_maintenance");
}
```

### Priority 2: Error Scenarios

#### Test 5: State Sync Disabled
```rust
#[tokio::test]
async fn test_state_sync_fails_when_disabled() {
    let config = test_config_without_state_sync();
    let http_manager = HttpAgentManager::new(config);
    
    let result = http_manager.execute_state_sync("test-node").await;
    
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not enabled"));
}
```

#### Test 6: Node Already Busy
```rust
#[tokio::test]
async fn test_state_sync_fails_when_node_busy() {
    let app = setup_test_app().await;
    
    // Start pruning operation (makes node busy)
    app.operation_tracker.try_start_operation("test-node", "pruning", None).await.unwrap();
    
    // Try state sync (should fail)
    let response = app
        .post("/api/state-sync/test-node/execute")
        .await;
    
    assert_eq!(response.status(), 409); // CONFLICT
}
```

#### Test 7: RPC Failover
```rust
#[tokio::test]
async fn test_state_sync_rpc_failover() {
    let mock_rpc1 = MockRpcServer::start().await;
    let mock_rpc2 = MockRpcServer::start().await;
    
    // First RPC fails
    mock_rpc1.mock_error("/block", 500, "Service unavailable").await;
    
    // Second RPC succeeds
    mock_rpc2.mock_latest_block(17047661).await;
    mock_rpc2.mock_block_at_height(17045661, "D24EA1ED...").await;
    
    let rpc_sources = vec![mock_rpc1.url(), mock_rpc2.url()];
    let result = fetch_state_sync_params(&rpc_sources, 2000).await;
    
    // Should succeed using second RPC
    assert!(result.is_ok());
    
    // But should still return BOTH RPC servers
    assert_eq!(result.unwrap().rpc_servers.len(), 2);
}
```

### Priority 3: Replace Placeholder Tests

All tests in `manual_operations_integration.rs` lines 26-90 need to be implemented:

```rust
// Currently just:
#[tokio::test]
async fn test_state_sync_endpoint_exists() {
    let endpoint = "/api/state-sync/test-node/execute";
    assert!(endpoint.starts_with("/api/state-sync/"));
}

// Should be:
#[tokio::test]
async fn test_state_sync_endpoint_returns_200_on_success() {
    let app = setup_test_app_with_state_sync_enabled().await;
    let mock_agent = MockAgentServer::start().await;
    mock_agent.mock_state_sync_success("job_123").await;
    
    let response = app.post("/api/state-sync/test-node/execute").await;
    
    assert_eq!(response.status(), 200);
    let body: Value = response.json().await;
    assert_eq!(body["data"]["status"], "started");
    assert_eq!(body["data"]["node_name"], "test-node");
}
```

## Test Infrastructure Needed

### 1. Test Config Builder
```rust
pub fn test_config_with_state_sync() -> Config {
    Config {
        nodes: hashmap! {
            "test-node" => NodeConfig {
                deploy_path: Some("/opt/deploy/nolus/test-node"),
                state_sync_enabled: Some(true),
                state_sync_rpc_sources: Some(vec![
                    "http://rpc1.example.com:26657",
                    "http://rpc2.example.com:26657",
                ]),
                state_sync_trust_height_offset: Some(2000),
                state_sync_max_sync_timeout_seconds: Some(1800),
                ..Default::default()
            }
        },
        ..Default::default()
    }
}
```

### 2. Test App Setup
```rust
pub async fn setup_test_app() -> TestApp {
    let config = test_config_with_state_sync();
    let database = test_database().await;
    let operation_tracker = Arc::new(SimpleOperationTracker::new());
    let maintenance_tracker = Arc::new(MaintenanceTracker::new(database.clone()));
    let http_manager = Arc::new(HttpAgentManager::new(config.clone(), ...));
    
    TestApp {
        config,
        database,
        operation_tracker,
        maintenance_tracker,
        http_manager,
    }
}
```

## Estimated Impact

### Current Test Coverage: ~30%
- ✅ Config parsing
- ✅ Basic maintenance tracking
- ❌ No integration tests
- ❌ No path validation
- ❌ No error scenarios

### After Improvements: ~85%
- ✅ Config parsing
- ✅ Maintenance tracking
- ✅ End-to-end integration
- ✅ Path validation
- ✅ RPC parameter fetching
- ✅ Error scenarios
- ✅ Web handler integration

## Implementation Priority

1. **CRITICAL** (Do first):
   - Test 2: Path configuration
   - Test 3: Multiple RPC servers
   - Test 4: Maintenance tracking

2. **HIGH** (Do next):
   - Test 1: End-to-end flow
   - Test 5: State sync disabled
   - Test 6: Node already busy

3. **MEDIUM** (Nice to have):
   - Test 7: RPC failover
   - Replace all placeholder tests
   - Test infrastructure improvements

## Recommendation

**Start with Tests 2 and 3** - these directly validate the bugs we just fixed:
- deploy_path vs pruning_deploy_path
- Single RPC server vs multiple RPC servers

These tests will prevent regressions and give confidence in the fixes.
