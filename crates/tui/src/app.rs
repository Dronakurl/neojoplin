// Main TUI application

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use joplin_domain::{now_ms, Folder, Note, NoteTag, Storage, SyncEvent, Tag};
use neojoplin_core::AutoSyncScheduler;
use neojoplin_storage::SqliteStorage;
use std::path::Path;

use crate::command_line::{complete_path_input, parse_command, CommandAction, CompletionState};
use crate::importer::{
    default_cli_database_path, default_desktop_database_path, import_database, resolve_import_path,
};
use crate::state::{
    build_folder_display_names, AppState, ContentViewMode, FocusPanel, NoteSortMode,
    NotebookSortMode, PendingDelete, TagPopupFocus, TagPopupItem,
};
use crate::ui;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct PersistedUiState {
    #[serde(default)]
    note_sort: String,
    #[serde(default)]
    notebook_sort: String,
    selected_folder_id: Option<String>,
    selected_note_id: Option<String>,
    #[serde(default)]
    all_notebooks_mode: bool,
}

type SyncTaskResult =
    Result<(String, bool, std::path::PathBuf, SyncStats), joplin_domain::DomainError>;

/// AI configuration - supports both Mistral and Ollama
#[derive(Debug, Clone)]
struct AiConfig {
    /// API base URL
    api_url: String,
    /// Model to use
    model: String,
    /// API key (for Mistral)
    api_key: Option<String>,
    /// Timeout in seconds
    timeout_seconds: u64,
}

impl Default for AiConfig {
    fn default() -> Self {
        // Load Mistral API key from ~/.env if available
        let api_key = std::fs::read_to_string(
            dirs::home_dir()
                .expect("Could not determine home directory")
                .join(".env"),
        )
        .ok()
        .and_then(|content| {
            content
                .lines()
                .find(|line| line.starts_with("MISTRAL_API_KEY="))
                .map(|line| line.trim_start_matches("MISTRAL_API_KEY=").to_string())
        });

        Self {
            api_url: "https://api.mistral.ai/v1/chat/completions".to_string(),
            model: "mistral-tiny".to_string(),
            api_key,
            timeout_seconds: 120,
        }
    }
}

/// AI client for Mistral API
#[derive(Clone)]
struct AiClient {
    config: AiConfig,
}

impl AiClient {
    fn new() -> Self {
        Self {
            config: AiConfig::default(),
        }
    }

    fn create_agent(&self) -> ureq::Agent {
        ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(self.config.timeout_seconds))
            .build()
    }

    async fn generate_response(&self, prompt: &str) -> Result<String> {
        use serde_json::json;

        let messages = vec![json!({
            "role": "user",
            "content": prompt
        })];

        let request_body = json!({
            "model": self.config.model,
            "messages": messages,
            "temperature": 0.7,
            "max_tokens": 2048,
        });

        let agent = self.create_agent();
        let mut request = agent
            .post(&self.config.api_url)
            .set("Content-Type", "application/json");

        if let Some(ref key) = self.config.api_key {
            request = request.set("Authorization", &format!("Bearer {}", key));
        }

        let response = request
            .send_json(request_body)
            .map_err(|e| anyhow::anyhow!("Failed to send request to Mistral API: {}", e))?;

        let body: serde_json::Value = response
            .into_json()
            .map_err(|e| anyhow::anyhow!("Failed to parse Mistral response: {}", e))?;

        // Mistral API response format: { "choices": [{ "message": { "content": "..." } }] }
        body["choices"]
            .as_array()
            .and_then(|choices| choices.first())
            .and_then(|choice| choice["message"]["content"].as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Invalid response format from Mistral API"))
    }
}

/// Main TUI application
pub struct App {
    state: AppState,
    storage: Arc<SqliteStorage>,
    show_help: bool,
    help_scroll: u16,
    help_search_active: bool,
    help_search_input: String,
    help_search_query: String,
    pending_motion: Option<char>,
    command_history: Vec<String>,
    command_history_index: Option<usize>,
    command_history_draft: String,
    auto_sync_scheduler: AutoSyncScheduler,
    /// Background sync task handle
    sync_task: Option<JoinHandle<SyncTaskResult>>,
    /// AI client for chat (Mistral API)
    ai_client: AiClient,
    /// Background chat task handle
    chat_task: Option<JoinHandle<Result<String>>>,
}

#[derive(Default, Debug, Clone, Copy)]
struct SyncItemBreakdown {
    notes: usize,
    folders: usize,
    tags: usize,
    note_tags: usize,
    other: usize,
}

impl SyncItemBreakdown {
    fn record(&mut self, item_type: &str) {
        match item_type {
            "note" => self.notes += 1,
            "folder" => self.folders += 1,
            "tag" => self.tags += 1,
            "note_tag" => self.note_tags += 1,
            _ => self.other += 1,
        }
    }

    fn total(&self) -> usize {
        self.notes + self.folders + self.tags + self.note_tags + self.other
    }
}

#[derive(Default, Debug, Clone, Copy)]
struct SyncStats {
    uploaded: SyncItemBreakdown,
    downloaded: SyncItemBreakdown,
}

impl SyncStats {
    fn record_upload(&mut self, item_type: &str) {
        self.uploaded.record(item_type);
    }

    fn record_download(&mut self, item_type: &str) {
        self.downloaded.record(item_type);
    }

    fn summary(self) -> String {
        format!(
            "↑ total:{} n:{} f:{} t:{} nt:{} o:{} | ↓ total:{} n:{} f:{} t:{} nt:{} o:{}",
            self.uploaded.total(),
            self.uploaded.notes,
            self.uploaded.folders,
            self.uploaded.tags,
            self.uploaded.note_tags,
            self.uploaded.other,
            self.downloaded.total(),
            self.downloaded.notes,
            self.downloaded.folders,
            self.downloaded.tags,
            self.downloaded.note_tags,
            self.downloaded.other
        )
    }
}

impl App {
    /// Create new application
    pub async fn new() -> Result<Self> {
        let storage = Arc::new(SqliteStorage::new().await?);
        let data_dir = neojoplin_core::Config::data_dir()?;

        // Load E2EE service
        let e2ee_service = Arc::new(load_e2ee_service(&data_dir).await?);

        let mut state = AppState::new();
        state.e2ee_service = Some(e2ee_service);

        // Create default sync config if it doesn't exist
        let sync_config_path = data_dir.join("sync-config.json");
        if !sync_config_path.exists() {
            let default_sync_config = serde_json::json!({
                "type": "local",
                "path": data_dir.join("sync")
            });
            tokio::fs::create_dir_all(data_dir.join("sync")).await?;
            tokio::fs::write(
                &sync_config_path,
                serde_json::to_string_pretty(&default_sync_config)?,
            )
            .await?;
        }

        // Load folders
        let mut folders = storage.list_folders().await?;

        // Create default notebook if none exist
        if folders.is_empty() {
            let default_folder = Folder {
                id: joplin_domain::joplin_id(),
                title: "My Notebook".to_string(),
                parent_id: String::new(),
                created_time: now_ms(),
                updated_time: now_ms(),
                user_created_time: 0,
                user_updated_time: 0,
                is_shared: 0,
                share_id: None,
                master_key_id: None,
                encryption_applied: 0,
                encryption_cipher_text: None,
                encryption_blob_encrypted: 0,
                icon: String::new(),
            };

            storage.create_folder(&default_folder).await?;
            folders = vec![default_folder];
            state.set_status("Created default notebook: My Notebook");
        }

        // Start in "All Notebooks" mode and load all notes
        let mut notes = storage.list_notes(None).await?;
        state.sort_folders(&mut folders, &notes);
        state.set_folders(folders);
        state.set_folder(None);
        state.sort_notes(&mut notes);
        state.set_notes(notes);

        // Load all settings (encryption, sync, and UI)
        state.settings.load_all_settings(&data_dir).await?;

        // Apply loaded UI settings to state
        state.note_filter_query = state.settings.ui.note_filter_query.clone();
        state.show_completed_todos = state.settings.ui.show_completed_todos;
        state.note_filter_mode = state.settings.ui.note_filter_mode;

        let auto_sync_scheduler = AutoSyncScheduler::new(state.settings.auto_sync.interval_seconds);

        let ai_client = AiClient::new();

        let mut app = Self {
            state,
            storage,
            show_help: false,
            help_scroll: 0,
            help_search_active: false,
            help_search_input: String::new(),
            help_search_query: String::new(),
            pending_motion: None,
            command_history: Vec::new(),
            command_history_index: None,
            command_history_draft: String::new(),
            auto_sync_scheduler,
            sync_task: None,
            ai_client,
            chat_task: None,
        };
        app.restore_ui_state(&data_dir).await?;
        app.refresh_sync_status().await?;
        Ok(app)
    }

    async fn restore_ui_state(&mut self, data_dir: &Path) -> Result<()> {
        let config_path = data_dir.join("settings.json");
        if !config_path.exists() {
            return Ok(());
        }
        let content = tokio::fs::read_to_string(&config_path).await?;
        let value: serde_json::Value = serde_json::from_str(&content)?;

        let persisted = PersistedUiState {
            note_sort: value
                .get("ui.note_sort")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            notebook_sort: value
                .get("ui.notebook_sort")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            selected_folder_id: value
                .get("ui.selected_folder_id")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string()),
            selected_note_id: value
                .get("ui.selected_note_id")
                .and_then(|v| v.as_str())
                .map(|v| v.to_string()),
            all_notebooks_mode: value
                .get("ui.all_notebooks_mode")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        };

        self.state.note_sort = match persisted.note_sort.as_str() {
            "time_desc" => NoteSortMode::TimeDesc,
            "name_asc" => NoteSortMode::NameAsc,
            _ => NoteSortMode::TimeAsc,
        };
        self.state.notebook_sort = match persisted.notebook_sort.as_str() {
            "time_desc" => NotebookSortMode::TimeDesc,
            "name_asc" => NotebookSortMode::NameAsc,
            "recent_note" => NotebookSortMode::RecentNote,
            _ => NotebookSortMode::TimeAsc,
        };

