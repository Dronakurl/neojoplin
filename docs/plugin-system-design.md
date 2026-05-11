# NeoJoplin Plugin System Design

This document describes a proposed plugin system for NeoJoplin, inspired by Joplin's plugin architecture but designed natively for Rust. This allows AI features (and other extensions) to be added modularly without modifying the core application.

## Overview

A plugin system for NeoJoplin enables:
- **AI Integration** - Add LLM providers, semantic search, auto-tagging, etc.
- **Custom Commands** - Extend CLI with new commands
- **Custom TUI Panels** - Add new UI components
- **Storage Backends** - Support new sync/storage providers
- **Note Processors** - Add custom note transformations

## Architecture

### Plugin Types

#### 1. **Dynamic Library Plugins** (Recommended)
- Loaded at runtime as `.so` (Linux), `.dll` (Windows), `.dylib` (macOS)
- Most flexible, allows third-party plugins
- Uses Rust's `libloading` crate

#### 2. **Compile-Time Plugins** (Optional)
- Integrated via Cargo features
- Zero runtime overhead
- Limited to plugins bundled with neojoplin

### Directory Structure

```
~/.config/neojoplin/
├── plugins/                   # Plugin directory
│   ├── enabled/              # Symlinks to enabled plugins
│   │   └── ai-ollama.so -> ../available/ai-ollama/0.1.0/ai-ollama.so
│   ├── available/            # Installed plugins
│   │   └── ai-ollama/        # Plugin package
│   │       ├── 0.1.0/        # Version
│   │       │   ├── ai-ollama.so
│   │       │   ├── plugin.toml
│   │       │   └── README.md
│   │       └── 0.1.1/        # Another version
│   │           └── ...
│   └── disabled/             # Disabled plugins
│       └── experimental-ai.so
└── plugin-config.json        # Plugin configuration
```

## Plugin Interface

### Core Trait

```rust
// crates/plugin/src/lib.rs

use anyhow::Result;
use async_trait::async_trait;
use joplin_domain::{Note, Folder, Storage};
use std::sync::Arc;

/// Plugin context provides access to NeoJoplin services
#[derive(Debug, Clone)]
pub struct PluginContext {
    /// Storage access
    pub storage: Arc<dyn Storage>,
    /// Configuration
    pub config: PluginConfig,
    /// Plugin metadata
    pub metadata: PluginMetadata,
}

/// Plugin metadata from plugin.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub license: Option<String>,
    pub dependencies: Vec<String>,
    pub capabilities: Vec<PluginCapability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginCapability {
    /// Can provide CLI commands
    CliCommands,
    /// Can provide TUI panels
    TuiPanels,
    /// Can process notes on create/update
    NoteProcessor,
    /// Can provide storage backends
    StorageBackend,
    /// Can provide sync targets
    SyncTarget,
    /// Can provide AI services
    AiProvider,
}

/// Plugin configuration
#[derive(Debug, Clone, Default)]
pub struct PluginConfig {
    pub enabled: bool,
    pub settings: serde_json::Value,
}

/// Main plugin trait - all plugins must implement this
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Called when plugin is loaded
    async fn initialize(&mut self, context: PluginContext) -> Result<()>;
    
    /// Called when plugin is unloaded
    async fn shutdown(&mut self) -> Result<()>;
    
    /// Get plugin metadata
    fn metadata(&self) -> &PluginMetadata;
    
    /// Get plugin capabilities
    fn capabilities(&self) -> &[PluginCapability];
}

/// Trait for CLI command providers
#[async_trait]
pub trait CliCommandProvider: Plugin {
    /// Register CLI commands
    fn register_commands(&self, app: clap::Command) -> clap::Command;
    
    /// Handle CLI command execution
    async fn handle_command(&self, command: &str, args: &[String]) -> Result<String>;
}

/// Trait for TUI panel providers
#[async_trait]
pub trait TuiPanelProvider: Plugin {
    /// Get panel name
    fn panel_name(&self) -> &str;
    
    /// Get panel key binding
    fn key_binding(&self) -> Option<char>;
    
    /// Render panel content
    async fn render_panel(&self, area: ratatui::prelude::Rect) -> Result<ratatui::prelude::Frame>;
    
    /// Handle panel input
    async fn handle_input(&mut self, event: crossterm::event::KeyEvent) -> Result<bool>;
}

/// Trait for note processors
#[async_trait]
pub trait NoteProcessor: Plugin {
    /// Called before note is saved
    async fn before_save(&self, note: &mut Note) -> Result<()>;
    
    /// Called after note is loaded
    async fn after_load(&self, note: &mut Note) -> Result<()>;
}

/// Trait for AI providers
#[async_trait]
pub trait AiProvider: Plugin {
    /// Generate text
    async fn generate_text(&self, prompt: &str, context: Option<&str>) -> Result<String>;
    
    /// Generate embeddings
    async fn generate_embeddings(&self, text: &str) -> Result<Vec<f32>>;
    
    /// Find similar notes
    async fn find_similar_notes(&self, note: &Note, all_notes: &[Note], limit: usize) -> Result<Vec<Note>>;
}
```

