use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use chrono::Utc;
use joplin_domain::Note;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, sqlite::SqlitePoolOptions, SqlitePool};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Plugin not enabled: {0}")]
    NotEnabled(String),
    #[error("Plugin not found: {0}")]
    NotFound(String),
    #[error("Provider error: {0}")]
    Provider(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    Available,
    Enabled,
    Disabled,
}

#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub manifest: PluginManifest,
    pub state: PluginState,
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub session_id: String,
    pub answer: String,
    pub suggested_note_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

impl ChatRole {
    fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "system" => Self::System,
            "assistant" => Self::Assistant,
            _ => Self::User,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[async_trait::async_trait]
trait ChatProvider: Send + Sync {
    async fn complete(&self, messages: &[ChatMessage]) -> Result<String, PluginError>;
}

#[derive(Debug, Clone)]
struct OllamaProvider {
    client: Client,
    base_url: String,
    model: String,
}

#[derive(Debug, Clone)]
struct OpenAiCompatProvider {
    client: Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
}

#[async_trait::async_trait]
impl ChatProvider for OllamaProvider {
    async fn complete(&self, messages: &[ChatMessage]) -> Result<String, PluginError> {
        let payload = serde_json::json!({
            "model": self.model,
            "stream": false,
            "messages": messages.iter().map(|m| serde_json::json!({
                "role": m.role.as_str(),
                "content": m.content
            })).collect::<Vec<_>>()
        });

        let url = format!("{}/api/chat", self.base_url.trim_end_matches('/'));
        let response = self
            .client
            .post(url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| PluginError::Provider(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(PluginError::Provider(format!(
                "Ollama API request failed ({status}): {body}"
            )));
        }

        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|e| PluginError::Provider(e.to_string()))?;
        let content = value
            .get("message")
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or_default();

        Ok(content.to_string())
    }
}

#[async_trait::async_trait]
impl ChatProvider for OpenAiCompatProvider {
    async fn complete(&self, messages: &[ChatMessage]) -> Result<String, PluginError> {
        let payload = serde_json::json!({
            "model": self.model,
            "temperature": 0.2,
            "messages": messages.iter().map(|m| serde_json::json!({
                "role": m.role.as_str(),
                "content": m.content
            })).collect::<Vec<_>>()
        });

        let mut req = self
            .client
            .post(format!(
                "{}/v1/chat/completions",
                self.base_url.trim_end_matches('/')
            ))
            .json(&payload);

        if let Some(key) = &self.api_key {
            req = req.bearer_auth(key);
        }

        let response = req
            .send()
            .await
            .map_err(|e| PluginError::Provider(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(PluginError::Provider(format!(
                "OpenAI-compatible API request failed ({status}): {body}"
            )));
        }

        let value: serde_json::Value = response
            .json()
            .await
            .map_err(|e| PluginError::Provider(e.to_string()))?;
        let content = value
            .get("choices")
            .and_then(|v| v.as_array())
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|msg| msg.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or_default();

        Ok(content.to_string())
    }
}

#[derive(Debug, Clone)]
struct ProviderConfig {
    provider: String,
    ollama_base_url: String,
    ollama_model: String,
    openai_base_url: String,
    openai_model: String,
    openai_api_key: Option<String>,
}

impl ProviderConfig {
    fn load(default_provider: &str) -> Self {
        let provider = read_env_key("NEOJOPLIN_AI_PROVIDER")
            .unwrap_or_else(|| default_provider.to_string())
            .to_lowercase();
        let ollama_base_url =
            read_env_key("OLLAMA_BASE_URL").unwrap_or_else(|| "http://127.0.0.1:11434".to_string());
        let ollama_model = read_env_key("OLLAMA_MODEL").unwrap_or_else(|| "llama3.2".to_string());
        let openai_base_url =
            read_env_key("OPENAI_BASE_URL").unwrap_or_else(|| "https://api.openai.com".to_string());
        let openai_model =
            read_env_key("OPENAI_MODEL").unwrap_or_else(|| "gpt-4.1-mini".to_string());
        let openai_api_key = read_env_key("OPENAI_API_KEY");
        Self {
            provider,
            ollama_base_url,
            ollama_model,
            openai_base_url,
            openai_model,
            openai_api_key,
        }
    }
}

fn read_env_key(key: &str) -> Option<String> {
    if let Ok(value) = std::env::var(key) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    for path in [
        PathBuf::from(".env"),
        dirs::home_dir().map(|p| p.join(".env"))?,
    ] {
        if !path.exists() {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(path) {
            for line in content.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((lhs, rhs)) = line.split_once('=') {
                    if lhs.trim() == key {
                        let value = rhs.trim().trim_matches('"').trim_matches('\'');
                        if !value.is_empty() {
                            return Some(value.to_string());
                        }
                    }
                }
            }
        }
    }

    None
}

fn build_provider(config: &ProviderConfig) -> Result<Box<dyn ChatProvider>, PluginError> {
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| PluginError::Config(format!("Failed to build HTTP client: {}", e)))?;

