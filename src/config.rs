use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub api_key: Option<String>,
    pub default_model: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: None,
            default_model: Some("anthropic/claude-3.5-sonnet".to_string()),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = get_config_path()?;
        
        if !config_path.exists() {
            return Ok(Config::default());
        }

        let config_str = std::fs::read_to_string(&config_path)
            .context("Failed to read config file")?;
        
        toml::from_str(&config_str)
            .context("Failed to parse config file")
    }

    pub fn save(&self) -> Result<()> {
        let config_path = get_config_path()?;
        
        // Ensure parent directory exists
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
        }

        let config_str = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        
        std::fs::write(&config_path, config_str)
            .context("Failed to write config file")?;
        
        Ok(())
    }
}

pub fn get_config_path() -> Result<PathBuf> {
    let mut path = dirs::config_dir()
        .context("Failed to determine config directory")?;
    path.push("llm-chat");
    path.push("config.toml");
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.api_key.is_none());
        assert_eq!(config.default_model, Some("anthropic/claude-3.5-sonnet".to_string()));
    }
}
