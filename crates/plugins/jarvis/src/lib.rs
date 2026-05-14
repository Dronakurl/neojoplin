//! Jarvis AI Chat Plugin for NeoJoplin
//!
//! This plugin provides an AI chat overlay panel for the TUI.
//! It implements the TuiPanelProvider trait to integrate with the NeoJoplin TUI.

use anyhow::Result;
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use neojoplin_plugin::{
    AiProvider, Plugin, PluginCapability, PluginConfig, PluginContext, PluginMetadata,
    TuiPanelProvider,
};
use once_cell::sync::Lazy;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::text::{Line, Span};
use ratatui::style::{Color, Style};
use ratatui::Frame;
use std::sync::Arc;

/// Jarvis plugin metadata
static METADATA: Lazy<PluginMetadata> = Lazy::new(|| PluginMetadata {
    id: "jarvis".to_string(),
    name: "Jarvis AI Chat".to_string(),
    version: "0.1.0".to_string(),
    description: "AI chat overlay panel for NeoJoplin TUI".to_string(),
    author: "NeoJoplin Team".to_string(),
    license: Some("MIT".to_string()),
    dependencies: vec!["ai-ollama".to_string()],
    capabilities: vec![PluginCapability::TuiPanels],
});

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

/// Jarvis plugin - provides AI chat overlay for TUI
#[derive(Clone)]
pub struct JarvisPlugin {
    state: ChatOverlayState,
    ai_provider: Option<Arc<dyn AiProvider>>,
    plugin_config: PluginConfig,
}

impl JarvisPlugin {
    pub fn new() -> Self {
        Self {
            state: ChatOverlayState::default(),
            ai_provider: None,
            plugin_config: PluginConfig::default(),
        }
    }

    /// Add a message to the chat overlay
    pub fn add_message(&mut self, role: &str, content: &str) {
        self.state.messages.push(ChatMessage {
            role: role.to_string(),
            content: content.to_string(),
        });
        self.state.scroll = self.state.messages.len().saturating_sub(1);
    }

    /// Toggle the chat overlay visibility
    pub fn toggle_visible(&mut self) {
        self.state.visible = !self.state.visible;
    }

    /// Show the chat overlay
    pub fn show(&mut self) {
        self.state.visible = true;
    }

    /// Hide the chat overlay
    pub fn hide(&mut self) {
        self.state.visible = false;
        self.state.input.clear();
    }
}

#[async_trait]
impl Plugin for JarvisPlugin {
    async fn initialize(&mut self, _context: PluginContext) -> Result<()> {
        // Try to get an AI provider from loaded plugins
        // For now, we'll look it up when needed
        tracing::info!("Jarvis plugin initialized");
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
    
    fn as_tui_panel_provider(&self) -> Option<&dyn TuiPanelProvider> {
        Some(self)
    }
    
    fn as_mut_tui_panel_provider(&mut self) -> Option<&mut dyn TuiPanelProvider> {
        Some(self)
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
        if !self.state.visible {
            return Ok(());
        }

        // Create the chat overlay UI
        let block = Block::default()
            .title("AI Chat (P)")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue));

        let inner_area = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };

        // Render messages
        let mut cursor_y = inner_area.y;
        for msg in &self.state.messages {
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

            // Wrap the content
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
        if self.state.visible {
            let input_text = format!("> {}", self.state.input);
            let input_paragraph = Paragraph::new(input_text)
                .style(Style::default().fg(Color::Yellow));
            f.render_widget(input_paragraph, Rect {
                x: inner_area.x,
                y: cursor_y,
                width: inner_area.width,
                height: 1,
            });

            // Show pending indicator
            if self.state.pending {
                let pending_text = "... thinking ...";
                let pending_paragraph = Paragraph::new(pending_text)
                    .style(Style::default().fg(Color::Magenta));
                f.render_widget(pending_paragraph, Rect {
                    x: inner_area.x,
                    y: cursor_y + 1,
                    width: inner_area.width,
                    height: 1,
                });
            }
        }

        // Draw the border
        f.render_widget(block, area);

        Ok(())
    }

    fn handle_input(&mut self, event: KeyEvent) -> Result<bool> {
        if !self.state.visible {
            return Ok(false);
        }

        match event.code {
            KeyCode::Esc => {
                self.hide();
                Ok(true)
            }
            KeyCode::Enter => {
                if !self.state.input.trim().is_empty() && !self.state.pending {
                    let question = self.state.input.trim().to_string();
                    self.add_message("You", &question);
                    self.state.input.clear();
                    self.state.pending = true;
                    
                    // TODO: This would need to trigger async AI call
                    // For now, just add a placeholder response
                    self.add_message("Jarvis", "I'm thinking about that...");
                    self.state.pending = false;
                }
                Ok(true)
            }
            KeyCode::Backspace => {
                self.state.input.pop();
                Ok(true)
            }
            KeyCode::Char(c) => {
                self.state.input.push(c);
                Ok(true)
            }
            _ => Ok(false),
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
