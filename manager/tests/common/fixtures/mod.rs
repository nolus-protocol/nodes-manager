//! This module provides reusable test utilities:
//! - Mock HTTP servers (agent, RPC, webhook)
//! - Test configuration builders
//! - In-memory test databases
//! - Common test data

// Allow unused code in test fixtures - they are utilities for future tests
#![allow(dead_code)]
#![allow(unused_imports)]

pub mod mock_agent;
pub mod mock_rpc;
pub mod mock_webhook;
pub mod test_config;
pub mod test_data;
pub mod test_database;

// Re-export commonly used items
pub use mock_agent::MockAgentServer;
pub use mock_rpc::MockRpcServer;
pub use mock_webhook::MockWebhookServer;
pub use test_config::TestConfigBuilder;
pub use test_data::*;
pub use test_database::TestDatabase;
