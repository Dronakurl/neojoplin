//! NeoJoplin Plugin System
//!
//! This crate provides a plugin system for NeoJoplin, allowing AI and other
//! features to be added as modular dynamic libraries.

pub mod loader;
pub mod manager;
pub mod traits;

// Re-export main types
pub use loader::{PluginConstructor, PluginLoader};
pub use manager::{PluginManager, PluginManagerConfig};
pub use traits::{
    AiProvider, CliCommandProvider, NoteProcessor, Plugin, PluginCapability,
    PluginConfig, PluginContext, PluginMetadata, TuiPanelProvider,
};
