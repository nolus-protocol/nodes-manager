# Refactoring Suggestions for Nodes Manager

This document contains potential refactoring opportunities to improve code quality, performance, and maintainability. **These are suggestions only - review carefully before implementing.**

---

## 🎯 High Priority Refactorings

### 1. Create Custom Error Types (Replace `anyhow::anyhow!`)

**Current State:** 89 occurrences of `anyhow::anyhow!` for error creation
**Issue:** Generic error handling makes it hard to handle specific error cases

**Suggestion:**
```rust
// Create manager/src/error.rs
#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("Configuration error: {0}")]
    Config(String),
    
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    #[error("HTTP agent error on {server}: {message}")]
    AgentCommunication { server: String, message: String },
    
    #[error("Node not found: {0}")]
    NodeNotFound(String),
    
    #[error("Operation in progress: {0}")]
    OperationInProgress(String),
    
    #[error("RPC error: {0}")]
    Rpc(String),
    
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, ManagerError>;
```

**Benefits:**
- Type-safe error handling
- Better error context
- Easier to handle specific errors
- Better API documentation

**Risk:** Medium - Requires updating all error handling code
**Effort:** High - Touch ~90 locations

---

### 2. Extract Configuration Validation Logic

**Current State:** Configuration validation scattered across multiple files
**Issue:** Hard to maintain and ensure consistency

**Suggestion:**
```rust
// In manager/src/config/validation.rs
pub struct ConfigValidator;

impl ConfigValidator {
    pub fn validate_node_config(config: &NodeConfig) -> Result<()> {
        Self::validate_paths(config)?;
        Self::validate_schedules(config)?;
        Self::validate_network_settings(config)?;
        Ok(())
    }
    
    fn validate_paths(config: &NodeConfig) -> Result<()> {
        // Centralized path validation
        if let Some(path) = &config.pruning_deploy_path {
            if !path.starts_with('/') {
                return Err(ManagerError::Config(
                    format!("pruning_deploy_path must be absolute: {}", path)
                ));
            }
        }
        Ok(())
    }
    
    fn validate_schedules(config: &NodeConfig) -> Result<()> {
        // Validate cron expressions
        if let Some(schedule) = &config.pruning_schedule {
            CronValidator::validate(schedule)?;
        }
        Ok(())
    }
    
    fn validate_network_settings(config: &NodeConfig) -> Result<()> {
        // Validate network-specific settings
        Ok(())
    }
}
```

**Benefits:**
- Single source of truth for validation
- Easier to add new validation rules
- Better error messages
- Testable validation logic

**Risk:** Low - Pure refactoring, no behavior change
**Effort:** Medium - Extract and consolidate validation

---

### 3. Reduce Clone Usage with Cow<'a, str>

**Current State:** 188 `.clone()` calls, many on strings
**Issue:** Unnecessary allocations for temporary string usage

**Suggestion:**
```rust
use std::borrow::Cow;

// Before:
pub fn get_node_name(&self) -> String {
    self.node_name.clone()
}

// After (for read-only access):
pub fn get_node_name(&self) -> &str {
    &self.node_name
}

// Or when conditional cloning is needed:
pub fn format_node_info(&self) -> Cow<'_, str> {
    if self.needs_formatting {
        Cow::Owned(format!("Node: {}", self.node_name))
    } else {
        Cow::Borrowed(&self.node_name)
    }
}
```

**Benefits:**
- Reduced memory allocations
- Better performance for read-heavy operations
- More explicit about ownership

**Risk:** Medium - Requires careful lifetime management
**Effort:** High - Review each clone call individually

---

### 4. Implement Builder Pattern for Complex Structs

**Current State:** Large constructors with many Arc parameters
**Issue:** Hard to read, easy to mix up parameters

