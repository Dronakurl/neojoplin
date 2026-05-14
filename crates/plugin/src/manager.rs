//! Plugin manager - handles plugin discovery, installation, and configuration

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

use crate::loader::PluginLoader;
use crate::traits::PluginContext;

/// Default plugin directory name
pub const PLUGIN_DIR_NAME: &str = "plugins";

/// Subdirectory for enabled plugins (contains symlinks)
pub const ENABLED_DIR_NAME: &str = "enabled";

/// Subdirectory for installed plugins (contains versioned packages)
pub const AVAILABLE_DIR_NAME: &str = "available";

/// Subdirectory for disabled plugins
pub const DISABLED_DIR_NAME: &str = "disabled";

/// Subdirectory for plugin configurations
pub const CONFIG_DIR_NAME: &str = "config";

/// Plugin manager handles plugin discovery, installation, and configuration
pub struct PluginManager {
    /// The plugin loader
    pub loader: PluginLoader,
    /// Plugin manager configuration
    config: PluginManagerConfig,
}

/// Configuration for the plugin manager
#[derive(Debug, Clone)]
pub struct PluginManagerConfig {
    /// Base directory for plugins
    pub plugin_dir: PathBuf,
    /// List of enabled plugin IDs
    pub enabled_plugins: Vec<String>,
    /// List of disabled plugin IDs
    pub disabled_plugins: Vec<String>,
}

impl Default for PluginManagerConfig {
    fn default() -> Self {
        let home = dirs::home_dir().expect("Could not determine home directory");
        let base_dir = if std::env::var("NEOJOPLIN_TEST_MODE").is_ok() {
            home.join(".config/neojoplin-test")
        } else {
            home.join(".config/neojoplin")
        };
        Self {
            plugin_dir: base_dir.join(PLUGIN_DIR_NAME),
            enabled_plugins: Vec::new(),
            disabled_plugins: Vec::new(),
        }
    }
}

impl PluginManager {
    /// Create a new plugin manager with default configuration
    pub fn new() -> Self {
        Self {
            loader: PluginLoader::new(),
            config: PluginManagerConfig::default(),
        }
    }

    /// Create a new plugin manager with custom configuration
    pub fn with_config(config: PluginManagerConfig) -> Self {
        Self {
            loader: PluginLoader::new(),
            config,
        }
    }

    /// Initialize the plugin manager - creates directories and loads configuration
    pub async fn initialize(&mut self) -> Result<()> {
        // Create plugin directories if they don't exist
        self.ensure_directories()?;

        // Add plugin directories to loader
        let enabled_dir = self.get_enabled_dir();
        self.loader.add_plugin_dir(enabled_dir.clone());

        tracing::info!("Plugin manager initialized, plugin dir: {}", self.config.plugin_dir.display());

        // Check for test mode and add test plugin directory
        if std::env::var("NEOJOPLIN_TEST_MODE").is_ok() {
            let plugin_dir = self.config.plugin_dir.clone();
            let test_dir = plugin_dir.parent()
                .expect("Invalid plugin dir")
                .join("neojoplin-test")
                .join(PLUGIN_DIR_NAME)
                .join(ENABLED_DIR_NAME);
            std::fs::create_dir_all(&test_dir)?;
            let test_dir_clone = test_dir.clone();
            self.loader.add_plugin_dir(test_dir_clone);
            tracing::info!("Test mode enabled, also checking: {}", test_dir.display());
        }

        Ok(())
    }

    /// Ensure all plugin directories exist
    fn ensure_directories(&self) -> Result<()> {
        let dirs = vec![
            self.get_available_dir(),
            self.get_enabled_dir(),
            self.get_disabled_dir(),
            self.get_config_dir(),
        ];

        for dir in dirs {
            if !dir.exists() {
                tracing::debug!("Creating plugin directory: {}", dir.display());
                std::fs::create_dir_all(&dir)?;
            }
        }

        Ok(())
    }

    /// Get the available plugins directory
    pub fn get_available_dir(&self) -> PathBuf {
        self.config.plugin_dir.join(AVAILABLE_DIR_NAME)
    }

    /// Get the enabled plugins directory
    pub fn get_enabled_dir(&self) -> PathBuf {
        self.config.plugin_dir.join(ENABLED_DIR_NAME)
    }

    /// Get the disabled plugins directory
    pub fn get_disabled_dir(&self) -> PathBuf {
        self.config.plugin_dir.join(DISABLED_DIR_NAME)
    }

    /// Get the plugin config directory
    pub fn get_config_dir(&self) -> PathBuf {
        self.config.plugin_dir.join(CONFIG_DIR_NAME)
    }

    /// Load all enabled plugins
    pub async fn load_enabled_plugins(&mut self, context: PluginContext) -> Result<()> {
        self.loader.load_all(context).await
    }

    /// Get the loader
    pub fn loader(&self) -> &PluginLoader {
        &self.loader
    }

    /// Get the loader mutably
    pub fn loader_mut(&mut self) -> &mut PluginLoader {
        &mut self.loader
    }