    if config.provider == "openai" || config.provider == "openai_compatible" {
        Ok(Box::new(OpenAiCompatProvider {
            client,
            base_url: config.openai_base_url.clone(),
            model: config.openai_model.clone(),
            api_key: config.openai_api_key.clone(),
        }))
    } else {
        Ok(Box::new(OllamaProvider {
            client,
            base_url: config.ollama_base_url.clone(),
            model: config.ollama_model.clone(),
        }))
    }
}

#[derive(Clone)]
struct ChatStore {
    pool: SqlitePool,
}

impl ChatStore {
    async fn new(path: &Path) -> Result<Self, PluginError> {
        let filename = path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or_default();
        if filename == "joplin.db" {
            return Err(PluginError::Storage(
                "Refusing to initialize chat store on joplin.db".to_string(),
            ));
        }

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", path.display()))
            .map_err(|e| PluginError::Storage(e.to_string()))?
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);

        let pool = SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await
            .map_err(|e| PluginError::Storage(e.to_string()))?;

        let store = Self { pool };
        store.initialize().await?;
        Ok(store)
    }

    async fn initialize(&self) -> Result<(), PluginError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PluginError::Storage(e.to_string()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                compacted INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY(session_id) REFERENCES sessions(id)
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PluginError::Storage(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_messages_session_created ON messages(session_id, created_at)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| PluginError::Storage(e.to_string()))?;

        Ok(())
    }

