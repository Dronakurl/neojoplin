# AI Integration Guide for NeoJoplin

This document provides guidance on integrating AI support into NeoJoplin as a Rust module.

## Overview

NeoJoplin is a terminal-based Joplin client written in Rust. This guide describes how to add AI capabilities (such as LLM integration, smart search, content generation, etc.) as a modular Rust crate.

## Project Structure

The NeoJoplin workspace consists of several crates:

```
neojoplin/
├── crates/
│   ├── cli/          # Command-line interface
│   ├── core/         # Core functionality (config, editor, etc.)
│   ├── e2ee/         # End-to-end encryption
│   ├── joplin/       # Joplin domain types
│   ├── joplin-domain/# Domain models and traits
│   ├── joplin-sync/  # Sync functionality
│   ├── storage/      # Database storage (SQLite)
│   ├── sync/         # Sync engine
│   ├── test-utils/   # Test utilities
│   ├── tui/          # Terminal UI (ratatui)
│   └── ai/           # <--- NEW: AI module (proposed)
└── Cargo.toml       # Workspace manifest
```

## Step 1: Create the AI Crate

Create a new crate in the `crates/` directory:

```bash
cd neojoplin
cargo new --lib crates/ai
```

### `crates/ai/Cargo.toml`

```toml
[package]
name = "neojoplin-ai"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
# Workspace dependencies
joplin-domain = { path = "../joplin-domain" }
neojoplin-core = { path = "../core" }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
tracing = { workspace = true }

# AI-specific dependencies
# Choose based on your needs:
# - For local LLMs (via REST API):
reqwest = { workspace = true }

# - For embeddings (optional):
# fastembed = "0.7"
# or
# candle-core = "0.6"

# - For tokenization:
# tokenizers = "0.15"
```

## Step 2: Define AI Module Traits

Create a clean abstraction for AI functionality. This allows for:
- Multiple AI provider implementations
- Easy testing with mock implementations
- Swapping providers without changing business logic

### `crates/ai/src/traits.rs`

```rust
use anyhow::Result;
use joplin_domain::{Note, Folder};
use serde_json::Value;

/// Trait for AI text generation
pub trait TextGenerator: Send + Sync {
    /// Generate text based on a prompt
    fn generate(&self, prompt: &str, context: Option<&TextGenerationContext>) -> Result<String>;
    
    /// Stream text generation (for real-time output)
    fn generate_stream(
        &self,
        prompt: &str,
        context: Option<&TextGenerationContext>,
        callback: Box<dyn Fn(String) + Send + Sync>,
    ) -> Result<()>;
}

/// Context for text generation
#[derive(Debug, Clone, Default)]
pub struct TextGenerationContext {
    pub note: Option<Note>,
    pub folder: Option<Folder>,
    pub related_notes: Vec<Note>,
    pub system_prompt: Option<String>,
}

/// Trait for embedding generation
pub trait EmbeddingGenerator: Send + Sync {
    /// Generate embeddings for text
    fn generate_embeddings(&self, text: &str) -> Result<Vec<f32>>;
    
    /// Generate embeddings for multiple texts
    fn generate_embeddings_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

/// Trait for semantic search
pub trait SemanticSearch: Send + Sync {
    /// Search notes by semantic similarity
    fn search_similar(
        &self,
        query: &str,
        notes: &[Note],
        limit: usize,
    ) -> Result<Vec<(Note, f32)>; // (note, similarity_score)
    
    /// Find related notes
    fn find_related(&self, note: &Note, all_notes: &[Note], limit: usize) -> Result<Vec<Note>>;
}

/// Trait for note analysis
pub trait NoteAnalyzer: Send + Sync {
    /// Extract tags from note content
    fn extract_tags(&self, note: &Note) -> Result<Vec<String>>;
    
    /// Summarize note content
    fn summarize(&self, note: &Note, max_length: Option<usize>) -> Result<String>;
    
    /// Generate a title from note content
    fn generate_title(&self, note: &Note) -> Result<String>;
}

/// AI Provider configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AiProviderConfig {
    pub provider_type: AiProviderType,
    pub api_url: Option<String>,
    pub api_key: Option<String>,
    pub model: String,
    pub timeout_seconds: u64,
    pub temperature: f32,
    pub max_tokens: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AiProviderType {
    None,
    OpenAI,
    LocalLLM,
    Anthropic,
    Mistral,
    Ollama,
}

impl Default for AiProviderType {
    fn default() -> Self {
        Self::None
    }
}
```

