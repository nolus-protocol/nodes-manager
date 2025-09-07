// File: manager/src/config/manager.rs
use super::{Config, ServerConfigFile};
use anyhow::{anyhow, Result};
use glob::glob;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::fs;
use tracing::{debug, info};

pub struct ConfigManager {
    current_config: Arc<Config>,
}

impl ConfigManager {
    pub async fn new(config_dir: String) -> Result<Self> {
        let config = Self::load_configuration(&config_dir).await?;
        Ok(Self {
            current_config: Arc::new(config),
        })
    }

    pub fn get_current_config(&self) -> Arc<Config> {
        self.current_config.clone()
    }

    async fn load_configuration(config_dir: &str) -> Result<Config> {
        let main_config_path = format!("{}/main.toml", config_dir);
        let main_config_content = fs::read_to_string(&main_config_path).await
            .map_err(|e| anyhow!("Failed to read main config {}: {}", main_config_path, e))?;

        let mut config: Config = toml::from_str(&main_config_content)
            .map_err(|e| anyhow!("Failed to parse main config: {}", e))?;

        // Load server-specific configurations
        let pattern = format!("{}/*.toml", config_dir);
        let mut server_configs = HashMap::new();
        let mut all_nodes = HashMap::new();
        let mut all_hermes = HashMap::new();
        let mut all_etl = HashMap::new();

        for entry in glob(&pattern).map_err(|e| anyhow!("Glob pattern error: {}", e))? {
            let path = entry.map_err(|e| anyhow!("Glob entry error: {}", e))?;
            let filename = path.file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| anyhow!("Invalid filename"))?;

            // Skip main.toml as it's already loaded
            if filename == "main.toml" {
                continue;
            }

            let server_name = filename.strip_suffix(".toml")
                .ok_or_else(|| anyhow!("Invalid config filename: {}", filename))?;

            debug!("Loading server config: {}", path.display());

            let content = fs::read_to_string(&path).await
                .map_err(|e| anyhow!("Failed to read {}: {}", path.display(), e))?;

            let server_config_file: ServerConfigFile = toml::from_str(&content)
                .map_err(|e| anyhow!("Failed to parse {}: {}", path.display(), e))?;

            // Store server configuration
            server_configs.insert(server_name.to_string(), server_config_file.server);

            // FIXED: Collect nodes from this server with smart naming
            for (node_name, mut node_config) in server_config_file.nodes {
                node_config.server_host = server_name.to_string();

                // FIXED: Smart node naming - don't double-prefix if already prefixed
                let final_node_name = if node_name.starts_with(&format!("{}-", server_name)) {
                    // Node name already includes server prefix, use as-is
                    node_name
                } else {
                    // Node name doesn't include server prefix, add it
                    format!("{}-{}", server_name, node_name)
                };

                all_nodes.insert(final_node_name, node_config);
            }

            // FIXED: Collect Hermes instances from this server with smart naming
            if let Some(hermes_configs) = server_config_file.hermes {
                for (hermes_name, mut hermes_config) in hermes_configs {
                    hermes_config.server_host = server_name.to_string();

                    // FIXED: Smart hermes naming - don't double-prefix if already prefixed
                    let final_hermes_name = if hermes_name.starts_with(&format!("{}-", server_name)) {
                        // Hermes name already includes server prefix, use as-is
                        hermes_name
                    } else {
                        // Hermes name doesn't include server prefix, add it
                        format!("{}-{}", server_name, hermes_name)
                    };

                    all_hermes.insert(final_hermes_name, hermes_config);
                }
            }

            // NEW: Collect ETL services from this server with smart naming
            if let Some(etl_configs) = server_config_file.etl {
                for (etl_name, mut etl_config) in etl_configs {
                    etl_config.server_host = server_name.to_string();

                    // Smart ETL naming - don't double-prefix if already prefixed
                    let final_etl_name = if etl_name.starts_with(&format!("{}-", server_name)) {
                        // ETL name already includes server prefix, use as-is
                        etl_name
                    } else {
                        // ETL name doesn't include server prefix, add it
                        format!("{}-{}", server_name, etl_name)
                    };

                    all_etl.insert(final_etl_name, etl_config);
                }
            }
        }

        config.servers = server_configs;
        config.nodes = all_nodes;
        config.hermes = all_hermes;
        config.etl = all_etl;

        info!("Loaded {} servers, {} nodes, {} hermes instances, {} ETL services",
            config.servers.len(),
            config.nodes.len(),
            config.hermes.len(),
            config.etl.len()
        );

        Ok(config)
    }
}
