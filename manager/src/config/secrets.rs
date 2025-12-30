// File: manager/src/config/secrets.rs
//! Secrets loader for API keys and other sensitive configuration.
//!
//! Secrets are stored in a separate TOML file (config/secrets.toml) that should
//! be excluded from version control. The database stores references to secrets
//! (api_key_ref), and this module resolves them to actual values at runtime.
//!
//! Example secrets.toml:
//! ```toml
//! [servers]
//! enterprise = "secret-api-key-1"
//! discovery = "secret-api-key-2"
//! ```

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use tracing::{info, warn};

/// Structure matching the secrets.toml file format
#[derive(Debug, Deserialize, Default)]
pub struct SecretsFile {
    #[serde(default)]
    pub servers: HashMap<String, String>,
}

/// Loader for secrets from the secrets.toml file
pub struct SecretsLoader {
    secrets: SecretsFile,
}

impl SecretsLoader {
    /// Load secrets from the specified file path.
    /// Returns an empty loader if the file doesn't exist.
    pub fn load(secrets_path: &Path) -> Result<Self> {
        if !secrets_path.exists() {
            warn!(
                "Secrets file not found at {:?}, API keys will need to be configured",
                secrets_path
            );
            return Ok(Self {
                secrets: SecretsFile::default(),
            });
        }

        let content = std::fs::read_to_string(secrets_path)
            .with_context(|| format!("Failed to read secrets file: {:?}", secrets_path))?;

        let secrets: SecretsFile = toml::from_str(&content)
            .with_context(|| format!("Failed to parse secrets file: {:?}", secrets_path))?;

        info!(
            "Loaded secrets for {} servers from {:?}",
            secrets.servers.len(),
            secrets_path
        );

        Ok(Self { secrets })
    }

    /// Get the API key for a server by its reference name.
    /// Returns None if the secret is not found.
    pub fn get_server_api_key(&self, api_key_ref: &str) -> Option<&str> {
        self.secrets.servers.get(api_key_ref).map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_load_secrets() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[servers]
enterprise = "secret-key-1"
discovery = "secret-key-2"
"#
        )
        .unwrap();

        let loader = SecretsLoader::load(file.path()).unwrap();

        assert_eq!(
            loader.get_server_api_key("enterprise"),
            Some("secret-key-1")
        );
        assert_eq!(loader.get_server_api_key("discovery"), Some("secret-key-2"));
        assert_eq!(loader.get_server_api_key("unknown"), None);
    }

    #[test]
    fn test_missing_file() {
        let loader = SecretsLoader::load(Path::new("/nonexistent/path/secrets.toml")).unwrap();
        assert_eq!(loader.get_server_api_key("any"), None);
    }
}