## Plugin Manifest

Each plugin must include a `plugin.toml` file:

```toml
# plugin.toml for ai-ollama plugin
[package]
id = "ai-ollama"
name = "Ollama AI Provider"
version = "0.1.0"
description = "Local LLM integration via Ollama"
author = "Your Name"
license = "MIT"

[capabilities]
enabled = ["AiProvider", "CliCommands"]

[dependencies]
# Optional plugin dependencies
# neojoplin = "0.1.4"

[settings]
# Default settings
model = "llama3:8b"
api_url = "http://localhost:11434"
timeout = 30
```

## Plugin Loader

### `crates/plugin/src/loader.rs`

```rust
use anyhow::{Context, Result};
use libloading::{Library, Symbol};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub type PluginConstructor = Box<dyn Fn() -> Box<dyn Plugin> + Send + Sync>;

/// Plugin loader manages loading and unloading plugins
#[derive(Debug)]
pub struct PluginLoader {
    plugins: HashMap<String, Arc<dyn Plugin>>,
    libraries: HashMap<String, Library>,
    plugin_dirs: Vec<PathBuf>,
}

impl PluginLoader {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            libraries: HashMap::new(),
            plugin_dirs: Vec::new(),
        }
    }
    
    /// Add a plugin directory
    pub fn add_plugin_dir(&mut self, dir: PathBuf) {
        self.plugin_dirs.push(dir);
    }
    
    /// Load all enabled plugins
    pub async fn load_all(&mut self, context: PluginContext) -> Result<()> {
        for plugin_dir in &self.plugin_dirs {
            self.load_from_dir(plugin_dir, &context).await?;
        }
        Ok(())
    }
    
    /// Load plugins from a directory
    async fn load_from_dir(&mut self, dir: &Path, context: &PluginContext) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }
        
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            
            // Load plugin library
            if path.extension().is_some_and(|ext| {
                ext == "so" || ext == "dll" || ext == "dylib"
            }) {
                self.load_plugin(&path, context).await?;
            }
        }
        
        Ok(())
    }
    
    /// Load a single plugin
    async fn load_plugin(&mut self, lib_path: &Path, context: &PluginContext) -> Result<()> {
        // Check if already loaded
        if let Some(plugin_id) = lib_path.file_stem().and_then(|s| s.to_str()) {
            if self.plugins.contains_key(plugin_id) {
                return Ok(());
            }
        }
        
        // Load the library
        let lib = Library::new(lib_path)
            .with_context(|| format!("Failed to load plugin: {}", lib_path.display()))?;
        
        // Get the plugin constructor
        type ConstructorFn = fn() -> Box<dyn Plugin>;
        let constructor: Symbol<ConstructorFn> = lib.get(b"plugin_constructor")
            .with_context(|| format!("Plugin {} has no plugin_constructor symbol", lib_path.display()))?;
        
        // Create the plugin instance
        let plugin = constructor();
        let metadata = plugin.metadata().clone();
        
        // Initialize the plugin
        plugin.initialize(context.clone()).await
            .with_context(|| format!("Failed to initialize plugin: {}", metadata.id))?;
        
        // Store the plugin
        self.plugins.insert(metadata.id.clone(), Arc::from(plugin));
        self.libraries.insert(metadata.id, lib);
        
        tracing::info!("Loaded plugin: {} v{}", metadata.name, metadata.version);
        
        Ok(())
    }
    
    /// Get a plugin by ID
    pub fn get_plugin<T: Plugin + 'static>(&self, id: &str) -> Option<Arc<T>> {
        self.plugins.get(id)
            .and_then(|p| p.clone().downcast::<T>().ok())
            .map(Arc::from)
    }
    
    /// Get all plugins with a specific capability
    pub fn get_plugins_with_capability(&self, capability: PluginCapability) -> Vec<Arc<dyn Plugin>> {
        self.plugins.values()
            .filter(|p| p.capabilities().contains(&capability))
            .cloned()
            .collect()
    }
    
    /// Unload a plugin
    pub async fn unload_plugin(&mut self, id: &str) -> Result<()> {
        if let Some(plugin) = self.plugins.remove(id) {
            plugin.shutdown().await?;
        }
        
        if let Some(lib) = self.libraries.remove(id) {
            // Library is automatically unloaded when dropped
        }
        
        Ok(())
    }
    
    /// Unload all plugins
    pub async fn unload_all(&mut self) -> Result<()> {
        for (id, _) in self.plugins.iter() {
            self.unload_plugin(id).await?;
        }
        self.plugins.clear();
        self.libraries.clear();
        Ok(())
    }
}

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new()
    }
}
```