**Suggestion:**
```rust
// manager/src/http/builder.rs
pub struct HttpAgentManagerBuilder {
    config: Option<Arc<Config>>,
    operation_tracker: Option<Arc<SimpleOperationTracker>>,
    maintenance_tracker: Option<Arc<MaintenanceTracker>>,
    timeout: Duration,
    connect_timeout: Duration,
}

impl HttpAgentManagerBuilder {
    pub fn new() -> Self {
        Self {
            config: None,
            operation_tracker: None,
            maintenance_tracker: None,
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
        }
    }
    
    pub fn config(mut self, config: Arc<Config>) -> Self {
        self.config = Some(config);
        self
    }
    
    pub fn operation_tracker(mut self, tracker: Arc<SimpleOperationTracker>) -> Self {
        self.operation_tracker = Some(tracker);
        self
    }
    
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    
    pub fn build(self) -> Result<HttpAgentManager> {
        Ok(HttpAgentManager {
            config: self.config.ok_or(ManagerError::Config("Missing config".into()))?,
            operation_tracker: self.operation_tracker.ok_or(ManagerError::Config("Missing tracker".into()))?,
            maintenance_tracker: self.maintenance_tracker.ok_or(ManagerError::Config("Missing maintenance tracker".into()))?,
            client: Client::builder()
                .timeout(self.timeout)
                .connect_timeout(self.connect_timeout)
                .build()?,
        })
    }
}

// Usage:
let http_manager = HttpAgentManagerBuilder::new()
    .config(config)
    .operation_tracker(operation_tracker)
    .maintenance_tracker(maintenance_tracker)
    .timeout(Duration::from_secs(60))
    .build()?;
```

**Benefits:**
- Self-documenting API
- Type-safe construction
- Easy to add optional parameters
- Compile-time validation

**Risk:** Low - Additive change
**Effort:** Medium - Create builders for complex structs

---

## 🔧 Medium Priority Refactorings

### 5. Extract Repeated HTTP Request Logic

**Current State:** HTTP request patterns repeated across multiple methods
**Issue:** Code duplication, hard to maintain

**Suggestion:**
```rust
// In HttpAgentManager
struct RequestBuilder<'a> {
    manager: &'a HttpAgentManager,
    endpoint: String,
    payload: Value,
}

impl<'a> RequestBuilder<'a> {
    fn new(manager: &'a HttpAgentManager, endpoint: impl Into<String>) -> Self {
        Self {
            manager,
            endpoint: endpoint.into(),
            payload: json!({}),
        }
    }
    
    fn payload(mut self, payload: Value) -> Self {
        self.payload = payload;
        self
    }
    
    async fn send_to(self, server_name: &str) -> Result<Value> {
        self.manager.execute_operation(server_name, &self.endpoint, self.payload).await
    }
}

// Usage:
let result = RequestBuilder::new(self, "/pruning/execute")
    .payload(json!({
        "service_name": node_config.pruning_service_name,
        "keep_blocks": keep_blocks,
    }))
    .send_to(&node_config.server_host)
    .await?;
```

**Benefits:**
- DRY principle
- Easier to add middleware (logging, retries, etc.)
- Consistent error handling

**Risk:** Low - Internal refactoring
**Effort:** Medium

---

### 6. Add Tracing Spans for Better Observability

**Current State:** Individual log statements without context
**Issue:** Hard to trace operations through the system

**Suggestion:**
```rust
use tracing::{instrument, span, Level};

// Before:
pub async fn execute_node_pruning(&self, node_name: &str) -> Result<()> {
    info!("Starting pruning for {}", node_name);
    // ... operation
    info!("Completed pruning for {}", node_name);
}

// After:
#[instrument(skip(self), fields(node_name = %node_name))]
pub async fn execute_node_pruning(&self, node_name: &str) -> Result<()> {
    let span = span!(Level::INFO, "pruning_operation");
    let _guard = span.enter();
    
    // All logs within this function will automatically include context
    info!("Starting pruning");
    // ... operation
    info!("Completed pruning");
    Ok(())
}
```

**Benefits:**
- Automatic context propagation
- Better debugging
- Easier to trace distributed operations
- Structured logging

**Risk:** Low - Additive change
**Effort:** Medium - Add to key functions

---

### 7. Consolidate Status Checking Logic

**Current State:** Similar status checking patterns in multiple places
**Issue:** Code duplication

**Suggestion:**
```rust
// manager/src/status.rs
pub struct StatusChecker {
    client: Client,
}

impl StatusChecker {
    pub async fn check_service_status(
        &self,
        host: &str,
        port: u16,
        endpoint: &str,
    ) -> Result<ServiceStatus> {
        // Unified status checking logic
    }
    
    pub async fn wait_for_status(
        &self,
        host: &str,
        port: u16,
        expected: ServiceStatus,
        timeout: Duration,
    ) -> Result<()> {
        // Polling with exponential backoff
    }
}
```

**Benefits:**
- Consistent status checking
- Easier to add retry logic
- Testable in isolation

**Risk:** Low
**Effort:** Low-Medium

---

### 8. Use Type State Pattern for Operation States

**Current State:** Runtime checks for operation states
**Issue:** Can't catch state errors at compile time

