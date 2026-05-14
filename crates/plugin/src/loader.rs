//! Plugin loader - handles loading dynamic library plugins

use anyhow::{Context, Result};
use libloading::{Library, Symbol};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::traits::{Plugin, PluginContext, PluginMetadata};

/// Type for the plugin constructor function
/// Each plugin library must export a function with this signature:
/// ```ignore
/// #[no_mangle]
/// pub extern "C" fn plugin_constructor() -> Box<dyn Plugin> { ... }
/// ```
pub type PluginConstructor = fn() -> Box<dyn Plugin>;

/// Plugin loader manages loading and unloading plugins
pub struct PluginLoader {
    /// Loaded plugins by ID
    plugins: HashMap<String, Box<dyn Plugin>>,
    /// Loaded libraries by ID
    libraries: HashMap<String, Library>,
    /// Directories to search for plugins
    plugin_dirs: Vec<PathBuf>,
}

impl PluginLoader {
    /// Create a new plugin loader
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            libraries: HashMap::new(),
            plugin_dirs: Vec::new(),
        }
    }

    /// Add a directory to search for plugins
    pub fn add_plugin_dir(&mut self, dir: PathBuf) {
        self.plugin_dirs.push(dir);
    }

    /// Load all plugins from registered directories
    pub async fn load_all(&mut self, context: PluginContext) -> Result<()> {
        // Clone the dirs to avoid borrow checker issues
        let dirs = self.plugin_dirs.clone();
        for plugin_dir in dirs {
            self.load_from_dir(&plugin_dir, &context)
                .await
                .with_context(|| format!("Failed to load plugins from {}", plugin_dir.display()))?;
        }
        Ok(())
    }

    /// Load plugins from a specific directory
    pub async fn load_from_dir(&mut self, dir: &Path, context: &PluginContext) -> Result<()> {
        if !dir.exists() {
            tracing::debug!("Plugin directory does not exist: {}", dir.display());
            return Ok(());
        }

        tracing::info!("Loading plugins from: {}", dir.display());

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            // Only load dynamic library files
            if is_plugin_library(&path) {
                self.load_plugin(&path, context)
                    .await
                    .with_context(|| format!("Failed to load plugin: {}", path.display()))?;
            }
        }

        Ok(())
    }

    /// Load a single plugin from a library file
    pub async fn load_plugin(&mut self, lib_path: &Path, context: &PluginContext) -> Result<()> {
        // Extract plugin ID from filename (without extension)
        let plugin_id = lib_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid plugin filename: {}", lib_path.display()))?
            .to_string();

        // Check if already loaded
        if self.plugins.contains_key(&plugin_id) {
            tracing::debug!("Plugin {} already loaded, skipping", plugin_id);
            return Ok(());
        }

        tracing::info!("Loading plugin library: {}", lib_path.display());

        // Load the dynamic library (unsafe because it can fail at runtime)
        let lib = unsafe { Library::new(lib_path) }
            .with_context(|| format!("Failed to load plugin library: {}", lib_path.display()))?;

        // Get the plugin constructor function (unsafe because it can fail at runtime)
        // Each plugin must export: `pub extern "C" fn plugin_constructor() -> Box<dyn Plugin>`
        let constructor: Symbol<PluginConstructor> = unsafe {
            lib.get(b"plugin_constructor")
        }
            .with_context(|| {
                format!(
                    "Plugin {} is missing required 'plugin_constructor' symbol",
                    lib_path.display()
                )
            })?;

        // Create the plugin instance
        let plugin = constructor();
        let metadata = plugin.metadata().clone();

        tracing::info!(
            "Initializing plugin: {} v{} by {}",
            metadata.name,
            metadata.version,
            metadata.author
        );

        // Initialize the plugin with context
        let mut plugin = constructor();
        let metadata = plugin.metadata().clone();

        plugin
            .initialize(context.clone())
            .await
            .with_context(|| format!("Failed to initialize plugin: {}", metadata.id))?;

        // Store the plugin and library
        self.plugins.insert(plugin_id.clone(), plugin);
        self.libraries.insert(plugin_id, lib);

        tracing::info!("Successfully loaded plugin: {}", metadata.name);

        Ok(())
    }

    /// Get a plugin by ID, downcast to a specific type
    pub fn get_plugin<T: Plugin + 'static>(&self, id: &str) -> Option<&T> {
        self.plugins
            .get(id)
            .and_then(|boxed| boxed.downcast_ref::<T>())
    }

    /// Get a mutable reference to a plugin by ID, downcast to a specific type
    pub fn get_plugin_mut<T: Plugin + 'static>(&mut self, id: &str) -> Option<&mut T> {
        self.plugins
            .get_mut(id)
            .and_then(|boxed| boxed.downcast_mut::<T>())
    }

    /// Get all plugins with a specific capability
    pub fn get_plugins_with_capability(&self, capability: crate::traits::PluginCapability) -> Vec<&dyn Plugin> {
        self.plugins
            .values()
            .filter(|p| p.capabilities().contains(&capability))
            .map(|p| &**p as &dyn Plugin)
            .collect()
    }

    /// Get all loaded plugins
    pub fn get_all_plugins(&self) -> Vec<&dyn Plugin> {
        self.plugins.values().map(|p| &**p as &dyn Plugin).collect()
    }

    /// Get plugin metadata by ID
    pub fn get_plugin_metadata(&self, id: &str) -> Option<&PluginMetadata> {
        self.plugins.get(id).map(|p| p.metadata())
    }

    /// Check if a plugin is loaded
    pub fn is_plugin_loaded(&self, id: &str) -> bool {
        self.plugins.contains_key(id)
    }

    /// Unload a plugin by ID
    pub async fn unload_plugin(&mut self, id: &str) -> Result<()> {
        tracing::info!("Unloading plugin: {}", id);

        if let Some(mut plugin) = self.plugins.remove(id) {
            plugin
                .shutdown()
                .await
                .with_context(|| format!("Failed to shutdown plugin: {}", id))?;
        }

        if let Some(_lib) = self.libraries.remove(id) {
            // Library is automatically unloaded when dropped
            tracing::debug!("Plugin library unloaded: {}", id);
        }

        Ok(())
    }

    /// Unload all plugins
    pub async fn unload_all(&mut self) -> Result<()> {
        tracing::info!("Unloading all plugins");

        let plugin_ids: Vec<String> = self.plugins.keys().cloned().collect();
        for id in plugin_ids {
            self.unload_plugin(&id).await?;
        }

        self.plugins.clear();
        self.libraries.clear();

        Ok(())
    }

    /// Get number of loaded plugins
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }
}

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if a path is a plugin library file
fn is_plugin_library(path: &Path) -> bool {
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
    use crate::traits::{Plugin, PluginCapability, PluginMetadata, PluginContext};
    use async_trait::async_trait;

    #[derive(Debug, Default)]
    struct MockPlugin {
        initialized: bool,
        shutdown: bool,
    }

    static MOCK_METADATA: PluginMetadata = PluginMetadata {
        id: "mock-plugin".to_string(),
        name: "Mock Plugin".to_string(),
        version: "1.0.0".to_string(),
        description: "A mock plugin for testing".to_string(),
        author: "Test".to_string(),
        license: None,
        dependencies: vec![],
        capabilities: vec![],
    };

    #[async_trait]
    impl Plugin for MockPlugin {
        async fn initialize(&mut self, _context: PluginContext) -> Result<()> {
            self.initialized = true;
            Ok(())
        }

        async fn shutdown(&mut self) -> Result<()> {
            self.shutdown = true;
            Ok(())
        }

        fn metadata(&self) -> &PluginMetadata {
            &MOCK_METADATA
        }

        fn capabilities(&self) -> &[PluginCapability] {
            &[]
        }
    }

    impl downcast_rs::DowncastSync for MockPlugin {}

    #[test]
    fn test_plugin_loader_new() {
        let loader = PluginLoader::new();
        assert_eq!(loader.plugin_count(), 0);
    }

    #[test]
    fn test_add_plugin_dir() {
        let mut loader = PluginLoader::new();
        loader.add_plugin_dir(PathBuf::from("/tmp/plugins"));
        // Can't easily test loading without actual plugin files
    }
}