## Step 3: Implement AI Providers

### Local LLM Provider (Ollama example)

### `crates/ai/src/providers/ollama.rs`

```rust
use crate::traits::{TextGenerator, TextGenerationContext};
use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::json;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    model: String,
    timeout: Duration,
}

impl OllamaProvider {
    pub fn new(base_url: &str, model: &str, timeout_seconds: u64) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.to_string(),
            model: model.to_string(),
            timeout: Duration::from_secs(timeout_seconds),
        }
    }
    
    async fn generate_internal(&self, prompt: &str, system: Option<&str>) -> Result<String> {
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
            "model": self.model,
            "messages": messages,
            "stream": false
        });
        
        let response = self
            .client
            .post(&format!("{}/api/chat", self.base_url))
            .json(&request)
            .timeout(self.timeout)
            .send()
            .await
            .context("Failed to send request to Ollama")?
            .error_for_status()
            .context("Ollama API error")?;
        
        let body: serde_json::Value = response.json().await?;
        
        body["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Invalid response format from Ollama"))
    }
}

impl TextGenerator for OllamaProvider {
    fn generate(&self, prompt: &str, context: Option<&TextGenerationContext>) -> Result<String> {
        // Use tokio runtime for async operations
        let runtime = tokio::runtime::Runtime::new()?;
        
        let system_prompt = context
            .and_then(|c| c.system_prompt.as_deref())
            .unwrap_or("You are a helpful assistant.");
        
        runtime.block_on(self.generate_internal(prompt, Some(system_prompt)))
    }
    
    fn generate_stream(
        &self,
        prompt: &str,
        context: Option<&TextGenerationContext>,
        callback: Box<dyn Fn(String) + Send + Sync>,
    ) -> Result<()> {
        // Stream implementation would use streaming API
        // For simplicity, we'll use non-streaming here
        let result = self.generate(prompt, context)?;
        callback(result);
        Ok(())
    }
}
```

### OpenAI Provider

### `crates/ai/src/providers/openai.rs`

```rust
use crate::traits::{TextGenerator, TextGenerationContext};
use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::json;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct OpenAIProvider {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
    timeout: Duration,
}

impl OpenAIProvider {
    pub fn new(api_key: String, model: &str, timeout_seconds: u64) -> Self {
        Self {
            client: Client::new(),
            api_key,
            base_url: "https://api.openai.com/v1".to_string(),
            model: model.to_string(),
            timeout: Duration::from_secs(timeout_seconds),
        }
    }
    
    pub fn with_base_url(mut self, base_url: &str) -> Self {
        self.base_url = base_url.to_string();
        self
    }
}

impl TextGenerator for OpenAIProvider {
    fn generate(&self, prompt: &str, context: Option<&TextGenerationContext>) -> Result<String> {
        let runtime = tokio::runtime::Runtime::new()?;
        
        let system_prompt = context
            .and_then(|c| c.system_prompt.as_deref())
            .unwrap_or("You are a helpful assistant.");
        
        runtime.block_on(async {
            let mut messages = vec![
                json!({"role": "system", "content": system_prompt}),
                json!({"role": "user", "content": prompt}),
            ];
            
            let request = json!({
                "model": self.model,
                "messages": messages,
                "temperature": 0.7,
            });
            
            let response = self
                .client
                .post(&format!("{}/chat/completions", self.base_url))
                .bearer_auth(&self.api_key)
                .json(&request)
                .timeout(self.timeout)
                .send()
                .await
                .context("Failed to send request to OpenAI")?
                .error_for_status()
                .context("OpenAI API error")?;
            
            let body: serde_json::Value = response.json().await?;
            
            body["choices"][0]["message"]["content"]
                .as_str()
                .map(|s| s.to_string())
                .ok_or_else(|| anyhow::anyhow!("Invalid response format from OpenAI"))
        })
    }
    
    fn generate_stream(
        &self,
        _prompt: &str,
        _context: Option<&TextGenerationContext>,
        _callback: Box<dyn Fn(String) + Send + Sync>,
    ) -> Result<()> {
        // Streaming implementation would use SSE
        // For now, fall back to non-streaming
        unimplemented!("Streaming not yet implemented for OpenAI")
    }
}
```