## Plugin Manager

### `crates/plugin/src/manager.rs`

```rust
use anyhow::Result;
use std::path::PathBuf;

/// Plugin manager handles plugin discovery, installation, and configuration
#[derive(Debug)]
pub struct PluginManager {
    loader: PluginLoader,
    config: PluginManagerConfig,
}

#[derive(Debug, Clone)]
pub struct PluginManagerConfig {
    pub plugin_dir: PathBuf,
    pub enabled_plugins: Vec<String>,
    pub disabled_plugins: Vec<String>,
}

impl Default for PluginManagerConfig {
    fn default() -> Self {
        let home = dirs::home_dir().expect("Could not determine home directory");
        Self {
            plugin_dir: home.join(".config/neojoplin/plugins"),
            enabled_plugins: Vec::new(),
            disabled_plugins: Vec::new(),
        }
    }
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            loader: PluginLoader::new(),
            config: PluginManagerConfig::default(),
        }
    }
    
    pub fn with_config(config: PluginManagerConfig) -> Self {
        Self {
            loader: PluginLoader::new(),
            config,
        }
    }
    
    /// Initialize plugin manager
    pub async fn initialize(&mut self) -> Result<()> {
        // Create plugin directories if they don't exist
        let enabled_dir = self.config.plugin_dir.join("enabled");
        let available_dir = self.config.plugin_dir.join("available");
        let disabled_dir = self.config.plugin_dir.join("disabled");
        
        std::fs::create_dir_all(&enabled_dir)?;
        std::fs::create_dir_all(&available_dir)?;
        std::fs::create_dir_all(&disabled_dir)?;
        
        // Load enabled plugins
        self.loader.add_plugin_dir(enabled_dir);
        
        // Load from test mode if enabled
        if std::env::var("NEOJOPLIN_TEST_MODE").is_ok() {
            let test_dir = self.config.plugin_dir.parent()
                .expect("Invalid plugin dir")
                .join("neojoplin-test/plugins/enabled");
            std::fs::create_dir_all(&test_dir)?;
            self.loader.add_plugin_dir(test_dir);
        }
        
        Ok(())
    }
    
    /// Load all enabled plugins
    pub async fn load_enabled_plugins(&mut self, context: PluginContext) -> Result<()> {
        self.loader.load_all(context).await
    }
    
    /// Install a plugin from a path
    pub async fn install_plugin(&mut self, path: &PathBuf) -> Result<()> {
        // Copy plugin to available directory
        let dest = self.config.plugin_dir.join("available");
        // Implementation: copy plugin files
        unimplemented!()
    }
    
    /// Enable a plugin
    pub async fn enable_plugin(&mut self, plugin_id: &str) -> Result<()> {
        // Create symlink in enabled directory
        let available_dir = self.config.plugin_dir.join("available").join(plugin_id);
        let enabled_dir = self.config.plugin_dir.join("enabled");
        
        if !available_dir.exists() {
            anyhow::bail!("Plugin {} not installed", plugin_id);
        }
        
        // Find the library file
        let lib_path = self.find_library(&available_dir)?;
        let link_path = enabled_dir.join(lib_path.file_name().unwrap());
        
        #[cfg(unix)]
        std::os::unix::fs::symlink(lib_path, link_path)?;
        
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(lib_path, link_path)?;
        
        Ok(())
    }
    
    fn find_library(&self, dir: &PathBuf) -> Result<PathBuf> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| {
                ext == "so" || ext == "dll" || ext == "dylib"
            }) {
                return Ok(path);
            }
        }
        anyhow::bail!("No library found in plugin directory");
    }
}
```

