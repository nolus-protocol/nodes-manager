// File: manager/src/config/manager.rs
use super::{Config, ConfigStore, SecretsLoader, ServerConfigFile};
use crate::database::Database;
use anyhow::{anyhow, Result};
use glob::glob;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Configuration source indicates where configuration was loaded from
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigSource {
    /// Configuration loaded from database
    Database,
    /// Configuration loaded from TOML files (legacy mode, used by tests)
    #[allow(dead_code)]
    TomlFiles,
}

pub struct ConfigManager {
    config: Arc<RwLock<Arc<Config>>>,
    config_store: Option<Arc<ConfigStore>>,
    secrets: Arc<SecretsLoader>,
    config_dir: String,
    source: ConfigSource,
}

impl ConfigManager {
    /// Create a new ConfigManager that loads configuration from database if available,
    /// otherwise falls back to TOML files.
    pub async fn new(config_dir: String, db: Arc<Database>) -> Result<Self> {
        // Load secrets first
        let secrets_path = Path::new(&config_dir).join("secrets.toml");
        let secrets = Arc::new(SecretsLoader::load(&secrets_path)?);

        // Create config store
        let config_store = Arc::new(ConfigStore::new(db.clone()));

        // Check if database has configuration
        if config_store.has_config_data().await? {
            info!("Loading configuration from database");
            let config = Self::load_from_database(&config_store, &secrets).await?;
            Ok(Self {
                config: Arc::new(RwLock::new(Arc::new(config))),
                config_store: Some(config_store),
                secrets,
                config_dir,
                source: ConfigSource::Database,
            })
        } else {
            info!("No configuration in database, loading from TOML files");
            let config = Self::load_from_toml(&config_dir).await?;

            // Auto-import to database for future use
            info!("Importing TOML configuration to database");
            let import_result = config_store.import_from_config(&config).await?;
            info!(
                "Imported to database: {} servers, {} nodes, {} hermes",
                import_result.servers_created + import_result.servers_updated,
                import_result.nodes_created + import_result.nodes_updated,
                import_result.hermes_created + import_result.hermes_updated,
            );

            // Apply secrets to the config
            let config = Self::apply_secrets_to_config(config, &secrets);

            Ok(Self {
                config: Arc::new(RwLock::new(Arc::new(config))),
                config_store: Some(config_store),
                secrets,
                config_dir,
                source: ConfigSource::Database, // Now using database
            })
        }
    }

    /// Create a ConfigManager that only uses TOML files (legacy mode, for testing)
    #[allow(dead_code)]
    pub async fn new_legacy(config_dir: String) -> Result<Self> {
        let secrets_path = Path::new(&config_dir).join("secrets.toml");
        let secrets = Arc::new(SecretsLoader::load(&secrets_path)?);

        let config = Self::load_from_toml(&config_dir).await?;
        let config = Self::apply_secrets_to_config(config, &secrets);

        Ok(Self {
            config: Arc::new(RwLock::new(Arc::new(config))),
            config_store: None,
            secrets,
            config_dir,
            source: ConfigSource::TomlFiles,
        })
    }

    pub async fn get_current_config(&self) -> Arc<Config> {
        self.config.read().await.clone()
    }

    /// Get the configuration source
    pub fn get_source(&self) -> &ConfigSource {
        &self.source
    }

    /// Get the config store for direct database operations
    pub fn get_store(&self) -> Option<Arc<ConfigStore>> {
        self.config_store.clone()
    }

    /// Reload configuration from the database
    pub async fn reload_from_database(&self) -> Result<()> {
        if let Some(ref store) = self.config_store {
            let config = Self::load_from_database(store, &self.secrets).await?;
            let mut config_guard = self.config.write().await;
            *config_guard = Arc::new(config);
            info!("Configuration reloaded from database");
            Ok(())
        } else {
            Err(anyhow!("Database-backed configuration not available"))
        }
    }

    /// Re-import configuration from TOML files to database (merge mode)
    pub async fn reimport_from_toml(&self) -> Result<super::ImportResult> {
        if let Some(ref store) = self.config_store {
            let toml_config = Self::load_from_toml(&self.config_dir).await?;
            let result = store.import_from_config(&toml_config).await?;

            // Reload the configuration
            self.reload_from_database().await?;

            Ok(result)
        } else {
            Err(anyhow!("Database-backed configuration not available"))
        }
    }

