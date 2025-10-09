//! Mock RPC server for testing blockchain node interactions
//!
//! This simulates blockchain RPC responses without requiring a real node.

use serde_json::json;
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

/// Mock RPC server that simulates blockchain node responses
pub struct MockRpcServer {
    pub server: MockServer,
    pub base_url: String,
}

impl MockRpcServer {
    /// Create a new mock RPC server
    pub async fn start() -> Self {
        let server = MockServer::start().await;
        let base_url = server.uri();
        Self { server, base_url }
    }

    /// Mock healthy synced node
    pub async fn mock_healthy_synced(&self, network: &str, latest_block: u64) {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "node_info": {
                        "network": network,
                        "moniker": "test-node"
                    },
                    "sync_info": {
                        "latest_block_height": latest_block.to_string(),
                        "catching_up": false
                    }
                }
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock node that is catching up (syncing)
    pub async fn mock_catching_up(&self, network: &str, latest_block: u64) {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "node_info": {
                        "network": network,
                        "moniker": "test-node"
                    },
                    "sync_info": {
                        "latest_block_height": latest_block.to_string(),
                        "catching_up": true
                    }
                }
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock unhealthy node (connection failure)
    pub async fn mock_unhealthy(&self) {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&self.server)
            .await;
    }

    /// Mock RPC timeout
    pub async fn mock_timeout(&self) {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(408))
            .mount(&self.server)
            .await;
    }

    /// Mock state sync RPC endpoints for trusted block
    pub async fn mock_state_sync_info(&self, trusted_height: u64, trusted_hash: &str) {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "sync_info": {
                        "latest_block_height": trusted_height.to_string(),
                        "latest_block_hash": trusted_hash
                    }
                }
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock block endpoint for state sync trusted block lookup
    pub async fn mock_block_at_height(&self, height: u64, block_hash: &str, block_time: &str) {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "block_id": {
                        "hash": block_hash
                    },
                    "block": {
                        "header": {
                            "height": height.to_string(),
                            "time": block_time
                        }
                    }
                }
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock node that is far behind (for testing sync issues)
    pub async fn mock_behind_sync(&self, network: &str, current_height: u64, catching_up: bool) {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "node_info": {
                        "network": network,
                        "moniker": "test-node"
                    },
                    "sync_info": {
                        "latest_block_height": current_height.to_string(),
                        "catching_up": catching_up,
                        "earliest_block_height": "1"
                    }
                }
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock node with stale data (hasn't updated in a while)
    pub async fn mock_stale_data(&self, network: &str, stale_height: u64) {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "node_info": {
                        "network": network,
                        "moniker": "test-node"
                    },
                    "sync_info": {
                        "latest_block_height": stale_height.to_string(),
                        "catching_up": false,
                        "latest_block_time": "2020-01-01T00:00:00Z"
                    }
                }
            })))
            .mount(&self.server)
            .await;
    }

    /// Mock multiple sequential responses (for testing state changes)
    pub async fn mock_progressive_sync(&self, network: &str, heights: Vec<u64>) {
        for (i, height) in heights.iter().enumerate() {
            let catching_up = i < heights.len() - 1;
            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "result": {
                        "node_info": {
                            "network": network,
                            "moniker": "test-node"
                        },
                        "sync_info": {
                            "latest_block_height": height.to_string(),
                            "catching_up": catching_up
                        }
                    }
                })))
                .up_to_n_times(1)
                .mount(&self.server)
                .await;
        }
    }

    /// Mock node with custom sync info
    pub async fn mock_custom_sync_info(
        &self,
        network: &str,
        latest_block: u64,
        catching_up: bool,
        earliest_block: u64,
        block_time: &str,
    ) {
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "node_info": {
                        "network": network,
                        "moniker": "test-node"
                    },
                    "sync_info": {
                        "latest_block_height": latest_block.to_string(),
                        "earliest_block_height": earliest_block.to_string(),
                        "latest_block_time": block_time,
                        "catching_up": catching_up
                    }
                }
            })))
            .mount(&self.server)
            .await;
    }
}