## AI Plugin Example

### `crates/plugins/ai-ollama/src/lib.rs`

```rust
use anyhow::Result;
use async_trait::async_trait;
use joplin_domain::Note;
use neojoplin_plugin::{AiProvider, CliCommandProvider, Plugin, PluginContext, PluginMetadata, PluginCapability};
use reqwest::Client;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

/// Ollama AI plugin
pub struct OllamaPlugin {
    client: Client,
    config: OllamaConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaConfig {
    pub model: String,
    pub api_url: String,
    pub timeout_seconds: u64,
}

impl OllamaPlugin {
    fn new() -> Self {
        Self {
            client: Client::new(),
            config: OllamaConfig {
                model: "llama3:8b".to_string(),
                api_url: "http://localhost:11434".to_string(),
                timeout_seconds: 30,
            },
        }
    }
    
    async fn call_ollama(&self, prompt: &str, system: Option<&str>) -> Result<String> {
        let mut messages = vec![];
        
        if let Some(system_prompt) = system {
            messages.push(json!({
                "role": "system",
                "content": system_prompt
            }));
        }
        
        messages.push(json!({
            "role": "user",
            "content": prompt
        }));
        
        let request = json!({
            "model": self.config.model,
            "messages": messages,
            "stream": false
        });
        
        let response = self.client
            .post(&format!("{}/api/chat", self.config.api_url))
            .json(&request)
            .timeout(Duration::from_secs(self.config.timeout_seconds))
            .send()
            .await?
            .error_for_status()?;
        
        let body: serde_json::Value = response.json().await?;
        
        body["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Invalid response"))
    }
}

#[async_trait]
impl Plugin for OllamaPlugin {
    async fn initialize(&mut self, context: PluginContext) -> Result<()> {
        // Load configuration from plugin settings
        if let Some(settings) = context.config.settings.get("ollama") {
            self.config = serde_json::from_value(settings.clone())?;
        }
        Ok(())
    }
    
    async fn shutdown(&mut self) -> Result<()> {
        Ok(())
    }
    
    fn metadata(&self) -> &PluginMetadata {
        &PLUGIN_METADATA
    }
    
    fn capabilities(&self) -> &[PluginCapability] {
        &[PluginCapability::AiProvider, PluginCapability::CliCommands]
    }
}

#[async_trait]
impl AiProvider for OllamaPlugin {
    async fn generate_text(&self, prompt: &str, context: Option<&str>) -> Result<String> {
        self.call_ollama(prompt, context).await
    }
    
    async fn generate_embeddings(&self, text: &str) -> Result<Vec<f32>> {
        // Use Ollama's embeddings API
        let request = json!({
            "model": self.config.model,
            "input": text
        });
        
        let response = self.client
            .post(&format!("{}/api/embeddings", self.config.api_url))
            .json(&request)
            .timeout(Duration::from_secs(self.config.timeout_seconds))
            .send()
            .await?
            .error_for_status()?;
        
        let body: serde_json::Value = response.json().await?;
        body["embeddings"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
            .ok_or_else(|| anyhow::anyhow!("Invalid embeddings response"))
    }
    
    async fn find_similar_notes(&self, note: &Note, all_notes: &[Note], limit: usize) -> Result<Vec<Note>> {
        // Generate embeddings for the query note
        let query_embedding = self.generate_embeddings(&note.body).await?;
        
        // Generate embeddings for all notes (in practice, use cached embeddings)
        let mut results = Vec::new();
        for n in all_notes {
            let embedding = self.generate_embeddings(&n.body).await?;
            let similarity = cosine_similarity(&query_embedding, &embedding);
            results.push((n.clone(), similarity));
        }
        
        // Sort by similarity and return top N
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        Ok(results.into_iter().take(limit).map(|(n, _)| n).collect())
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

#[async_trait]
impl CliCommandProvider for OllamaPlugin {
    fn register_commands(&self, app: clap::Command) -> clap::Command {
        app.subcommand(
            clap::Command::new("ai")
                .about("AI commands")
                .subcommand(
                    clap::Command::new("generate")
                        .about("Generate text with AI")
                        .arg(clap::arg!(<PROMPT> "The prompt for text generation"))
                )
                .subcommand(
                    clap::Command::new("summarize")
                        .about("Summarize a note")
                        .arg(clap::arg!(<NOTE> "Note ID or title"))
                )
        )
    }
    
    async fn handle_command(&self, command: &str, args: &[String]) -> Result<String> {
        match command {
            "ai/generate" => {
                let prompt = args.first().unwrap_or(&"".to_string());
                self.generate_text(prompt, None).await
            }
            "ai/summarize" => {
                // Get note and summarize it
                unimplemented!()
            }
            _ => Err(anyhow::anyhow!("Unknown command"))
        }
    }
}

/// Plugin metadata
pub static PLUGIN_METADATA: PluginMetadata = PluginMetadata {
    id: "ai-ollama".to_string(),
    name: "Ollama AI Provider".to_string(),
    version: "0.1.0".to_string(),
    description: "Local LLM integration via Ollama".to_string(),
    author: "Your Name".to_string(),
    license: Some("MIT".to_string()),
    dependencies: Vec::new(),
    capabilities: vec![PluginCapability::AiProvider, PluginCapability::CliCommands],
};

/// Plugin constructor - must be exported with this exact name
#[no_mangle]
pub extern "C" fn plugin_constructor() -> Box<dyn Plugin> {
    Box::new(OllamaPlugin::new())
}
```

