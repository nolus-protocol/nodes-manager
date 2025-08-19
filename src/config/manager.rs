// File: src/config/manager.rs

use anyhow::Result;
use glob::glob;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{info, warn, error};

use crate::config::{validate_config, MainConfig, ServerConfigFile};
use crate::{Config, HermesConfig, NodeConfig, ServerConfig};

pub struct ConfigManager {
    config_dir: PathBuf,
    current_config: Arc<RwLock<Config>>,
}

impl ConfigManager {
    pub fn new(config_dir: PathBuf) -> Self {
        Self {
            config_dir,
            current_config: Arc::new(RwLock::new(Config::default())),
        }
    }

    pub async fn load_configs(&self) -> Result<Config> {
        info!("Loading configurations from: {}", self.config_dir.display());

        // Load main configuration
        let main_config = self.load_main_config().await?;
        info!("Loaded main configuration");

        // Load server configurations
        let (servers, nodes, hermes_instances) = self.load_server_configs().await?;
        info!(
            "Loaded {} servers, {} nodes, {} hermes instances",
            servers.len(),
            nodes.len(),
            hermes_instances.len()
        );

        let config = Config {
            host: main_config.host,
            port: main_config.port,
            check_interval_seconds: main_config.check_interval_seconds,
            rpc_timeout_seconds: main_config.rpc_timeout_seconds,
            alarm_webhook_url: main_config.alarm_webhook_url,
            hermes_min_uptime_minutes: main_config.hermes_min_uptime_minutes,
            servers,
            nodes,
            hermes: hermes_instances,
        };

        // Validate configuration
        validate_config(&config)?;
        info!("Configuration validation passed");

        // Store current config
        let mut current = self.current_config.write().await;
        *current = config.clone();

        Ok(config)
    }

    pub async fn reload_configs(&self) -> Result<Config> {
        info!("Reloading configurations");
        self.load_configs().await
    }

    pub async fn get_current_config(&self) -> Config {
        let config = self.current_config.read().await;
        config.clone()
    }

    pub async fn list_config_files(&self) -> Result<Vec<PathBuf>> {
        let pattern = format!("{}/*.toml", self.config_dir.display());
        let mut files = Vec::new();

        for entry in glob(&pattern)? {
            match entry {
                Ok(path) => files.push(path),
                Err(e) => warn!("Error reading config file: {}", e),
            }
        }

        files.sort();
        Ok(files)
    }

    pub fn validate_config_content(&self, config: &Config) -> Result<()> {
        validate_config(config)
    }

    async fn load_main_config(&self) -> Result<MainConfig> {
        let main_path = self.config_dir.join("main.toml");

        if !main_path.exists() {
            return Err(anyhow::anyhow!(
                "Main configuration file not found: {}",
                main_path.display()
            ));
        }

        let content = fs::read_to_string(&main_path).await?;
        let config: MainConfig = toml::from_str(&content)?;

        Ok(config)
    }

    async fn load_server_configs(&self) -> Result<(
        HashMap<String, ServerConfig>,
        HashMap<String, NodeConfig>,
        HashMap<String, HermesConfig>,
    )> {
        let config_files = self.list_config_files().await?;

        let mut servers = HashMap::new();
        let mut nodes = HashMap::new();
        let mut hermes_instances = HashMap::new();

        for config_file in config_files {
            // Skip main.toml
            if config_file.file_stem().and_then(|s| s.to_str()) == Some("main") {
                continue;
            }

            let server_name = config_file
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| anyhow::anyhow!("Invalid config file name: {:?}", config_file))?
                .to_string();

            match self.load_server_config(&config_file, &server_name).await {
                Ok((server_config, server_nodes, server_hermes)) => {
                    servers.insert(server_name.clone(), server_config);

                    // Add nodes with proper naming
                    for (node_key, node_config) in server_nodes {
                        let full_node_name = format!("{}-{}", server_name, node_key);
                        nodes.insert(full_node_name, node_config);
                    }

                    // Add hermes instances with proper naming
                    for (hermes_key, hermes_config) in server_hermes {
                        let full_hermes_name = format!("{}-{}", server_name, hermes_key);
                        hermes_instances.insert(full_hermes_name, hermes_config);
                    }

                    info!("Loaded configuration for server: {}", server_name);
                }
                Err(e) => {
                    error!("Failed to load config for {}: {}", server_name, e);
                    return Err(e);
                }
            }
        }

        Ok((servers, nodes, hermes_instances))
    }

    async fn load_server_config(
        &self,
        config_path: &Path,
        server_name: &str,
    ) -> Result<(
        ServerConfig,
        HashMap<String, NodeConfig>,
        HashMap<String, HermesConfig>,
    )> {
        let content = fs::read_to_string(config_path).await.map_err(|e| {
            anyhow::anyhow!("Failed to read config file {}: {}", config_path.display(), e)
        })?;

        let server_config: ServerConfigFile = toml::from_str(&content).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse config file {}: {}",
                config_path.display(),
                e
            )
        })?;

        // Extract server config
        let server = server_config.server;

        // Extract and convert nodes
        let mut nodes = HashMap::new();
        if let Some(file_nodes) = server_config.nodes {
            for (node_name, node_config) in file_nodes {
                nodes.insert(node_name, node_config.into());
            }
        }

        // Extract and convert hermes instances
        let mut hermes_instances = HashMap::new();
        if let Some(file_hermes) = server_config.hermes {
            for (hermes_name, hermes_config) in file_hermes {
                hermes_instances.insert(hermes_name, hermes_config.into());
            }
        }

        // Validate server-specific settings
        self.validate_server_config(&server, server_name)?;

        Ok((server, nodes, hermes_instances))
    }

    fn validate_server_config(&self, server: &ServerConfig, server_name: &str) -> Result<()> {
        // Validate SSH key path exists
        if !Path::new(&server.ssh_key_path).exists() {
            warn!(
                "SSH key path does not exist for server {}: {}",
                server_name, server.ssh_key_path
            );
        }

        // Validate reasonable timeout values
        if server.ssh_timeout_seconds < 5 || server.ssh_timeout_seconds > 300 {
            warn!(
                "SSH timeout for server {} seems unreasonable: {}s",
                server_name, server.ssh_timeout_seconds
            );
        }

        // Validate reasonable concurrency limits (if specified)
        if let Some(max_concurrent) = server.max_concurrent_ssh {
            if max_concurrent == 0 || max_concurrent > 20 {
                return Err(anyhow::anyhow!(
                    "Invalid max_concurrent_ssh for server {}: {}",
                    server_name,
                    max_concurrent
                ));
            }
        }

        Ok(())
    }

    pub async fn update_node_config(&self, node_name: &str, new_config: &NodeConfig) -> Result<()> {
        let mut current = self.current_config.write().await;
        current.nodes.insert(node_name.to_string(), new_config.clone());

        // Validate the updated configuration
        validate_config(&current)?;

        info!("Updated configuration for node: {}", node_name);
        Ok(())
    }

    pub async fn get_nodes_for_server(&self, server_host: &str) -> Vec<String> {
        let config = self.current_config.read().await;
        config
            .nodes
            .iter()
            .filter_map(|(name, node)| {
                if node.server_host == server_host {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub async fn get_hermes_for_server(&self, server_host: &str) -> Vec<String> {
        let config = self.current_config.read().await;
        config
            .hermes
            .iter()
            .filter_map(|(name, hermes)| {
                if hermes.server_host == server_host {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}
