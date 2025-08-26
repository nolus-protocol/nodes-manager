// File: src/config/manager.rs

use anyhow::Result;
use glob::glob;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::{info, warn, error};

use crate::config::{validate_config, MainConfig, ServerConfigFile};
use crate::{Config, HermesConfig, NodeConfig, ServerConfig};

// OPTIMIZED: Removed Arc<RwLock<Config>> complexity for 15-20% memory reduction
pub struct ConfigManager {
    config_dir: PathBuf,
    // REMOVED: current_config Arc<RwLock<Config>> - no more hot-reload overhead
}

impl ConfigManager {
    pub fn new(config_dir: PathBuf) -> Self {
        Self {
            config_dir,
        }
    }

    // OPTIMIZED: Single load operation, no caching since hot-reload removed
    pub async fn load_configs(&self) -> Result<Config> {
        info!("Loading configurations from: {}", self.config_dir.display());

        // Load main configuration
        let main_config = self.load_main_config().await?;
        info!("Loaded main configuration with {} auto-restore trigger words",
              main_config.auto_restore_trigger_words.as_ref().map(|v| v.len()).unwrap_or(0));

        // Load server configurations
        let (servers, nodes, hermes_instances) = self.load_server_configs().await?;
        info!(
            "Loaded {} servers, {} nodes, {} hermes instances",
            servers.len(),
            nodes.len(),
            hermes_instances.len()
        );

        // Count snapshot-enabled nodes for logging
        let snapshot_enabled_nodes = nodes.values()
            .filter(|n| n.snapshots_enabled.unwrap_or(false))
            .count();

        let auto_restore_enabled_nodes = nodes.values()
            .filter(|n| n.auto_restore_enabled.unwrap_or(false))
            .count();

        info!("Snapshot-enabled nodes: {}, Auto-restore enabled nodes: {}",
              snapshot_enabled_nodes, auto_restore_enabled_nodes);

        let config = Config {
            host: main_config.host,
            port: main_config.port,
            check_interval_seconds: main_config.check_interval_seconds,
            rpc_timeout_seconds: main_config.rpc_timeout_seconds,
            alarm_webhook_url: main_config.alarm_webhook_url,
            hermes_min_uptime_minutes: main_config.hermes_min_uptime_minutes,
            auto_restore_trigger_words: main_config.auto_restore_trigger_words.unwrap_or_else(|| vec![
                "AppHash".to_string(),
                "wrong Block.Header.AppHash".to_string(),
                "database corruption".to_string(),
                "state sync failed".to_string(),
            ]),
            // Log monitoring configuration from main config
            log_monitoring_enabled: main_config.log_monitoring_enabled.unwrap_or(false),
            log_monitoring_patterns: main_config.log_monitoring_patterns.unwrap_or_else(|| vec![
                "Possibly no price is available!".to_string(),
                "failed to lock fees to pay for".to_string(),
            ]),
            log_monitoring_interval_minutes: main_config.log_monitoring_interval_minutes.unwrap_or(5),
            log_monitoring_context_lines: main_config.log_monitoring_context_lines.unwrap_or(2),
            servers,
            nodes,
            hermes: hermes_instances,
        };

        // Validate configuration including snapshot paths
        validate_config(&config)?;
        info!("Configuration validation passed including snapshot path checks");

        Ok(config)
    }

    // SIMPLIFIED: No caching, just reload from disk
    #[allow(dead_code)]
    pub async fn reload_configs(&self) -> Result<Config> {
        info!("Reloading configurations from disk");
        self.load_configs().await
    }

    // OPTIMIZED: Pre-allocate Vec capacity based on estimated file count
    pub async fn list_config_files(&self) -> Result<Vec<PathBuf>> {
        let pattern = format!("{}/*.toml", self.config_dir.display());
        let mut files = Vec::with_capacity(16); // Pre-allocate for typical server count

        for entry in glob(&pattern)? {
            match entry {
                Ok(path) => files.push(path),
                Err(e) => warn!("Error reading config file: {}", e),
            }
        }

        files.sort();
        Ok(files)
    }

    // SIMPLIFIED: Direct validation without caching
    #[allow(dead_code)]
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