## Integration with NeoJoplin

### Update Workspace

#### `Cargo.toml`

```toml
[workspace]
members = [
  "crates/joplin-domain",
  "crates/joplin",
  "crates/joplin-sync",
  "crates/core",
  "crates/storage",
  "crates/sync",
  "crates/e2ee",
  "crates/tui",
  "crates/cli",
  "crates/test-utils",
  "crates/plugin",       # New: plugin system
]

[workspace.dependencies]
libloading = "0.8"
async-trait = "0.1"
```

### `crates/plugin/Cargo.toml`

```toml
[package]
name = "neojoplin-plugin"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
joplin-domain = { path = "../joplin-domain" }
neojoplin-core = { path = "../core" }
tokio = { workspace = true }
serde = { workspace = true, features = ["derive"] }
serde_json = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }
libloading = "0.8"
async-trait = "0.1"
dirs = "5.0"
```

### Modify CLI to Support Plugins

#### `crates/cli/src/main.rs`

```rust
// Add plugin support
use neojoplin_plugin::{PluginManager, PluginManagerConfig, PluginContext};
use neojoplin_storage::SqliteStorage;

#[tokio::main]
async fn main() -> Result<()> {
    // Check for test mode
    if std::env::var("NEOJOPLIN_TEST_MODE").is_ok() {
        std::env::set_var("NEOJOPLIN_TEST_MODE", "1");
    }

    let cli = Cli::parse();

    // Enable test mode if flag is set
    if cli.test_mode {
        std::env::set_var("NEOJOPLIN_TEST_MODE", "1");
    }

    // Initialize plugin manager
    let mut plugin_manager = PluginManager::new();
    plugin_manager.initialize().await?;
    
    // Build plugin context
    let storage = Arc::new(SqliteStorage::new().await?);
    let context = PluginContext {
        storage: storage.clone(),
        config: Default::default(),
        metadata: Default::default(),
    };
    
    // Load enabled plugins
    plugin_manager.load_enabled_plugins(context).await?;
    
    // Get CLI commands from plugins
    let mut app = Cli::command();
    for plugin in plugin_manager.loader.get_plugins_with_capability(PluginCapability::CliCommands) {
        if let Some(cmd_provider) = plugin.clone().downcast::<dyn CliCommandProvider>().ok() {
            app = cmd_provider.register_commands(app);
        }
    }
    
    // If no command or --tui, launch TUI with plugins
    if cli.command.is_none() || cli.tui {
        return neojoplin_tui::run_app_with_plugins(plugin_manager).await;
    }

    // Handle CLI commands with plugin support
    match cli.command.unwrap() {
        Commands::Ai { command } => {
            // Handle AI commands from plugins
            handle_ai_command(&plugin_manager, command).await
        }
        // ... other commands
    }
}

async fn handle_ai_command(plugin_manager: &PluginManager, command: AiCommands) -> Result<()> {
    // Find AI provider plugin
    let ai_plugins = plugin_manager.loader.get_plugins_with_capability(PluginCapability::AiProvider);
    
    if ai_plugins.is_empty() {
        return Err(anyhow::anyhow!("No AI provider plugin loaded"));
    }
    
    // For now, use the first AI provider
    let ai_plugin = &ai_plugins[0];
    
    if let Some(ai_provider) = ai_plugin.clone().downcast::<dyn AiProvider>().ok() {
        match command {
            AiCommands::Generate { prompt, system } => {
                let result = ai_provider.generate_text(&prompt, system.as_deref()).await?;
                println!("{}", result);
            }
            // ... other AI commands
        }
    }
    
    Ok(())
}
```

