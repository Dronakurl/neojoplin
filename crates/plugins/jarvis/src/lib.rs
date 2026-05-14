//! Jarvis AI Chat Plugin for NeoJoplin
//!
//! This plugin provides an AI chat overlay panel for the TUI with Ollama integration.
//! It implements both AiProvider (for AI capabilities) and TuiPanelProvider (for TUI integration).

use anyhow::Result;
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use joplin_domain::Note;
use neojoplin_plugin::traits::cosine_similarity;
use neojoplin_plugin::{
    AiProvider, CliCommandProvider, Plugin, PluginCapability, PluginConfig, PluginContext,
    PluginMetadata, TuiPanelProvider,
};
use once_cell::sync::Lazy;
use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use ureq::Agent;

/// Ollama configuration
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
    "gemma2:2b".to_string()
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

/// Message in the chat overlay
#[derive(Debug, Clone, Default)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// State for the chat overlay panel
#[derive(Debug, Default, Clone)]
pub struct ChatOverlayState {
    pub visible: bool,
    pub input: String,
    pub session_id: Option<String>,
    pub messages: Vec<ChatMessage>,
    pub pending: bool,
    pub scroll: usize,
}

/// Ollama client for making API calls
#[derive(Clone)]
pub struct OllamaClient {
    config: OllamaConfig,
}

impl OllamaClient {
    pub fn new(config: OllamaConfig) -> Self {
        Self { config }
    }