## Step 4: AI Service Factory

### `crates/ai/src/service.rs`

```rust
use crate::traits::{AiProviderConfig, AiProviderType, TextGenerator};
use anyhow::Result;
use std::sync::Arc;

/// AI Service that provides access to AI capabilities
#[derive(Debug, Clone)]
pub struct AiService {
    text_generator: Option<Arc<dyn TextGenerator>>,
    // Add other AI capabilities as needed
    config: AiProviderConfig,
}

impl AiService {
    /// Create a new AI service with the given configuration
    pub fn new(config: AiProviderConfig) -> Result<Self> {
        let text_generator = Self::create_text_generator(&config)?;
        
        Ok(Self {
            text_generator,
            config,
        })
    }
    
    /// Create a text generator based on configuration
    fn create_text_generator(config: &AiProviderConfig) -> Result<Option<Arc<dyn TextGenerator>>> {
        match config.provider_type {
            AiProviderType::None => Ok(None),
            AiProviderType::Ollama => {
                let provider = super::providers::ollama::OllamaProvider::new(
                    config.api_url.as_deref().unwrap_or("http://localhost:11434"),
                    &config.model,
                    config.timeout_seconds,
                );
                Ok(Some(Arc::new(provider)))
            }
            AiProviderType::OpenAI => {
                let api_key = config.api_key.clone()
                    .ok_or_else(|| anyhow::anyhow!("API key required for OpenAI provider"))?;
                let provider = super::providers::openai::OpenAIProvider::new(
                    api_key,
                    &config.model,
                    config.timeout_seconds,
                );
                Ok(Some(Arc::new(provider)))
            }
            AiProviderType::Mistral => {
                // Implement Mistral provider
                unimplemented!("Mistral provider not yet implemented")
            }
            _ => {
                tracing::warn!("AI provider {} not yet implemented", config.provider_type as i32);
                Ok(None)
            }
        }
    }
    
    /// Generate text using the configured AI provider
    pub fn generate(&self, prompt: &str) -> Result<String> {
        let generator = self.text_generator
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No AI provider configured"))?;
        
        generator.generate(prompt, None)
    }
    
    /// Get the current configuration
    pub fn config(&self) -> &AiProviderConfig {
        &self.config
    }
    
    /// Check if AI is enabled
    pub fn is_enabled(&self) -> bool {
        self.text_generator.is_some()
    }
}

impl Default for AiService {
    fn default() -> Self {
        Self {
            text_generator: None,
            config: AiProviderConfig {
                provider_type: AiProviderType::None,
                api_url: None,
                api_key: None,
                model: "llama3".to_string(),
                timeout_seconds: 30,
                temperature: 0.7,
                max_tokens: None,
            },
        }
    }
}
```

## Step 5: Module Exports

### `crates/ai/src/lib.rs`

```rust
//! NeoJoplin AI Module
//!
//! This module provides AI capabilities for NeoJoplin including:
//! - Text generation (LLM integration)
//! - Semantic search
//! - Note analysis (tag extraction, summarization)
//! - Content recommendations

pub mod traits;
pub mod providers;
pub mod service;
pub mod config;

// Re-export main types
pub use traits::{
    AiProviderConfig, AiProviderType, EmbeddingGenerator, NoteAnalyzer,
    SemanticSearch, TextGenerator, TextGenerationContext,
};
pub use service::AiService;
```

## Step 6: Add AI Crate to Workspace

### `Cargo.toml` (workspace root)

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
  "crates/ai",  # Add the new AI crate
]
```

## Step 7: Integrate AI into TUI

### Add AI service to TUI app state

### `crates/tui/src/state.rs`

```rust
use neojoplin_ai::AiService;