### TUI Integration

#### `crates/tui/src/app.rs`

```rust
use neojoplin_plugin::{PluginManager, PluginCapability};

pub struct App {
    // ... existing fields ...
    pub plugin_manager: PluginManager,
}

impl App {
    pub async fn new() -> Result<Self> {
        // ... existing initialization ...
        
        // Initialize plugin manager
        let mut plugin_manager = PluginManager::new();
        plugin_manager.initialize().await?;
        
        let context = PluginContext {
            storage: storage.clone(),
            config: Default::default(),
            metadata: Default::default(),
        };
        
        plugin_manager.load_enabled_plugins(context).await?;
        
        Ok(Self {
            // ... existing fields ...
            plugin_manager,
        })
    }
    
    /// Get AI provider plugins
    pub fn get_ai_providers(&self) -> Vec<Arc<dyn AiProvider>> {
        self.plugin_manager.loader.get_plugins_with_capability(PluginCapability::AiProvider)
            .into_iter()
            .filter_map(|p| p.clone().downcast::<dyn AiProvider>().ok())
            .map(Arc::from)
            .collect()
    }
}
```

## Plugin Development Workflow

### 1. Create a Plugin Crate

```bash
cd neojoplin
cargo new --lib crates/plugins/ai-ollama
```

### 2. Add Dependencies

