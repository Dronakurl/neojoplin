// Configuration management for NeoJoplin TUI

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// WebDAV configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebDavConfig {
    pub url: String,
    pub username: String,
    pub password: String,
    pub remote_path: String,
}

impl Default for WebDavConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            username: String::new(),
            password: String::new(),
            remote_path: "/neojoplin".to_string(),
        }
    }
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub webdav: WebDavConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            webdav: WebDavConfig::default(),
        }
    }
}

impl Config {
    /// Get configuration file path
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("Failed to determine config directory")?;
        Ok(config_dir.join("neojoplin").join("config.toml"))
    }

    /// Load configuration from file
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;

        if !path.exists() {
            // Create default config
            let config = Config::default();
            config.save()?;
            return Ok(config);
        }

        let content = fs::read_to_string(&path)
            .context("Failed to read config file")?;

        let config: Config = toml::from_str(&content)
            .context("Failed to parse config file")?;

        Ok(config)
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;

        // Create config directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create config directory")?;
        }

        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;

        fs::write(&path, content)
            .context("Failed to write config file")?;

        Ok(())
    }

    /// Update WebDAV configuration
    pub fn update_webdav<F>(&mut self, f: F) -> Result<()>
    where
        F: FnOnce(&mut WebDavConfig),
    {
        f(&mut self.webdav);
        self.save()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.webdav.url, "");
        assert_eq!(config.webdav.remote_path, "/neojoplin");
    }

    #[test]
    fn test_webdav_config_default() {
        let webdav = WebDavConfig::default();
        assert_eq!(webdav.url, "");
        assert_eq!(webdav.username, "");
        assert_eq!(webdav.password, "");
        assert_eq!(webdav.remote_path, "/neojoplin");
    }
}