    /// Load configuration from the database and apply secrets
    async fn load_from_database(store: &ConfigStore, secrets: &SecretsLoader) -> Result<Config> {
        let config = store
            .load_config()
            .await?
            .ok_or_else(|| anyhow!("No configuration found in database"))?;

        Ok(Self::apply_secrets_to_config(config, secrets))
    }

    /// Apply secrets (API keys) to a configuration
    fn apply_secrets_to_config(mut config: Config, secrets: &SecretsLoader) -> Config {
        for (server_name, server_config) in config.servers.iter_mut() {
            if let Some(api_key) = secrets.get_server_api_key(server_name) {
                server_config.api_key = api_key.to_string();
                debug!("Applied API key for server: {}", server_name);
            } else {
                warn!("No API key found in secrets for server: {}", server_name);
            }
        }
        config
    }

    /// Load configuration from TOML files (legacy method)
    async fn load_from_toml(config_dir: &str) -> Result<Config> {
        let main_config_path = format!("{}/main.toml", config_dir);
        let main_config_content = fs::read_to_string(&main_config_path)
            .await
            .map_err(|e| anyhow!("Failed to read main config {}: {}", main_config_path, e))?;

        let mut config: Config = toml::from_str(&main_config_content)
            .map_err(|e| anyhow!("Failed to parse main config: {}", e))?;

        // Load server-specific configurations
        let pattern = format!("{}/*.toml", config_dir);
        let mut server_configs = HashMap::new();
        let mut all_nodes = HashMap::new();
        let mut all_hermes = HashMap::new();

        for entry in glob(&pattern).map_err(|e| anyhow!("Glob pattern error: {}", e))? {
            let path = entry.map_err(|e| anyhow!("Glob entry error: {}", e))?;
            let filename = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| anyhow!("Invalid filename"))?;

            // Skip main.toml and secrets.toml
            if filename == "main.toml" || filename == "secrets.toml" {
                continue;
            }

            let server_name = filename
                .strip_suffix(".toml")
                .ok_or_else(|| anyhow!("Invalid config filename: {}", filename))?;

            debug!("Loading server config: {}", path.display());

            let content = fs::read_to_string(&path)
                .await
                .map_err(|e| anyhow!("Failed to read {}: {}", path.display(), e))?;

            let server_config_file: ServerConfigFile = toml::from_str(&content)
                .map_err(|e| anyhow!("Failed to parse {}: {}", path.display(), e))?;

            // Store server configuration
            server_configs.insert(server_name.to_string(), server_config_file.server);

            // Collect nodes from this server with smart naming
            for (node_name, mut node_config) in server_config_file.nodes {
                node_config.server_host = server_name.to_string();

                // Smart node naming - don't double-prefix if already prefixed
                let final_node_name = if node_name.starts_with(&format!("{}-", server_name)) {
                    node_name.clone()
                } else {
                    format!("{}-{}", server_name, node_name)
                };

                // Auto-detect network from RPC if not specified or set to "auto"
                if node_config.network.is_empty() || node_config.network == "auto" {
                    debug!(
                        "Auto-detecting network for {} from RPC {}",
                        final_node_name, node_config.rpc_url
                    );
                    match crate::rpc::fetch_network_from_rpc_standalone(&node_config.rpc_url).await
                    {
                        Ok(detected_network) => {
                            info!(
                                "Auto-detected network for {}: {}",
                                final_node_name, detected_network
                            );
                            node_config.network = detected_network;
                        }
                        Err(e) => {
                            warn!("Failed to auto-detect network for {}: {}. Please specify 'network' in config.", final_node_name, e);
                        }
                    }
                }

                // Apply smart defaults to node config
                node_config =
                    node_config.with_defaults(&server_config_file.defaults, &final_node_name);

                all_nodes.insert(final_node_name, node_config);
            }

            // Collect Hermes instances from this server with smart naming
            if let Some(hermes_configs) = server_config_file.hermes {
                for (hermes_name, mut hermes_config) in hermes_configs {
                    hermes_config.server_host = server_name.to_string();

                    let final_hermes_name = if hermes_name.starts_with(&format!("{}-", server_name))
                    {
                        hermes_name
                    } else {
                        format!("{}-{}", server_name, hermes_name)
                    };

                    all_hermes.insert(final_hermes_name, hermes_config);
                }
            }
        }

        config.servers = server_configs;
        config.nodes = all_nodes;
        config.hermes = all_hermes;

        info!(
            "Loaded {} servers, {} nodes, {} hermes instances from TOML",
            config.servers.len(),
            config.nodes.len(),
            config.hermes.len()
        );

        Ok(config)
    }
}