```toml
# crates/plugins/ai-ollama/Cargo.toml
[package]
name = "ai-ollama"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]  # Important: must be a dynamic library

[dependencies]
neojoplin-plugin = { path = "../../plugin" }
joplin-domain = { path = "../../joplin-domain" }
reqwest = { version = "0.13", features = ["json"] }
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
async-trait = "0.1"
```

### 3. Create Plugin Manifest

```toml
# crates/plugins/ai-ollama/plugin.toml
[package]
id = "ai-ollama"
name = "Ollama AI Provider"
version = "0.1.0"
description = "Local LLM integration via Ollama"
author = "Your Name"
license = "MIT"

[capabilities]
enabled = ["AiProvider", "CliCommands"]

[settings]
model = "llama3:8b"
api_url = "http://localhost:11434"
timeout = 30
```

### 4. Build and Install

```bash
# Build the plugin
cd crates/plugins/ai-ollama
cargo build --release

# Copy to plugins directory
cp target/release/libai_ollama.so ~/.config/neojoplin/plugins/available/ai-ollama/0.1.0/
mv ai-ollama.so ai-ollama.so  # Rename if needed

# Enable the plugin
ln -s ../../available/ai-ollama/0.1.0/ai-ollama.so ~/.config/neojoplin/plugins/enabled/
```

### 5. Test

```bash
NEOJOPLIN_TEST_MODE=1 cargo run --bin neojoplin -- ai generate "Hello, world!"
```

## Plugin Distribution

Plugins can be distributed as:

1. **Pre-built binaries** - .so/.dll/.dylib files
2. **Source packages** - Rust crates with build scripts
3. **Plugin registry** - Central repository for NeoJoplin plugins

### Plugin Package Format

```
ai-ollama-0.1.0.npk (zip archive)
├── plugin.toml
├── README.md
├── LICENSE
├── libai_ollama.so (Linux)
├── ai_ollama.dll (Windows)
└── libai_ollama.dylib (macOS)
```

## Advantages of Plugin System

1. **Modularity** - AI features are optional and can be added/removed
2. **Isolation** - Plugins run in separate context, can be disabled if problematic
3. **Extensibility** - Third parties can create plugins
4. **Testability** - Plugins can be tested independently
5. **Versioning** - Plugins can have their own version cycles
6. **Dependencies** - Plugins can have their own dependencies without affecting core

## Comparison with Joplin's Plugin System

| Feature | Joplin (Desktop) | NeoJoplin (Proposed) |
|---------|------------------|---------------------|
| Language | TypeScript/JavaScript | Rust |
| Loading | Dynamic (Node.js) | Dynamic (libloading) |
| API | Joplin API | Direct Storage Access |
| UI Integration | React components | Ratatui widgets |
| Async | Promise-based | Tokio async/await |
| Distribution | npm | Custom .npk packages |

## Implementation Roadmap

### Phase 1: Core Plugin System
- [ ] Create `neojoplin-plugin` crate
- [ ] Implement `Plugin` trait and loader
- [ ] Add plugin discovery and loading
- [ ] Add plugin configuration
- [ ] Test mode support for plugins

### Phase 2: CLI Integration
- [ ] Add plugin command registration
- [ ] Add plugin command handling
- [ ] Add `--list-plugins` command
- [ ] Add `--install-plugin` command
- [ ] Add `--enable-plugin` command

### Phase 3: TUI Integration
- [ ] Add plugin panel registration
- [ ] Add plugin key bindings
- [ ] Add plugin UI rendering
- [ ] Add plugin settings UI