#[derive(Debug, Clone)]
pub struct AppState {
    // ... existing fields ...
    pub ai_service: AiService,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            // ... existing fields ...
            ai_service: AiService::default(),
        }
    }
}
```

### Initialize AI in app startup

### `crates/tui/src/app.rs`

```rust
use neojoplin_ai::{AiProviderConfig, AiProviderType, AiService};

impl App {
    pub async fn new() -> Result<Self> {
        // ... existing initialization ...
        
        // Load AI configuration
        let ai_config = Self::load_ai_config().await?;
        let ai_service = AiService::new(ai_config)?;
        
        let state = AppState {
            // ... existing state fields ...
            ai_service,
        };
        
        // ... rest of initialization ...
    }
    
    async fn load_ai_config() -> Result<AiProviderConfig> {
        // Load from environment or settings file
        let provider_type = std::env::var("AI_PROVIDER")
            .ok()
            .and_then(|s| s.parse::<AiProviderType>().ok())
            .unwrap_or(AiProviderType::None);
        
        let api_key = std::env::var("AI_API_KEY").ok();
        let api_url = std::env::var("AI_API_URL").ok();
        let model = std::env::var("AI_MODEL").unwrap_or_else(|_| "llama3".to_string());
        
        Ok(AiProviderConfig {
            provider_type,
            api_key,
            api_url,
            model,
            timeout_seconds: 30,
            temperature: 0.7,
            max_tokens: None,
        })
    }
}
```

## Step 8: Add AI Commands to CLI

### `crates/cli/src/main.rs`

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...
    
    /// AI-related commands
    Ai {
        #[command(subcommand)]
        command: AiCommands,
    },
}

#[derive(Subcommand)]
enum AiCommands {
    /// Generate text using AI
    Generate {
        /// Prompt for text generation
        prompt: String,
        
        /// System prompt/context
        #[arg(short, long)]
        system: Option<String>,
    },
    
    /// Summarize a note
    Summarize {
        /// Note ID or title
        note: String,
    },
    
    /// Generate tags for a note
    Tag {
        /// Note ID or title
        note: String,
    },
    
    /// Search notes semantically
    Search {
        /// Search query
        query: String,
        
        /// Number of results
        #[arg(short, long, default_value = "5")]
        limit: usize,
    },
    
    /// Show AI configuration
    Config,
}

// In main() match arm:
Commands::Ai { command } => {
    match command {
        AiCommands::Generate { prompt, system } => {
            let ai_config = load_ai_config()?;
            let ai_service = AiService::new(ai_config)?;
            
            let context = TextGenerationContext {
                system_prompt: system,
                ..Default::default()
            };
            
            // Note: This is synchronous, for async use tokio::spawn
            let runtime = tokio::runtime::Runtime::new()?;
            let result = runtime.block_on(async {
                let generator = ai_service.text_generator.ok_or_else(|| 
                    anyhow::anyhow!("No AI provider configured"))?;
                generator.generate(&prompt, Some(&context))
            })?;
            
            println!("{}", result);
            Ok(())
        }
        // ... other AI commands ...
    }
}
```

## Step 9: Add AI Settings to TUI

### `crates/tui/src/settings.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiProviderType {
    None,
    Ollama,
    OpenAI,
    Mistral,
    Anthropic,
}

#[derive(Debug, Clone)]
pub struct AiSettings {
    pub provider: AiProviderType,
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: Option<usize>,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            provider: AiProviderType::None,
            api_url: "http://localhost:11434".to_string(),
            api_key: String::new(),
            model: "llama3".to_string(),
            temperature: 0.7,
            max_tokens: None,
        }
    }
}

// Add to Settings struct:
pub struct Settings {
    // ... existing fields ...
    pub ai: AiSettings,
}
```

## Using AI in Test Mode

When developing AI features, use the `--test-mode` flag to isolate your configuration:

```bash
# Using the flag
NEOJOPLIN_TEST_MODE=1 cargo run --bin neojoplin

# Or with the environment variable
NEOJOPLIN_TEST_MODE=1 neojoplin

# Or with the CLI flag
neojoplin --test-mode
```

This will use:
- Config directory: `~/.config/neojoplin-test/`
- Data directory: `~/.local/share/neojoplin-test/`

## Testing AI Integration

### Unit Tests

```rust
// crates/ai/tests/test_service.rs