    /// Enable a plugin by creating a symlink in the enabled directory
    pub fn enable_plugin(&self, plugin_id: &str) -> Result<()> {
        let available_dir = self.get_available_dir().join(plugin_id);
        let enabled_dir = self.get_enabled_dir();

        if !available_dir.exists() {
            anyhow::bail!("Plugin '{}' not found in available plugins", plugin_id);
        }

        // Find the library file in the plugin directory
        let lib_path = self.find_library_file(&available_dir)?;
        let link_path = enabled_dir.join(lib_path.file_name().unwrap());

        // Remove existing link if it exists
        if link_path.exists() {
            std::fs::remove_file(&link_path)?;
        }

        // Create symlink
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(lib_path, link_path)?;
        }

        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(lib_path, link_path)?;
        }

        tracing::info!("Enabled plugin: {}", plugin_id);
        Ok(())
    }

    /// Disable a plugin by removing its symlink from the enabled directory
    pub fn disable_plugin(&self, plugin_id: &str) -> Result<()> {
        let enabled_dir = self.get_enabled_dir();
        let link_path = enabled_dir.join(plugin_id);

        // Also try with .so, .dll, .dylib extensions
        let mut possible_paths = vec![link_path.clone()];
        possible_paths.push(enabled_dir.join(format!("{}.so", plugin_id)));
        possible_paths.push(enabled_dir.join(format!("{}.dll", plugin_id)));
        possible_paths.push(enabled_dir.join(format!("{}.dylib", plugin_id)));

        for path in possible_paths {
            if path.exists() {
                std::fs::remove_file(&path)?;
                tracing::info!("Disabled plugin: {}", plugin_id);
                return Ok(());
            }
        }

        anyhow::bail!("Plugin '{}' not found in enabled plugins", plugin_id);
    }

    /// Find the library file in a plugin directory
    fn find_library_file(&self, dir: &Path) -> Result<PathBuf> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if is_library_file(&path) {
                return Ok(path);
            }
        }
        anyhow::bail!("No library file found in plugin directory: {}", dir.display());
    }

    /// List all available plugins
    pub fn list_available_plugins(&self) -> Result<Vec<PluginInfo>> {
        let available_dir = self.get_available_dir();
        if !available_dir.exists() {
            return Ok(Vec::new());
        }

        let mut plugins = Vec::new();
        for entry in std::fs::read_dir(available_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Read plugin.toml if it exists
                let manifest_path = path.join("plugin.toml");
                if manifest_path.exists() {
                    if let Ok(info) = self.read_plugin_info(&manifest_path) {
                        plugins.push(info);
                    }
                } else {
                    // Use directory name as plugin ID
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        plugins.push(PluginInfo {
                            id: name.to_string(),
                            name: name.to_string(),
                            version: "unknown".to_string(),
                            description: "".to_string(),
                            author: "".to_string(),
                            enabled: self.is_plugin_enabled(name),
                            path: path.clone(),
                        });
                    }
                }
            }
        }

        Ok(plugins)
    }

    /// Read plugin info from plugin.toml
    fn read_plugin_info(&self, manifest_path: &Path) -> Result<PluginInfo> {
        use std::collections::HashMap;

        let content = std::fs::read_to_string(manifest_path)?;
        let value: serde_json::Value = serde_json::from_str(&content)
            .unwrap_or_else(|_| serde_json::json!({}));

        // Try to parse as TOML first, then JSON
        let table: HashMap<String, serde_json::Value> = if content.contains('=') {
            // Simple TOML parsing (for basic fields)
            let mut map = HashMap::new();
            for line in content.lines() {
                if let Some((key, value)) = line.split_once('=') {
                    let key = key.trim().to_string();
                    let value = value.trim().trim_matches('"').to_string();
                    map.insert(key, serde_json::json!(value));
                }
            }
            map
        } else {
            // Already parsed as JSON
            serde_json::from_value::<HashMap<String, serde_json::Value>>(value)
                .unwrap_or_default()
        };

        let get_string = |key: &str| -> String {
            table
                .get(key)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default()
        };

        let id = get_string("id");
        let name = get_string("name");
        let version = get_string("version");
        let description = get_string("description");
        let author = get_string("author");
        let plugin_id = if id.is_empty() { name.clone() } else { id.clone() };

        Ok(PluginInfo {
            id: plugin_id.clone(),
            name,
            version,
            description,
            author,
            enabled: self.is_plugin_enabled(&plugin_id),
            path: manifest_path.parent().unwrap().to_path_buf(),
        })
    }

    /// Check if a plugin is enabled
    pub fn is_plugin_enabled(&self, plugin_id: &str) -> bool {
        let enabled_dir = self.get_enabled_dir();

        // Check for symlink with various extensions
        let paths = vec![
            enabled_dir.join(plugin_id),
            enabled_dir.join(format!("{}.so", plugin_id)),
            enabled_dir.join(format!("{}.dll", plugin_id)),
            enabled_dir.join(format!("{}.dylib", plugin_id)),
        ];

        for path in paths {
            if path.exists() {
                return true;
            }
        }

        false
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about a plugin
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub enabled: bool,
    pub path: PathBuf,
}

/// Check if a path is a plugin library file
fn is_library_file(path: &Path) -> bool {
    path.extension()
        .map(|ext| {
            let ext_str = ext.to_string_lossy().to_lowercase();
            ext_str == "so" || ext_str == "dll" || ext_str == "dylib"
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manager_new() {
        let manager = PluginManager::new();
        assert_eq!(manager.loader.plugin_count(), 0);
    }

    #[test]
    fn test_plugin_manager_config_default() {
        let config = PluginManagerConfig::default();
        assert!(config.plugin_dir.to_string_lossy().contains("neojoplin"));
    }

    #[test]
    fn test_get_directories() {
        let manager = PluginManager::new();
        
        let available = manager.get_available_dir();
        assert!(available.to_string_lossy().contains("available"));
        
        let enabled = manager.get_enabled_dir();
        assert!(enabled.to_string_lossy().contains("enabled"));
    }
}