### Phase 4: Example Plugins
- [ ] AI Ollama plugin
- [ ] AI OpenAI plugin
- [ ] Semantic search plugin
- [ ] Note annotation plugin

### Phase 5: Plugin Ecosystem
- [ ] Plugin documentation
- [ ] Plugin registry/server
- [ ] Plugin packaging tool
- [ ] Plugin update system

## Security Considerations

1. **Code Signing** - Verify plugin integrity before loading
2. **Sandboxing** - Consider running plugins in sandboxed environment
3. **Permissions** - Request user permission before plugin operations
4. **Audit Logging** - Log plugin actions for security auditing
5. **Dependency Checking** - Verify plugin dependencies are safe

## Performance Considerations

1. **Lazy Loading** - Load plugins only when needed
2. **Plugin Caching** - Cache plugin metadata
3. **Parallel Initialization** - Initialize plugins concurrently
4. **Memory Management** - Unload unused plugins
5. **Error Isolation** - One plugin crash shouldn't affect others

## Plugin Configuration

### Global Plugin Settings

```json
{
  "plugins": {
    "enabled": ["ai-ollama", "semantic-search"],
    "disabled": ["experimental-ai"],
    "settings": {
      "ai-ollama": {
        "model": "llama3:8b",
        "api_url": "http://localhost:11434"
      }
    }
  }
}
```

### Per-Plugin Configuration

Each plugin can store its own configuration in:
- `~/.config/neojoplin/plugins/config/ai-ollama.json`

## CLI Commands for Plugin Management

```bash
# List installed plugins
neojoplin plugins list

# List enabled plugins
neojoplin plugins list --enabled

# Install a plugin
neojoplin plugins install ./ai-ollama.npk

# Enable a plugin
neojoplin plugins enable ai-ollama

# Disable a plugin
neojoplin plugins disable ai-ollama

# Uninstall a plugin
neojoplin plugins uninstall ai-ollama

# Configure a plugin
neojoplin plugins config ai-ollama --set model=llama3:70b
```

## Testing Plugins

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_plugin_metadata() {
        let plugin = OllamaPlugin::new();
        assert_eq!(plugin.metadata().id, "ai-ollama");
        assert!(plugin.capabilities().contains(&PluginCapability::AiProvider));
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_plugin_loading() {
    let mut plugin_manager = PluginManager::new();
    plugin_manager.initialize().await.unwrap();
    
    let context = PluginContext::default();
    plugin_manager.load_enabled_plugins(context).await.unwrap();
    
    assert!(!plugin_manager.loader.get_plugins_with_capability(PluginCapability::AiProvider).is_empty());
}
```

## Plugin Development Best Practices

1. **Idempotent Initialization** - Plugin initialization should be safe to call multiple times
2. **Graceful Degradation** - Handle missing dependencies gracefully
3. **Error Handling** - Provide clear error messages
4. **Documentation** - Include README with usage instructions
5. **Versioning** - Follow semantic versioning
6. **Configuration** - Use sensible defaults
7. **Performance** - Avoid blocking operations
8. **Memory** - Clean up resources on shutdown

## Example: AI-Powered Note Summarization

With the plugin system, implementing AI summarization is simple:

```rust
#[async_trait]
impl NoteProcessor for AiSummaryPlugin {
    async fn before_save(&self, note: &mut Note) -> Result<()> {
        // Auto-summarize long notes
        if note.body.len() > 1000 {
            let summary = self.generate_summary(&note.body).await?;
            note.body.push_str(&format!("\n\n---\n// AI Summary:\n{}", summary));
        }
        Ok(())
    }
}
```

## Migration from Tight Integration

If you've already started integrating AI directly into neojoplin, you can migrate to the plugin system:

1. Extract AI code into a separate crate
2. Implement the `AiProvider` trait
3. Export the plugin constructor
4. Move configuration to plugin settings
5. Remove direct AI dependencies from core

The plugin system provides a cleaner architecture and better separation of concerns.
