// Configuration types and management

use crate::ConfigError;
use joplin_domain::SyncTarget;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Sync configuration
    pub sync: SyncConfig,

    /// Editor configuration
    pub editor: EditorConfig,

    /// UI configuration
    pub ui: UiConfig,

    /// Advanced settings
    pub advanced: AdvancedConfig,
}

/// Sync configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Sync target (WebDAV, OneDrive, etc.)
    pub target: SyncTarget,

    /// Remote path on sync target
    pub remote_path: String,

    /// Sync interval in seconds (0 = manual only)
    pub sync_interval: u64,

    /// Maximum number of retries for failed operations
    pub max_retries: usize,

    /// Retry delay in milliseconds
    pub retry_delay: u64,

    /// Lock timeout in seconds
    pub lock_timeout: u64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            target: SyncTarget::WebDAV,
            remote_path: "/neojoplin".to_string(),
            sync_interval: 0,
            max_retries: 5,
            retry_delay: 200,
            lock_timeout: 300,
        }
    }
}

/// Editor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorConfig {
    /// External editor command
    pub command: Option<String>,

    /// Editor arguments
    pub args: Vec<String>,

    /// Use embedded editor (TUI only)
    pub embedded: bool,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            command: std::env::var("EDITOR").ok(),
            args: Vec::new(),
            embedded: false,
        }
    }
}

/// UI configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    /// Show emoji icons
    pub show_emoji: bool,

    /// Date format for display
    pub date_format: String,

    /// Time format for display
    pub time_format: String,

    /// Items per page in lists
    pub page_size: usize,

    /// Show hidden files
    pub show_hidden: bool,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_emoji: true,
            date_format: "%Y-%m-%d".to_string(),
            time_format: "%H:%M".to_string(),
            page_size: 50,
            show_hidden: false,
        }
    }
}

/// Advanced configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedConfig {
    /// Enable debug logging
    pub debug: bool,

    /// Log file path
    pub log_file: Option<PathBuf>,

    /// Database path
    pub database_path: Option<PathBuf>,

    /// Temporary directory
    pub temp_dir: Option<PathBuf>,

    /// Maximum concurrent operations
    pub max_concurrent_ops: usize,

    /// Connection timeout in seconds
    pub connection_timeout: u64,
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            debug: false,
            log_file: None,
            database_path: None,
            temp_dir: None,
            max_concurrent_ops: 10,
            connection_timeout: 30,
        }
    }
}

impl Config {
    /// Load configuration from file
    pub fn load_from_file(path: &PathBuf) -> Result<Self, crate::ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|_e| ConfigError::NotFound(path.to_string_lossy().to_string()))?;

        let config: Config = serde_json::from_str(&content)
            .map_err(|e| ConfigError::InvalidFormat(format!("Invalid JSON: {}", e)))?;

        Ok(config)
    }

    /// Save configuration to file
    pub fn save_to_file(&self, path: &PathBuf) -> Result<(), crate::ConfigError> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| ConfigError::InvalidFormat(format!("Serialization error: {}", e)))?;

        std::fs::write(path, content).map_err(ConfigError::from)?;

        Ok(())
    }

    /// Get default configuration directory
    pub fn config_dir() -> Result<PathBuf, crate::ConfigError> {
        let dir = dirs::home_dir()
            .map(|p| p.join(".config/neojoplin"))
            .ok_or_else(|| {
                ConfigError::NotFound("Could not determine home directory".to_string())
            })?;

        Ok(dir)
    }

    /// Get default configuration file path
    pub fn default_config_path() -> Result<PathBuf, crate::ConfigError> {
        Ok(Self::config_dir()?.join("config.json"))
    }

    /// Load configuration from default location
    pub fn load() -> Result<Self, crate::ConfigError> {
        let path = Self::default_config_path()?;

        if !path.exists() {
            // Return default config if file doesn't exist
            return Ok(Config::default());
        }

        Self::load_from_file(&path)
    }

    /// Get data directory
    pub fn data_dir() -> Result<PathBuf, crate::ConfigError> {
        let dir = dirs::home_dir()
            .map(|p| p.join(".local/share/neojoplin"))
            .ok_or_else(|| {
                ConfigError::NotFound("Could not determine home directory".to_string())
            })?;

        Ok(dir)
    }

    /// Ensure data directory exists
    pub fn ensure_data_dir() -> Result<PathBuf, crate::ConfigError> {
        let dir = Self::data_dir()?;
        std::fs::create_dir_all(&dir).map_err(ConfigError::from)?;
        Ok(dir)
    }

    /// Get database path
    pub fn database_path() -> Result<PathBuf, crate::ConfigError> {
        Ok(Self::data_dir()?.join("joplin.db"))
    }

    /// Get temp directory
    pub fn temp_dir() -> Result<PathBuf, crate::ConfigError> {
        let dir = Self::data_dir()?.join("temp");
        std::fs::create_dir_all(&dir).map_err(ConfigError::from)?;
        Ok(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.sync.remote_path, "/neojoplin");
        assert!(config.ui.show_emoji);
    }

    #[test]
    fn test_sync_config_default() {
        let sync = SyncConfig::default();
        assert_eq!(sync.max_retries, 5);
        assert_eq!(sync.retry_delay, 200);
    }

    #[test]
    fn test_editor_config_from_env() {
        // This test depends on environment, so we just verify the struct works
        let editor = EditorConfig::default();
        // The command should be Some if EDITOR is set, None otherwise
        // We can't assert the exact value since it depends on the test environment
        assert!(editor.args.is_empty());
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("remote_path"));
    }
}
