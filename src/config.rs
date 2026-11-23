use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

pub const DEFAULT_MODEL: &str = "anthropic/claude-3.5-sonnet";
pub const API_KEY_VARIABLE: &str = "OPENROUTER_API_KEY";

#[derive(Debug, Deserialize)]
pub struct Config {
    pub api_key: Option<String>,
    pub default_model: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_key: None,
            default_model: Some(DEFAULT_MODEL.to_string()),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = get_config_path()?;

        let mut config: Config = if config_path.exists() {
            let config_str =
                std::fs::read_to_string(&config_path).context("Failed to read config file")?;
            toml::from_str(&config_str).context("Failed to parse config file")?
        } else {
            Config::default()
        };

        if let Some(key) = std::env::var(API_KEY_VARIABLE)
            .ok()
            .filter(|key| !key.trim().is_empty())
        {
            config.api_key = Some(key);
        }

        Ok(config)
    }
}

pub fn get_config_path() -> Result<PathBuf> {
    let mut path = dirs::config_dir().context("Failed to determine config directory")?;
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
        assert_eq!(
            config.default_model,
            Some("anthropic/claude-3.5-sonnet".to_string())
        );
    }
}
