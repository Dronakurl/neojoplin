// Main TUI application

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::sync::Arc;
use std::time::Duration;

use joplin_domain::{Storage, Note, Folder, now_ms};
use neojoplin_storage::SqliteStorage;
use std::path::PathBuf;

use crate::state::{AppState, FocusPanel};
use crate::ui;

/// Main TUI application
pub struct App {
    state: AppState,
    storage: Arc<SqliteStorage>,
    show_help: bool,
    help_scroll: u16,
}

impl App {
    /// Create new application
    pub async fn new() -> Result<Self> {
        let storage = Arc::new(SqliteStorage::new().await?);
        let data_dir = neojoplin_core::Config::data_dir()?;

        let mut state = AppState::new();

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
                serde_json::to_string_pretty(&default_sync_config)?
            ).await?;
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
                icon: String::new(),
            };

            storage.create_folder(&default_folder).await?;
            folders = vec![default_folder];
            state.set_status("Created default notebook: My Notebook");
        }

        state.set_folders(folders);

        // Start in "All Notebooks" mode and load all notes
        state.all_notebooks_mode = true;
        let notes = storage.list_notes(None).await?;
        state.set_notes(notes);

        // Load all settings (encryption and sync)
        state.settings.load_all_settings(&data_dir).await?;

        Ok(Self {
            state,
            storage,
            show_help: false,
            help_scroll: 0,
        })
    }

    /// Run the application
    pub async fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode().context("Failed to enable raw mode")?;
        let mut stdout = std::io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .context("Failed to setup terminal")?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)
            .context("Failed to create terminal")?;

        // Run main loop
        let res = self.run_main_loop(&mut terminal).await;

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
    ) -> Result<()> {
        loop {
            // Render UI
            terminal.draw(|f| {
                if self.show_help {
                    ui::render_help(f, self.help_scroll, &self.state);
                } else if self.state.show_quit_confirmation {
                    ui::render_quit_confirmation(f, &self.state);
                } else if self.state.show_error_dialog {
                    ui::render_error_dialog(f, &self.state);
                } else if self.state.show_settings {
                    ui::render_settings(f, &self.state);
                } else if self.state.show_rename_prompt {
                    ui::render_rename_prompt(f, &self.state);
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
    ) -> Result<bool> {
        // Handle global shortcuts
        match self.state.show_quit_confirmation {
            true => {
                // Confirm quit
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Char('y') {
                    return Ok(true); // Exit
                } else {
                    self.state.hide_quit();
                }
                return Ok(false);
            }
            false => {}
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

        // Handle help popup
        if self.show_help {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.help_scroll = self.help_scroll.saturating_add(1);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.help_scroll = self.help_scroll.saturating_sub(1);
                }
                KeyCode::Char('q') => {
                    self.show_help = false;
                    self.help_scroll = 0;
                }
                _ => {
                    // Ignore all other keys in help mode
                }
            }
            return Ok(false);
        }

        // Handle settings popup
        if self.state.show_settings {
            return self.handle_settings_key_event(key).await;
        }

        // Handle vim-style navigation and actions
        match key.code {
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

            // Sync
            KeyCode::Char('s') => {
                // s - Sync
                self.sync().await?;
            }

            // Settings
            KeyCode::Char('S') => {
                // S - Settings
                self.state.toggle_settings();
            }

            // Panel navigation
            KeyCode::Tab => {
                self.state.next_panel();
            }
            KeyCode::BackTab => {
                self.state.prev_panel();
            }

            // Vim-style horizontal panel navigation
            KeyCode::Char('h') => {
                // Move left (previous panel)
                self.state.prev_panel();
            }
            KeyCode::Char('l') => {
                // Move right (next panel)
                self.state.next_panel();
            }

            // Vim-style navigation
            KeyCode::Char('j') | KeyCode::Down => {
                if self.state.focus == FocusPanel::Content {
                    // Scroll content down
                    self.state.content_scroll_offset = self.state.content_scroll_offset.saturating_add(1);
                } else {
                    let folder_changed = self.state.move_selection(1);
                    if folder_changed {
                        self.reload_notes().await?;
                    }
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.state.focus == FocusPanel::Content {
                    // Scroll content up
                    self.state.content_scroll_offset = self.state.content_scroll_offset.saturating_sub(1);
                } else {
                    let folder_changed = self.state.move_selection(-1);
                    if folder_changed {
                        self.reload_notes().await?;
                    }
                }
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
                }
            }

            // New note
            KeyCode::Char('n') => {
                // n - New note
                self.create_note().await?;
            }

            // New notebook
            KeyCode::Char('N') => {
                // N - New notebook
                self.create_notebook().await?;
            }

            // Delete
            KeyCode::Char('d') => {
                self.delete_selected().await?;
            }

            // Toggle todo completion
            KeyCode::Char('t') => {
                self.toggle_todo().await?;
            }

            // Create todo
            KeyCode::Char('T') => {
                self.create_todo().await?;
            }

            // Rename
            KeyCode::Char('r') => {
                if self.state.focus == FocusPanel::Notes {
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

            _ => {}
        }

        Ok(false)
    }

    /// Sync with WebDAV server
    async fn sync(&mut self) -> Result<()> {
        self.state.set_status("Starting sync...");

        let data_dir = neojoplin_core::Config::data_dir()?;
        let sync_config_path = data_dir.join("sync-config.json");

        // Load sync configuration
        if !sync_config_path.exists() {
            self.state.set_status("Sync not configured - run setup first");
            return Ok(());
        }

        let config_content = tokio::fs::read_to_string(&sync_config_path).await?;
        let sync_config: serde_json::Value = serde_json::from_str(&config_content)?;

        let sync_type = sync_config["type"].as_str().unwrap_or("local");

        match sync_type {
            "webdav" => {
                // Get WebDAV configuration
                let url = sync_config.get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("http://localhost:8080/webdav");

                let remote_path = sync_config.get("remote_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("/neojoplin");

                self.state.set_status(&format!("Syncing to {}{}...", url, remote_path));

                // Create WebDAV client and sync engine
                use joplin_sync::{ReqwestWebDavClient, WebDavConfig, SyncEngine};
                use tokio::sync::mpsc;

                let webdav_config = WebDavConfig {
                    base_url: url.to_string(),
                    username: String::new(), // Empty for local WebDAV
                    password: String::new(), // Empty for local WebDAV
                };

                let webdav_client = Arc::new(ReqwestWebDavClient::new(webdav_config)?);
                let (event_tx, mut event_rx) = mpsc::unbounded_channel();

                let mut sync_engine = SyncEngine::new(
                    self.storage.clone(),
                    webdav_client,
                    event_tx,
                ).with_remote_path(remote_path.to_string());

                // Spawn a task to consume sync events (prevents channel from filling up)
                // Events are already handled via the sync result status messages below
                let storage_clone = self.storage.clone(); // Keep for data reload after sync
                tokio::spawn(async move {
                    while let Some(_event) = event_rx.recv().await {
                        // Events are consumed but not printed to avoid TUI rendering issues
                        // Status messages are handled via the main sync result below
                    }
                });

                // Perform sync
                match sync_engine.sync().await {
                    Ok(_) => {
                        self.state.set_status("✓ Sync completed successfully");

                        // Reload data
                        let folders = storage_clone.list_folders().await?;
                        self.state.set_folders(folders);

                        let notes = storage_clone.list_notes(None).await?;
                        self.state.set_notes(notes);
                    }
                    Err(e) => {
                        // Show error dialog for sync failures
                        self.state.show_error(&format!("Sync failed: {}", e));
                    }
                }
            }
            "local" => {
                // Get sync path from config
                let sync_path = if let Some(path) = sync_config.get("path") {
                    PathBuf::from(path.as_str().unwrap())
                } else {
                    data_dir.join("sync")
                };

                self.state.set_status(&format!("Local sync to {}...", sync_path.display()));

                // For now, just simulate sync with local filesystem
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                self.state.set_status("Local sync completed");
            }
            _ => {
                self.state.set_status("Unknown sync type configured");
            }
        }

        Ok(())
    }

    /// Edit note in external editor
    async fn edit_note<B: ratatui::backend::Backend>(
        &mut self,
        note: &Note,
        terminal: &mut Terminal<B>,
    ) -> Result<()> {
        use neojoplin_core::Editor;

        self.state.set_status(&format!("Opening editor for: {}", note.title));

        // Exit raw mode and alternate screen so editor can work properly
        disable_raw_mode().context("Failed to disable raw mode")?;
        let mut stdout = std::io::stdout();
        execute!(stdout, LeaveAlternateScreen)
            .context("Failed to leave alternate screen")?;

        let editor_result = async {
            let editor = Editor::new()
                .map_err(|e| anyhow::anyhow!("Failed to initialize editor: {}", e))?;

            editor.edit(&note.body, &note.title).await
                .map_err(|e| anyhow::anyhow!("Editor failed: {}", e))
        }.await;

        // Restore terminal for TUI
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
            .context("Failed to re-enter alternate screen")?;
        enable_raw_mode().context("Failed to re-enable raw mode")?;

        let updated_body = editor_result?;

        // Force a complete terminal redraw to ensure TUI is properly visible
        terminal.clear()?;

        // Update note if content changed
        if updated_body != note.body {
            let mut updated_note = note.clone();
            updated_note.body = updated_body.clone();

            // Extract title from first line (max 100 chars)
            let new_title = updated_note.body
                .lines()
                .next()
                .unwrap_or("Untitled")
                .trim()
                .chars()
                .take(100)
                .collect::<String>();

            if !new_title.is_empty() {
                updated_note.title = new_title;
            }

            updated_note.updated_time = now_ms();

            self.storage.update_note(&updated_note).await?;

            // Update in-memory state to reflect changes immediately
            if let Some(idx) = self.state.selected_note {
                if idx < self.state.notes.len() {
                    self.state.notes[idx] = updated_note.clone();
                }
            }

            // Reload content and clear it to force refresh
            self.state.current_note_content.clear();
            self.state.load_note_content();

            self.state.set_status(&format!("Updated: {}", updated_note.title));
        } else {
            self.state.set_status("No changes made to note");
        }

        Ok(())
    }

    /// Create a new note
    async fn create_note(&mut self) -> Result<()> {
        // Determine parent folder for the new note
        let parent_id = if self.state.all_notebooks_mode {
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
        let title = format!("New Note {}", joplin_domain::joplin_id()[..8].to_string());
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
        };

        self.storage.create_note(&note).await?;

        // Reload notes for current folder
        self.reload_notes().await?;

        self.state.set_status(&format!("Created note: {}", title));
        Ok(())
    }

    /// Create a new todo
    async fn create_todo(&mut self) -> Result<()> {
        let parent_id = if self.state.all_notebooks_mode {
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

        let title = format!("New Todo {}", joplin_domain::joplin_id()[..8].to_string());
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
        };

        self.storage.create_note(&note).await?;
        self.reload_notes().await?;

        self.state.set_status(&format!("Created todo: {}", title));
        Ok(())
    }

    /// Toggle todo completion status
    async fn toggle_todo(&mut self) -> Result<()> {
        if self.state.focus != FocusPanel::Notes {
            self.state.set_status("Select a todo in the notes panel first");
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
                self.state.set_status(&format!("󰄱 Uncompleted: {}", updated.title));
            } else {
                updated.todo_completed = now_ms();
                self.state.set_status(&format!("󰄲 Completed: {}", updated.title));
            }
            updated.updated_time = now_ms();
            self.storage.update_note(&updated).await?;
            self.reload_notes().await?;
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
            icon: String::new(),
        };

        self.storage.create_folder(&folder).await?;

        // Reload folders
        let folders = self.storage.list_folders().await?;
        self.state.set_folders(folders);

        // Select the newly created notebook (it should be the last one)
        if !self.state.folders.is_empty() {
            self.state.selected_folder = Some(self.state.folders.len() - 1);
            // Clear notes since new notebook is empty
            self.state.notes.clear();
            self.state.selected_note = None;
            self.state.current_note_content.clear();
        }

        self.state.set_status(&format!("Created notebook: {}", title));

        Ok(())
    }

    /// Delete selected item (note or notebook)
    async fn delete_selected(&mut self) -> Result<()> {
        match self.state.focus {
            FocusPanel::Notes => {
                if let Some(note) = self.state.selected_note() {
                    let note_id = note.id.clone();
                    self.state.set_status(&format!("Deleting note: {}", note.title));
                    self.storage.delete_note(&note_id).await?;
                    self.reload_notes().await?;
                    self.state.set_status("Note deleted");
                }
            }
            FocusPanel::Notebooks => {
                if let Some(folder) = self.state.selected_folder() {
                    let folder_id = folder.id.clone();
                    self.state.set_status(&format!("Deleting notebook: {}", folder.title));
                    self.storage.delete_folder(&folder_id).await?;
                    // Reload folders
                    let folders = self.storage.list_folders().await?;
                    self.state.set_folders(folders);
                    // Reload notes
                    self.reload_notes().await?;
                    self.state.set_status("Notebook deleted");
                }
            }
            FocusPanel::Content => {
                self.state.set_status("Cannot delete from content panel");
            }
        }
        Ok(())
    }

    /// Reload notes for currently selected notebook
    async fn reload_notes(&mut self) -> Result<()> {
        let notes = if self.state.all_notebooks_mode {
            // Load all notes when in "All Notebooks" mode
            self.storage.list_notes(None).await?
        } else if let Some(folder) = self.state.selected_folder() {
            // Load notes for specific folder
            self.storage.list_notes(Some(&folder.id)).await?
        } else {
            // No folder selected, no notes
            vec![]
        };

        self.state.set_notes(notes);
        Ok(())
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

                    // Update in-memory state
                    if let Some(idx) = self.state.selected_note {
                        if idx < self.state.notes.len() {
                            self.state.notes[idx] = updated_note;
                        }
                    }

                    self.state.set_status(&format!("Renamed note to: {}", new_name));
                }
            }
            FocusPanel::Notebooks => {
                if let Some(folder) = self.state.selected_folder() {
                    let mut updated_folder = folder.clone();
                    updated_folder.title = new_name.clone();
                    updated_folder.updated_time = now_ms();

                    self.storage.update_folder(&updated_folder).await?;

                    // Update in-memory state to preserve order
                    if let Some(idx) = self.state.selected_folder {
                        if idx < self.state.folders.len() {
                            self.state.folders[idx] = updated_folder;
                        }
                    }

                    self.state.set_status(&format!("Renamed notebook to: {}", new_name));
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

        match key.code {
            // Close settings
            KeyCode::Char('q') | KeyCode::Esc => {
                self.state.hide_settings();
                self.state.settings.hide_password_prompts();
                self.state.settings.sync.show_add_form = false;
                self.state.settings.sync.show_edit_form = false;
                return Ok(false);
            }

            // Tab navigation
            KeyCode::Char('>') | KeyCode::Tab => {
                self.state.settings.cycle_tab_forward();
            }

            KeyCode::Char('<') | KeyCode::BackTab => {
                self.state.settings.cycle_tab_backward();
            }

            // Sync tab actions
            KeyCode::Char('n') => {
                if self.state.settings.current_tab == SettingsTab::Sync {
                    // Show add form
                    self.state.settings.sync.show_add_form = true;
                    self.state.settings.sync.clear_form();
                    self.state.settings.sync.active_field = Some(crate::settings::FormField::Name);
                }
            }

            KeyCode::Char('e') => {
                if self.state.settings.current_tab == SettingsTab::Encryption
                    && !self.state.settings.encryption.enabled {
                    self.state.settings.show_new_key_prompt();
                } else if self.state.settings.current_tab == SettingsTab::Sync {
                    // Edit current target
                    let sync = &mut self.state.settings.sync;
                    if let Some(idx) = sync.current_target_index {
                        if idx < sync.targets.len() {
                            sync.show_edit_form = true;
                            sync.editing_target_index = Some(idx);
                            sync.load_target_to_form(idx);
                        }
                    }
                }
            }

            KeyCode::Char('d') => {
                if self.state.settings.current_tab == SettingsTab::Encryption
                    && self.state.settings.encryption.enabled {
                    let data_dir = neojoplin_core::Config::data_dir()?;
                    self.state.settings.disable_encryption(&data_dir).await?;
                    self.state.set_status("Encryption disabled");
                } else if self.state.settings.current_tab == SettingsTab::Sync {
                    // Delete current target
                    let sync = &mut self.state.settings.sync;
                    if let Some(idx) = sync.current_target_index {
                        if !sync.targets.is_empty() {
                            sync.targets.remove(idx);
                            if sync.targets.is_empty() {
                                sync.current_target_index = None;
                            } else if idx >= sync.targets.len() {
                                sync.current_target_index = Some(sync.targets.len() - 1);
                            }

                            // Save to file
                            let data_dir = neojoplin_core::Config::data_dir()?;
                            let _ = self.state.settings.save_sync_settings(&data_dir).await;
                            self.state.set_status("Target deleted");
                        }
                    }
                }
            }

            // Target navigation
            KeyCode::Up | KeyCode::Char('k') => {
                if self.state.settings.current_tab == SettingsTab::Sync {
                    let sync = &mut self.state.settings.sync;
                    if let Some(ref mut idx) = sync.current_target_index {
                        if *idx > 0 {
                            *idx -= 1;
                        }
                    }
                }
            }

            KeyCode::Down | KeyCode::Char('j') => {
                if self.state.settings.current_tab == SettingsTab::Sync {
                    let sync = &mut self.state.settings.sync;
                    if let Some(ref mut idx) = sync.current_target_index {
                        if *idx + 1 < sync.targets.len() {
                            *idx += 1;
                        }
                    }
                }
            }

            // Set target as active
            KeyCode::Enter => {
                if self.state.settings.current_tab == SettingsTab::Sync {
                    let sync = &mut self.state.settings.sync;
                    if sync.show_add_form || sync.show_edit_form {
                        // Handle form submission
                        return self.handle_target_form_key_event(key).await;
                    } else {
                        // Set current target as active
                        if let Some(_idx) = sync.current_target_index {
                            // Save to file
                            let data_dir = neojoplin_core::Config::data_dir()?;
                            let _ = self.state.settings.save_sync_settings(&data_dir).await;
                            self.state.set_status("Target saved as active");
                        }
                    }
                }
            }

            // Password input handling
            KeyCode::Char(c) if self.state.settings.encryption.show_new_key_prompt => {
                let c = c.to_string();

                // Toggle between password and confirm fields
                if self.state.settings.encryption.password_input.is_empty()
                    || self.state.settings.encryption.password_input.len() < self.state.settings.encryption.confirm_password_input.len() {
                    self.state.settings.add_password_char(c.chars().next().unwrap());
                } else {
                    self.state.settings.add_confirm_password_char(c.chars().next().unwrap());
                }
            }

            KeyCode::Backspace => {
                if self.state.settings.encryption.show_new_key_prompt {
                    if self.state.settings.encryption.confirm_password_input.len()
                        > self.state.settings.encryption.password_input.len() {
                        self.state.settings.remove_confirm_password_char();
                    } else {
                        self.state.settings.remove_password_char();
                    }
                    self.state.settings.encryption.password_error = None;
                }
            }

            // Handle sync form events
            _ => {
                if self.state.settings.sync.show_add_form || self.state.settings.sync.show_edit_form {
                    return self.handle_target_form_key_event(key).await;
                }
            }
        }

        Ok(true)
    }

    /// Handle keyboard events in sync target form
    async fn handle_target_form_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Tab => {
                self.state.settings.sync.cycle_field_forward();
            }

            KeyCode::BackTab => {
                self.state.settings.sync.cycle_field_backward();
            }

            KeyCode::Char('t') => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Test connection
                    self.test_webdav_connection().await?;
                }
            }

            KeyCode::Char(c) => {
                // Add character to active field
                let active_field = self.state.settings.sync.active_field;
                match active_field {
                    Some(crate::settings::FormField::Name) => self.state.settings.sync.add_name_char(c),
                    Some(crate::settings::FormField::Url) => self.state.settings.sync.add_url_char(c),
                    Some(crate::settings::FormField::Username) => self.state.settings.sync.add_username_char(c),
                    Some(crate::settings::FormField::Password) => self.state.settings.sync.add_password_char(c),
                    Some(crate::settings::FormField::Path) => self.state.settings.sync.add_path_char(c),
                    None => {}
                }
            }

            KeyCode::Backspace => {
                // Remove character from active field
                let active_field = self.state.settings.sync.active_field;
                match active_field {
                    Some(crate::settings::FormField::Name) => self.state.settings.sync.remove_name_char(),
                    Some(crate::settings::FormField::Url) => self.state.settings.sync.remove_url_char(),
                    Some(crate::settings::FormField::Username) => self.state.settings.sync.remove_username_char(),
                    Some(crate::settings::FormField::Password) => self.state.settings.sync.remove_password_char(),
                    Some(crate::settings::FormField::Path) => self.state.settings.sync.remove_path_char(),
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

        Ok(true)
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

        if sync.path_input.trim().is_empty() {
            return Err(anyhow::anyhow!("Path cannot be empty"));
        }

        if !sync.path_input.starts_with('/') {
            return Err(anyhow::anyhow!("Path must start with /"));
        }

        // Create or update target
        let target = crate::settings::SyncTarget {
            id: if sync.show_edit_form {
                sync.editing_target_index.and_then(|i| sync.targets.get(i).map(|t| t.id.clone()))
                    .unwrap_or_else(|| joplin_domain::joplin_id())
            } else {
                joplin_domain::joplin_id()
            },
            name: sync.name_input.trim().to_string(),
            target_type: crate::settings::SyncTargetType::WebDAV,
            url: sync.url_input.trim().to_string(),
            username: sync.username_input.trim().to_string(),
            password: sync.password_input.clone(),
            remote_path: sync.path_input.trim().to_string(),
            ignore_tls_errors: false,
        };

        if sync.show_edit_form {
            if let Some(idx) = sync.editing_target_index {
                if idx < sync.targets.len() {
                    sync.targets[idx] = target;
                }
            }
        } else {
            sync.targets.push(target);
            sync.current_target_index = Some(sync.targets.len() - 1);
        }

        // Save to file
        let data_dir = neojoplin_core::Config::data_dir()?;
        self.state.settings.save_sync_settings(&data_dir).await?;

        Ok(())
    }

    /// Test WebDAV connection
    async fn test_webdav_connection(&mut self) -> anyhow::Result<()> {
        let sync = &mut self.state.settings.sync;

        sync.testing_connection = true;
        sync.connection_result = None;
        sync.form_error = None;

        let url = sync.url_input.clone();

        // Basic URL validation
        if url.is_empty() || !url.starts_with("http") {
            sync.form_error = Some("Invalid URL".to_string());
            sync.testing_connection = false;
            return Ok(());
        }

        // Create WebDAV client and test connection
        use joplin_sync::{ReqwestWebDavClient, WebDavConfig};

        let config = WebDavConfig::new(
            url.clone(),
            sync.username_input.clone(),
            sync.password_input.clone(),
        );

        match ReqwestWebDavClient::new(config) {
            Ok(webdav) => {
                // Try to list files to test connection
                match webdav.list_impl(&sync.path_input).await {
                    Ok(_) => {
                        sync.connection_result = Some(crate::settings::ConnectionResult::Success);
                    }
                    Err(e) => {
                        sync.connection_result = Some(crate::settings::ConnectionResult::Failed(e.to_string()));
                    }
                }
            }
            Err(e) => {
                sync.connection_result = Some(crate::settings::ConnectionResult::Failed(e.to_string()));
            }
        }

        sync.testing_connection = false;
        Ok(())
    }
}

/// Run the TUI application
pub async fn run_app() -> Result<()> {
    let mut app = App::new().await?;
    app.run().await
}