    fn create_agent(&self) -> Agent {
        ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(self.config.timeout_seconds))
            .build()
    }

    pub async fn generate_text(
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

        let request_body = json!({
            "model": self.config.model,
            "messages": messages,
            "stream": false,
            "options": {
                "temperature": 0.7,
                "num_predict": 2048,
            }
        });

        let url = format!("{}/api/chat", self.config.api_url);
        let agent = self.create_agent();

        let mut request = agent.post(&url).set("Content-Type", "application/json");

        if let Some(ref key) = self.config.api_key {
            request = request.set("Authorization", &format!("Bearer {}", key));
        }

        let response = request
            .send_json(request_body)
            .map_err(|e| anyhow::anyhow!("Failed to send request to Ollama at {}: {}", url, e))?;

        let body: serde_json::Value = response
            .into_json()
            .map_err(|e| anyhow::anyhow!("Failed to parse JSON response: {}", e))?;

        body["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Invalid response format from Ollama"))
    }

    pub async fn generate_embeddings(&self, text: &str) -> Result<Vec<f32>> {
        let request_body = json!({
            "model": self.config.model,
            "input": text
        });

        let url = format!("{}/api/embeddings", self.config.api_url);
        let agent = self.create_agent();

        let mut request = agent.post(&url).set("Content-Type", "application/json");

        if let Some(ref key) = self.config.api_key {
            request = request.set("Authorization", &format!("Bearer {}", key));
        }

        let response = request
            .send_json(request_body)
            .map_err(|e| anyhow::anyhow!("Failed to send embeddings request to Ollama at {}: {}", url, e))?;

        let body: serde_json::Value = response
            .into_json()
            .map_err(|e| anyhow::anyhow!("Failed to parse JSON response: {}", e))?;

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

/// Jarvis plugin metadata
static METADATA: Lazy<PluginMetadata> = Lazy::new(|| PluginMetadata {
    id: "jarvis".to_string(),
    name: "Jarvis AI Chat".to_string(),
    version: "0.1.0".to_string(),
    description: "AI chat overlay panel with Ollama integration for NeoJoplin TUI".to_string(),
    author: "NeoJoplin Team".to_string(),
    license: Some("MIT".to_string()),
    dependencies: vec![],
    capabilities: vec![
        PluginCapability::AiProvider,
        PluginCapability::CliCommands,
        PluginCapability::TuiPanels,
    ],
});

/// Jarvis plugin - provides AI chat overlay for TUI with integrated Ollama support
#[derive(Clone)]
pub struct JarvisPlugin {
    chat_state: ChatOverlayState,
    ollama_client: Arc<Mutex<OllamaClient>>,
    config: OllamaConfig,
    plugin_config: PluginConfig,
}

impl JarvisPlugin {
    pub fn new() -> Self {
        Self {
            chat_state: ChatOverlayState::default(),
            ollama_client: Arc::new(Mutex::new(OllamaClient::new(OllamaConfig::default()))),
            config: OllamaConfig::default(),
            plugin_config: PluginConfig::default(),
        }
    }

    fn load_config(&mut self) {
        if let Some(settings) = self.plugin_config.settings.get("ollama") {
            if let Ok(config) = serde_json::from_value::<OllamaConfig>(settings.clone()) {
                self.config = config;
                *self.ollama_client.blocking_lock() = OllamaClient::new(self.config.clone());
                tracing::info!(
                    "Loaded Ollama config: model={}, api_url={}",
                    self.config.model,
                    self.config.api_url
                );
            }
        }
    }

    /// Add a message to the chat overlay
    pub fn add_message(&mut self, role: &str, content: &str) {
        self.chat_state.messages.push(ChatMessage {
            role: role.to_string(),
            content: content.to_string(),
        });
        self.chat_state.scroll = self.chat_state.messages.len().saturating_sub(1);
    }

    /// Show the chat overlay
    pub fn show(&mut self) {
        self.chat_state.visible = true;
    }

    /// Hide the chat overlay
    pub fn hide(&mut self) {
        self.chat_state.visible = false;
        self.chat_state.input.clear();
    }
}

#[async_trait]
impl Plugin for JarvisPlugin {
    async fn initialize(&mut self, context: PluginContext) -> Result<()> {
        self.plugin_config = context.config;
        self.load_config();

        tracing::info!("Jarvis plugin initialized with Ollama support");
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<()> {
        tracing::info!("Jarvis plugin shutdown");
        Ok(())
    }

    fn metadata(&self) -> &PluginMetadata {
        &METADATA
    }

    fn capabilities(&self) -> &[PluginCapability] {
        &METADATA.capabilities
    }

    fn clone_box(&self) -> Box<dyn Plugin> {
        Box::new(self.clone())
    }

    fn as_ai_provider(&self) -> Option<&dyn AiProvider> {
        Some(self)
    }

    fn as_cli_command_provider(&self) -> Option<&dyn CliCommandProvider> {
        Some(self)
    }

    fn as_tui_panel_provider(&self) -> Option<&dyn TuiPanelProvider> {
        Some(self)
    }

    fn as_mut_tui_panel_provider(&mut self) -> Option<&mut dyn TuiPanelProvider> {
        Some(self)
    }
}

#[async_trait]
impl AiProvider for JarvisPlugin {
    async fn generate_text(&self, prompt: &str, system_prompt: Option<&str>) -> Result<String> {
        let client = self.ollama_client.lock().await;
        client.generate_text(prompt, system_prompt).await
    }

    async fn generate_embeddings(&self, text: &str) -> Result<Vec<f32>> {
        let client = self.ollama_client.lock().await;
        client.generate_embeddings(text).await
    }

    async fn find_similar_notes(
        &self,
        query_note: &Note,
        all_notes: &[Note],
        limit: usize,
    ) -> Result<Vec<Note>> {
        let client = self.ollama_client.lock().await;

        // Generate embeddings for query
        let query_embedding = client.generate_embeddings(&query_note.body).await?;

        // Calculate similarity with all notes
        let mut results: Vec<(Note, f32)> = Vec::new();

        for note in all_notes {
            if note.id == query_note.id {
                continue;
            }

            let note_embedding = client.generate_embeddings(&note.body).await?;
            let similarity = cosine_similarity(&query_embedding, &note_embedding);
            results.push((note.clone(), similarity));
        }

        // Sort by similarity (descending)
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(results.into_iter().take(limit).map(|(n, _)| n).collect())
    }

    async fn summarize(&self, note: &Note, max_length: Option<usize>) -> Result<String> {
        let client = self.ollama_client.lock().await;

        let prompt = if let Some(max) = max_length {
            format!(
                "Summarize this note in {} characters or less:\n\n{}",
                max, note.body
            )
        } else {
            format!("Summarize this note:\n\n{}", note.body)
        };

        client.generate_text(&prompt, None).await
    }

    async fn generate_tags(&self, note: &Note, limit: usize) -> Result<Vec<String>> {
        let client = self.ollama_client.lock().await;

        let prompt = format!(
            "Extract up to {} relevant tags from this note. Return ONLY a comma-separated list, no other text:\n\n{}",
            limit, note.body
        );

        let result = client.generate_text(&prompt, None).await?;

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
impl CliCommandProvider for JarvisPlugin {
    fn register_commands(&self, app: clap::Command) -> clap::Command {
        app.subcommand(
            clap::Command::new("ai")
                .about("AI commands (Ollama via Jarvis)")
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
        let client = self.ollama_client.lock().await;

        match command_path {
            "ai/generate" => {
                let prompt = args.get(0).map(|s| s.as_str()).unwrap_or("");
                let system = args.get(1).map(|s| s.as_str());
                client.generate_text(prompt, system).await
            }
            "ai/summarize" => {
                Err(anyhow::anyhow!(
                    "Note loading not yet implemented in plugin CLI commands"
                ))
            }
            "ai/tags" => {
                Err(anyhow::anyhow!(
                    "Note loading not yet implemented in plugin CLI commands"
                ))
            }
            "ai/similar" => {
                Err(anyhow::anyhow!(
                    "Note loading not yet implemented in plugin CLI commands"
                ))
            }
            _ => Err(anyhow::anyhow!("Unknown command: {}", command_path)),
        }
    }
}

impl TuiPanelProvider for JarvisPlugin {
    fn panel_name(&self) -> &str {
        "AI Chat"
    }

    fn key_binding(&self) -> Option<char> {
        Some('P')
    }

    fn render_panel(&self, f: &mut Frame, area: Rect) -> Result<()> {
        if !self.chat_state.visible {
            return Ok(());
        }

        // The area passed in is the full screen area.
        // We want to render the chat overlay over the notebooks+notes panels (left side)
        // but leave the preview panel (right side) visible.
        // We'll use 60% of the width for the chat overlay to leave room for preview.
        let chat_width = (area.width * 60) / 100;
        let chat_area = Rect {
            x: area.x,
            y: area.y,
            width: chat_width.min(area.width),
            height: area.height,
        };

        let block = Block::default()
            .title("AI Chat (P to toggle, Tab to focus preview)")
            .borders(Borders::ALL);

        let inner_area = Rect {
            x: chat_area.x + 1,
            y: chat_area.y + 1,
            width: chat_area.width.saturating_sub(2),
            height: chat_area.height.saturating_sub(2),
        };

        // Render messages
        let mut cursor_y = inner_area.y;
        for msg in &self.chat_state.messages {
            let role_style = match msg.role.as_str() {
                "You" => Style::default().fg(Color::Green),
                "Jarvis" | "Assistant" => Style::default().fg(Color::Cyan),
                "System" => Style::default().fg(Color::Red),
                _ => Style::default(),
            };

            let role_paragraph = Paragraph::new(Line::from(vec![
                Span::styled(&msg.role, role_style),
                Span::raw(": "),
            ]));
            f.render_widget(role_paragraph, Rect {
                x: inner_area.x,
                y: cursor_y,
                width: inner_area.width,
                height: 1,
            });
            cursor_y += 1;

            let content_paragraph = Paragraph::new(msg.content.clone())
                .wrap(Wrap { trim: true });
            f.render_widget(content_paragraph, Rect {
                x: inner_area.x + 2,
                y: cursor_y,
                width: inner_area.width.saturating_sub(2),
                height: 1,
            });
            cursor_y += 1;
        }

        // Render input line
        let input_text = format!("> {}", self.chat_state.input);
        let input_paragraph = Paragraph::new(input_text);
        f.render_widget(input_paragraph, Rect {
            x: inner_area.x,
            y: cursor_y,
            width: inner_area.width,
            height: 1,
        });

        // Show pending indicator
        if self.chat_state.pending {
            let pending_text = "... thinking ...";
            let pending_paragraph = Paragraph::new(pending_text);
            f.render_widget(pending_paragraph, Rect {
                x: inner_area.x,
                y: cursor_y + 1,
                width: inner_area.width,
                height: 1,
            });
        }

        f.render_widget(block, chat_area);

        Ok(())
    }

    fn handle_input(&mut self, event: KeyEvent) -> Result<bool> {
        match event.code {
            // Toggle chat overlay visibility when the binding key is pressed
            KeyCode::Char('P') | KeyCode::Char('p') => {
                self.chat_state.visible = !self.chat_state.visible;
                if !self.chat_state.visible {
                    self.chat_state.input.clear();
                    self.chat_state.pending = false;
                }
                Ok(true)
            }
            KeyCode::Esc => {
                self.hide();
                Ok(true)
            }
            _ => {
                // Only handle input if chat is visible
                if !self.chat_state.visible {
                    return Ok(false);
                }

                match event.code {
                    KeyCode::Enter => {
                        if !self.chat_state.input.trim().is_empty() && !self.chat_state.pending {
                            let question = self.chat_state.input.trim().to_string();
                            self.add_message("You", &question);
                            self.chat_state.input.clear();
                            self.chat_state.pending = true;

                            // TODO: Spawn async task to call Ollama AI
                            // For now, add a placeholder response
                            self.add_message("Jarvis", "Let me think about that...");
                            self.chat_state.pending = false;
                        }
                        Ok(true)
                    }
                    KeyCode::Backspace => {
                        self.chat_state.input.pop();
                        Ok(true)
                    }
                    KeyCode::Char(c) => {
                        self.chat_state.input.push(c);
                        Ok(true)
                    }
                    // Pass Tab through to allow switching to preview panel
                    KeyCode::Tab => Ok(false),
                    _ => Ok(false),
                }
            }
        }
    }
}

// Plugin constructor
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub extern "C" fn plugin_constructor() -> Box<dyn Plugin> {
    Box::new(JarvisPlugin::new())
}

// Export for testing
pub fn create_plugin() -> Box<dyn Plugin> {
    plugin_constructor()
}
