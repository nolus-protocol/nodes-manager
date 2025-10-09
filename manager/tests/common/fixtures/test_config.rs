//! Test configuration builder for creating test configs programmatically

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Builder for creating test configurations
pub struct TestConfigBuilder {
    temp_dir: TempDir,
    main_config: MainConfigBuilder,
    server_configs: HashMap<String, ServerConfigBuilder>,
}

impl TestConfigBuilder {
    /// Create a new test config builder
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        Self {
            temp_dir,
            main_config: MainConfigBuilder::default(),
            server_configs: HashMap::new(),
        }
    }

    /// Configure main settings
    pub fn with_main_config<F>(mut self, f: F) -> Self
    where
        F: FnOnce(MainConfigBuilder) -> MainConfigBuilder,
    {
        self.main_config = f(self.main_config);
        self
    }

    /// Add a server configuration
    pub fn with_server<F>(mut self, server_name: &str, f: F) -> Self
    where
        F: FnOnce(ServerConfigBuilder) -> ServerConfigBuilder,
    {
        let builder = f(ServerConfigBuilder::new(server_name));
        self.server_configs.insert(server_name.to_string(), builder);
        self
    }

    /// Build and write config files to temp directory
    pub fn build(self) -> TestConfig {
        let config_dir = self.temp_dir.path().join("config");
        fs::create_dir_all(&config_dir).expect("Failed to create config dir");

        // Write main.toml
        let main_toml = self.main_config.to_toml();
        fs::write(config_dir.join("main.toml"), main_toml).expect("Failed to write main.toml");

        // Write server configs
        for (name, builder) in self.server_configs {
            let server_toml = builder.to_toml();
            fs::write(config_dir.join(format!("{}.toml", name)), server_toml)
                .expect("Failed to write server config");
        }

        TestConfig {
            _temp_dir: self.temp_dir,
            config_dir,
        }
    }
}

impl Default for TestConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Main configuration builder
#[derive(Clone)]
pub struct MainConfigBuilder {
    host: String,
    port: u16,
    health_check_interval_seconds: u32,
    alert_webhook_url: Option<String>,
}

impl MainConfigBuilder {
    pub fn host(mut self, host: &str) -> Self {
        self.host = host.to_string();
        self
    }

    pub fn port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    pub fn health_check_interval(mut self, seconds: u32) -> Self {
        self.health_check_interval_seconds = seconds;
        self
    }

    pub fn alert_webhook(mut self, url: &str) -> Self {
        self.alert_webhook_url = Some(url.to_string());
        self
    }

    fn to_toml(&self) -> String {
        let webhook = self.alert_webhook_url.as_deref().unwrap_or("");
        format!(
            r#"
[server]
host = "{}"
port = {}

[health]
check_interval_seconds = {}

[alerts]
webhook_url = "{}"
"#,
            self.host, self.port, self.health_check_interval_seconds, webhook
        )
    }
}

impl Default for MainConfigBuilder {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            health_check_interval_seconds: 90,
            alert_webhook_url: None,
        }
    }
}

/// Server configuration builder
pub struct ServerConfigBuilder {
    name: String,
    host: String,
    agent_port: u16,
    api_key: String,
    nodes: Vec<NodeConfigBuilder>,
}

impl ServerConfigBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            host: "localhost".to_string(),
            agent_port: 8745,
            api_key: "test-api-key".to_string(),
            nodes: Vec::new(),
        }
    }

    pub fn host(mut self, host: &str) -> Self {
        self.host = host.to_string();
        self
    }

    pub fn agent_port(mut self, port: u16) -> Self {
        self.agent_port = port;
        self
    }

    pub fn api_key(mut self, key: &str) -> Self {
        self.api_key = key.to_string();
        self
    }

    pub fn add_node<F>(mut self, f: F) -> Self
    where
        F: FnOnce(NodeConfigBuilder) -> NodeConfigBuilder,
    {
        let builder = f(NodeConfigBuilder::default());
        self.nodes.push(builder);
        self
    }

    fn to_toml(&self) -> String {
        let mut toml = format!(
            r#"
[server]
host = "{}"
agent_port = {}
api_key = "{}"
"#,
            self.host, self.agent_port, self.api_key
        );

        for node in &self.nodes {
            toml.push_str(&node.to_toml());
        }

        toml
    }
}

/// Node configuration builder
#[derive(Clone)]
pub struct NodeConfigBuilder {
    name: String,
    rpc_url: String,
    network: String,
    enable_pruning: bool,
    enable_snapshots: bool,
}

impl NodeConfigBuilder {
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn rpc_url(mut self, url: &str) -> Self {
        self.rpc_url = url.to_string();
        self
    }

    pub fn network(mut self, network: &str) -> Self {
        self.network = network.to_string();
        self
    }

    pub fn enable_pruning(mut self, enable: bool) -> Self {
        self.enable_pruning = enable;
        self
    }

    pub fn enable_snapshots(mut self, enable: bool) -> Self {
        self.enable_snapshots = enable;
        self
    }

    fn to_toml(&self) -> String {
        format!(
            r#"
[[nodes]]
name = "{}"
rpc_url = "{}"
network = "{}"
enable_pruning = {}
enable_snapshots = {}
"#,
            self.name, self.rpc_url, self.network, self.enable_pruning, self.enable_snapshots
        )
    }
}

impl Default for NodeConfigBuilder {
    fn default() -> Self {
        Self {
            name: "test-node".to_string(),
            rpc_url: "http://localhost:26657".to_string(),
            network: "test-network".to_string(),
            enable_pruning: true,
            enable_snapshots: true,
        }
    }
}

/// Built test configuration with temp directory
pub struct TestConfig {
    _temp_dir: TempDir,
    pub config_dir: PathBuf,
}

impl TestConfig {
    /// Get the config directory path
    pub fn config_dir(&self) -> &PathBuf {
        &self.config_dir
    }
}