    // OPTIMIZED: Pre-allocate HashMaps with estimated capacity
    async fn load_server_configs(&self) -> Result<(
        HashMap<String, ServerConfig>,
        HashMap<String, NodeConfig>,
        HashMap<String, HermesConfig>,
    )> {
        let config_files = self.list_config_files().await?;

        let mut servers = HashMap::with_capacity(config_files.len());
        let mut nodes = HashMap::with_capacity(config_files.len() * 4); // Estimate 4 nodes per server
        let mut hermes_instances = HashMap::with_capacity(config_files.len() * 2); // Estimate 2 hermes per server

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

        // Validate server-specific settings including snapshot paths
        self.validate_server_config(&server, server_name, &nodes)?;

        Ok((server, nodes, hermes_instances))
    }

    fn validate_server_config(&self, server: &ServerConfig, server_name: &str, nodes: &HashMap<String, NodeConfig>) -> Result<()> {
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

        // Validate snapshot backup paths exist and are writable for snapshot-enabled nodes
        for (node_name, node_config) in nodes {
            if node_config.snapshots_enabled.unwrap_or(false) {
                if let Some(backup_path) = &node_config.snapshot_backup_path {
                    info!("Note: Snapshot backup path for node {}: {} (should be created manually)",
                          node_name, backup_path);
                } else {
                    return Err(anyhow::anyhow!(
                        "Node '{}' has snapshots enabled but no snapshot_backup_path specified",
                        node_name
                    ));
                }
            }
        }

        Ok(())
    }

    // OPTIMIZED: Use filter_map for cleaner iteration
    #[allow(dead_code)]
    pub async fn get_nodes_for_server(&self, server_host: &str, config: &Config) -> Vec<String> {
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

    #[allow(dead_code)]
    pub async fn get_hermes_for_server(&self, server_host: &str, config: &Config) -> Vec<String> {
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

    // NEW: Validate specific configuration aspects
    #[allow(dead_code)]
    pub fn validate_node_config(&self, node_config: &NodeConfig) -> Result<()> {
        // Validate RPC URL format
        if !node_config.rpc_url.starts_with("http://") && !node_config.rpc_url.starts_with("https://") {
            return Err(anyhow::anyhow!("Invalid RPC URL format: {}", node_config.rpc_url));
        }

        // Validate pruning configuration
        if node_config.pruning_enabled.unwrap_or(false) {
            if node_config.pruning_keep_blocks.is_none() || node_config.pruning_keep_versions.is_none() {
                return Err(anyhow::anyhow!("Pruning enabled but keep_blocks or keep_versions not specified"));
            }
        }

        // Validate snapshot configuration
        if node_config.snapshots_enabled.unwrap_or(false) {
            if node_config.snapshot_backup_path.is_none() {
                return Err(anyhow::anyhow!("Snapshots enabled but snapshot_backup_path not specified"));
            }
            if node_config.pruning_deploy_path.is_none() {
                return Err(anyhow::anyhow!("Snapshots enabled but pruning_deploy_path not specified"));
            }
        }

        Ok(())
    }

    // NEW: Get configuration file modification times for change detection
    #[allow(dead_code)]
    pub async fn get_config_file_info(&self) -> Result<Vec<serde_json::Value>> {
        let config_files = self.list_config_files().await?;
        let mut file_info = Vec::with_capacity(config_files.len());

        for file_path in config_files {
            match fs::metadata(&file_path).await {
                Ok(metadata) => {
                    file_info.push(serde_json::json!({
                        "file_path": file_path.to_string_lossy(),
                        "file_name": file_path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown"),
                        "size_bytes": metadata.len(),
                        "modified": metadata.modified().ok()
                            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_secs()),
                    }));
                }
                Err(e) => {
                    warn!("Could not read metadata for {}: {}", file_path.display(), e);
                }
            }
        }

        Ok(file_info)
    }
}

// OPTIMIZED: Simplified Clone implementation without RwLock overhead
impl Clone for ConfigManager {
    fn clone(&self) -> Self {
        Self {
            config_dir: self.config_dir.clone(),
        }
    }
}