**Suggestion:**
```rust
// Phantom types for compile-time state tracking
struct Pending;
struct Running;
struct Completed;

struct Operation<State> {
    id: String,
    target: String,
    _state: PhantomData<State>,
}

impl Operation<Pending> {
    pub fn new(id: String, target: String) -> Self {
        Self {
            id,
            target,
            _state: PhantomData,
        }
    }
    
    pub fn start(self) -> Operation<Running> {
        Operation {
            id: self.id,
            target: self.target,
            _state: PhantomData,
        }
    }
}

impl Operation<Running> {
    pub fn complete(self) -> Operation<Completed> {
        Operation {
            id: self.id,
            target: self.target,
            _state: PhantomData,
        }
    }
    
    // Can only cancel running operations
    pub fn cancel(self) -> Result<()> {
        Ok(())
    }
}

// This won't compile - can't cancel completed operations!
// let op = Operation::<Completed>::new(...);
// op.cancel(); // Compile error!
```

**Benefits:**
- Compile-time safety
- Impossible states become impossible
- Self-documenting state transitions

**Risk:** Medium - Requires significant refactoring
**Effort:** High

---

## 📊 Low Priority / Nice to Have

### 9. Extract Magic Numbers to Constants

**Current State:** Hardcoded timeouts and limits scattered in code
**Issue:** Hard to maintain and configure

**Suggestion:**
```rust
// manager/src/constants.rs
pub mod timeouts {
    use std::time::Duration;
    
    pub const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
    pub const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
    pub const OPERATION_POLL_INTERVAL: Duration = Duration::from_secs(10);
    pub const HEALTH_CHECK_INTERVAL: Duration = Duration::from_secs(90);
    
    pub const PRUNING_TIMEOUT_HOURS: u64 = 5;
    pub const SNAPSHOT_TIMEOUT_HOURS: u64 = 24;
    pub const STATE_SYNC_TIMEOUT_HOURS: u64 = 24;
}

pub mod limits {
    pub const MAX_CONCURRENT_OPERATIONS: usize = 10;
    pub const MAX_RETRY_ATTEMPTS: u32 = 3;
    pub const OPERATION_CLEANUP_HOURS: i64 = 24;
    pub const MAINTENANCE_CLEANUP_HOURS: i64 = 48;
}

pub mod alert {
    pub const FIRST_ALERT_AFTER_CHECKS: u32 = 3;
    pub const SECOND_ALERT_HOURS: i64 = 6;
    pub const THIRD_ALERT_HOURS: i64 = 6;
    pub const FOURTH_ALERT_HOURS: i64 = 12;
    pub const SUBSEQUENT_ALERT_HOURS: i64 = 24;
}
```

**Benefits:**
- Single source of truth
- Easy to adjust timeouts
- Self-documenting

**Risk:** Very Low
**Effort:** Low

---

### 10. Add Integration Test Utilities

**Current State:** Limited test infrastructure
**Issue:** Hard to test integration scenarios

**Suggestion:**
```rust
// tests/common/mod.rs
pub struct TestContext {
    pub config: Arc<Config>,
    pub database: Arc<Database>,
    pub temp_dir: TempDir,
}

impl TestContext {
    pub async fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        Self {
            config: Arc::new(Self::test_config()),
            database: Arc::new(Database::new(db_path.to_str().unwrap()).await.unwrap()),
            temp_dir,
        }
    }
    
    fn test_config() -> Config {
        // Create minimal test configuration
    }
    
    pub async fn create_mock_node(&self, name: &str) -> NodeConfig {
        // Helper to create test nodes
    }
}

// Usage in tests:
#[tokio::test]
async fn test_pruning_operation() {
    let ctx = TestContext::new().await;
    let node = ctx.create_mock_node("test-node").await;
    // ... test logic
}
```

**Benefits:**
- Easier to write tests
- Consistent test setup
- Better test coverage

**Risk:** Very Low - Test only
**Effort:** Medium

---

### 11. Use `#[non_exhaustive]` for Future-Proof Enums

**Current State:** Public enums without protection
**Issue:** Adding variants is a breaking change

**Suggestion:**
```rust
// Before:
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceStatus {
    Running,
    Stopped,
    Failed,
    Unknown,
}

// After:
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServiceStatus {
    Running,
    Stopped,
    Failed,
    Unknown,
}
```

**Benefits:**
- Can add variants without breaking API
- Forces users to handle unknown cases
- Future-proof

**Risk:** Very Low
**Effort:** Very Low

---

### 12. Cache Frequently Accessed Configuration

**Current State:** Configuration cloned/accessed repeatedly
**Issue:** Potential performance bottleneck