#[cfg(test)]
mod tests {
    use neojoplin_ai::{AiProviderConfig, AiProviderType, AiService};
    
    #[test]
    fn test_ai_service_creation() {
        let config = AiProviderConfig {
            provider_type: AiProviderType::None,
            api_url: None,
            api_key: None,
            model: "test".to_string(),
            timeout_seconds: 10,
            temperature: 0.5,
            max_tokens: Some(100),
        };
        
        let service = AiService::new(config).unwrap();
        assert!(!service.is_enabled());
    }
}
```

### Integration Tests

```rust
// crates/ai/tests/integration.rs

#[tokio::test]
async fn test_ollama_integration() {
    // Skip if Ollama is not running
    if std::env::var("SKIP_OLLAMA_TESTS").is_ok() {
        return;
    }
    
    use neojoplin_ai::{AiProviderConfig, AiProviderType, AiService, TextGenerationContext};
    
    let config = AiProviderConfig {
        provider_type: AiProviderType::Ollama,
        api_url: Some("http://localhost:11434".to_string()),
        api_key: None,
        model: "llama3:8b".to_string(),
        timeout_seconds: 60,
        temperature: 0.7,
        max_tokens: Some(100),
    };
    
    let service = AiService::new(config).unwrap();
    
    // Test simple generation
    let result = service.generate("Hello, how are you?").unwrap();
    assert!(!result.is_empty());
    assert!(result.len() > 10); // Reasonable response length
}
```

## Performance Considerations

1. **Async Operations**: All AI operations should be async to avoid blocking the UI thread.
2. **Caching**: Consider caching embeddings and AI responses to avoid repeated computations.
3. **Rate Limiting**: Implement rate limiting for API calls to avoid hitting provider limits.
4. **Timeout Handling**: Always set reasonable timeouts for network requests.
5. **Error Handling**: Provide user-friendly error messages for AI failures.

## Security Considerations

1. **API Key Storage**: Store API keys securely (use keyring or encrypted storage).
2. **Input Sanitization**: Sanitize prompts to avoid prompt injection attacks.
3. **Content Filtering**: Consider filtering sensitive content before sending to external APIs.
4. **Local Mode**: Prefer local LLM providers (Ollama, etc.) for privacy-sensitive use cases.

## Suggested AI Features

### Phase 1: Core AI Capabilities
- [ ] Text generation for new notes
- [ ] Note summarization
- [ ] Tag suggestion/auto-tagging
- [ ] Smart search (semantic)

### Phase 2: Enhanced Features
- [ ] Note content analysis (sentiment, topics)
- [ ] Related note recommendations
- [ ] Intelligent note linking
- [ ] Content translation

### Phase 3: Advanced Features
- [ ] Voice-to-text (whisper integration)
- [ ] OCR for image-based notes
- [ ] Code explanation/generation
- [ ] Meeting notes auto-generation

## Provider Configuration Examples

### Ollama (Local LLM)

```bash
# Install Ollama
curl -fsSL https://ollama.com/install.sh | sh

# Pull a model
ollama pull llama3:8b

# Run Ollama
ollama serve
```

Environment variables:
```bash
export AI_PROVIDER=Ollama
export AI_API_URL=http://localhost:11434
export AI_MODEL=llama3:8b
```

### OpenAI

```bash
export AI_PROVIDER=OpenAI
export AI_API_KEY=sk-...
export AI_MODEL=gpt-4o-mini
```

### Mistral

```bash
export AI_PROVIDER=Mistral
export AI_API_KEY=...  
export AI_API_URL=https://api.mistral.ai/v1
export AI_MODEL=mistral-tiny
```

## Documentation

Add user documentation for AI features in:
- `docs/ai-features.md` - User-facing AI feature documentation
- Update `README.md` with AI setup instructions

## Versioning

When adding AI features, consider:
- Making AI optional (compile-time feature flag)
- Providing fallback behavior when AI is unavailable
- Clear error messages when AI features require configuration

## Contributing AI Features

When contributing AI-related changes:
1. Add tests for new AI functionality
2. Document any new dependencies
3. Update configuration documentation
4. Consider privacy implications
5. Keep AI operations opt-in where appropriate