        self.refresh_lists(
            persisted.all_notebooks_mode,
            persisted.selected_folder_id,
            persisted.selected_note_id,
        )
        .await?;
        Ok(())
    }

    async fn persist_ui_state(&self) -> Result<()> {
        let data_dir = neojoplin_core::Config::data_dir()?;
        let config_path = data_dir.join("settings.json");

        let mut config = if config_path.exists() {
            let content = tokio::fs::read_to_string(&config_path).await?;
            serde_json::from_str::<serde_json::Value>(&content)?
        } else {
            serde_json::json!({
                "$schema": "https://joplinapp.org/schema/settings.json",
            })
        };

        let note_sort = match self.state.note_sort {
            NoteSortMode::TimeAsc => "time_asc",
            NoteSortMode::TimeDesc => "time_desc",
            NoteSortMode::NameAsc => "name_asc",
        };
        let notebook_sort = match self.state.notebook_sort {
            NotebookSortMode::TimeAsc => "time_asc",
            NotebookSortMode::TimeDesc => "time_desc",
            NotebookSortMode::NameAsc => "name_asc",
            NotebookSortMode::RecentNote => "recent_note",
        };

        config["ui.note_sort"] = serde_json::json!(note_sort);
        config["ui.notebook_sort"] = serde_json::json!(notebook_sort);
        config["ui.selected_folder_id"] = self
            .state
            .selected_folder_id()
            .map(|id| serde_json::json!(id))
            .unwrap_or(serde_json::Value::Null);
        config["ui.selected_note_id"] = self
            .state
            .selected_note_id()
            .map(|id| serde_json::json!(id))
            .unwrap_or(serde_json::Value::Null);
        config["ui.all_notebooks_mode"] = serde_json::json!(self.state.all_notebooks_mode);

        tokio::fs::write(&config_path, serde_json::to_string_pretty(&config)?).await?;
        Ok(())
    }

    /// Run the application
    pub async fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode().context("Failed to enable raw mode")?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .context("Failed to setup terminal")?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

        // Run main loop
        let res = self.run_main_loop(&mut terminal).await;

        if let Err(err) = self.persist_ui_state().await {
            tracing::warn!("Failed to persist UI state: {}", err);
        }

        // Restore terminal
        disable_raw_mode().context("Failed to disable raw mode")?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )
        .context("Failed to restore terminal")?;
        terminal.show_cursor().context("Failed to show cursor")?;

        res
    }

    /// Main event loop
    async fn run_main_loop<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> Result<()>
    where
        B::Error: std::error::Error + Send + Sync + 'static,
    {
        loop {
            self.run_auto_sync_if_due().await?;
            self.check_sync_task().await?;
            self.check_chat_task().await?;
            self.state
                .settings
                .set_next_auto_sync_in_seconds(self.auto_sync_scheduler.seconds_until_next_run());

            // Render UI
            terminal.draw(|f| {
                if self.show_help {
                    ui::render_help(
                        f,
                        self.help_scroll,
                        &self.state,
                        Some(&self.help_search_query),
                        if self.help_search_active {
                            Some(&self.help_search_input)
                        } else {
                            None
                        },
                    );
                } else if self.state.show_quit_confirmation {
                    ui::render_quit_confirmation(f, &self.state);
                } else if self.state.pending_delete.is_some() {
                    ui::render_delete_confirmation(f, &self.state);
                } else if self.state.show_error_dialog {
                    ui::render_error_dialog(f, &self.state);
                } else if self.state.show_settings {
                    ui::render_settings(f, &self.state);
                } else if self.state.tag_popup.visible {
                    ui::render_tag_popup(f, &self.state);
                } else if self.state.show_rename_prompt {
                    ui::render_rename_prompt(f, &self.state);
                } else if self.state.show_sort_popup {
                    ui::render_sort_popup(f, &self.state);
                } else {
                    ui::render_ui(f, &self.state);
                }
            })?;

            // Handle events
            if event::poll(Duration::from_millis(100))? {
                if let event::Event::Key(key) = event::read()? {
                    if self.handle_key_event(key, terminal).await? {
                        break; // Exit requested
                    }
                }
            }
        }
        Ok(())
    }

    /// Handle keyboard events
    async fn handle_key_event<B: ratatui::backend::Backend>(
        &mut self,
        key: KeyEvent,
        terminal: &mut Terminal<B>,
    ) -> Result<bool>
    where
        B::Error: std::error::Error + Send + Sync + 'static,
    {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Ok(true);
        }

        // Handle chat overlay input
        if self.state.chat_overlay.visible {
            // Check if there's a note to navigate to
            if let Some(note_id) = self.state.chat_overlay.navigate_to_note_id.take() {
                self.state.select_note_by_id(&note_id);
            }

            match key.code {
                // P toggles chat overlay (closes it when open)
                KeyCode::Char('P') | KeyCode::Char('p') => {
                    self.state.close_chat_overlay();
                    return Ok(false);
                }
                KeyCode::Esc => {
                    self.state.close_chat_overlay();
                    return Ok(false);
                }
                KeyCode::Enter => {
                    if !self.state.chat_overlay.input.trim().is_empty()
                        && !self.state.chat_overlay.pending
                    {
                        let question = self.state.chat_overlay.input.trim().to_string();
                        self.state.chat_add_message("You", question.clone());
                        self.state.chat_overlay.input.clear();
                        self.state.chat_overlay.pending = true;

                        // Call AI backend - spawn async task
                        self.spawn_chat_response_task(question).await;
                    }
                    return Ok(false);
                }
                KeyCode::Backspace => {
                    self.state.chat_overlay.input.pop();
                    return Ok(false);
                }
                KeyCode::Char(c) => {
                    self.state.chat_overlay.input.push(c);
                    return Ok(false);
                }
                // Pass Tab through to allow switching to preview panel
                KeyCode::Tab => {}
                _ => {}
            }
        }

        // Handle global shortcuts
        if self.state.show_quit_confirmation {
            // Confirm quit
            if key.code == KeyCode::Char('q')
                || key.code == KeyCode::Char('y')
                || key.code == KeyCode::Enter
            {
                return Ok(true); // Exit
            } else {
                self.state.hide_quit();
            }
            return Ok(false);
        }

        // Handle error dialog
        if self.state.show_error_dialog {
            match key.code {
                KeyCode::Enter | KeyCode::Char('q') | KeyCode::Esc => {
                    self.state.hide_error();
                }
                _ => {}
            }
            return Ok(false);
        }

        if self.state.pending_delete.is_some() {
            return self.handle_pending_delete_key_event(key).await;
        }

        // Handle rename prompt
        if self.state.show_rename_prompt {
            match key.code {
                KeyCode::Char(c) => {
                    self.state.add_rename_char(c);
                }
                KeyCode::Backspace => {
                    self.state.remove_rename_char();
                }
                KeyCode::Enter => {
                    if !self.state.rename_input.is_empty() {
                        self.rename_item().await?;
                    }
                    self.state.hide_rename_prompt();
                }
                KeyCode::Esc => {
                    self.state.hide_rename_prompt();
                }
                _ => {}
            }
            return Ok(false);
        }

        if self.state.show_filter_prompt {
            return self.handle_filter_prompt_key_event(key).await;
        }

        if self.state.tag_popup.visible {
            return self.handle_tag_popup_key_event(key).await;
        }

        if self.state.command_prompt.visible {
            return self.handle_command_prompt_key_event(key).await;
        }

        // Handle help popup
        if self.show_help {
            if self.help_search_active {
                match key.code {
                    KeyCode::Esc => {
                        self.help_search_active = false;
                        self.help_search_input.clear();
                    }
                    KeyCode::Enter => {
                        self.help_search_query = self.help_search_input.clone();
                        self.help_search_active = false;
                        self.apply_help_search();
                    }
                    KeyCode::Backspace => {
                        self.help_search_input.pop();
                        self.help_search_query = self.help_search_input.clone();
                        self.apply_help_search();
                    }
                    KeyCode::Char(c)
                        if !key.modifiers.contains(KeyModifiers::CONTROL)
                            && !key.modifiers.contains(KeyModifiers::ALT) =>
                    {
                        self.help_search_input.push(c);
                        self.help_search_query = self.help_search_input.clone();
                        self.apply_help_search();
                    }
                    _ => {}
                }
                return Ok(false);
            }

            match key.code {
                KeyCode::Char('/') => {
                    self.help_search_active = true;
                    self.help_search_input = self.help_search_query.clone();
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.help_scroll = self.help_scroll.saturating_add(1);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.help_scroll = self.help_scroll.saturating_sub(1);
                }
                KeyCode::Char('q') => {
                    self.show_help = false;
                    self.help_scroll = 0;
                    self.help_search_active = false;
                    self.help_search_input.clear();
                }
                _ => {
                    // Ignore all other keys in help mode
                }
            }
            return Ok(false);
        }

        if self.state.show_sort_popup {
            return self.handle_sort_popup_key_event(key).await;
        }

        // Handle settings popup
        if self.state.show_settings {
            return self.handle_settings_key_event(key).await;
        }

        if self.pending_motion == Some('g') {
            match key.code {
                KeyCode::Char('g') => {
                    self.pending_motion = None;
                    self.jump_to_list_boundary(true).await?;
                    return Ok(false);
                }
                KeyCode::Char('e') => {
                    self.pending_motion = None;
                    self.jump_to_list_boundary(false).await?;
                    return Ok(false);
                }
                _ => {
                    self.pending_motion = None;
                }
            }
        }

        // When chat overlay is visible, block all panel navigation
        // This prevents Tab, arrow keys, etc. from interacting with underlying panels
        if self.state.chat_overlay.visible {
            return Ok(false);
        }

        // Handle vim-style navigation and actions
        match key.code {
            // Escape - clear active filters
            KeyCode::Esc if self.state.has_active_filter(self.state.focus) => {
                match self.state.focus {
                    FocusPanel::Notebooks => self.state.notebook_filter_query.clear(),
                    FocusPanel::Notes | FocusPanel::Content => self.state.clear_note_filters(),
                }
                self.refresh_current_lists().await?;
            }

            // Quit
            KeyCode::Char('q') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    return Ok(true); // Ctrl+Q always quits
                } else {
                    self.state.show_quit();
                }
            }
            // Help
            KeyCode::Char('?') => {
                self.show_help = true;
            }

            KeyCode::Char(':') => {
                self.open_command_prompt(String::new());
            }

            KeyCode::Char(',') => {
                if matches!(self.state.focus, FocusPanel::Notebooks | FocusPanel::Notes) {
                    self.state.open_sort_popup();
                } else {
                    self.state
                        .set_status("Focus notebooks or notes to change sorting");
                }
            }

            KeyCode::Char('f') => {
                if matches!(
                    self.state.focus,
                    FocusPanel::Notebooks | FocusPanel::Notes | FocusPanel::Content
                ) {
                    let preview_focus = self.state.focus == FocusPanel::Content;
                    self.state.open_filter_prompt(preview_focus);
                    if preview_focus {
                        self.state
                            .set_status("Preview filter: enter query and press Enter");
                    }
                } else {
                    self.state
                        .set_status("Focus notebooks or notes to filter the current list");
                }
            }

            KeyCode::Char('F') => {
                if matches!(
                    self.state.focus,
                    FocusPanel::Notebooks | FocusPanel::Notes | FocusPanel::Content
                ) {
                    let notes_full_text =
                        matches!(self.state.focus, FocusPanel::Notes | FocusPanel::Content);
                    self.state.open_filter_prompt(notes_full_text);
                    if notes_full_text {
                        self.state
                            .set_status("Full-text filter: enter query and press Enter");
                    }
                } else {
                    self.state
                        .set_status("Focus notebooks or notes to filter the current list");
                }
            }

            KeyCode::Char('c') => {
                self.state.toggle_show_completed_todos();
                self.refresh_current_lists().await?;
                if self.state.show_completed_todos {
                    self.state
                        .set_status("Showing completed todos in the notes list");
                } else {
                    self.state
                        .set_status("Hiding completed todos in the notes list");
                }
                let data_dir = neojoplin_core::Config::data_dir()?;
                self.state.settings.save_all_settings(&data_dir).await?;
            }

            KeyCode::Char('C') => {
                self.state.toggle_completed_only_filter();
                self.refresh_current_lists().await?;
                if self.state.completed_only_filter {
                    self.state
                        .set_status("Filter enabled: showing completed todos only");
                } else {
                    self.state.set_status("Completed-todo-only filter disabled");
                }
            }

            // Sync
            KeyCode::Char('s') => {
                // s - Sync
                self.sync().await?;
            }

            // Settings
            KeyCode::Char('S') => {
                self.refresh_sync_status().await?;
                self.state.toggle_settings();
            }

            // AI Chat overlay
            KeyCode::Char('P') => {
                self.state.open_chat_overlay();
            }

            // Panel navigation
            KeyCode::Tab => {
                self.state.next_panel();
            }
            KeyCode::BackTab => {
                self.state.prev_panel();
            }

            // Vim-style horizontal panel navigation
            KeyCode::Char('h') | KeyCode::Left => {
                // Move left (previous panel)
                self.state.prev_panel();
            }
            KeyCode::Char('l') | KeyCode::Right => {
                // Move right (next panel)
                self.state.next_panel();
            }

            // Vim-style navigation
            KeyCode::Char('j') | KeyCode::Down => {
                if self.state.focus == FocusPanel::Content {
                    if self.state.content_view_mode == ContentViewMode::VersionList {
                        self.state.move_version_selection(1);
                    } else {
                        self.state.content_scroll_offset =
                            self.state.content_scroll_offset.saturating_add(1);
                    }
                } else {
                    let folder_changed = self.state.move_selection(1);
                    if folder_changed {
                        self.reload_notes().await?;
                    }
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.state.focus == FocusPanel::Content {
                    if self.state.content_view_mode == ContentViewMode::VersionList {
                        self.state.move_version_selection(-1);
                    } else {
                        self.state.content_scroll_offset =
                            self.state.content_scroll_offset.saturating_sub(1);
                    }
                } else {
                    let folder_changed = self.state.move_selection(-1);
                    if folder_changed {
                        self.reload_notes().await?;
                    }
                }
            }

            KeyCode::Char('g') => {
                self.pending_motion = Some('g');
            }

            KeyCode::Char('G') => {
                self.jump_to_list_boundary(false).await?;
            }

            // Enter - edit selected note, or switch to notes panel from notebooks
            KeyCode::Enter => {
                if self.state.focus == FocusPanel::Notes {
                    if let Some(note) = self.state.selected_note() {
                        let note_clone = note.clone();
                        self.edit_note(&note_clone, terminal).await?;
                    }
                } else if self.state.focus == FocusPanel::Notebooks {
                    // Switch to notes panel when Enter is pressed on notebooks
                    self.state.next_panel(); // Switch from Notebooks to Notes
                    self.reload_notes().await?;
                } else if self.state.focus == FocusPanel::Content
                    && self.state.content_view_mode == ContentViewMode::VersionList
                {
                    self.preview_selected_version().await?;
                }
            }

            KeyCode::Char('e') if self.state.focus == FocusPanel::Content => {
                if let Some(note) = self.state.selected_note() {
                    let note_clone = note.clone();
                    self.edit_note(&note_clone, terminal).await?;
                } else {
                    self.state.set_status("Select a note to edit");
                }
            }

            // New item (context-aware: notebook or note based on focus)
            KeyCode::Char('n') => match self.state.focus {
                FocusPanel::Notebooks => {
                    self.create_notebook().await?;
                }
                FocusPanel::Notes => {
                    self.create_note().await?;
                }
                FocusPanel::Content => {
                    self.state
                        .set_status("Focus notebooks or notes panel to create new items");
                }
            },

            // Delete
            KeyCode::Char('d') => {
                self.request_delete_selected().await?;
            }

            // Immediate note delete (hidden from ribbon)
            KeyCode::Char('D') => {
                self.delete_selected_note_immediately().await?;
            }

            // Move shortcut (m) - visible via ? help but not in ribbon
            KeyCode::Char('m') => match self.state.focus {
                FocusPanel::Notes if self.state.selected_note().is_some() => {
                    self.open_command_prompt("move ".to_string());
                }
                FocusPanel::Notebooks if self.state.selected_folder().is_some() => {
                    self.open_command_prompt("move ".to_string());
                }
                FocusPanel::Notes => {
                    self.state
                        .set_status("Select a note before choosing a destination notebook");
                }
                FocusPanel::Notebooks => {
                    self.state
                        .set_status("Select a notebook before choosing its parent notebook");
                }
                FocusPanel::Content => {
                    self.state
                        .set_status("Focus notes or notebooks before using move");
                }
            },

            KeyCode::Char('a') => {
                if matches!(self.state.focus, FocusPanel::Notes | FocusPanel::Content) {
                    self.open_tag_popup().await?;
                } else {
                    self.state
                        .set_status("Focus a note before editing its tags");
                }
            }

            // Toggle todo completion (space bar, like most task managers)
            KeyCode::Char(' ') if self.state.focus == FocusPanel::Notes => {
                self.toggle_todo().await?;
            }

            // Create todo
            KeyCode::Char('t') => {
                self.create_todo().await?;
            }

            // Convert note type
            KeyCode::Char('T') => {
                self.convert_note_type().await?;
            }

            // Rename
            KeyCode::Char('r') => {
                if self.state.focus == FocusPanel::Content
                    && self.state.content_view_mode != ContentViewMode::Note
                {
                    self.restore_selected_version().await?;
                } else if self.state.focus == FocusPanel::Notes {
                    if let Some(note) = self.state.selected_note() {
                        self.state.rename_input = note.title.clone();
                        self.state.show_rename_prompt();
                    }
                } else if self.state.focus == FocusPanel::Notebooks {
                    if let Some(folder) = self.state.selected_folder() {
                        self.state.rename_input = folder.title.clone();
                        self.state.show_rename_prompt();
                    }
                }
            }

            KeyCode::Char('v') if self.state.focus == FocusPanel::Content => {
                self.toggle_version_view().await?;
            }

            // Restore from trash (R key)
            KeyCode::Char('R') => {
                self.restore_selected_note().await?;
            }

            _ => {}
        }

        Ok(false)
    }

    /// Sync with WebDAV server - spawns a background task
    async fn sync(&mut self) -> Result<()> {
        self.start_sync(false).await
    }

    /// Start a sync operation in the background
    async fn start_sync(&mut self, automatic: bool) -> Result<()> {
        // Check if sync is already in progress
        if self.state.sync_in_progress {
            self.state.set_status("Sync already in progress...");
            return Ok(());
        }

        let data_dir = neojoplin_core::Config::data_dir()?;

        // Use the loaded settings (from settings.json) to get the sync target
        let sync_settings = &self.state.settings.sync;
        let target = match sync_settings
            .current_target_index
            .and_then(|i| sync_settings.targets.get(i))
        {
            Some(t) => t.clone(),
            None => {
                if !automatic {
                    self.state.set_status(
                        "Sync not configured. Go to Settings (s) → Sync tab to add a WebDAV target.",
                    );
                }
                self.auto_sync_scheduler.reset();
                return Ok(());
            }
        };

        if target.url.is_empty() {
            if !automatic {
                self.state
                    .set_status("Sync URL is empty. Go to Settings (s) → Sync tab to configure.");
            }
            self.auto_sync_scheduler.reset();
            return Ok(());
        }

        // Split the full URL (e.g. http://localhost:8080/webdav/shared) into
        // base_url (http://localhost:8080/webdav) + remote_path (/shared).
        let (base_url, remote_path) = split_webdav_url(&target.url);

        if !automatic {
            self.state
                .set_status(&format!("Starting sync to {}{}...", base_url, remote_path));
        }

        use joplin_sync::{ReqwestWebDavClient, SyncEngine, WebDavConfig};

        let webdav_config = WebDavConfig {
            base_url: base_url.clone(),
            username: target.username.clone(),
            password: target.password.clone(),
        };

        // Load E2EE service from .env / encryption.json (same logic as CLI)
        let e2ee = load_e2ee_service(&data_dir).await?;
        let encryption_enabled = e2ee.is_enabled();
        let use_e2ee = encryption_enabled || e2ee.get_master_password().is_some();

        // Set sync in progress flag
        self.state.set_sync_in_progress(true);
        self.auto_sync_scheduler.reset();

        // Clone all needed data for the background task
        let storage = self.storage.clone();
        let webdav_config_clone = webdav_config.clone();
        let remote_path_clone = remote_path.clone();
        let e2ee_clone = if use_e2ee { Some(e2ee) } else { None };
        let encryption_enabled_clone = encryption_enabled;
        let target_name = target.name.clone();
        let data_dir_clone = data_dir.clone();

        // Spawn the sync in a background task
        let task = tokio::spawn(async move {
            let webdav_client = Arc::new(ReqwestWebDavClient::new(webdav_config_clone)?);
            let (event_tx, mut event_rx) = mpsc::unbounded_channel();

            let mut sync_engine = SyncEngine::new(storage, webdav_client, event_tx)
                .with_remote_path(remote_path_clone);

            if let Some(e2ee) = e2ee_clone {
                sync_engine = sync_engine.with_e2ee(e2ee);
            }

            let mut stats = SyncStats::default();
            let mut sync_future = Box::pin(sync_engine.sync());
            let sync_result = loop {
                tokio::select! {
                    result = &mut sync_future => break result,
                    maybe_event = event_rx.recv() => {
                        let Some(event) = maybe_event else { continue };
                        match event {
                            SyncEvent::ItemUploadComplete { item_type, .. } => {
                                stats.record_upload(&item_type);
                            }
                            SyncEvent::ItemDownloadComplete { item_type, .. } => {
                                stats.record_download(&item_type);
                            }
                            _ => {}
                        }
                    }
                }
            };

            while let Ok(event) = event_rx.try_recv() {
                match event {
                    SyncEvent::ItemUploadComplete { item_type, .. } => {
                        stats.record_upload(&item_type);
                    }
                    SyncEvent::ItemDownloadComplete { item_type, .. } => {
                        stats.record_download(&item_type);
                    }
                    _ => {}
                }
            }

            sync_result.map(|_| (target_name, encryption_enabled_clone, data_dir_clone, stats))
        });

        self.sync_task = Some(task);

        Ok(())
    }

    /// Check if a background sync task has completed and handle the result
    async fn check_sync_task(&mut self) -> Result<()> {
        if let Some(task) = self.sync_task.take() {
            if task.is_finished() {
                // Task completed, get the result
                match task.await {
                    Ok(Ok((target_name, encryption_enabled, data_dir, stats))) => {
                        // Record successful sync
                        self.state.settings.record_sync_result(
                            target_name,
                            true,
                            None,
                            encryption_enabled,
                        );
                        self.state.settings.save_sync_status(&data_dir).await?;
                        self.state
                            .set_status(&format!("✓ Sync completed ({})", stats.summary()));
                        let selected_folder_id =
                            self.state.selected_folder_id().map(str::to_string);
                        let selected_note_id = self.state.selected_note_id().map(str::to_string);
                        let all_notebooks_mode = self.state.all_notebooks_mode;
                        self.refresh_lists(
                            all_notebooks_mode,
                            selected_folder_id,
                            selected_note_id,
                        )
                        .await?;
                        self.refresh_sync_status().await?;
                    }
                    Ok(Err(e)) => {
                        let error_message = e.to_string();
                        // We don't have target info here, so just show error
                        self.state
                            .show_error(&format!("Sync failed: {}", error_message));
                        self.refresh_sync_status().await?;
                    }
                    Err(join_err) => {
                        self.state
                            .show_error(&format!("Sync task error: {}", join_err));
                    }
                }
                self.state.set_sync_in_progress(false);
            } else {
                // Task still running, put it back
                self.sync_task = Some(task);
            }
        }
        Ok(())
    }

    /// Check if a background chat task has completed and handle the result
    async fn check_chat_task(&mut self) -> Result<()> {
        if let Some(task) = self.chat_task.take() {
            if task.is_finished() {
                match task.await {
                    Ok(Ok(response)) => {
                        self.state.chat_overlay.pending = false;
                        self.state.chat_add_message("Jarvis", response);
                    }
                    Ok(Err(e)) => {
                        self.state.chat_overlay.pending = false;
                        self.state
                            .chat_add_message("System", format!("Error: {}", e));
                    }
                    Err(join_err) => {
                        self.state.chat_overlay.pending = false;
                        self.state
                            .chat_add_message("System", format!("Chat task failed: {}", join_err));
                    }
                }
            } else {
                // Task still running, put it back
                self.chat_task = Some(task);
            }
        }
        Ok(())
    }

    /// Spawn a background task to get a chat response from Mistral API
    async fn spawn_chat_response_task(&mut self, prompt: String) {
        let client = self.ai_client.clone();
        let storage = self.storage.clone();
        let selected_note = self.state.selected_note().cloned();
        let selected_folder = self
            .state
            .selected_folder
            .and_then(|idx| self.state.folders.get(idx).cloned());

        // Search for relevant notes using FTS
        let matching_notes = storage
            .search_notes(&prompt, Some(5))
            .await
            .unwrap_or_default();

        // Build context from selected note (full content)
        let selected_context = if let Some(note) = &selected_note {
            format!(
                "Current note:\nTitle: {}\nContent:\n{}",
                note.title, note.body
            )
        } else {
            String::new()
        };

        // Build context from selected notebook
        let folder_context = if let Some(folder) = &selected_folder {
            format!("Current notebook: {}", folder.title)
        } else {
            String::new()
        };

        // Build context from matching notes
        let matching_context: String = matching_notes
            .iter()
            .filter(|note| {
                // Skip selected note (already included above)
                selected_note.as_ref().is_none_or(|s| s.id != note.id)
            })
            .map(|note| format!("--- Note: {} ---\n{}", note.title, note.body))
            .collect::<Vec<_>>()
            .join("\n\n");

        // Combine all context with prompt
        let full_prompt = format!(
            "You are a helpful AI assistant with access to the user's notes and notebooks. \
Use the information below to provide accurate, context-aware answers. \
If the user asks about specific information (like IBAN, account numbers, etc.), search for it in the notes.\n\n{}{}{}\n\nUser question: {}",
            folder_context,
            if !selected_context.is_empty() { format!("\n\n{}", selected_context) } else { String::new() },
            if !matching_context.is_empty() { format!("\n\nRelevant notes:\n{}", matching_context) } else { String::new() },
            prompt
        );

        let task = tokio::spawn(async move { client.generate_response(&full_prompt).await });
        self.chat_task = Some(task);
    }

    async fn refresh_sync_status(&mut self) -> Result<()> {
        let data_dir = neojoplin_core::Config::data_dir()?;
        self.state
            .settings
            .load_encryption_settings(&data_dir)
            .await?;
        self.state.settings.load_sync_status(&data_dir).await?;
        let conflict_count = self
            .storage
            .list_notes(None)
            .await?
            .into_iter()
            .filter(|note| note.is_conflict != 0)
            .count();
        self.state.settings.update_runtime_status(conflict_count);
        self.state
            .settings
            .set_next_auto_sync_in_seconds(self.auto_sync_scheduler.seconds_until_next_run());
        Ok(())
    }

    async fn run_auto_sync_if_due(&mut self) -> Result<()> {
        if !self.auto_sync_scheduler.is_due() || !self.can_auto_sync_now() {
            return Ok(());
        }

        if self.auto_sync_scheduler.consume_due() {
            self.start_sync(true).await?;
        }

        Ok(())
    }

    fn can_auto_sync_now(&self) -> bool {
        self.auto_sync_scheduler.is_enabled()
            && !self.show_help
            && !self.state.show_settings
            && !self.state.show_rename_prompt
            && !self.state.show_filter_prompt
            && !self.state.show_sort_popup
            && !self.state.tag_popup.visible
            && !self.state.command_prompt.visible
            && !self.state.show_error_dialog
            && !self.state.show_quit_confirmation
            && self.state.pending_delete.is_none()
            && !self.state.sync_in_progress
    }

    /// Edit note in external editor
    async fn edit_note<B: ratatui::backend::Backend>(
        &mut self,
        note: &Note,
        terminal: &mut Terminal<B>,
    ) -> Result<()>
    where
        B::Error: std::error::Error + Send + Sync + 'static,
    {
        use neojoplin_core::Editor;

        self.state
            .set_status(&format!("Opening editor for: {}", note.title));

        // Exit raw mode and alternate screen so editor can work properly
        disable_raw_mode().context("Failed to disable raw mode")?;
        let mut stdout = std::io::stdout();
        execute!(stdout, LeaveAlternateScreen).context("Failed to leave alternate screen")?;

        let editor_result = async {
            let editor =
                Editor::new().map_err(|e| anyhow::anyhow!("Failed to initialize editor: {}", e))?;

            // Open editor with title as first line so the user can rename by editing it.
            // Body follows after a blank line (same convention as Joplin's desktop editor).
            let full_content = if note.body.is_empty() {
                format!("{}\n", note.title)
            } else {
                format!("{}\n\n{}", note.title, note.body)
            };
            editor
                .edit(&full_content, &note.title)
                .await
                .map_err(|e| anyhow::anyhow!("Editor failed: {}", e))
        }
        .await;

        // Restore terminal for TUI
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .context("Failed to re-enter alternate screen")?;
        enable_raw_mode().context("Failed to re-enable raw mode")?;

        let full_content = match editor_result {
            Ok(content) => content,
            Err(error) => {
                terminal.clear()?;
                self.state.show_error(&error.to_string());
                return Ok(());
            }
        };

        // Force a complete terminal redraw to ensure TUI is properly visible
        terminal.clear()?;

        // Reconstruct title + body: first line → title, rest → body
        let mut lines = full_content.lines();
        let new_title = lines.next().unwrap_or("").trim().to_string();
        // Skip a single blank separator line if present
        let rest: String = {
            let remaining: Vec<&str> = lines.collect();
            // Drop a leading blank line that acts as separator
            let skip = if remaining
                .first()
                .map(|l| l.trim().is_empty())
                .unwrap_or(false)
            {
                1
            } else {
                0
            };
            remaining[skip..].join("\n")
        };
        let new_body = rest.trim_end().to_string();

        let updated_title = if new_title.is_empty() {
            note.title.clone()
        } else {
            new_title
        };

        // Update note if anything changed
        if updated_title != note.title || new_body != note.body {
            let mut updated_note = note.clone();
            updated_note.title = updated_title;
            updated_note.body = new_body;

            updated_note.updated_time = now_ms();

            self.storage.update_note(&updated_note).await?;
            if updated_note.title != note.title {
                self.state.clear_new_note_marker_if(&updated_note.id);
            }

            let all_notebooks_mode = self.state.all_notebooks_mode;
            let selected_folder_id = self.state.selected_folder_id().map(str::to_string);
            let selected_note_id = Some(updated_note.id.clone());
            self.refresh_lists(all_notebooks_mode, selected_folder_id, selected_note_id)
                .await?;
            self.state
                .set_status(&format!("Updated: {}", updated_note.title));
        } else {
            self.state.set_status("No changes made to note");
        }

        Ok(())
    }

    /// Create a new note
    async fn create_note(&mut self) -> Result<()> {
        // Determine parent folder for the new note
        let parent_id = if self.state.orphan_mode {
            // In orphan mode, create note without a parent (orphaned)
            String::new()
        } else if self.state.all_notebooks_mode {
            // In "All Notebooks" mode, use the first available notebook
            if let Some(folder) = self.state.folders.first() {
                folder.id.clone()
            } else {
                // No notebooks exist, create one first
                self.create_notebook().await?;
                if let Some(folder) = self.state.folders.first() {
                    folder.id.clone()
                } else {
                    return Err(anyhow::anyhow!("Failed to create notebook for note"));
                }
            }
        } else if let Some(folder) = self.state.selected_folder() {
            folder.id.clone()
        } else {
            return Err(anyhow::anyhow!("No notebook selected"));
        };

        self.state.set_status("Creating new note...");

        // For simplicity, create a note with a default title
        let title = format!("New Note {}", &joplin_domain::joplin_id()[..8]);
        let note = Note {
            id: joplin_domain::joplin_id(),
            title: title.clone(),
            body: String::new(),
            parent_id: parent_id.clone(),
            created_time: now_ms(),
            updated_time: now_ms(),
            user_created_time: 0,
            user_updated_time: 0,
            is_shared: 0,
            share_id: None,
            master_key_id: None,
            encryption_applied: 0,
            encryption_cipher_text: None,
            is_conflict: 0,
            is_todo: 0,
            todo_completed: 0,
            todo_due: 0,
            source: String::new(),
            source_application: String::new(),
            order: 0,
            latitude: 0,
            longitude: 0,
            altitude: 0,
            author: String::new(),
            source_url: String::new(),
            application_data: String::new(),
            markup_language: 1,
            encryption_blob_encrypted: 0,
            conflict_original_id: String::new(),
            deleted_time: 0,
        };

        self.storage.create_note(&note).await?;

        self.state.mark_new_note(note.id.clone());
        self.state.focus = FocusPanel::Notes;
        self.refresh_lists(
            self.state.all_notebooks_mode,
            self.state.selected_folder_id().map(str::to_string),
            Some(note.id.clone()),
        )
        .await?;

        self.state
            .set_status(&format!("Created note: {} - press r to rename it", title));
        Ok(())
    }

    /// Create a new todo
    async fn create_todo(&mut self) -> Result<()> {
        let parent_id = if self.state.orphan_mode {
            // In orphan mode, create todo without a parent (orphaned)
            String::new()
        } else if self.state.all_notebooks_mode {
            if let Some(folder) = self.state.folders.first() {
                folder.id.clone()
            } else {
                self.create_notebook().await?;
                if let Some(folder) = self.state.folders.first() {
                    folder.id.clone()
                } else {
                    return Err(anyhow::anyhow!("Failed to create notebook for todo"));
                }
            }
        } else if let Some(folder) = self.state.selected_folder() {
            folder.id.clone()
        } else {
            return Err(anyhow::anyhow!("No notebook selected"));
        };

        self.state.set_status("Creating new todo...");

        let title = format!("New Todo {}", &joplin_domain::joplin_id()[..8]);
        let note = Note {
            id: joplin_domain::joplin_id(),
            title: title.clone(),
            body: String::new(),
            parent_id: parent_id.clone(),
            created_time: now_ms(),
            updated_time: now_ms(),
            user_created_time: 0,
            user_updated_time: 0,
            is_shared: 0,
            share_id: None,
            master_key_id: None,
            encryption_applied: 0,
            encryption_cipher_text: None,
            is_conflict: 0,
            is_todo: 1,
            todo_completed: 0,
            todo_due: 0,
            source: String::new(),
            source_application: String::new(),
            order: 0,
            latitude: 0,
            longitude: 0,
            altitude: 0,
            author: String::new(),
            source_url: String::new(),
            application_data: String::new(),
            markup_language: 1,
            encryption_blob_encrypted: 0,
            conflict_original_id: String::new(),
            deleted_time: 0,
        };

        self.storage.create_note(&note).await?;
        self.state.mark_new_note(note.id.clone());
        self.state.focus = FocusPanel::Notes;
        self.refresh_lists(
            self.state.all_notebooks_mode,
            self.state.selected_folder_id().map(str::to_string),
            Some(note.id.clone()),
        )
        .await?;

        self.state
            .set_status(&format!("Created todo: {} - press r to rename it", title));
        Ok(())
    }

    /// Toggle todo completion status
    async fn toggle_todo(&mut self) -> Result<()> {
        if self.state.focus != FocusPanel::Notes {
            self.state
                .set_status("Select a todo in the notes panel first");
            return Ok(());
        }

        if let Some(note) = self.state.selected_note() {
            if note.is_todo != 1 {
                self.state.set_status("Selected item is not a todo");
                return Ok(());
            }

            let mut updated = note.clone();
            if updated.todo_completed > 0 {
                updated.todo_completed = 0;
                self.state
                    .set_status(&format!("󰄱 Uncompleted: {}", updated.title));
            } else {
                updated.todo_completed = now_ms();
                self.state
                    .set_status(&format!("󰄲 Completed: {}", updated.title));
            }
            updated.updated_time = now_ms();
            self.storage.update_note(&updated).await?;
            self.refresh_lists(
                self.state.all_notebooks_mode,
                self.state.selected_folder_id().map(str::to_string),
                Some(updated.id.clone()),
            )
            .await?;
        }

        Ok(())
    }

    /// Create a new notebook
    async fn create_notebook(&mut self) -> Result<()> {
        self.state.set_status("Creating new notebook...");

        let title = "New notebook".to_string();
        let folder = Folder {
            id: joplin_domain::joplin_id(),
            title: title.clone(),
            parent_id: String::new(), // Root notebook
            created_time: now_ms(),
            updated_time: now_ms(),
            user_created_time: 0,
            user_updated_time: 0,
            is_shared: 0,
            share_id: None,
            master_key_id: None,
            encryption_applied: 0,
            encryption_cipher_text: None,
            encryption_blob_encrypted: 0,
            icon: String::new(),
        };

        self.storage.create_folder(&folder).await?;
        self.state.mark_new_folder(folder.id.clone());
        self.state.focus = FocusPanel::Notebooks;
        self.refresh_lists(false, Some(folder.id.clone()), None)
            .await?;
        self.state.set_status(&format!(
            "Created notebook: {} - press r to rename it",
            title
        ));

        Ok(())
    }

    /// Delete selected item (note or notebook)
    async fn request_delete_selected(&mut self) -> Result<()> {
        match self.state.focus {
            FocusPanel::Notes => {
                if let Some(note) = self.state.selected_note() {
                    let permanent = self.state.trash_mode;
                    self.state.confirm_delete(PendingDelete::Note {
                        id: note.id.clone(),
                        title: note.title.clone(),
                        permanent,
                    });
                }
            }
            FocusPanel::Notebooks => {
                if let Some(folder) = self.state.selected_folder().cloned() {
                    let note_count = self.storage.list_notes(Some(&folder.id)).await?.len();
                    self.state.confirm_delete(PendingDelete::Notebook {
                        id: folder.id.clone(),
                        title: folder.title.clone(),
                        note_count,
                    });
                }
            }
            FocusPanel::Content => {
                if let Some(note) = self.state.selected_note() {
                    let permanent = self.state.trash_mode;
                    self.state.confirm_delete(PendingDelete::Note {
                        id: note.id.clone(),
                        title: note.title.clone(),
                        permanent,
                    });
                } else {
                    self.state.set_status("Select a note before deleting");
                }
            }
        }

        Ok(())
    }

    /// Reload notes for currently selected notebook
    async fn reload_notes(&mut self) -> Result<()> {
        if self.state.trash_mode {
            return self
                .refresh_trash_list(self.state.selected_note_id().map(str::to_string))
                .await;
        }
        self.refresh_lists(
            self.state.all_notebooks_mode,
            self.state.selected_folder_id().map(str::to_string),
            self.state.selected_note_id().map(str::to_string),
        )
        .await
    }

    /// Rename selected item (note or notebook)
    async fn rename_item(&mut self) -> Result<()> {
        let new_name = self.state.rename_input.clone();

        match self.state.focus {
            FocusPanel::Notes => {
                if let Some(note) = self.state.selected_note() {
                    let mut updated_note = note.clone();
                    updated_note.title = new_name.clone();
                    updated_note.updated_time = now_ms();

                    self.storage.update_note(&updated_note).await?;
                    self.state.clear_new_note_marker_if(&updated_note.id);
                    self.refresh_lists(
                        self.state.all_notebooks_mode,
                        self.state.selected_folder_id().map(str::to_string),
                        Some(updated_note.id.clone()),
                    )
                    .await?;

                    self.state
                        .set_status(&format!("Renamed note to: {}", new_name));
                }
            }
            FocusPanel::Notebooks => {
                if let Some(folder) = self.state.selected_folder() {
                    let mut updated_folder = folder.clone();
                    updated_folder.title = new_name.clone();
                    updated_folder.updated_time = now_ms();

                    self.storage.update_folder(&updated_folder).await?;
                    self.state.clear_new_folder_marker_if(&updated_folder.id);
                    self.refresh_lists(
                        false,
                        Some(updated_folder.id.clone()),
                        self.state.selected_note_id().map(str::to_string),
                    )
                    .await?;

                    self.state
                        .set_status(&format!("Renamed notebook to: {}", new_name));
                }
            }
            FocusPanel::Content => {
                self.state.set_status("Cannot rename content panel");
            }
        }
        Ok(())
    }

    /// Handle keyboard events in settings mode
    async fn handle_settings_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        use crate::settings::SettingsTab;

        // Priority 1: sync form is open — all keys go to the form handler
        if self.state.settings.sync.show_add_form || self.state.settings.sync.show_edit_form {
            return self.handle_target_form_key_event(key).await;
        }

        // Priority 2: delete confirmation dialog
        if self.state.settings.sync.confirm_delete {
            return self.handle_delete_confirm_key_event(key).await;
        }

        if self.state.settings.sync.confirm_activate {
            return self.handle_activate_target_key_event(key).await;
        }

        // Priority 3: encryption password prompt
        if self.state.settings.encryption.show_new_key_prompt {
            return self.handle_encryption_prompt_key_event(key).await;
        }

        // Normal settings navigation
        match key.code {
            // Close settings
            KeyCode::Char('q') | KeyCode::Esc => {
                self.state.hide_settings();
                self.state.settings.hide_password_prompts();
                self.state.settings.sync.show_add_form = false;
                self.state.settings.sync.show_edit_form = false;
                self.state.settings.sync.confirm_activate = false;
                self.state.settings.sync.activate_target_index = None;
                return Ok(false);
            }

            // Tab navigation (h/l and </> and [] and Tab/BackTab and Left/Right)
            KeyCode::Char('l')
            | KeyCode::Char('>')
            | KeyCode::Char(']')
            | KeyCode::Tab
            | KeyCode::Right => {
                self.state.settings.cycle_tab_forward();
            }

            KeyCode::Char('h')
            | KeyCode::Char('<')
            | KeyCode::Char('[')
            | KeyCode::BackTab
            | KeyCode::Left => {
                self.state.settings.cycle_tab_backward();
            }

            // Add new sync target
            KeyCode::Char('n') if self.state.settings.current_tab == SettingsTab::Sync => {
                self.state.settings.sync.show_add_form = true;
                self.state.settings.sync.clear_form();
                self.state.settings.sync.active_field = Some(crate::settings::FormField::Name);
            }

            // Edit / enable encryption
            KeyCode::Char('e') => {
                if self.state.settings.current_tab == SettingsTab::Encryption
                    && !self.state.settings.encryption.enabled
                {
                    self.state.settings.show_new_key_prompt();
                } else if self.state.settings.current_tab == SettingsTab::Sync {
                    let sync = &mut self.state.settings.sync;
                    if let Some(idx) = sync.selected_target_index {
                        if idx < sync.targets.len() {
                            sync.show_edit_form = true;
                            sync.editing_target_index = Some(idx);
                            sync.load_target_to_form(idx);
                        }
                    }
                }
            }

            // Delete / disable encryption
            KeyCode::Char('d') => {
                if self.state.settings.current_tab == SettingsTab::Encryption
                    && self.state.settings.encryption.enabled
                {
                    let data_dir = neojoplin_core::Config::data_dir()?;
                    self.state.settings.disable_encryption(&data_dir).await?;
                    self.refresh_sync_status().await?;
                    self.state.set_status("Encryption disabled");
                } else if self.state.settings.current_tab == SettingsTab::Sync {
                    let sync = &mut self.state.settings.sync;
                    if sync.selected_target_index.is_some() && !sync.targets.is_empty() {
                        sync.confirm_delete = true;
                    }
                }
            }

            KeyCode::Up | KeyCode::Char('k')
                if self.state.settings.current_tab == SettingsTab::AutoSync =>
            {
                self.state.settings.auto_sync.move_selection(false);
            }

            KeyCode::Down | KeyCode::Char('j')
                if self.state.settings.current_tab == SettingsTab::AutoSync =>
            {
                self.state.settings.auto_sync.move_selection(true);
            }

            KeyCode::Enter if self.state.settings.current_tab == SettingsTab::AutoSync => {
                let changed = self.state.settings.auto_sync.apply_selected();
                self.auto_sync_scheduler
                    .set_interval_seconds(self.state.settings.auto_sync.interval_seconds);
                let data_dir = neojoplin_core::Config::data_dir()?;
                self.state.settings.save_all_settings(&data_dir).await?;
                self.state
                    .settings
                    .update_runtime_status(self.state.settings.status.current_conflict_count);
                self.state.settings.set_next_auto_sync_in_seconds(
                    self.auto_sync_scheduler.seconds_until_next_run(),
                );
                self.state.set_status(if changed {
                    "Auto-sync interval updated"
                } else {
                    "Auto-sync interval unchanged"
                });
            }

            // Navigate target list
            KeyCode::Up | KeyCode::Char('k')
                if self.state.settings.current_tab == SettingsTab::Sync =>
            {
                let sync = &mut self.state.settings.sync;
                sync.move_selection(false);
            }

            KeyCode::Down | KeyCode::Char('j')
                if self.state.settings.current_tab == SettingsTab::Sync =>
            {
                let sync = &mut self.state.settings.sync;
                sync.move_selection(true);
            }

            // Save active target
            KeyCode::Enter if self.state.settings.current_tab == SettingsTab::Sync => {
                let sync = &mut self.state.settings.sync;
                if let Some(idx) = sync.selected_target_index {
                    if sync.current_target_index == Some(idx) {
                        self.state.set_status("Target is already active");
                    } else {
                        sync.confirm_activate = true;
                        sync.activate_target_index = Some(idx);
                    }
                }
            }

            KeyCode::Char('r') if self.state.settings.current_tab == SettingsTab::Status => {
                self.refresh_sync_status().await?;
                self.state.set_status("Sync status refreshed");
            }

            KeyCode::Char('b') if self.state.settings.current_tab == SettingsTab::Status => {
                self.state.settings.show_ribbon = !self.state.settings.show_ribbon;
                let data_dir = neojoplin_core::Config::data_dir()?;
                self.state.settings.save_all_settings(&data_dir).await?;
                self.state.set_status(if self.state.settings.show_ribbon {
                    "Ribbon enabled"
                } else {
                    "Ribbon disabled"
                });
            }

            _ => {}
        }

        Ok(false)
    }

    async fn handle_sort_popup_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc | KeyCode::Char(',') | KeyCode::Char('q') => {
                self.state.close_sort_popup();
            }
            KeyCode::Char('t') => {
                self.state.close_sort_popup();
                match self.state.focus {
                    FocusPanel::Notes => {
                        self.state.note_sort = NoteSortMode::TimeAsc;
                        self.refresh_current_lists().await?;
                        self.state.set_status("Sorted notes by time");
                    }
                    FocusPanel::Notebooks => {
                        self.state.notebook_sort = NotebookSortMode::TimeAsc;
                        self.refresh_current_lists().await?;
                        self.state.set_status("Sorted notebooks by time");
                    }
                    FocusPanel::Content => {}
                }
            }
            KeyCode::Char('T') => {
                self.state.close_sort_popup();
                match self.state.focus {
                    FocusPanel::Notes => {
                        self.state.note_sort = NoteSortMode::TimeDesc;
                        self.refresh_current_lists().await?;
                        self.state.set_status("Sorted notes by descending time");
                    }
                    FocusPanel::Notebooks => {
                        self.state.notebook_sort = NotebookSortMode::TimeDesc;
                        self.refresh_current_lists().await?;
                        self.state.set_status("Sorted notebooks by descending time");
                    }
                    FocusPanel::Content => {}
                }
            }
            KeyCode::Char('a') => {
                self.state.close_sort_popup();
                match self.state.focus {
                    FocusPanel::Notes => {
                        self.state.note_sort = NoteSortMode::NameAsc;
                        self.refresh_current_lists().await?;
                        self.state.set_status("Sorted notes by name");
                    }
                    FocusPanel::Notebooks => {
                        self.state.notebook_sort = NotebookSortMode::NameAsc;
                        self.refresh_current_lists().await?;
                        self.state.set_status("Sorted notebooks by name");
                    }
                    FocusPanel::Content => {}
                }
            }
            KeyCode::Char('m') if self.state.focus == FocusPanel::Notebooks => {
                self.state.close_sort_popup();
                self.state.notebook_sort = NotebookSortMode::RecentNote;
                self.refresh_current_lists().await?;
                self.state
                    .set_status("Sorted notebooks by most recently changed note");
            }
            _ => {}
        }

        Ok(false)
    }

    async fn refresh_current_lists(&mut self) -> Result<()> {
        self.refresh_lists(
            self.state.all_notebooks_mode,
            self.state.selected_folder_id().map(str::to_string),
            self.state.selected_note_id().map(str::to_string),
        )
        .await
    }

    async fn refresh_lists(
        &mut self,
        all_notebooks_mode: bool,
        selected_folder_id: Option<String>,
        selected_note_id: Option<String>,
    ) -> Result<()> {
        let all_notes = self.storage.list_notes(None).await?;
        let deleted_notes = self.storage.list_deleted_notes().await?;
        let mut folders = self.storage.list_folders().await?;
        let folder_ids: HashSet<String> = folders.iter().map(|folder| folder.id.clone()).collect();
        self.state.orphan_note_count = all_notes
            .iter()
            .filter(|note| note.parent_id.is_empty() || !folder_ids.contains(&note.parent_id))
            .count();
        self.state.trash_note_count = deleted_notes.len();

        self.state.sort_folders(&mut folders, &all_notes);
        folders = self.state.filter_folders(folders);
        self.state.set_folders(folders);

        if self.state.trash_mode {
            let mut notes = deleted_notes;
            let note_tags = self.load_note_tag_titles(&notes).await?;
            self.state.set_note_tags(note_tags);
            self.state.sort_notes(&mut notes);
            self.state.set_notes(notes);
            if let Some(note_id) = selected_note_id.as_deref() {
                self.state.select_note_by_id(note_id);
            }
            return Ok(());
        } else if all_notebooks_mode {
            self.state.set_folder(None);
        } else if self.state.orphan_mode && selected_folder_id.is_none() {
            // Keep orphan_mode active, don't reset it
            self.state.set_orphan_mode(true);
        } else if let Some(folder_id) = selected_folder_id.as_deref() {
            if !self.state.select_folder_by_id(folder_id) && !self.state.folders.is_empty() {
                self.state.set_folder(Some(0));
            }
        } else if self.state.folders.is_empty() && !self.state.orphan_mode {
            self.state.set_folder(None);
        }

        let mut notes = if self.state.orphan_mode {
            all_notes
                .into_iter()
                .filter(|note| note.parent_id.is_empty() || !folder_ids.contains(&note.parent_id))
                .collect()
        } else if self.state.all_notebooks_mode {
            all_notes
        } else if let Some(folder) = self.state.selected_folder() {
            self.storage.list_notes(Some(&folder.id)).await?
        } else {
            Vec::new()
        };
        let note_tags = self.load_note_tag_titles(&notes).await?;
        self.state.set_note_tags(note_tags);
        self.state.sort_notes(&mut notes);
        notes = self.state.filter_notes(notes);
        self.state.set_notes(notes);

        if let Some(note_id) = selected_note_id.as_deref() {
            self.state.select_note_by_id(note_id);
        }

        Ok(())
    }

    /// Reload the trash (deleted notes) list.
    async fn refresh_trash_list(&mut self, selected_note_id: Option<String>) -> Result<()> {
        let mut notes = self.storage.list_deleted_notes().await?;
        let note_tags = self.load_note_tag_titles(&notes).await?;
        self.state.set_note_tags(note_tags);
        self.state.sort_notes(&mut notes);
        self.state.set_notes(notes);
        if let Some(note_id) = selected_note_id.as_deref() {
            self.state.select_note_by_id(note_id);
        }
        Ok(())
    }

    /// Restore the selected note from the Trash.
    async fn restore_selected_note(&mut self) -> Result<()> {
        if !self.state.trash_mode {
            self.state
                .set_status("R restores notes only from the Trash panel");
            return Ok(());
        }
        if let Some(note) = self.state.selected_note() {
            let note_id = note.id.clone();
            let note_title = note.title.clone();
            self.storage.restore_note(&note_id).await?;
            self.refresh_trash_list(None).await?;
            self.state.set_status(&format!("Restored: {}", note_title));
        } else {
            self.state
                .set_status("Select a note in the Trash to restore it");
        }
        Ok(())
    }

    async fn toggle_version_view(&mut self) -> Result<()> {
        if self.state.content_view_mode == ContentViewMode::Note {
            let Some(note) = self.state.selected_note() else {
                self.state.set_status("Select a note to view versions");
                return Ok(());
            };
            let revisions = self.storage.list_note_revisions(&note.id).await?;
            if revisions.is_empty() {
                self.state
                    .set_status("No versions are available for this note yet");
                return Ok(());
            }
            self.state.open_version_list(revisions);
            self.state
                .set_status("Version list opened - Enter to preview, r to restore");
        } else {
            self.state.clear_version_view();
            self.state.set_status("Returned to current note preview");
        }
        Ok(())
    }

    async fn preview_selected_version(&mut self) -> Result<()> {
        let Some(note_id) = self.state.selected_note_id().map(str::to_string) else {
            self.state.set_status("Select a note to preview a version");
            return Ok(());
        };
        let Some(revision) = self.state.selected_note_version().cloned() else {
            self.state.set_status("Select a version to preview");
            return Ok(());
        };
        let snapshot = self
            .storage
            .get_note_revision_snapshot(&note_id, &revision.id)
            .await?;
        self.state
            .show_version_preview(snapshot.title, snapshot.body);
        self.state
            .set_status("Previewing selected version - press v to return");
        Ok(())
    }

    async fn restore_selected_version(&mut self) -> Result<()> {
        let Some(note_id) = self.state.selected_note_id().map(str::to_string) else {
            self.state.set_status("Select a note to restore a version");
            return Ok(());
        };
        let Some(revision) = self.state.selected_note_version().cloned() else {
            self.state.set_status("Select a version to restore");
            return Ok(());
        };
        let note = self
            .storage
            .restore_note_to_revision(&note_id, &revision.id)
            .await?;
        let selected_folder_id = self.state.selected_folder_id().map(str::to_string);
        self.refresh_lists(
            self.state.all_notebooks_mode,
            selected_folder_id,
            Some(note.id.clone()),
        )
        .await?;
        self.state.clear_version_view();
        self.state
            .set_status(&format!("Restored version for note: {}", note.title));
        Ok(())
    }

    /// Convert the selected note between note and to-do.
    async fn convert_note_type(&mut self) -> Result<()> {
        if self.state.focus != FocusPanel::Notes {
            self.state
                .set_status("Select a note in the notes panel first");
            return Ok(());
        }
        if let Some(note) = self.state.selected_note().cloned() {
            let mut updated = note.clone();
            updated.is_todo = if note.is_todo == 1 { 0 } else { 1 };
            if updated.is_todo == 0 {
                updated.todo_completed = 0;
            }
            updated.updated_time = now_ms();
            self.storage.update_note(&updated).await?;
            let kind = if updated.is_todo == 1 {
                "to-do"
            } else {
                "note"
            };
            let all_notebooks_mode = self.state.all_notebooks_mode;
            let selected_folder_id = self.state.selected_folder_id().map(str::to_string);
            let selected_note_id = Some(updated.id.clone());
            self.refresh_lists(all_notebooks_mode, selected_folder_id, selected_note_id)
                .await?;
            self.state
                .set_status(&format!("Converted to {}: {}", kind, updated.title));
        } else {
            self.state
                .set_status("Select a note or to-do to convert it");
        }
        Ok(())
    }

    /// Create a new note with a given title.
    async fn create_note_with_title(&mut self, title: &str) -> Result<()> {
        let parent_id = self.default_parent_folder_id().await?;
        let note = Note {
            id: joplin_domain::joplin_id(),
            title: title.to_string(),
            body: String::new(),
            parent_id,
            created_time: now_ms(),
            updated_time: now_ms(),
            is_todo: 0,
            deleted_time: 0,
            ..Default::default()
        };
        self.storage.create_note(&note).await?;
        self.state.mark_new_note(note.id.clone());
        self.state.focus = FocusPanel::Notes;
        self.refresh_lists(
            self.state.all_notebooks_mode,
            self.state.selected_folder_id().map(str::to_string),
            Some(note.id.clone()),
        )
        .await?;
        self.state
            .set_status(&format!("Created note: {} - press r to rename it", title));
        Ok(())
    }

    /// Create a new to-do with a given title.
    async fn create_todo_with_title(&mut self, title: &str) -> Result<()> {
        let parent_id = self.default_parent_folder_id().await?;
        let note = Note {
            id: joplin_domain::joplin_id(),
            title: title.to_string(),
            body: String::new(),
            parent_id,
            created_time: now_ms(),
            updated_time: now_ms(),
            is_todo: 1,
            deleted_time: 0,
            ..Default::default()
        };
        self.storage.create_note(&note).await?;
        self.state.mark_new_note(note.id.clone());
        self.state.focus = FocusPanel::Notes;
        self.refresh_lists(
            self.state.all_notebooks_mode,
            self.state.selected_folder_id().map(str::to_string),
            Some(note.id.clone()),
        )
        .await?;
        self.state
            .set_status(&format!("Created to-do: {} - press r to rename it", title));
        Ok(())
    }

    /// Create a new notebook with a given title.
    async fn create_notebook_with_title(&mut self, title: &str) -> Result<()> {
        let folder = Folder {
            id: joplin_domain::joplin_id(),
            title: title.to_string(),
            parent_id: String::new(),
            created_time: now_ms(),
            updated_time: now_ms(),
            user_created_time: 0,
            user_updated_time: 0,
            is_shared: 0,
            share_id: None,
            master_key_id: None,
            encryption_applied: 0,
            encryption_cipher_text: None,
            encryption_blob_encrypted: 0,
            icon: String::new(),
        };
        self.storage.create_folder(&folder).await?;
        self.state.mark_new_folder(folder.id.clone());
        self.state.focus = FocusPanel::Notebooks;
        self.refresh_lists(
            false,
            Some(folder.id.clone()),
            self.state.selected_note_id().map(str::to_string),
        )
        .await?;
        self.state.set_status(&format!(
            "Created notebook: {} - press r to rename it",
            title
        ));
        Ok(())
    }

    async fn load_note_tag_titles(&self, notes: &[Note]) -> Result<HashMap<String, Vec<String>>> {
        let mut note_tags = HashMap::new();

        for note in notes {
            let tags = self
                .storage
                .get_note_tags(&note.id)
                .await?
                .into_iter()
                .map(|tag| tag.title)
                .collect();
            note_tags.insert(note.id.clone(), tags);
        }

        Ok(note_tags)
    }

    /// Handle keyboard events in the encryption password prompt
    async fn handle_encryption_prompt_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        use crate::settings::EncryptionField;

        match key.code {
            KeyCode::Esc => {
                self.state.settings.hide_password_prompts();
            }

            // Tab / arrows cycle between Password and Confirm fields
            KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => {
                self.state.settings.encryption.active_field =
                    match self.state.settings.encryption.active_field {
                        EncryptionField::Password => EncryptionField::Confirm,
                        EncryptionField::Confirm => EncryptionField::Password,
                    };
            }

            KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => {
                self.state.settings.encryption.active_field =
                    match self.state.settings.encryption.active_field {
                        EncryptionField::Password => EncryptionField::Confirm,
                        EncryptionField::Confirm => EncryptionField::Password,
                    };
            }

            KeyCode::Enter => {
                let password = self.state.settings.encryption.password_input.clone();
                let data_dir = neojoplin_core::Config::data_dir()?;
                self.state
                    .settings
                    .enable_encryption(&password, &data_dir)
                    .await?;
                self.refresh_sync_status().await?;
            }

            KeyCode::Backspace => {
                match self.state.settings.encryption.active_field {
                    EncryptionField::Password => self.state.settings.remove_password_char(),
                    EncryptionField::Confirm => self.state.settings.remove_confirm_password_char(),
                }
                self.state.settings.encryption.password_error = None;
            }

            KeyCode::Char(c) => match self.state.settings.encryption.active_field {
                EncryptionField::Password => self.state.settings.add_password_char(c),
                EncryptionField::Confirm => self.state.settings.add_confirm_password_char(c),
            },

            _ => {}
        }

        Ok(false)
    }

    /// Handle keyboard events in the delete confirmation dialog
    async fn handle_delete_confirm_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                let sync = &mut self.state.settings.sync;
                sync.confirm_delete = false;
                if let Some(idx) = sync.selected_target_index {
                    if !sync.targets.is_empty() {
                        sync.targets.remove(idx);
                        if sync.targets.is_empty() {
                            sync.current_target_index = None;
                            sync.selected_target_index = None;
                        } else if idx >= sync.targets.len() {
                            if sync.current_target_index == Some(idx) {
                                sync.current_target_index = Some(sync.targets.len() - 1);
                            }
                            sync.selected_target_index = Some(sync.targets.len() - 1);
                        } else {
                            if sync.current_target_index == Some(idx) {
                                sync.current_target_index = Some(idx.min(sync.targets.len() - 1));
                            }
                            sync.selected_target_index = Some(idx);
                        }
                        let data_dir = neojoplin_core::Config::data_dir()?;
                        let _ = self.state.settings.save_all_settings(&data_dir).await;
                        self.state.set_status("Target deleted");
                    }
                }
            }

            KeyCode::Char('n') | KeyCode::Esc => {
                self.state.settings.sync.confirm_delete = false;
            }

            _ => {}
        }
        Ok(false)
    }

    async fn handle_pending_delete_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                self.perform_pending_delete(false).await?;
            }
            KeyCode::Char('Y') => {
                self.perform_pending_delete(true).await?;
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.state.clear_pending_delete();
            }
            _ => {}
        }

        Ok(false)
    }

    async fn handle_activate_target_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Enter => {
                self.apply_selected_sync_target(false).await?;
            }
            KeyCode::Char('y') => {
                self.apply_selected_sync_target(true).await?;
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.state.settings.sync.confirm_activate = false;
                self.state.settings.sync.activate_target_index = None;
            }
            _ => {}
        }

        Ok(false)
    }

    async fn handle_filter_prompt_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char(c) => {
                self.state.add_filter_char(c);
                self.update_filter_tag_completion().await?;
                self.refresh_current_lists().await?;
            }
            KeyCode::Backspace => {
                self.state.remove_filter_char();
                self.update_filter_tag_completion().await?;
                self.refresh_current_lists().await?;
            }
            KeyCode::Tab => {
                self.cycle_filter_tag_completion(false).await?;
                self.refresh_current_lists().await?;
            }
            KeyCode::BackTab => {
                self.cycle_filter_tag_completion(true).await?;
                self.refresh_current_lists().await?;
            }
            KeyCode::Enter => {
                self.state.close_filter_prompt(false);
                self.refresh_current_lists().await?;
                let data_dir = neojoplin_core::Config::data_dir()?;
                self.state.settings.save_all_settings(&data_dir).await?;
            }
            KeyCode::Esc => {
                if matches!(self.state.filter_target, FocusPanel::Notebooks) {
                    self.state.notebook_filter_query.clear();
                } else {
                    self.state.clear_note_filters();
                }
                self.state.close_filter_prompt(false);
                self.refresh_current_lists().await?;
                let data_dir = neojoplin_core::Config::data_dir()?;
                self.state.settings.save_all_settings(&data_dir).await?;
            }
            _ => {}
        }

        Ok(false)
    }

    async fn handle_command_prompt_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => {
                self.state.close_command_prompt();
                self.command_history_index = None;
                self.command_history_draft.clear();
            }
            KeyCode::Backspace => {
                self.state.command_prompt.pop_char();
                self.command_history_index = None;
            }
            KeyCode::Tab => {
                self.cycle_command_completion(false).await?;
            }
            KeyCode::BackTab => {
                self.cycle_command_completion(true).await?;
            }
            KeyCode::Up => {
                self.navigate_command_history(true);
            }
            KeyCode::Down => {
                self.navigate_command_history(false);
            }
            KeyCode::Enter => {
                let input = self.state.command_prompt.input.clone();
                match parse_command(&input) {
                    Ok(action) => {
                        self.remember_command(&input);
                        match self.execute_command(action).await {
                            Ok(should_exit) => {
                                self.state.close_command_prompt();
                                self.command_history_index = None;
                                self.command_history_draft.clear();
                                return Ok(should_exit);
                            }
                            Err(error) => self.state.command_prompt.set_error(error.to_string()),
                        }
                    }
                    Err(error) => self.state.command_prompt.set_error(error),
                }
            }
            KeyCode::Char(c)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.state.command_prompt.push_char(c);
                self.command_history_index = None;
            }
            _ => {}
        }

        Ok(false)
    }

    fn open_command_prompt(&mut self, initial_input: String) {
        self.state.open_command_prompt(initial_input.clone());
        self.command_history_index = None;
        self.command_history_draft = initial_input;
    }

    fn navigate_command_history(&mut self, older: bool) {
        if self.command_history.is_empty() {
            return;
        }

        if older {
            let next_index = match self.command_history_index {
                Some(index) if index > 0 => index - 1,
                Some(index) => index,
                None => {
                    self.command_history_draft = self.state.command_prompt.input.clone();
                    self.command_history.len().saturating_sub(1)
                }
            };
            self.command_history_index = Some(next_index);
            self.state
                .command_prompt
                .set_input(self.command_history[next_index].clone());
        } else if let Some(index) = self.command_history_index {
            if index + 1 < self.command_history.len() {
                let next_index = index + 1;
                self.command_history_index = Some(next_index);
                self.state
                    .command_prompt
                    .set_input(self.command_history[next_index].clone());
            } else {
                self.command_history_index = None;
                self.state
                    .command_prompt
                    .set_input(self.command_history_draft.clone());
            }
        }
    }

    fn remember_command(&mut self, input: &str) {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return;
        }
        if self
            .command_history
            .last()
            .map(|last| last == trimmed)
            .unwrap_or(false)
        {
            return;
        }
        self.command_history.push(trimmed.to_string());
    }

    async fn open_tag_popup(&mut self) -> Result<()> {
        if self.state.selected_note().is_none() {
            self.state
                .set_status("Select a note before editing its tags");
            return Ok(());
        }

        let items = self.load_tag_popup_items().await?;
        self.state.open_tag_popup(items);
        Ok(())
    }

    async fn handle_tag_popup_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        if self.state.tag_popup.pending_delete_tag.is_some() {
            match key.code {
                KeyCode::Char('y') => {
                    self.confirm_delete_selected_tag_from_popup().await?;
                }
                KeyCode::Char('n') | KeyCode::Esc => {
                    self.state.tag_popup.pending_delete_tag = None;
                }
                _ => {}
            }
            return Ok(false);
        }

        match key.code {
            KeyCode::Esc => {
                self.state.close_tag_popup();
            }
            KeyCode::Tab => {
                self.state.tag_popup.focus = match self.state.tag_popup.focus {
                    TagPopupFocus::List => TagPopupFocus::Input,
                    TagPopupFocus::Input => TagPopupFocus::List,
                };
            }
            KeyCode::Up | KeyCode::Char('k')
                if self.state.tag_popup.focus == TagPopupFocus::List =>
            {
                self.state.tag_popup.move_selection(false);
            }
            KeyCode::Down | KeyCode::Char('j')
                if self.state.tag_popup.focus == TagPopupFocus::List =>
            {
                self.state.tag_popup.move_selection(true);
            }
            KeyCode::Char(' ') if self.state.tag_popup.focus == TagPopupFocus::List => {
                self.toggle_selected_tag_from_popup().await?;
            }
            KeyCode::Delete | KeyCode::Char('d')
                if self.state.tag_popup.focus == TagPopupFocus::List =>
            {
                self.request_delete_selected_tag_from_popup();
            }
            KeyCode::Enter => {
                if self.state.tag_popup.focus == TagPopupFocus::Input {
                    self.create_or_attach_tag_from_popup_input().await?;
                } else {
                    self.toggle_selected_tag_from_popup().await?;
                }
            }
            KeyCode::Backspace if self.state.tag_popup.focus == TagPopupFocus::Input => {
                self.state.tag_popup.input.pop();
            }
            KeyCode::Char(c)
                if self.state.tag_popup.focus == TagPopupFocus::Input
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                self.state.tag_popup.input.push(c);
            }
            _ => {}
        }

        Ok(false)
    }

    async fn load_tag_popup_items(&self) -> Result<Vec<TagPopupItem>> {
        let note = self
            .state
            .selected_note()
            .ok_or_else(|| anyhow::anyhow!("Select a note before editing tags"))?;
        let attached_tags = self.storage.get_note_tags(&note.id).await?;
        let attached_ids: HashSet<String> =
            attached_tags.iter().map(|tag| tag.id.clone()).collect();
        let mut items: Vec<TagPopupItem> = self
            .storage
            .list_tags()
            .await?
            .into_iter()
            .map(|tag| TagPopupItem {
                attached: attached_ids.contains(&tag.id),
                id: tag.id,
                title: tag.title,
            })
            .collect();
        items.sort_by_key(|item| item.title.to_lowercase());
        Ok(items)
    }

    async fn refresh_tag_popup_items(&mut self, preferred_tag_id: Option<&str>) -> Result<()> {
        let items = self.load_tag_popup_items().await?;
        let previous_focus = self.state.tag_popup.focus;
        let previous_input = self.state.tag_popup.input.clone();
        self.state.open_tag_popup(items);
        self.state.tag_popup.focus = previous_focus;
        self.state.tag_popup.input = previous_input;
        if let Some(tag_id) = preferred_tag_id {
            if let Some(index) = self
                .state
                .tag_popup
                .items
                .iter()
                .position(|item| item.id == tag_id)
            {
                self.state.tag_popup.selected_index = index;
            }
        }
        Ok(())
    }

    async fn refresh_note_tag_cache(&mut self) -> Result<()> {
        let note_tags = self.load_note_tag_titles(&self.state.notes).await?;
        self.state.set_note_tags(note_tags);
        Ok(())
    }

    async fn toggle_selected_tag_from_popup(&mut self) -> Result<()> {
        let item = match self.state.tag_popup.current_item() {
            Some(item) => item.clone(),
            None => return Ok(()),
        };

        if item.attached {
            self.untag_selected_note_by_id(&item.id).await?;
            self.state
                .set_status(&format!("Removed tag {}", item.title));
        } else {
            self.tag_selected_note_by_id(&item.id).await?;
            self.state.set_status(&format!("Added tag {}", item.title));
        }

        self.refresh_note_tag_cache().await?;
        self.refresh_tag_popup_items(Some(&item.id)).await?;
        Ok(())
    }

    async fn create_or_attach_tag_from_popup_input(&mut self) -> Result<()> {
        let tag_name = self.state.tag_popup.input.trim().to_string();
        if tag_name.is_empty() {
            self.state.tag_popup.focus = TagPopupFocus::List;
            return Ok(());
        }

        let existing_tags = self.storage.list_tags().await?;
        let tag = if let Some(existing_tag) = resolve_tag_by_title(&existing_tags, &tag_name) {
            existing_tag.clone()
        } else {
            let tag = Tag {
                id: joplin_domain::joplin_id(),
                title: tag_name.clone(),
                created_time: now_ms(),
                updated_time: now_ms(),
                user_created_time: 0,
                user_updated_time: 0,
                parent_id: String::new(),
                is_shared: 0,
                encryption_applied: 0,
                encryption_cipher_text: None,
                encryption_blob_encrypted: 0,
                master_key_id: None,
            };
            self.storage.create_tag(&tag).await?;
            tag
        };

        self.tag_selected_note_by_id(&tag.id).await?;
        self.state.tag_popup.input.clear();
        self.state.tag_popup.focus = TagPopupFocus::List;
        self.refresh_note_tag_cache().await?;
        self.refresh_tag_popup_items(Some(&tag.id)).await?;
        self.state.set_status(&format!("Added tag {}", tag.title));
        Ok(())
    }

    fn request_delete_selected_tag_from_popup(&mut self) {
        if let Some(item) = self.state.tag_popup.current_item() {
            self.state.tag_popup.pending_delete_tag = Some((item.id.clone(), item.title.clone()));
        }
    }

    async fn confirm_delete_selected_tag_from_popup(&mut self) -> Result<()> {
        let Some((tag_id, title)) = self.state.tag_popup.pending_delete_tag.clone() else {
            return Ok(());
        };
        self.state.tag_popup.pending_delete_tag = None;
        self.storage.delete_tag(&tag_id).await?;
        self.refresh_note_tag_cache().await?;
        self.refresh_tag_popup_items(None).await?;
        self.state.set_status(&format!("Deleted tag {}", title));
        Ok(())
    }

    async fn jump_to_list_boundary(&mut self, to_start: bool) -> Result<()> {
        match self.state.focus {
            FocusPanel::Notebooks => {
                if to_start {
                    self.state.set_folder(None);
                } else if self.state.trash_note_count > 0 {
                    self.state.set_trash_mode(true);
                } else if self.state.orphan_note_count > 0 {
                    self.state.set_orphan_mode(true);
                } else if !self.state.folders.is_empty() {
                    self.state.set_folder(Some(self.state.folders.len() - 1));
                } else {
                    self.state.set_folder(None);
                }
                self.reload_notes().await?;
            }
            FocusPanel::Notes => {
                if self.state.notes.is_empty() {
                    self.state.selected_note = None;
                } else {
                    self.state.selected_note = Some(if to_start {
                        0
                    } else {
                        self.state.notes.len() - 1
                    });
                    self.state.load_note_content();
                }
            }
            FocusPanel::Content => {
                if self.state.content_view_mode == ContentViewMode::VersionList {
                    if self.state.note_versions.is_empty() {
                        self.state.selected_note_version = None;
                    } else {
                        self.state.selected_note_version = Some(if to_start {
                            0
                        } else {
                            self.state.note_versions.len() - 1
                        });
                    }
                } else {
                    self.state.content_scroll_offset = if to_start { 0 } else { usize::MAX / 2 };
                }
            }
        }

        Ok(())
    }

    async fn cycle_command_completion(&mut self, backwards: bool) -> Result<()> {
        let current_input = self.state.command_prompt.input.clone();
        let reuse_existing = self
            .state
            .command_prompt
            .completion
            .as_ref()
            .and_then(|completion| completion.current().map(|current| (completion, current)))
            .map(|(completion, current)| current_input == current && !completion.items.is_empty())
            .unwrap_or(false);

        if reuse_existing {
            if let Some(completion) = self.state.command_prompt.completion.as_mut() {
                completion.advance(backwards);
                if let Some(current) = completion.current() {
                    self.state.command_prompt.input = current.to_string();
                }
            }
            return Ok(());
        }

        let items = self.command_completion_items(&current_input).await?;
        if items.is_empty() {
            return Ok(());
        }

        let mut completion = CompletionState { items, index: 0 };
        if backwards {
            completion.index = completion.items.len() - 1;
        }
        if let Some(current) = completion.current() {
            self.state.command_prompt.input = current.to_string();
        }
        self.state.command_prompt.completion = Some(completion);
        Ok(())
    }

    async fn command_completion_items(&self, input: &str) -> Result<Vec<String>> {
        let trimmed = input.trim_start();
        if trimmed.is_empty() {
            return Ok(crate::command_line::COMMANDS
                .iter()
                .filter(|command| !command.hidden_from_completion)
                .map(|command| command.name.to_string())
                .collect());
        }

        let (command_name, arg, has_argument_context) = split_command_input(trimmed);
        if !has_argument_context {
            let mut items: Vec<String> = crate::command_line::COMMANDS
                .iter()
                .filter(|command| {
                    !command.hidden_from_completion
                        && starts_with_ignore_case(command.name, command_name)
                })
                .map(|command| command.name.to_string())
                .collect();
            items.sort_by_key(|item| item.to_lowercase());
            items.dedup();
            return Ok(items);
        }

        let argument = arg.trim_start();
        let mut items = match command_name {
            "move" | "mv" => {
                let folders = self.storage.list_folders().await?;
                let display_names = build_folder_display_names(&folders);
                let command_prefix = if command_name == "mv" { "mv" } else { "move" };
                let mut items: Vec<String> = display_names
                    .values()
                    .filter(|name| starts_with_ignore_case(name, argument))
                    .map(|name| format!("{} {}", command_prefix, name))
                    .collect();
                if starts_with_ignore_case("root", argument) {
                    items.push(format!("{} root", command_prefix));
                }
                items
            }
            "tag" => {
                let trimmed_argument = argument.trim_start();
                let (subcommand, subarg, sub_has_argument_context) =
                    split_command_input(trimmed_argument);
                if !sub_has_argument_context {
                    let mut items = ["add", "remove", "list"]
                        .into_iter()
                        .filter(|value| starts_with_ignore_case(value, subcommand))
                        .map(|value| format!("tag {}", value))
                        .collect::<Vec<_>>();
                    items.sort_by_key(|item| item.to_lowercase());
                    items
                } else {
                    match subcommand {
                        "add" => self
                            .storage
                            .list_tags()
                            .await?
                            .into_iter()
                            .filter(|tag| starts_with_ignore_case(&tag.title, subarg.trim_start()))
                            .map(|tag| format!("tag add {}", tag.title))
                            .collect(),
                        "remove" => {
                            let note_tags = if let Some(note) = self.state.selected_note() {
                                self.storage.get_note_tags(&note.id).await?
                            } else {
                                Vec::new()
                            };
                            note_tags
                                .into_iter()
                                .filter(|tag| {
                                    starts_with_ignore_case(&tag.title, subarg.trim_start())
                                })
                                .map(|tag| format!("tag remove {}", tag.title))
                                .collect()
                        }
                        _ => Vec::new(),
                    }
                }
            }
            "read" => complete_path_input("read", argument),
            "import" => complete_path_input("import", argument),
            "import-jex" => complete_path_input("import-jex", argument),
            "export-jex" => complete_path_input("export-jex", argument),
            _ => Vec::new(),
        };
        items.sort_by_key(|item: &String| item.to_lowercase());
        items.dedup();
        Ok(items)
    }

    async fn execute_command(&mut self, action: CommandAction) -> Result<bool> {
        match action {
            CommandAction::Move(notebook_name) => {
                match self.state.focus {
                    FocusPanel::Notebooks => {
                        self.move_selected_folder_to_notebook(&notebook_name)
                            .await?;
                    }
                    FocusPanel::Notes => {
                        self.move_selected_note_to_notebook(&notebook_name).await?;
                    }
                    FocusPanel::Content => {
                        anyhow::bail!("Focus notes or notebooks before using :move");
                    }
                }
                Ok(false)
            }
            CommandAction::DeleteOrphaned => {
                self.delete_orphaned_notes().await?;
                Ok(false)
            }
            CommandAction::Quit => Ok(true),
            CommandAction::ImportDesktop => {
                self.import_from_database(&default_desktop_database_path())
                    .await?;
                Ok(false)
            }
            CommandAction::Import(path) => {
                let import_path = path
                    .map(|value| resolve_import_path(&value))
                    .unwrap_or_else(default_cli_database_path);
                self.import_from_database(&import_path).await?;
                Ok(false)
            }
            CommandAction::ImportJex(path) => {
                self.import_from_jex(&resolve_import_path(&path)).await?;
                Ok(false)
            }
            CommandAction::ExportJex(path) => {
                self.export_to_jex(&resolve_import_path(&path)).await?;
                Ok(false)
            }
            CommandAction::Read(path) => {
                self.create_note_from_file(&resolve_import_path(&path))
                    .await?;
                Ok(false)
            }
            CommandAction::TagAdd(tag_name) => {
                self.tag_selected_note(&tag_name).await?;
                Ok(false)
            }
            CommandAction::TagRemove(tag_name) => {
                self.untag_selected_note(&tag_name).await?;
                Ok(false)
            }
            CommandAction::TagList => {
                self.list_selected_note_tags().await?;
                Ok(false)
            }
            CommandAction::MkNote(title) => {
                self.create_note_with_title(&title).await?;
                Ok(false)
            }
            CommandAction::MkTodo(title) => {
                self.create_todo_with_title(&title).await?;
                Ok(false)
            }
            CommandAction::MkBook(title) => {
                self.create_notebook_with_title(&title).await?;
                Ok(false)
            }
        }
    }

    /// Handle keyboard events in sync target form
    async fn handle_target_form_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Tab | KeyCode::Down => {
                self.state.settings.sync.cycle_field_forward();
            }

            KeyCode::BackTab | KeyCode::Up => {
                self.state.settings.sync.cycle_field_backward();
            }

            KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Test connection
                self.test_webdav_connection().await?;
            }

            KeyCode::Char(c) => {
                // Add character to active field
                let active_field = self.state.settings.sync.active_field;
                match active_field {
                    Some(crate::settings::FormField::Name) => {
                        self.state.settings.sync.add_name_char(c)
                    }
                    Some(crate::settings::FormField::Url) => {
                        self.state.settings.sync.add_url_char(c)
                    }
                    Some(crate::settings::FormField::Username) => {
                        self.state.settings.sync.add_username_char(c)
                    }
                    Some(crate::settings::FormField::Password) => {
                        self.state.settings.sync.add_password_char(c)
                    }
                    None => {}
                }
            }

            KeyCode::Backspace => {
                // Remove character from active field
                let active_field = self.state.settings.sync.active_field;
                match active_field {
                    Some(crate::settings::FormField::Name) => {
                        self.state.settings.sync.remove_name_char()
                    }
                    Some(crate::settings::FormField::Url) => {
                        self.state.settings.sync.remove_url_char()
                    }
                    Some(crate::settings::FormField::Username) => {
                        self.state.settings.sync.remove_username_char()
                    }
                    Some(crate::settings::FormField::Password) => {
                        self.state.settings.sync.remove_password_char()
                    }
                    None => {}
                }
            }

            KeyCode::Enter => {
                // Validate and save
                if let Err(e) = self.validate_and_save_target().await {
                    self.state.settings.sync.form_error = Some(e.to_string());
                } else {
                    self.state.settings.sync.show_add_form = false;
                    self.state.settings.sync.show_edit_form = false;
                    self.state.settings.sync.clear_form();
                    self.state.set_status("Target saved successfully");
                }
            }

            KeyCode::Esc => {
                // Cancel form
                self.state.settings.sync.show_add_form = false;
                self.state.settings.sync.show_edit_form = false;
                self.state.settings.sync.clear_form();
            }

            _ => {}
        }

        Ok(false)
    }

    /// Validate and save sync target
    async fn validate_and_save_target(&mut self) -> anyhow::Result<()> {
        let sync = &mut self.state.settings.sync;

        // Basic validation
        if sync.name_input.trim().is_empty() {
            return Err(anyhow::anyhow!("Name cannot be empty"));
        }

        if sync.url_input.trim().is_empty() {
            return Err(anyhow::anyhow!("URL cannot be empty"));
        }

        if !sync.url_input.starts_with("http://") && !sync.url_input.starts_with("https://") {
            return Err(anyhow::anyhow!("URL must start with http:// or https://"));
        }

        // Create or update target; remote_path is derived from URL at sync time
        let target = crate::settings::SyncTarget {
            id: if sync.show_edit_form {
                sync.editing_target_index
                    .and_then(|i| sync.targets.get(i).map(|t| t.id.clone()))
                    .unwrap_or_else(joplin_domain::joplin_id)
            } else {
                joplin_domain::joplin_id()
            },
            name: sync.name_input.trim().to_string(),
            target_type: crate::settings::SyncTargetType::WebDAV,
            url: sync.url_input.trim().to_string(),
            username: sync.username_input.trim().to_string(),
            password: sync.password_input.clone(),
            remote_path: split_webdav_url(sync.url_input.trim()).1,
            ignore_tls_errors: false,
        };

        if sync.show_edit_form {
            if let Some(idx) = sync.editing_target_index {
                if idx < sync.targets.len() {
                    sync.targets[idx] = target;
                    sync.selected_target_index = Some(idx);
                }
            }
        } else {
            sync.targets.push(target);
            let new_index = sync.targets.len() - 1;
            sync.selected_target_index = Some(new_index);
            if sync.current_target_index.is_none() {
                sync.current_target_index = Some(new_index);
            }
        }

        // Save to file
        let data_dir = neojoplin_core::Config::data_dir()?;
        self.state.settings.save_all_settings(&data_dir).await?;

        Ok(())
    }

    async fn apply_selected_sync_target(&mut self, sync_now: bool) -> Result<()> {
        let Some(idx) = self.state.settings.sync.activate_target_index else {
            return Ok(());
        };

        self.state.settings.sync.current_target_index = Some(idx);
        self.state.settings.sync.selected_target_index = Some(idx);
        self.state.settings.sync.confirm_activate = false;
        self.state.settings.sync.activate_target_index = None;

        let data_dir = neojoplin_core::Config::data_dir()?;
        self.state.settings.save_all_settings(&data_dir).await?;

        let target_name = self
            .state
            .settings
            .sync
            .targets
            .get(idx)
            .map(|target| target.name.clone())
            .unwrap_or_else(|| "target".to_string());

        self.state
            .set_status(&format!("Active sync target set to {}", target_name));

        if sync_now {
            self.sync().await?;
        }

        Ok(())
    }

    /// Test WebDAV connection
    async fn test_webdav_connection(&mut self) -> anyhow::Result<()> {
        let url = self.state.settings.sync.url_input.clone();
        let username = self.state.settings.sync.username_input.clone();
        let password = self.state.settings.sync.password_input.clone();

        {
            let sync = &mut self.state.settings.sync;
            sync.testing_connection = true;
            sync.connection_result = None;
            sync.form_error = None;
        }

        // Basic URL validation
        if url.is_empty() || !url.starts_with("http") {
            self.state.settings.sync.form_error = Some("Invalid URL".to_string());
            self.state.settings.sync.testing_connection = false;
            return Ok(());
        }

        // Derive base URL and remote path from the full URL
        let (base_url, remote_path) = split_webdav_url(&url);

        use joplin_sync::{ReqwestWebDavClient, WebDavConfig};
        let config = WebDavConfig::new(base_url, username, password);

        match ReqwestWebDavClient::new(config) {
            Ok(webdav) => {
                if !webdav.exists_impl(&remote_path).await.unwrap_or(false) {
                    webdav.mkdir_impl(&remote_path).await?;
                }

                let probe_path = format!(
                    "{}/.neojoplin-connection-test-{}",
                    remote_path.trim_end_matches('/'),
                    joplin_domain::joplin_id()
                );

                match webdav.put_impl(&probe_path, b"ok").await {
                    Ok(()) => {
                        let _ = webdav.delete_impl(&probe_path).await;
                        match webdav.list_impl(&remote_path).await {
                            Ok(_) => {
                                self.state.settings.sync.connection_result =
                                    Some(crate::settings::ConnectionResult::Success(
                                        "Remote path is reachable and writable".to_string(),
                                    ));
                            }
                            Err(e) => {
                                self.state.settings.sync.connection_result =
                                    Some(crate::settings::ConnectionResult::Failed(e.to_string()));
                            }
                        }
                    }
                    Err(e) => {
                        self.state.settings.sync.connection_result =
                            Some(crate::settings::ConnectionResult::Failed(e.to_string()));
                    }
                }
            }
            Err(e) => {
                self.state.settings.sync.connection_result =
                    Some(crate::settings::ConnectionResult::Failed(e.to_string()));
            }
        }

        self.state.settings.sync.testing_connection = false;
        Ok(())
    }

    async fn perform_pending_delete(&mut self, delete_notes_in_notebook: bool) -> Result<()> {
        let pending = self.state.pending_delete.clone();
        self.state.clear_pending_delete();

        match pending {
            Some(PendingDelete::Note {
                id,
                title,
                permanent,
            }) => {
                if permanent {
                    self.state
                        .set_status(&format!("Permanently deleting note: {}", title));
                    self.storage.delete_note(&id).await?;
                    self.state.clear_new_note_marker_if(&id);
                    self.refresh_trash_list(None).await?;
                    self.state.set_status("Note permanently deleted");
                } else {
                    self.state
                        .set_status(&format!("Moving to trash: {}", title));
                    self.storage.trash_note(&id).await?;
                    self.state.clear_new_note_marker_if(&id);
                    self.refresh_lists(
                        self.state.all_notebooks_mode,
                        self.state.selected_folder_id().map(str::to_string),
                        None,
                    )
                    .await?;
                    self.state.set_status("Note moved to trash");
                }
            }
            Some(PendingDelete::Notebook { id, title, .. }) => {
                self.state
                    .set_status(&format!("Deleting notebook: {}", title));
                if delete_notes_in_notebook {
                    for note in self.storage.list_notes(Some(&id)).await? {
                        self.storage.delete_note(&note.id).await?;
                        self.state.clear_new_note_marker_if(&note.id);
                    }
                }
                self.storage.delete_folder(&id).await?;
                self.state.clear_new_folder_marker_if(&id);
                self.refresh_lists(false, None, None).await?;
                self.state.set_status(if delete_notes_in_notebook {
                    "Notebook and contained notes deleted"
                } else {
                    "Notebook deleted; contained notes are now orphaned"
                });
            }
            None => {}
        }

        Ok(())
    }

    async fn delete_selected_note_immediately(&mut self) -> Result<()> {
        if self.state.focus != FocusPanel::Notes {
            self.state.set_status("D only works from the notes panel");
            return Ok(());
        }

        if let Some(note) = self.state.selected_note() {
            let note_id = note.id.clone();
            let note_title = note.title.clone();
            if self.state.trash_mode {
                self.state
                    .set_status(&format!("Permanently deleting: {}", note_title));
                self.storage.delete_note(&note_id).await?;
                self.state.clear_new_note_marker_if(&note_id);
                self.refresh_trash_list(None).await?;
                self.state.set_status("Note permanently deleted");
            } else {
                self.state
                    .set_status(&format!("Moving to trash: {}", note_title));
                self.storage.trash_note(&note_id).await?;
                self.state.clear_new_note_marker_if(&note_id);
                self.refresh_lists(
                    self.state.all_notebooks_mode,
                    self.state.selected_folder_id().map(str::to_string),
                    None,
                )
                .await?;
                self.state.set_status("Note moved to trash");
            }
        }

        Ok(())
    }

    async fn move_selected_note_to_notebook(&mut self, notebook_name: &str) -> Result<()> {
        let note = self
            .state
            .selected_note()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Select a note before using :move"))?;
        let folders = self.storage.list_folders().await?;
        let (target_folder_id, target_folder_title) =
            resolve_folder_destination(&folders, notebook_name)?;

        if note.parent_id == target_folder_id {
            self.state.set_status(&format!(
                "{} is already in {}",
                note.title, target_folder_title
            ));
            return Ok(());
        }

        let mut updated_note = note.clone();
        updated_note.parent_id = target_folder_id.clone();
        updated_note.updated_time = now_ms();
        self.storage.update_note(&updated_note).await?;

        self.refresh_lists(false, Some(target_folder_id), Some(updated_note.id.clone()))
            .await?;
        self.state.focus = FocusPanel::Notes;
        self.state.set_status(&format!(
            "Moved {} to {}",
            updated_note.title, target_folder_title
        ));
        Ok(())
    }

    async fn move_selected_folder_to_notebook(&mut self, notebook_name: &str) -> Result<()> {
        let folder = self
            .state
            .selected_folder()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Select a notebook before using :move"))?;
        let folders = self.storage.list_folders().await?;
        let (target_folder_id, target_folder_title) =
            resolve_folder_destination(&folders, notebook_name)?;

        if folder.id == target_folder_id {
            anyhow::bail!("A notebook cannot be moved into itself");
        }

        let mut ancestor_id = target_folder_id.as_str();
        while !ancestor_id.is_empty() {
            if ancestor_id == folder.id {
                anyhow::bail!("A notebook cannot be moved into one of its subnotebooks");
            }
            ancestor_id = folders
                .iter()
                .find(|candidate| candidate.id == ancestor_id)
                .map(|candidate| candidate.parent_id.as_str())
                .unwrap_or("");
        }

        if folder.parent_id == target_folder_id {
            self.state.set_status(&format!(
                "{} is already inside {}",
                folder.title, target_folder_title
            ));
            return Ok(());
        }

        let mut updated_folder = folder.clone();
        updated_folder.parent_id = target_folder_id.clone();
        updated_folder.updated_time = now_ms();
        self.storage.update_folder(&updated_folder).await?;
        self.refresh_lists(false, Some(updated_folder.id.clone()), None)
            .await?;
        self.state.focus = FocusPanel::Notebooks;
        self.state.set_status(&format!(
            "Moved notebook {} to {}",
            updated_folder.title, target_folder_title
        ));
        Ok(())
    }

    async fn delete_orphaned_notes(&mut self) -> Result<()> {
        let folder_ids: HashSet<String> = self
            .storage
            .list_folders()
            .await?
            .into_iter()
            .map(|folder| folder.id)
            .collect();

        let orphan_ids: Vec<String> = self
            .storage
            .list_notes(None)
            .await?
            .into_iter()
            .filter(|note| note.parent_id.is_empty() || !folder_ids.contains(&note.parent_id))
            .map(|note| note.id)
            .collect();

        if orphan_ids.is_empty() {
            self.state.set_status("No orphaned notes found");
            return Ok(());
        }

        for note_id in &orphan_ids {
            self.storage.delete_note(note_id).await?;
            self.state.clear_new_note_marker_if(note_id);
        }

        self.refresh_current_lists().await?;
        self.state
            .set_status(&format!("Deleted {} orphaned notes", orphan_ids.len()));
        Ok(())
    }

    async fn import_from_database(&mut self, source_path: &Path) -> Result<()> {
        self.state
            .set_status(&format!("Importing from {}...", source_path.display()));
        let summary = import_database(self.storage.as_ref(), source_path).await?;
        self.refresh_current_lists().await?;
        self.state.set_status(&summary.describe());
        Ok(())
    }

    async fn import_from_jex(&mut self, source_path: &Path) -> Result<()> {
        self.state
            .set_status(&format!("Importing JEX from {}...", source_path.display()));
        let summary = neojoplin_core::import_jex(self.storage.as_ref(), source_path).await?;
        self.refresh_current_lists().await?;
        self.state.set_status(&summary.describe_import(source_path));
        Ok(())
    }

    async fn export_to_jex(&mut self, dest_path: &Path) -> Result<()> {
        self.state
            .set_status(&format!("Exporting JEX to {}...", dest_path.display()));
        let summary = neojoplin_core::export_jex(self.storage.as_ref(), dest_path).await?;
        self.state.set_status(&summary.describe_export(dest_path));
        Ok(())
    }

    fn apply_help_search(&mut self) {
        self.help_scroll = 0;
    }

    async fn update_filter_tag_completion(&mut self) -> Result<()> {
        let Some((replacement_start, token_prefix, tag_prefix)) =
            active_filter_tag_token(&self.state.filter_input)
        else {
            self.state.filter_completion = None;
            return Ok(());
        };

        let mut items: Vec<String> = self
            .storage
            .list_tags()
            .await?
            .into_iter()
            .filter(|tag| starts_with_ignore_case(&tag.title, tag_prefix))
            .map(|tag| {
                let mut completed = self.state.filter_input[..replacement_start].to_string();
                completed.push_str(token_prefix);
                completed.push_str(&tag.title);
                completed
            })
            .collect();
        items.sort_by_key(|item| item.to_lowercase());
        items.dedup();
        self.state.filter_completion = if items.is_empty() {
            None
        } else {
            Some(CompletionState { items, index: 0 })
        };
        Ok(())
    }

    async fn cycle_filter_tag_completion(&mut self, backwards: bool) -> Result<()> {
        let current_input = self.state.filter_input.clone();
        let reuse_existing = self
            .state
            .filter_completion
            .as_ref()
            .and_then(|completion| completion.current().map(|current| (completion, current)))
            .map(|(completion, current)| current_input == current && !completion.items.is_empty())
            .unwrap_or(false);

        if reuse_existing {
            let current = if let Some(completion) = self.state.filter_completion.as_mut() {
                completion.advance(backwards);
                completion.current().map(|current| current.to_string())
            } else {
                None
            };
            if let Some(current) = current {
                self.state.filter_input = current.clone();
                self.state.set_filter_query(current);
            }
            return Ok(());
        }

        self.update_filter_tag_completion().await?;
        let current = if let Some(completion) = self.state.filter_completion.as_mut() {
            if backwards {
                completion.index = completion.items.len() - 1;
            }
            completion.current().map(|current| current.to_string())
        } else {
            None
        };
        if let Some(current) = current {
            self.state.filter_input = current.clone();
            self.state.set_filter_query(current);
        }
        Ok(())
    }

    async fn create_note_from_file(&mut self, file_path: &Path) -> Result<()> {
        let body = tokio::fs::read_to_string(file_path)
            .await
            .with_context(|| format!("Failed to read {}", file_path.display()))?;
        let parent_id = self.default_parent_folder_id().await?;
        let title = file_path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Imported file".to_string());

        let note = Note {
            id: joplin_domain::joplin_id(),
            title: title.clone(),
            body,
            parent_id,
            created_time: now_ms(),
            updated_time: now_ms(),
            user_created_time: 0,
            user_updated_time: 0,
            is_shared: 0,
            share_id: None,
            master_key_id: None,
            encryption_applied: 0,
            encryption_cipher_text: None,
            is_conflict: 0,
            is_todo: 0,
            todo_completed: 0,
            todo_due: 0,
            source: String::new(),
            source_application: String::new(),
            order: 0,
            latitude: 0,
            longitude: 0,
            altitude: 0,
            author: String::new(),
            source_url: String::new(),
            application_data: String::new(),
            markup_language: 1,
            encryption_blob_encrypted: 0,
            conflict_original_id: String::new(),
            deleted_time: 0,
        };

        self.storage.create_note(&note).await?;
        self.state.mark_new_note(note.id.clone());
        self.state.focus = FocusPanel::Notes;
        self.refresh_lists(
            self.state.all_notebooks_mode,
            self.state.selected_folder_id().map(str::to_string),
            Some(note.id.clone()),
        )
        .await?;
        self.state.set_status(&format!(
            "Created note from {} - press r to rename it",
            title
        ));
        Ok(())
    }

    async fn tag_selected_note(&mut self, tag_name: &str) -> Result<()> {
        let tag_name = tag_name.trim();
        if tag_name.is_empty() {
            anyhow::bail!("Usage: :tag add <tag>");
        }

        let existing_tags = self.storage.list_tags().await?;
        let tag = if let Some(existing_tag) = resolve_tag_by_title(&existing_tags, tag_name) {
            existing_tag.clone()
        } else {
            let tag = Tag {
                id: joplin_domain::joplin_id(),
                title: tag_name.to_string(),
                created_time: now_ms(),
                updated_time: now_ms(),
                user_created_time: 0,
                user_updated_time: 0,
                parent_id: String::new(),
                encryption_blob_encrypted: 0,
                is_shared: 0,
                encryption_applied: 0,
                encryption_cipher_text: None,
                master_key_id: None,
            };
            self.storage.create_tag(&tag).await?;
            tag
        };

        self.tag_selected_note_by_id(&tag.id).await?;
        self.refresh_note_tag_cache().await?;
        self.state.set_status(&format!("Added tag {}", tag.title));
        Ok(())
    }

    async fn tag_selected_note_by_id(&mut self, tag_id: &str) -> Result<()> {
        let note = self
            .state
            .selected_note()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Select a note before using :tag"))?;

        if self
            .storage
            .get_note_tags(&note.id)
            .await?
            .iter()
            .any(|existing| existing.id == tag_id)
        {
            return Ok(());
        }

        let note_tag = NoteTag {
            id: joplin_domain::joplin_id(),
            note_id: note.id,
            tag_id: tag_id.to_string(),
            created_time: now_ms(),
            updated_time: now_ms(),
            user_created_time: 0,
            user_updated_time: 0,
            is_shared: 0,
            encryption_applied: 0,
            encryption_cipher_text: None,
            encryption_blob_encrypted: 0,
            master_key_id: None,
        };
        self.storage.add_note_tag(&note_tag).await?;
        Ok(())
    }

    async fn untag_selected_note(&mut self, tag_name: &str) -> Result<()> {
        let note = self
            .state
            .selected_note()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Select a note before using :tag"))?;
        let tag_name = tag_name.trim();
        if tag_name.is_empty() {
            anyhow::bail!("Usage: :tag remove <tag>");
        }

        let tags = self.storage.get_note_tags(&note.id).await?;
        let tag = resolve_tag_by_title(&tags, tag_name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("{} does not have tag {}", note.title, tag_name))?;
        self.untag_selected_note_by_id(&tag.id).await?;
        self.refresh_note_tag_cache().await?;
        self.state.set_status(&format!("Removed tag {}", tag.title));
        Ok(())
    }

    async fn untag_selected_note_by_id(&mut self, tag_id: &str) -> Result<()> {
        let note = self
            .state
            .selected_note()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Select a note before using :tag"))?;
        self.storage.remove_note_tag(&note.id, tag_id).await?;
        Ok(())
    }

    async fn list_selected_note_tags(&mut self) -> Result<()> {
        let note = self
            .state
            .selected_note()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Select a note before using :tag"))?;
        let tags = self.storage.get_note_tags(&note.id).await?;
        if tags.is_empty() {
            self.state
                .set_status(&format!("{} has no tags", note.title));
        } else {
            self.state.set_status(&format!(
                "Tags: {}",
                tags.iter()
                    .map(|tag| tag.title.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        Ok(())
    }

    async fn default_parent_folder_id(&mut self) -> Result<String> {
        if self.state.all_notebooks_mode {
            if let Some(folder) = self.state.folders.first() {
                return Ok(folder.id.clone());
            }

            self.create_notebook().await?;
            return self
                .state
                .folders
                .first()
                .map(|folder| folder.id.clone())
                .ok_or_else(|| anyhow::anyhow!("Failed to create notebook for new note"));
        }

        self.state
            .selected_folder()
            .map(|folder| folder.id.clone())
            .ok_or_else(|| anyhow::anyhow!("No notebook selected"))
    }
}

/// Run the TUI application
pub async fn run_app() -> Result<()> {
    let mut app = App::new().await?;
    app.run().await
}

/// Split a full WebDAV URL into (base_url, remote_path).
///
/// Joplin stores `sync.6.path` as the full URL including the sync folder,
/// e.g. `http://localhost:8080/webdav/shared`.
/// The WebDAV client expects the server root (`http://localhost:8080/webdav`)
/// and `SyncEngine` takes the folder path (`/shared`) separately.
fn split_webdav_url(full_url: &str) -> (String, String) {
    let trimmed = full_url.trim_end_matches('/');
    // Find the last '/' that is not part of the scheme "://"
    let scheme_end = trimmed.find("://").map(|i| i + 3).unwrap_or(0);
    if let Some(slash_pos) = trimmed[scheme_end..].rfind('/') {
        let abs_pos = scheme_end + slash_pos;
        let base = &trimmed[..abs_pos];
        let path = &trimmed[abs_pos..]; // starts with '/'
        if !path.is_empty() && path != "/" {
            return (base.to_string(), path.to_string());
        }
    }
    // No sub-path; use a default remote folder
    (trimmed.to_string(), "/neojoplin".to_string())
}

fn split_command_input(input: &str) -> (&str, &str, bool) {
    if let Some(index) = input.find(char::is_whitespace) {
        let command = &input[..index];
        let argument = &input[index + 1..];
        (command, argument, true)
    } else {
        (input, "", false)
    }
}

fn active_filter_tag_token(input: &str) -> Option<(usize, &'static str, &str)> {
    let token_start = input
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(0);
    let token = &input[token_start..];
    if let Some(rest) = token.strip_prefix("#=") {
        return Some((token_start, "#=", rest));
    }
    if let Some(rest) = token.strip_prefix('#') {
        return Some((token_start, "#", rest));
    }
    None
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.to_lowercase().starts_with(&prefix.to_lowercase())
}

fn resolve_folder_destination(folders: &[Folder], title: &str) -> Result<(String, String)> {
    let original = title.trim();
    let normalized = original.to_lowercase();
    if normalized.is_empty() {
        anyhow::bail!("Usage: :move <notebook>");
    }

    if normalized == "root" {
        return Ok((String::new(), "root".to_string()));
    }

    // First, try exact match on plain folder titles (case-insensitive)
    let plain_matches: Vec<&Folder> = folders
        .iter()
        .filter(|folder| folder.title.to_lowercase() == normalized)
        .collect();

    if plain_matches.len() == 1 {
        let folder = plain_matches[0];
        return Ok((folder.id.clone(), folder.title.clone()));
    }

    // If multiple plain matches, try display names (for disambiguated names from tab completion)
    let display_names = build_folder_display_names(folders);
    if let Some((folder_id, display_name)) = display_names
        .iter()
        .find(|(_, display_name)| display_name.to_lowercase() == normalized)
    {
        return Ok((folder_id.clone(), display_name.clone()));
    }

    // If multiple plain matches and no display name match, show helpful error
    if plain_matches.len() > 1 {
        let names: Vec<String> = plain_matches.iter().map(|f| f.title.clone()).collect();
        anyhow::bail!(
            "Multiple notebooks match '{}': {}. Use tab completion to select the correct one.",
            original,
            names.join(", ")
        );
    }

    // Try partial match on display names as fallback
    if let Some((folder_id, display_name)) = display_names
        .iter()
        .find(|(_, display_name)| display_name.to_lowercase().contains(&normalized))
    {
        return Ok((folder_id.clone(), display_name.clone()));
    }

    // No match found
    let available: Vec<String> = folders.iter().map(|f| f.title.clone()).collect();
    anyhow::bail!(
        "Notebook '{}' not found. Available notebooks: {}",
        original,
        available.join(", ")
    );
}

fn resolve_tag_by_title<'a>(tags: &'a [Tag], title: &str) -> Option<&'a Tag> {
    let normalized = title.trim().to_lowercase();
    tags.iter()
        .find(|tag| tag.title.to_lowercase() == normalized)
}

/// Load the E2EE service from disk (encryption.json + key files).
/// Reads the password from encryption.json first, then falls back to the
/// E2EE_PASSWORD env var or the project `.env` file.
async fn load_e2ee_service(data_dir: &Path) -> Result<joplin_sync::E2eeService> {
    use joplin_sync::{E2eeService, MasterKey};

    let encryption_config_path = data_dir.join("encryption.json");
    let stored_password = if encryption_config_path.exists() {
        if let Ok(content) = tokio::fs::read_to_string(&encryption_config_path).await {
            if let Ok(config) = serde_json::from_str::<serde_json::Value>(&content) {
                config
                    .get("master_password")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let env_password = std::env::var("E2EE_PASSWORD").ok().or_else(|| {
        if let Ok(env_path) = std::env::current_dir() {
            let env_file = env_path.join(".env");
            if env_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&env_file) {
                    for line in content.lines() {
                        if let Some((key, value)) = line.split_once('=') {
                            if key.trim() == "E2EE_PASSWORD" {
                                return Some(value.trim().to_string());
                            }
                        }
                    }
                }
            }
        }
        None
    });

    let master_password = stored_password.or(env_password).unwrap_or_default();

    let mut e2ee = E2eeService::new();
    if !master_password.is_empty() {
        e2ee.set_master_password(master_password);
    }

    if !encryption_config_path.exists() {
        return Ok(e2ee);
    }

    let config_content = tokio::fs::read_to_string(&encryption_config_path).await?;
    let config: serde_json::Value = serde_json::from_str(&config_content)?;

    let enabled = config
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !enabled {
        return Ok(e2ee);
    }

    if let Some(active_key_id) = config.get("active_master_key_id").and_then(|v| v.as_str()) {
        let keys_dir = data_dir.join("keys");
        let key_file = keys_dir.join(format!("{}.json", active_key_id));
        if key_file.exists() {
            let key_content = tokio::fs::read_to_string(&key_file).await?;
            match serde_json::from_str::<MasterKey>(&key_content) {
                Ok(master_key) => match e2ee.load_master_key(&master_key) {
                    Ok(()) => e2ee.set_active_master_key(active_key_id.to_string()),
                    Err(e) => {
                        tracing::warn!(
                            "Skipping local master key {} because it could not be loaded: {}",
                            active_key_id,
                            e
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        "Skipping local master key {} because it could not be parsed: {}",
                        active_key_id,
                        e
                    );
                }
            }
        } else {
            tracing::warn!("Master key file not found: {}", key_file.display());
        }
    }

    Ok(e2ee)
}
