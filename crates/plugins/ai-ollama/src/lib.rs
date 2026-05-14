//! Ollama AI Plugin for NeoJoplin
//!
//! This plugin provides AI capabilities via Ollama's local LLM API.
//! It implements the AiProvider trait from neojoplin-plugin.

use anyhow::{Context, Result};
use async_trait::async_trait;
use joplin_domain::Note;
use neojoplin_plugin::traits::cosine_similarity;
use neojoplin_plugin::{
    AiProvider, CliCommandProvider, Plugin, PluginCapability, PluginConfig, PluginContext,
    PluginMetadata,
};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;

/// Ollama plugin configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_api_url")]
    pub api_url: String,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
    pub api_key: Option<String>,
}

fn default_model() -> String {
    "gemma2:2b".to_string()  // Model already downloaded in Docker volumes
}

fn default_api_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_timeout() -> u64 {
    120
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            model: default_model(),
            api_url: default_api_url(),
            timeout_seconds: default_timeout(),
            api_key: None,
        }
    }
}

/// Ollama AI plugin
pub struct OllamaPlugin {
    client: Client,
    config: OllamaConfig,
    plugin_config: PluginConfig,
}

impl OllamaPlugin {
    /// Create a new Ollama plugin instance
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            config: OllamaConfig::default(),
            plugin_config: PluginConfig::default(),
        }
    }

    /// Load configuration from plugin settings
    fn load_config(&mut self) {
        if let Some(settings) = self.plugin_config.settings.get("ollama") {
            if let Ok(config) = serde_json::from_value::<OllamaConfig>(settings.clone()) {
                self.config = config;
                tracing::info!(
                    "Loaded Ollama config: model={}, api_url={}",
                    self.config.model,
                    self.config.api_url
                );
            }
        }
    }

    /// Call Ollama chat API
    async fn call_ollama(
        &self,
        prompt: &str,
        system_prompt: Option<&str>,
    ) -> Result<String> {
        let mut messages = Vec::new();

        if let Some(system) = system_prompt {
            messages.push(json!({
                "role": "system",
                "content": system
            }));
        }

        messages.push(json!({
            "role": "user",
            "content": prompt
        }));

        let request = json!({
            "model": self.config.model,
            "messages": messages,
            "stream": false,
            "options": {
                "temperature": 0.7,
                "num_predict": 2048,
            }
        });

        let url = format!("{}/api/chat", self.config.api_url);

        let mut builder = self.client.post(&url).json(&request);

        // Add API key if configured
        if let Some(ref key) = self.config.api_key {
            builder = builder.bearer_auth(key);
        }

        let response = builder
            .timeout(Duration::from_secs(self.config.timeout_seconds))
            .send()
            .await
            .with_context(|| format!("Failed to send request to Ollama at {}", url))?
            .error_for_status()
            .with_context(|| "Ollama API returned an error")?;

        let body: serde_json::Value = response.json().await?;

        body["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Invalid response format from Ollama"))
    }

    /// Call Ollama embeddings API
    async fn call_ollama_embeddings(&self, text: &str) -> Result<Vec<f32>> {
        let request = json!({
            "model": self.config.model,
            "input": text
        });

        let url = format!("{}/api/embeddings", self.config.api_url);

        let mut builder = self.client.post(&url).json(&request);

        if let Some(ref key) = self.config.api_key {
            builder = builder.bearer_auth(key);
        }

        let response = builder
            .timeout(Duration::from_secs(self.config.timeout_seconds))
            .send()
            .await
            .with_context(|| format!("Failed to send embeddings request to Ollama at {}", url))?
            .error_for_status()?;

        let body: serde_json::Value = response.json().await?;

        body["embeddings"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_f64().map(|f| f as f32))
                    .collect()
            })
            .ok_or_else(|| anyhow::anyhow!("Invalid embeddings response format"))
    }
}

/// Plugin metadata (using Lazy for static initialization with env!)
static PLUGIN_METADATA: Lazy<PluginMetadata> = Lazy::new(|| {
    PluginMetadata {
        id: "ai-ollama".to_string(),
        name: "Ollama AI Provider".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: "Local LLM integration via Ollama API".to_string(),
        author: "NeoJoplin Team".to_string(),
        license: Some("MIT".to_string()),
        dependencies: vec![],
        capabilities: vec![PluginCapability::AiProvider, PluginCapability::CliCommands],
    }
});

#[async_trait]
impl Plugin for OllamaPlugin {
    async fn initialize(&mut self, context: PluginContext) -> Result<()> {
        self.plugin_config = context.config;
        self.load_config();

        tracing::info!(
            "Ollama plugin initialized: model={}, api_url={}",
            self.config.model,
            self.config.api_url
        );

        Ok(())
    }

    async fn shutdown(&mut self) -> Result<()> {
        tracing::info!("Ollama plugin shutdown");
        Ok(())
    }

    fn metadata(&self) -> &PluginMetadata {
        &PLUGIN_METADATA
    }

    fn capabilities(&self) -> &[PluginCapability] {
        &PLUGIN_METADATA.capabilities
    }
}

#[async_trait]
impl AiProvider for OllamaPlugin {
    async fn generate_text(&self, prompt: &str, system_prompt: Option<&str>) -> Result<String> {
        self.call_ollama(prompt, system_prompt).await
    }

    async fn generate_embeddings(&self, text: &str) -> Result<Vec<f32>> {
        self.call_ollama_embeddings(text).await
    }