**Suggestion:**
```rust
pub struct CachedConfig {
    inner: Arc<Config>,
    // Cache frequently accessed values
    node_lookup: DashMap<String, Arc<NodeConfig>>,
    server_lookup: DashMap<String, Arc<ServerConfig>>,
}

impl CachedConfig {
    pub fn get_node(&self, name: &str) -> Option<Arc<NodeConfig>> {
        self.node_lookup.get(name).map(|r| r.clone())
    }
}
```

**Benefits:**
- Reduced lock contention
- Faster lookups
- Better performance

**Risk:** Low - Internal optimization
**Effort:** Medium

---

## 🎨 Code Style Improvements

### 13. Consistent Naming Conventions

**Observations:**
- Some functions use `execute_*` prefix
- Others use `perform_*` or `run_*`
- Some use `check_*` vs `validate_*`

**Suggestion:** Establish consistent naming:
```rust
// Operation execution
execute_*  - For long-running operations (execute_pruning)
perform_*  - For quick actions (perform_restart)
run_*      - For scheduled tasks (run_maintenance)

// Validation/Checking
validate_* - For validation that can fail (validate_config)
check_*    - For boolean queries (check_is_healthy)
is_*       - For boolean properties (is_running)
has_*      - For boolean existence (has_snapshot)
```

---

### 14. Documentation Improvements

**Suggestion:** Add module-level documentation
```rust
//! # HTTP Agent Manager
//!
//! This module handles communication with remote HTTP agents deployed on blockchain servers.
//!
//! ## Architecture
//!
//! ```text
//! Manager → HTTP Request → Agent (port 8745)
//!    ↓
//! Job Polling ← Job ID ← Agent Response
//! ```
//!
//! ## Example
//!
//! ```rust,no_run
//! let manager = HttpAgentManager::new(config, tracker, maintenance);
//! let result = manager.execute_node_pruning("node-1").await?;
//! ```

pub struct HttpAgentManager { ... }
```

---

## ⚠️ Potential Issues to Monitor

### 1. Memory Usage with Large Snapshots
- Consider streaming for very large snapshot operations
- Monitor memory during LZ4 compression

### 2. Database Connection Pool
- Currently single connection
- Consider connection pooling for high concurrency

### 3. Error Recovery
- Add circuit breaker pattern for agent communication
- Implement exponential backoff for retries

---

## 📋 Refactoring Priority Matrix

| Refactoring | Impact | Risk | Effort | Priority |
|-------------|--------|------|--------|----------|
| Custom Error Types | High | Medium | High | 🔴 High |
| Config Validation | High | Low | Medium | 🔴 High |
| Reduce Clones | Medium | Medium | High | 🟡 Medium |
| Builder Pattern | Medium | Low | Medium | 🟡 Medium |
| HTTP Request Logic | Medium | Low | Medium | 🟡 Medium |
| Tracing Spans | High | Low | Medium | 🔴 High |
| Status Checking | Low | Low | Low | 🟢 Low |
| Type State Pattern | Medium | Medium | High | 🟡 Medium |
| Extract Constants | Low | Low | Low | 🟢 Low |
| Test Utilities | Medium | Low | Medium | 🟡 Medium |
| non_exhaustive | Low | Low | Low | 🟢 Low |
| Config Caching | Medium | Low | Medium | 🟡 Medium |

---

## 🚀 Implementation Approach

### Phase 1: Foundation (Low Risk, High Value)
1. Extract constants
2. Add `#[non_exhaustive]` to enums
3. Module documentation
4. Test utilities

### Phase 2: Error Handling (Medium Risk, High Value)
1. Create custom error types
2. Update error handling across codebase
3. Add better error context

### Phase 3: Performance (Medium Risk, Medium Value)
1. Reduce unnecessary clones
2. Add configuration caching
3. Optimize HTTP request patterns

### Phase 4: Advanced (Higher Risk, Medium Value)
1. Builder patterns for complex structs
2. Type state patterns
3. Enhanced tracing

---

## 📝 Notes

- **Always test thoroughly** before merging any refactoring
- **Measure performance** before and after optimizations
- **Keep backward compatibility** for configuration files
- **Update documentation** with each change
- **Review with team** before implementing high-risk changes

---

## ✅ Already Done Well

- Clean module structure
- Good use of Arc for shared state
- Async/await properly implemented
- Configuration hot-reload
- Comprehensive logging
- Non-blocking operations
- Good separation of concerns

---

*Last Updated: 2025-01-15*
*Review this document quarterly and update based on new findings*
