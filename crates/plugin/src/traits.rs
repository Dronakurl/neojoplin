//! Plugin traits and types

use anyhow::Result;
use async_trait::async_trait;
use downcast_rs::DowncastSync;
use joplin_domain::Note;
use ratatui::prelude::{Frame, Rect};
use std::sync::Arc;

/// Capabilities a plugin can provide
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// Plugin metadata from plugin.toml
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

/// Plugin configuration
#[derive(Debug, Clone, Default)]
pub struct PluginConfig {
    pub enabled: bool,
    pub settings: serde_json::Value,
}

/// Plugin context provides access to NeoJoplin services
#[derive(Clone)]
pub struct PluginContext {
    /// Storage access (optional, only if needed)
    pub storage: Option<Arc<dyn joplin_domain::Storage>>,
    /// Configuration
    pub config: PluginConfig,
    /// Plugin metadata
    pub metadata: PluginMetadata,
}

/// Main plugin trait - all plugins must implement this
/// 
/// Note: We use DowncastSync + Send to enable downcasting of trait objects.
/// This is necessary because we store plugins as Box<dyn Plugin> but need to
/// downcast them to specific plugin types (AiProvider, CliCommandProvider, etc.)
#[async_trait]
pub trait Plugin: Send + Sync + DowncastSync {
    /// Called when plugin is loaded
    async fn initialize(&mut self, context: PluginContext) -> Result<()>;

    /// Called when plugin is unloaded
    async fn shutdown(&mut self) -> Result<()>;

    /// Get plugin metadata
    fn metadata(&self) -> &PluginMetadata;

    /// Get plugin capabilities
    fn capabilities(&self) -> &[PluginCapability];
}

// Implement DowncastSync for Plugin trait
// This enables downcasting Box<dyn Plugin> to Box<dyn AiProvider>, etc.
downcast_rs::impl_downcast!(sync Plugin);



/// Trait for CLI command providers
#[async_trait]
pub trait CliCommandProvider: Plugin {
    /// Register CLI commands onto the app
    fn register_commands(&self, app: clap::Command) -> clap::Command;

    /// Handle CLI command execution
    async fn handle_command(&self, command_path: &str, args: &[String]) -> Result<String>;
}

/// Trait for TUI panel providers
#[async_trait]
pub trait TuiPanelProvider: Plugin {
    /// Get panel name
    fn panel_name(&self) -> &str;

    /// Get panel key binding (optional)
    fn key_binding(&self) -> Option<char>;

    /// Render panel content
    async fn render_panel(&self, area: Rect) -> Result<Frame>;

    /// Handle panel input
    async fn handle_input(&mut self, event: crossterm::event::KeyEvent) -> Result<bool>;
}

/// Trait for note processors
#[async_trait]
pub trait NoteProcessor: Plugin {
    /// Called before note is saved
    async fn before_save(&self, note: &mut Note) -> Result<()> {
        let _ = note;
        Ok(())
    }

    /// Called after note is loaded
    async fn after_load(&self, note: &mut Note) -> Result<()> {
        let _ = note;
        Ok(())
    }
}

/// Trait for AI providers
#[async_trait]
pub trait AiProvider: Plugin {
    /// Generate text based on a prompt
    async fn generate_text(&self, prompt: &str, system_prompt: Option<&str>) -> Result<String>;

    /// Generate embeddings for text
    async fn generate_embeddings(&self, text: &str) -> Result<Vec<f32>> {
        let _ = text;
        Err(anyhow::anyhow!("Embeddings not implemented"))
    }

    /// Find similar notes by semantic similarity
    async fn find_similar_notes(
        &self,
        note: &Note,
        all_notes: &[Note],
        limit: usize,
    ) -> Result<Vec<Note>> {
        let _ = (note, all_notes, limit);
        Err(anyhow::anyhow!("Similar notes not implemented"))
    }

    /// Summarize a note
    async fn summarize(&self, note: &Note, max_length: Option<usize>) -> Result<String> {
        let prompt = format!("Summarize this note:\n\n{}", note.body);
        if let Some(max) = max_length {
            self.generate_text(
                &format!("{}\n\nKeep summary under {} characters.", prompt, max),
                None,
            )
            .await
        } else {
            self.generate_text(&prompt, None).await
        }
    }

    /// Generate tags for a note
    async fn generate_tags(&self, note: &Note, limit: usize) -> Result<Vec<String>> {
        let prompt = format!(
            "Extract up to {} relevant tags from this note. Return as comma-separated list:\n\n{}",
            limit,
            note.body
        );
        let result = self.generate_text(&prompt, None).await?;
        Ok(result
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .take(limit)
            .collect())
    }
}

/// Helper function for cosine similarity
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 1.0);

        let c = vec![0.0, 1.0, 0.0];
        assert_eq!(cosine_similarity(&a, &c), 0.0);

        let d = vec![1.0, 1.0, 0.0];
        let similarity = cosine_similarity(&a, &d);
        assert!((similarity - 0.7071).abs() < 0.001);
    }
}