    async fn ensure_session(
        &self,
        requested: Option<&str>,
        fallback_title: &str,
    ) -> Result<String, PluginError> {
        if let Some(id) = requested {
            let exists: Option<String> = sqlx::query_scalar("SELECT id FROM sessions WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| PluginError::Storage(e.to_string()))?;
            if exists.is_some() {
                return Ok(id.to_string());
            }
        }

        if requested.is_none() {
            let latest: Option<String> =
                sqlx::query_scalar("SELECT id FROM sessions ORDER BY updated_at DESC LIMIT 1")
                    .fetch_optional(&self.pool)
                    .await
                    .map_err(|e| PluginError::Storage(e.to_string()))?;
            if let Some(existing) = latest {
                return Ok(existing);
            }
        }

        let id = Uuid::new_v4().simple().to_string();
        let now = now_ms();
        sqlx::query("INSERT INTO sessions (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind(&id)
            .bind(fallback_title)
            .bind(now)
            .bind(now)
            .execute(&self.pool)
            .await
            .map_err(|e| PluginError::Storage(e.to_string()))?;
        Ok(id)
    }

    async fn append_message(
        &self,
        session_id: &str,
        role: ChatRole,
        content: &str,
    ) -> Result<(), PluginError> {
        let now = now_ms();
        let message_id = Uuid::new_v4().simple().to_string();

        sqlx::query(
            "INSERT INTO messages (id, session_id, role, content, created_at, compacted) VALUES (?, ?, ?, ?, ?, 0)",
        )
        .bind(message_id)
        .bind(session_id)
        .bind(role.as_str())
        .bind(content)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| PluginError::Storage(e.to_string()))?;

        sqlx::query("UPDATE sessions SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(session_id)
            .execute(&self.pool)
            .await
            .map_err(|e| PluginError::Storage(e.to_string()))?;
        Ok(())
    }

    async fn active_messages(&self, session_id: &str) -> Result<Vec<StoredMessage>, PluginError> {
        let rows = sqlx::query_as::<_, StoredMessage>(
            r#"
            SELECT id, role, content, created_at
            FROM messages
            WHERE session_id = ? AND compacted = 0
            ORDER BY created_at ASC
            "#,
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| PluginError::Storage(e.to_string()))?;
        Ok(rows)
    }

    async fn latest_session_id(&self) -> Result<Option<String>, PluginError> {
        let session_id: Option<String> =
            sqlx::query_scalar("SELECT id FROM sessions ORDER BY updated_at DESC LIMIT 1")
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| PluginError::Storage(e.to_string()))?;
        Ok(session_id)
    }

    async fn compact_messages(
        &self,
        session_id: &str,
        message_ids: &[String],
        summary: &str,
    ) -> Result<(), PluginError> {
        if message_ids.is_empty() {
            return Ok(());
        }

        let now = now_ms();
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| PluginError::Storage(e.to_string()))?;

        let summary_id = Uuid::new_v4().simple().to_string();
        sqlx::query(
            "INSERT INTO messages (id, session_id, role, content, created_at, compacted) VALUES (?, ?, ?, ?, ?, 0)",
        )
        .bind(summary_id)
        .bind(session_id)
        .bind(ChatRole::System.as_str())
        .bind(summary)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(|e| PluginError::Storage(e.to_string()))?;

        for id in message_ids {
            sqlx::query("UPDATE messages SET compacted = 1 WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| PluginError::Storage(e.to_string()))?;
        }

        sqlx::query("UPDATE sessions SET updated_at = ? WHERE id = ?")
            .bind(now)
            .bind(session_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| PluginError::Storage(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| PluginError::Storage(e.to_string()))?;
        Ok(())
    }
}

#[derive(Debug, sqlx::FromRow)]
struct StoredMessage {
    id: String,
    role: String,
    content: String,
    #[allow(dead_code)]
    created_at: i64,
}

impl StoredMessage {
    fn to_chat_message(&self) -> ChatMessage {
        ChatMessage {
            role: ChatRole::from_str(&self.role),
            content: self.content.clone(),
        }
    }
}

#[derive(Clone)]
pub struct PluginRuntime {
    plugin_root: PathBuf,
    enabled_dir: PathBuf,
    disabled_dir: PathBuf,
    available_dir: PathBuf,
    chat_store: ChatStore,
    compaction_threshold: usize,
    default_provider: String,
}

impl PluginRuntime {
    pub async fn new() -> Result<Self, PluginError> {
        let config_dir =
            neojoplin_core::Config::config_dir().map_err(|e| PluginError::Config(e.to_string()))?;
        let data_dir =
            neojoplin_core::Config::data_dir().map_err(|e| PluginError::Config(e.to_string()))?;

        let plugin_root = config_dir.join("plugins");
        let enabled_dir = plugin_root.join("enabled");
        let disabled_dir = plugin_root.join("disabled");
        let available_dir = plugin_root.join("available");

        std::fs::create_dir_all(&enabled_dir)?;
        std::fs::create_dir_all(&disabled_dir)?;
        std::fs::create_dir_all(&available_dir)?;

        let chat_store = ChatStore::new(&data_dir.join("chat.db")).await?;
        let compaction_threshold = read_env_key("NEOJOPLIN_CHAT_COMPACTION_THRESHOLD")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(20)
            .max(10);

        Ok(Self {
            plugin_root,
            enabled_dir,
            disabled_dir,
            available_dir,
            chat_store,
            compaction_threshold,
            default_provider: "ollama".to_string(),
        })
    }

    pub fn plugin_root(&self) -> &Path {
        &self.plugin_root
    }

    pub async fn latest_session_id(&self) -> Result<Option<String>, PluginError> {
        self.chat_store.latest_session_id().await
    }

    pub async fn session_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<ChatMessage>, PluginError> {
        let rows = self.chat_store.active_messages(session_id).await?;
        Ok(rows.into_iter().map(|m| m.to_chat_message()).collect())
    }

    pub fn list_plugins(&self) -> Result<Vec<PluginInfo>, PluginError> {
        let mut all: HashMap<String, PluginInfo> = HashMap::new();
        for (dir, state) in [
            (&self.available_dir, PluginState::Available),
            (&self.disabled_dir, PluginState::Disabled),
            (&self.enabled_dir, PluginState::Enabled),
        ] {
            for manifest in discover_manifests(dir)? {
                all.insert(manifest.id.clone(), PluginInfo { manifest, state });
            }
        }

        if !all.contains_key("jarvis") {
            all.insert(
                "jarvis".to_string(),
                PluginInfo {
                    manifest: default_jarvis_manifest(),
                    state: PluginState::Available,
                },
            );
        }

        let mut plugins: Vec<_> = all.into_values().collect();
        plugins.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
        Ok(plugins)
    }

    pub fn is_enabled(&self, plugin_id: &str) -> Result<bool, PluginError> {
        for manifest in discover_manifests(&self.enabled_dir)? {
            if manifest.id == plugin_id {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn enable_plugin(&self, plugin_id: &str) -> Result<(), PluginError> {
        if self.is_enabled(plugin_id)? {
            return Ok(());
        }

        for src_dir in [&self.available_dir, &self.disabled_dir] {
            for path in discover_manifest_paths(src_dir)? {
                let content = std::fs::read_to_string(&path)?;
                if let Ok(manifest) = serde_json::from_str::<PluginManifest>(&content) {
                    if manifest.id == plugin_id {
                        let target = self.enabled_dir.join(file_name_for_manifest(&manifest));
                        std::fs::write(&target, serde_json::to_string_pretty(&manifest)?)?;
                        let _ = std::fs::remove_file(path);
                        return Ok(());
                    }
                }
            }
        }

        if plugin_id == "jarvis" {
            let manifest = default_jarvis_manifest();
            let target = self.enabled_dir.join(file_name_for_manifest(&manifest));
            std::fs::write(target, serde_json::to_string_pretty(&manifest)?)?;
            return Ok(());
        }

        Err(PluginError::NotFound(plugin_id.to_string()))
    }

    pub fn disable_plugin(&self, plugin_id: &str) -> Result<(), PluginError> {
        for path in discover_manifest_paths(&self.enabled_dir)? {
            let content = std::fs::read_to_string(&path)?;
            if let Ok(manifest) = serde_json::from_str::<PluginManifest>(&content) {
                if manifest.id == plugin_id {
                    let target = self.disabled_dir.join(file_name_for_manifest(&manifest));
                    std::fs::write(&target, serde_json::to_string_pretty(&manifest)?)?;
                    let _ = std::fs::remove_file(path);
                    return Ok(());
                }
            }
        }
        Err(PluginError::NotFound(plugin_id.to_string()))
    }

    pub async fn ask_jarvis(
        &self,
        question: &str,
        notes: &[Note],
        session_id: Option<&str>,
    ) -> Result<ChatResponse, PluginError> {
        if !self.is_enabled("jarvis")? {
            return Err(PluginError::NotEnabled("jarvis".to_string()));
        }

        let title = if question.len() > 64 {
            format!("{}...", &question[..64])
        } else {
            question.to_string()
        };
        let session_id = self.chat_store.ensure_session(session_id, &title).await?;
        self.chat_store
            .append_message(&session_id, ChatRole::User, question)
            .await?;

        let mut messages = self
            .chat_store
            .active_messages(&session_id)
            .await?
            .into_iter()
            .map(|m| m.to_chat_message())
            .collect::<Vec<_>>();

        let note_context = build_note_context(notes, question);
        let system_prompt = format!(
            "You are Jarvis inside NeoJoplin. Answer using only provided context where possible. \
If a specific note should be opened, start your response with 'OPEN_NOTE:<note_id>' on the first line, then your answer on the next lines.\n\nRelevant notes:\n{}",
            note_context
        );
        messages.insert(
            0,
            ChatMessage {
                role: ChatRole::System,
                content: system_prompt,
            },
        );

        let provider_config = ProviderConfig::load(&self.default_provider);
        let provider = build_provider(&provider_config)?;
        let raw_answer = provider.complete(&messages).await?;
        let (suggested_note_id, answer) = parse_note_open_directive(&raw_answer);

        self.chat_store
            .append_message(&session_id, ChatRole::Assistant, &answer)
            .await?;

        self.compact_if_needed(&session_id, provider.as_ref())
            .await?;

        Ok(ChatResponse {
            session_id,
            answer,
            suggested_note_id,
        })
    }

    async fn compact_if_needed(
        &self,
        session_id: &str,
        provider: &dyn ChatProvider,
    ) -> Result<(), PluginError> {
        let active = self.chat_store.active_messages(session_id).await?;
        if active.len() <= self.compaction_threshold {
            return Ok(());
        }

        let keep_count = self.compaction_threshold / 2;
        let compact_count = active.len().saturating_sub(keep_count);
        if compact_count < 2 {
            return Ok(());
        }

        let to_compact = &active[..compact_count];
        let compact_ids = to_compact
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<String>>();

        let compact_text = to_compact
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        let summary_prompt = vec![
            ChatMessage {
                role: ChatRole::System,
                content:
                    "Summarize this chat segment preserving user goals, constraints, and unresolved questions. Keep it concise."
                        .to_string(),
            },
            ChatMessage {
                role: ChatRole::User,
                content: compact_text,
            },
        ];

        let summary = provider.complete(&summary_prompt).await?;
        let summary_message = format!("Compacted summary of earlier chat:\n{}", summary);
        self.chat_store
            .compact_messages(session_id, &compact_ids, &summary_message)
            .await?;
        Ok(())
    }
}

fn parse_note_open_directive(raw: &str) -> (Option<String>, String) {
    let mut lines = raw.lines();
    if let Some(first) = lines.next() {
        if let Some(id) = first.strip_prefix("OPEN_NOTE:") {
            let note_id = id.trim();
            let remaining = lines.collect::<Vec<_>>().join("\n").trim().to_string();
            if !note_id.is_empty() {
                return (Some(note_id.to_string()), remaining);
            }
        }
    }
    (None, raw.trim().to_string())
}

fn discover_manifests(dir: &Path) -> Result<Vec<PluginManifest>, PluginError> {
    let mut manifests = Vec::new();
    for path in discover_manifest_paths(dir)? {
        let content = std::fs::read_to_string(path)?;
        if let Ok(manifest) = serde_json::from_str::<PluginManifest>(&content) {
            manifests.push(manifest);
        }
    }
    Ok(manifests)
}

fn discover_manifest_paths(dir: &Path) -> Result<Vec<PathBuf>, PluginError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "json") {
            out.push(path);
            continue;
        }
        if path.is_dir() {
            let manifest_path = path.join("manifest.json");
            if manifest_path.exists() {
                out.push(manifest_path);
            }
        }
    }
    Ok(out)
}

fn file_name_for_manifest(manifest: &PluginManifest) -> String {
    format!("{}.json", manifest.id)
}

fn default_jarvis_manifest() -> PluginManifest {
    PluginManifest {
        id: "jarvis".to_string(),
        name: "Jarvis".to_string(),
        description: "AI assistant for chatting with notes".to_string(),
        version: "0.1.0".to_string(),
    }
}

fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

fn build_note_context(notes: &[Note], question: &str) -> String {
    let scored = rank_notes_for_query(notes, question);
    if scored.is_empty() {
        return "No relevant notes found.".to_string();
    }

    scored
        .into_iter()
        .take(8)
        .map(|note| {
            let body = note.body.replace('\n', " ");
            let snippet = if body.len() > 240 {
                format!("{}...", &body[..240])
            } else {
                body
            };
            format!(
                "- id={} | title={} | updated={} | snippet={}",
                note.id, note.title, note.updated_time, snippet
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn rank_notes_for_query<'a>(notes: &'a [Note], query: &str) -> Vec<&'a Note> {
    let query_lower = query.to_lowercase();
    let tokens = query_lower
        .split_whitespace()
        .filter(|w| w.len() > 2)
        .collect::<Vec<_>>();

    let mut scored = notes
        .iter()
        .filter(|n| n.deleted_time == 0)
        .map(|note| {
            let title = note.title.to_lowercase();
            let body = note.body.to_lowercase();
            let mut score = 0i32;

            if title.contains(&query_lower) {
                score += 8;
            }
            if body.contains(&query_lower) {
                score += 4;
            }
            for token in &tokens {
                if title.contains(token) {
                    score += 3;
                }
                if body.contains(token) {
                    score += 1;
                }
            }
            (score, note)
        })
        .filter(|(score, _)| *score > 0)
        .collect::<Vec<_>>();

    scored.sort_by(|(sa, a), (sb, b)| sb.cmp(sa).then_with(|| b.updated_time.cmp(&a.updated_time)));
    scored.into_iter().map(|(_, note)| note).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_note_open_directive() {
        let (note_id, answer) = parse_note_open_directive("OPEN_NOTE:abc123\nHello world");
        assert_eq!(note_id.as_deref(), Some("abc123"));
        assert_eq!(answer, "Hello world");

        let (note_id, answer) = parse_note_open_directive("Just an answer");
        assert!(note_id.is_none());
        assert_eq!(answer, "Just an answer");
    }

    #[test]
    fn test_rank_notes_for_query_prefers_title_match() {
        let now = now_ms();
        let notes = vec![
            Note {
                title: "General".to_string(),
                body: "Talks about Pflege and support".to_string(),
                updated_time: now - 1000,
                ..Default::default()
            },
            Note {
                title: "Pflege checklist".to_string(),
                body: "Action items".to_string(),
                updated_time: now,
                ..Default::default()
            },
        ];

        let ranked = rank_notes_for_query(&notes, "pflege");
        assert_eq!(ranked.len(), 2);
        assert_eq!(ranked[0].title, "Pflege checklist");
    }

    #[tokio::test]
    async fn test_chat_store_rejects_joplin_db_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let forbidden = temp_dir.path().join("joplin.db");
        let result = ChatStore::new(&forbidden).await;
        assert!(result.is_err());
    }
}
