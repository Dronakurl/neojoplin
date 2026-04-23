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

use joplin_domain::{now_ms, Folder, Note, Storage};
use neojoplin_storage::SqliteStorage;
use std::path::Path;

use crate::state::{AppState, FocusPanel, NoteSortMode, NotebookSortMode, PendingDelete};
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
        let mut terminal = Terminal::new(backend).context("Failed to create terminal")?;

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
                } else if self.state.pending_delete.is_some() {
                    ui::render_delete_confirmation(f, &self.state);
                } else if self.state.show_error_dialog {
                    ui::render_error_dialog(f, &self.state);
                } else if self.state.show_settings {
                    ui::render_settings(f, &self.state);
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
    ) -> Result<bool> {
        // Handle global shortcuts
        if self.state.show_quit_confirmation {
            // Confirm quit
            if key.code == KeyCode::Char('q') || key.code == KeyCode::Char('y') {
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

        if self.state.show_sort_popup {
            return self.handle_sort_popup_key_event(key).await;
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

            KeyCode::Char(',') => {
                if matches!(self.state.focus, FocusPanel::Notebooks | FocusPanel::Notes) {
                    self.state.open_sort_popup();
                } else {
                    self.state
                        .set_status("Focus notebooks or notes to change sorting");
                }
            }

            KeyCode::Char('f') => {
                if matches!(self.state.focus, FocusPanel::Notebooks | FocusPanel::Notes) {
                    self.state.open_filter_prompt();
                } else {
                    self.state
                        .set_status("Focus notebooks or notes to filter the current list");
                }
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
                    self.state.content_scroll_offset =
                        self.state.content_scroll_offset.saturating_add(1);
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
                    self.state.content_scroll_offset =
                        self.state.content_scroll_offset.saturating_sub(1);
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
                self.request_delete_selected();
            }

            // Immediate note delete (hidden from ribbon)
            KeyCode::Char('D') => {
                self.delete_selected_note_immediately().await?;
            }

            // Toggle todo completion (space bar, like most task managers)
            KeyCode::Char(' ') if self.state.focus == FocusPanel::Notes => {
                self.toggle_todo().await?;
            }

            // Toggle todo completion (t key)
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

        // Use the loaded settings (from settings.json) to get the sync target
        let sync_settings = &self.state.settings.sync;
        let target = match sync_settings
            .current_target_index
            .and_then(|i| sync_settings.targets.get(i))
        {
            Some(t) => t.clone(),
            None => {
                self.state.set_status(
                    "Sync not configured. Go to Settings (s) → Sync tab to add a WebDAV target.",
                );
                return Ok(());
            }
        };

        if target.url.is_empty() {
            self.state
                .set_status("Sync URL is empty. Go to Settings (s) → Sync tab to configure.");
            return Ok(());
        }

        // Split the full URL (e.g. http://localhost:8080/webdav/shared) into
        // base_url (http://localhost:8080/webdav) + remote_path (/shared).
        let (base_url, remote_path) = split_webdav_url(&target.url);

        self.state
            .set_status(&format!("Syncing to {}{}...", base_url, remote_path));

        use joplin_sync::{ReqwestWebDavClient, SyncEngine, WebDavConfig};
        use tokio::sync::mpsc;

        let webdav_config = WebDavConfig {
            base_url: base_url.clone(),
            username: target.username.clone(),
            password: target.password.clone(),
        };

        let webdav_client = Arc::new(ReqwestWebDavClient::new(webdav_config)?);
        let (event_tx, mut event_rx) = mpsc::unbounded_channel();

        let mut sync_engine = SyncEngine::new(self.storage.clone(), webdav_client, event_tx)
            .with_remote_path(remote_path.clone());

        // Load E2EE service from .env / encryption.json (same logic as CLI)
        if let Ok(e2ee) = load_e2ee_service(&data_dir).await {
            if e2ee.is_enabled() {
                sync_engine = sync_engine.with_e2ee(e2ee);
            }
        }

        // Consume sync events without printing (avoids TUI rendering issues)
        tokio::spawn(async move { while let Some(_event) = event_rx.recv().await {} });

        match sync_engine.sync().await {
            Ok(_) => {
                self.state.set_status("✓ Sync completed successfully");
                let selected_folder_id = self.state.selected_folder_id().map(str::to_string);
                let selected_note_id = self.state.selected_note_id().map(str::to_string);
                let all_notebooks_mode = self.state.all_notebooks_mode;
                self.refresh_lists(all_notebooks_mode, selected_folder_id, selected_note_id)
                    .await?;
            }
            Err(e) => {
                self.state.show_error(&format!("Sync failed: {}", e));
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

        let full_content = editor_result?;

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
    fn request_delete_selected(&mut self) {
        match self.state.focus {
            FocusPanel::Notes => {
                if let Some(note) = self.state.selected_note() {
                    self.state.confirm_delete(PendingDelete::Note {
                        id: note.id.clone(),
                        title: note.title.clone(),
                    });
                }
            }
            FocusPanel::Notebooks => {
                if let Some(folder) = self.state.selected_folder() {
                    self.state.confirm_delete(PendingDelete::Notebook {
                        id: folder.id.clone(),
                        title: folder.title.clone(),
                    });
                }
            }
            FocusPanel::Content => {
                self.state.set_status("Cannot delete from content panel");
            }
        }
    }

    /// Reload notes for currently selected notebook
    async fn reload_notes(&mut self) -> Result<()> {
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
                return Ok(false);
            }

            // Tab navigation (h/l and </> and Tab/BackTab)
            KeyCode::Char('l') | KeyCode::Char('>') | KeyCode::Tab => {
                self.state.settings.cycle_tab_forward();
            }

            KeyCode::Char('h') | KeyCode::Char('<') | KeyCode::BackTab => {
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
                    if let Some(idx) = sync.current_target_index {
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
                    self.state.set_status("Encryption disabled");
                } else if self.state.settings.current_tab == SettingsTab::Sync {
                    let sync = &mut self.state.settings.sync;
                    if sync.current_target_index.is_some() && !sync.targets.is_empty() {
                        sync.confirm_delete = true;
                    }
                }
            }

            // Navigate target list
            KeyCode::Up | KeyCode::Char('k')
                if self.state.settings.current_tab == SettingsTab::Sync =>
            {
                let sync = &mut self.state.settings.sync;
                if let Some(ref mut idx) = sync.current_target_index {
                    if *idx > 0 {
                        *idx -= 1;
                    }
                }
            }

            KeyCode::Down | KeyCode::Char('j')
                if self.state.settings.current_tab == SettingsTab::Sync =>
            {
                let sync = &mut self.state.settings.sync;
                if let Some(ref mut idx) = sync.current_target_index {
                    if *idx + 1 < sync.targets.len() {
                        *idx += 1;
                    }
                }
            }

            // Save active target
            KeyCode::Enter if self.state.settings.current_tab == SettingsTab::Sync => {
                if let Some(_idx) = self.state.settings.sync.current_target_index {
                    let data_dir = neojoplin_core::Config::data_dir()?;
                    let _ = self.state.settings.save_sync_settings(&data_dir).await;
                    self.state.set_status("Target saved as active");
                }
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
        let mut folders = self.storage.list_folders().await?;
        self.state.sort_folders(&mut folders, &all_notes);
        folders = self.state.filter_folders(folders);
        self.state.set_folders(folders);

        if all_notebooks_mode {
            self.state.set_folder(None);
        } else if let Some(folder_id) = selected_folder_id.as_deref() {
            if !self.state.select_folder_by_id(folder_id) && !self.state.folders.is_empty() {
                self.state.set_folder(Some(0));
            }
        } else if self.state.folders.is_empty() {
            self.state.set_folder(None);
        }

        let mut notes = if self.state.all_notebooks_mode {
            all_notes
        } else if let Some(folder) = self.state.selected_folder() {
            self.storage.list_notes(Some(&folder.id)).await?
        } else {
            Vec::new()
        };
        self.state.sort_notes(&mut notes);
        notes = self.state.filter_notes(notes);
        self.state.set_notes(notes);

        if let Some(note_id) = selected_note_id.as_deref() {
            self.state.select_note_by_id(note_id);
        }

        Ok(())
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
                if let Some(idx) = sync.current_target_index {
                    if !sync.targets.is_empty() {
                        sync.targets.remove(idx);
                        if sync.targets.is_empty() {
                            sync.current_target_index = None;
                        } else if idx >= sync.targets.len() {
                            sync.current_target_index = Some(sync.targets.len() - 1);
                        }
                        let data_dir = neojoplin_core::Config::data_dir()?;
                        let _ = self.state.settings.save_sync_settings(&data_dir).await;
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
                self.perform_pending_delete().await?;
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.state.clear_pending_delete();
            }
            _ => {}
        }

        Ok(false)
    }

    async fn handle_filter_prompt_key_event(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char(c) => {
                self.state.add_filter_char(c);
                self.refresh_current_lists().await?;
            }
            KeyCode::Backspace => {
                self.state.remove_filter_char();
                self.refresh_current_lists().await?;
            }
            KeyCode::Enter => {
                self.state.close_filter_prompt(false);
                self.refresh_current_lists().await?;
            }
            KeyCode::Esc => {
                self.state.close_filter_prompt(true);
                self.refresh_current_lists().await?;
            }
            _ => {}
        }

        Ok(false)
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

    async fn perform_pending_delete(&mut self) -> Result<()> {
        let pending = self.state.pending_delete.clone();
        self.state.clear_pending_delete();

        match pending {
            Some(PendingDelete::Note { id, title }) => {
                self.state.set_status(&format!("Deleting note: {}", title));
                self.storage.delete_note(&id).await?;
                self.state.clear_new_note_marker_if(&id);
                self.refresh_lists(
                    self.state.all_notebooks_mode,
                    self.state.selected_folder_id().map(str::to_string),
                    None,
                )
                .await?;
                self.state.set_status("Note deleted");
            }
            Some(PendingDelete::Notebook { id, title }) => {
                self.state
                    .set_status(&format!("Deleting notebook: {}", title));
                self.storage.delete_folder(&id).await?;
                self.state.clear_new_folder_marker_if(&id);
                self.refresh_lists(false, None, None).await?;
                self.state.set_status("Notebook deleted");
            }
            None => {}
        }

        Ok(())
    }

    async fn delete_selected_note_immediately(&mut self) -> Result<()> {
        if self.state.focus != FocusPanel::Notes {
            self.state
                .set_status("D deletes notes immediately only from the notes panel");
            return Ok(());
        }

        if let Some(note) = self.state.selected_note() {
            let note_id = note.id.clone();
            self.state
                .set_status(&format!("Deleting note immediately: {}", note.title));
            self.storage.delete_note(&note_id).await?;
            self.state.clear_new_note_marker_if(&note_id);
            self.refresh_lists(
                self.state.all_notebooks_mode,
                self.state.selected_folder_id().map(str::to_string),
                None,
            )
            .await?;
            self.state.set_status("Note deleted");
        }

        Ok(())
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

/// Load the E2EE service from disk (encryption.json + key files).
/// Reads the password from the E2EE_PASSWORD env var or the project `.env` file.
async fn load_e2ee_service(data_dir: &Path) -> Result<joplin_sync::E2eeService> {
    use joplin_sync::{E2eeService, MasterKey};

    let encryption_config_path = data_dir.join("encryption.json");
    if !encryption_config_path.exists() {
        return Ok(E2eeService::new());
    }

    let config_content = tokio::fs::read_to_string(&encryption_config_path).await?;
    let config: serde_json::Value = serde_json::from_str(&config_content)?;

    let enabled = config
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !enabled {
        return Ok(E2eeService::new());
    }

    // Read master password from encryption.json (stored on enable), then fall back to env
    let master_password = config
        .get("master_password")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| std::env::var("E2EE_PASSWORD").ok())
        .unwrap_or_default();

    let mut e2ee = E2eeService::new();
    if !master_password.is_empty() {
        e2ee.set_master_password(master_password);
    }

    if let Some(active_key_id) = config.get("active_master_key_id").and_then(|v| v.as_str()) {
        let keys_dir = data_dir.join("keys");
        let key_file = keys_dir.join(format!("{}.json", active_key_id));
        if key_file.exists() {
            let key_content = tokio::fs::read_to_string(&key_file).await?;
            let master_key: MasterKey = serde_json::from_str(&key_content)?;
            e2ee.load_master_key(&master_key)?;
            e2ee.set_active_master_key(active_key_id.to_string());
        }
    }

    Ok(e2ee)
}
