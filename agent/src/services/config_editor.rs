// File: agent/src/services/config_editor.rs
use anyhow::Result;
use tracing::info;

/// Enable state sync in config.toml - FAIL FAST
pub async fn enable_state_sync(
    config_path: &str,
    rpc_servers: &[String],
    trust_height: i64,
    trust_hash: &str,
) -> Result<()> {
    info!("Enabling state sync in {}", config_path);

    // Read current config
    let config_content = tokio::fs::read_to_string(config_path).await
        .map_err(|e| anyhow::anyhow!("Failed to read config file: {}", e))?;

    // Parse and modify config
    let mut modified_config = config_content.clone();

    // Find [statesync] section and update it
    let rpc_servers_str = rpc_servers
        .iter()
        .map(|s| format!("\"{}\"", s))
        .collect::<Vec<_>>()
        .join(",");

    // Pattern to match the [statesync] section
    if let Some(start) = modified_config.find("[statesync]") {
        // Find the next section or end of file
        let after_section = &modified_config[start..];
        let next_section_pos = after_section[11..] // Skip "[statesync]"
            .find("\n[")
            .map(|p| p + start + 11);

        let section_end = next_section_pos.unwrap_or(modified_config.len());

        // Extract section content
        let section = &modified_config[start..section_end];

        // Create updated section
        let mut new_section = String::from("[statesync]\n");

        for line in section.lines().skip(1) { // Skip [statesync] line
            let trimmed = line.trim();

            if trimmed.starts_with("enable ") || trimmed.starts_with("enable=") {
                new_section.push_str("enable = true\n");
            } else if trimmed.starts_with("rpc_servers ") || trimmed.starts_with("rpc_servers=") {
                new_section.push_str(&format!("rpc_servers = \"{},{}\"\n", rpc_servers_str, rpc_servers_str));
            } else if trimmed.starts_with("trust_height ") || trimmed.starts_with("trust_height=") {
                new_section.push_str(&format!("trust_height = {}\n", trust_height));
            } else if trimmed.starts_with("trust_hash ") || trimmed.starts_with("trust_hash=") {
                new_section.push_str(&format!("trust_hash = \"{}\"\n", trust_hash));
            } else if trimmed.starts_with("trust_period ") || trimmed.starts_with("trust_period=") {
                new_section.push_str("trust_period = \"168h0m0s\"\n");
            } else if !trimmed.is_empty() && !trimmed.starts_with('#') {
                new_section.push_str(line);
                new_section.push('\n');
            }
        }

        // Replace section
        modified_config.replace_range(start..section_end, &new_section);
    } else {
        // [statesync] section not found, append it
        modified_config.push_str("\n\n");
        modified_config.push_str("[statesync]\n");
        modified_config.push_str("enable = true\n");
        modified_config.push_str(&format!("rpc_servers = \"{},{}\"\n", rpc_servers_str, rpc_servers_str));
        modified_config.push_str(&format!("trust_height = {}\n", trust_height));
        modified_config.push_str(&format!("trust_hash = \"{}\"\n", trust_hash));
        modified_config.push_str("trust_period = \"168h0m0s\"\n");
    }

    // Write modified config back
    tokio::fs::write(config_path, modified_config).await
        .map_err(|e| anyhow::anyhow!("Failed to write config file: {}", e))?;

    info!("✓ State sync enabled in config");
    Ok(())
}

/// Disable state sync in config.toml - FAIL FAST
pub async fn disable_state_sync(config_path: &str) -> Result<()> {
    info!("Disabling state sync in {}", config_path);

    // Read current config
    let config_content = tokio::fs::read_to_string(config_path).await
        .map_err(|e| anyhow::anyhow!("Failed to read config file: {}", e))?;

    // Parse and modify config
    let mut modified_config = config_content.clone();

    // Find [statesync] section and update enable flag
    if let Some(start) = modified_config.find("[statesync]") {
        // Find the next section or end of file
        let after_section = &modified_config[start..];
        let next_section_pos = after_section[11..] // Skip "[statesync]"
            .find("\n[")
            .map(|p| p + start + 11);

        let section_end = next_section_pos.unwrap_or(modified_config.len());

        // Extract section content
        let section = &modified_config[start..section_end];

        // Create updated section with enable = false
        let mut new_section = String::from("[statesync]\n");

        for line in section.lines().skip(1) { // Skip [statesync] line
            let trimmed = line.trim();

            if trimmed.starts_with("enable ") || trimmed.starts_with("enable=") {
                new_section.push_str("enable = false\n");
            } else if !trimmed.is_empty() {
                new_section.push_str(line);
                new_section.push('\n');
            }
        }

        // Replace section
        modified_config.replace_range(start..section_end, &new_section);
    } else {
        // No [statesync] section, nothing to do
        info!("No [statesync] section found, nothing to disable");
        return Ok(());
    }

    // Write modified config back
    tokio::fs::write(config_path, modified_config).await
        .map_err(|e| anyhow::anyhow!("Failed to write config file: {}", e))?;

    info!("✓ State sync disabled in config");
    Ok(())
}