    async fn find_similar_notes(
        &self,
        query_note: &Note,
        all_notes: &[Note],
        limit: usize,
    ) -> Result<Vec<Note>> {
        // Generate embeddings for query
        let query_embedding = self.generate_embeddings(&query_note.body).await?;

        // Calculate similarity with all notes
        let mut results: Vec<(Note, f32)> = Vec::new();

        for note in all_notes {
            // Skip the query note itself
            if note.id == query_note.id {
                continue;
            }

            let note_embedding = self.generate_embeddings(&note.body).await?;
            let similarity = cosine_similarity(&query_embedding, &note_embedding);
            results.push((note.clone(), similarity));
        }

        // Sort by similarity (descending)
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Return top N notes
        Ok(results.into_iter().take(limit).map(|(n, _)| n).collect())
    }

    async fn summarize(&self, note: &Note, max_length: Option<usize>) -> Result<String> {
        let prompt = if let Some(max) = max_length {
            format!(
                "Summarize this note in {} characters or less:\n\n{}",
                max, note.body
            )
        } else {
            format!("Summarize this note:\n\n{}", note.body)
        };

        self.generate_text(&prompt, None).await
    }

    async fn generate_tags(&self, note: &Note, limit: usize) -> Result<Vec<String>> {
        let prompt = format!(
            "Extract up to {} relevant tags from this note. Return ONLY a comma-separated list, no other text:\n\n{}",
            limit, note.body
        );

        let result = self.generate_text(&prompt, None).await?;

        // Clean up the result - remove markdown, extra text, etc.
        let cleaned = result
            .trim()
            .trim_start_matches('"')
            .trim_end_matches('"')
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim();

        Ok(cleaned
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .take(limit)
            .collect())
    }
}

#[async_trait]
impl CliCommandProvider for OllamaPlugin {
    fn register_commands(&self, app: clap::Command) -> clap::Command {
        app.subcommand(
            clap::Command::new("ai")
                .about("AI commands (Ollama)")
                .subcommand(
                    clap::Command::new("generate")
                        .about("Generate text with AI")
                        .arg(clap::arg!(<PROMPT> "The prompt for text generation"))
                        .arg(clap::arg!(--system <SYSTEM> "Optional system prompt").required(false)),
                )
                .subcommand(
                    clap::Command::new("summarize")
                        .about("Summarize a note")
                        .arg(clap::arg!(<NOTE> "Note ID or title")),
                )
                .subcommand(
                    clap::Command::new("tags")
                        .about("Generate tags for a note")
                        .arg(clap::arg!(<NOTE> "Note ID or title"))
                        .arg(clap::arg!(--limit <LIMIT> "Maximum number of tags").default_value("5")),
                )
                .subcommand(
                    clap::Command::new("similar")
                        .about("Find similar notes")
                        .arg(clap::arg!(<NOTE> "Note ID or title"))
                        .arg(clap::arg!(--limit <LIMIT> "Maximum results").default_value("5")),
                ),
        )
    }

    async fn handle_command(&self, command_path: &str, args: &[String]) -> Result<String> {
        match command_path {
            "ai/generate" => {
                let prompt = args.get(0).map(|s| s.as_str()).unwrap_or("");
                let system = args.get(1).map(|s| s.as_str());
                self.generate_text(prompt, system).await
            }
            "ai/summarize" => {
                // For now, just return a message - full note loading requires storage
                Err(anyhow::anyhow!(
                    "Note loading not yet implemented in plugin CLI commands"
                ))
            }
            "ai/tags" => {
                // For now, just return a message
                Err(anyhow::anyhow!(
                    "Note loading not yet implemented in plugin CLI commands"
                ))
            }
            "ai/similar" => {
                // For now, just return a message
                Err(anyhow::anyhow!(
                    "Note loading not yet implemented in plugin CLI commands"
                ))
            }
            _ => Err(anyhow::anyhow!("Unknown command: {}", command_path)),
        }
    }
}



/// Plugin constructor - must be exported with this exact name
/// This is the entry point that the plugin loader calls
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub extern "C" fn plugin_constructor() -> Box<dyn Plugin> {
    Box::new(OllamaPlugin::new())
}

/// Re-export for use in tests
pub fn create_plugin() -> Box<dyn Plugin> {
    plugin_constructor()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plugin_metadata() {
        let plugin = create_plugin();
        let metadata = plugin.metadata();

        assert_eq!(metadata.id, "ai-ollama");
        assert_eq!(metadata.name, "Ollama AI Provider");
        assert!(metadata.capabilities.contains(&PluginCapability::AiProvider));
        assert!(metadata.capabilities.contains(&PluginCapability::CliCommands));
    }

    #[tokio::test]
    async fn test_downcast() {
        let plugin = create_plugin();

        // Test downcasting to AiProvider
        if let Some(ai) = plugin.downcast_ref::<dyn AiProvider>() {
            assert_eq!(ai.metadata().id, "ai-ollama");
        } else {
            panic!("Failed to downcast to AiProvider");
        }

        // Test downcasting to CliCommandProvider
        if let Some(cli) = plugin.downcast_ref::<dyn CliCommandProvider>() {
            assert_eq!(cli.metadata().id, "ai-ollama");
        } else {
            panic!("Failed to downcast to CliCommandProvider");
        }
    }

    #[test]
    fn test_cosine_similarity() {
        use neojoplin_plugin::cosine_similarity;

        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 1.0);

        let c = vec![0.0, 1.0, 0.0];
        assert_eq!(cosine_similarity(&a, &c), 0.0);
    }
}
