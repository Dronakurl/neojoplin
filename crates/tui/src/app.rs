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

use neojoplin_core::{Storage, Note, Folder, now_ms};
use neojoplin_storage::SqliteStorage;
use neojoplin_sync::{SyncEngine, ReqwestWebDavClient, WebDavConfig};

use crate::config::Config;
use crate::state::{AppState, FocusPanel};
use crate::ui;

/// Main TUI application
pub struct App {
    state: AppState,
    storage: Arc<SqliteStorage>,
    config: Config,
    show_help: bool,
    help_scroll: u16,
}

impl App {
    /// Create new application
    pub async fn new() -> Result<Self> {
        let storage = Arc::new(SqliteStorage::new().await?);
        let config = Config::load()?;

        let mut state = AppState::new();

        // Load folders
        let folders = storage.list_folders().await?;
        state.set_folders(folders);

        // Load notes for first folder
        if let Some(folder) = state.selected_folder() {
            let notes = storage.list_notes(Some(&folder.id)).await?;
            state.set_notes(notes);
        }

        Ok(Self {
            state,
            storage,
            config,
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
                    ui::render_help(f, self.help_scroll);
                } else if self.state.show_quit_confirmation {
                    ui::render_quit_confirmation(f);
                } else if self.state.show_settings {
                    // TODO: Render settings
                } else {
                    ui::render_ui(f, &self.state);
                }
            })?;

            // Handle events
            if event::poll(Duration::from_millis(100))? {
                if let event::Event::Key(key) = event::read()? {
                    if self.handle_key_event(key).await? {
                        break; // Exit requested
                    }
                }
            }
        }
        Ok(())
    }

    /// Handle keyboard events
    async fn handle_key_event(&mut self, key: KeyEvent) -> Result<bool> {
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
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // S - Settings
                    self.state.toggle_settings();
                } else {
                    // s - Sync
                    self.sync().await?;
                }
            }

            // Panel navigation
            KeyCode::Tab => {
                self.state.next_panel();
            }
            KeyCode::BackTab => {
                self.state.prev_panel();
            }

            // Vim-style navigation
            KeyCode::Char('j') | KeyCode::Down => {
                self.state.move_selection(1);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.state.move_selection(-1);
            }

            // Enter - edit selected note
            KeyCode::Enter => {
                if self.state.focus == FocusPanel::Notes {
                    if let Some(note) = self.state.selected_note() {
                        let note_clone = note.clone();
                        self.edit_note(&note_clone).await?;
                    }
                }
            }

            // New note
            KeyCode::Char('n') => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // N - New folder
                    self.create_folder().await?;
                } else {
                    // n - New note
                    self.create_note().await?;
                }
            }

            // Delete
            KeyCode::Char('d') => {
                self.delete_selected().await?;
            }

            _ => {}
        }

        Ok(false)
    }

    /// Sync with WebDAV server
    async fn sync(&mut self) -> Result<()> {
        self.state.set_status("Starting sync...");
        // TODO: Implement actual sync with WebDAV
        // For now, just simulate sync
        std::thread::sleep(std::time::Duration::from_millis(500));
        self.state.set_status("Sync completed (not implemented)");
        Ok(())
    }

    /// Edit note in external editor
    async fn edit_note(&mut self, note: &Note) -> Result<()> {
        use neojoplin_core::Editor;

        self.state.set_status(&format!("Opening editor for: {}", note.title));

        let editor = Editor::new()
            .map_err(|e| anyhow::anyhow!("Failed to initialize editor: {}", e))?;

        let updated_body = editor.edit(&note.body, &note.title).await
            .map_err(|e| anyhow::anyhow!("Editor failed: {}", e))?;

        // Update note if content changed
        if updated_body != note.body {
            let mut updated_note = note.clone();
            updated_note.body = updated_body.clone();
            updated_note.updated_time = now_ms();

            self.storage.update_note(&updated_note).await?;

            // Update in-memory state
            if let Some(idx) = self.state.selected_note {
                if idx < self.state.notes.len() {
                    self.state.notes[idx].body = updated_body;
                }
            }
            self.state.load_note_content();

            self.state.set_status("Note updated successfully");
        } else {
            self.state.set_status("No changes made to note");
        }

        Ok(())
    }

    /// Create a new note
    async fn create_note(&mut self) -> Result<()> {
        use uuid::Uuid;

        self.state.set_status("Creating new note...");

        // For simplicity, create a note with a default title
        let title = format!("New Note {}", Uuid::new_v4().to_string()[..8].to_string());
        let note = Note {
            id: Uuid::new_v4().to_string(),
            title: title.clone(),
            body: String::new(),
            parent_id: self.state.selected_folder()
                .map(|f| f.id.clone())
                .unwrap_or_default(),
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

    /// Create a new folder
    async fn create_folder(&mut self) -> Result<()> {
        use uuid::Uuid;

        self.state.set_status("Creating new folder...");

        let title = format!("New Folder {}", Uuid::new_v4().to_string()[..8].to_string());
        let folder = Folder {
            id: Uuid::new_v4().to_string(),
            title: title.clone(),
            parent_id: String::new(), // Root folder
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

        // Reload notes for current folder
        self.reload_notes().await?;

        self.state.set_status(&format!("Created folder: {}", title));
        Ok(())
    }

    /// Delete selected item (note or folder)
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
                    self.state.set_status(&format!("Deleting folder: {}", folder.title));
                    self.storage.delete_folder(&folder_id).await?;
                    // Reload folders
                    let folders = self.storage.list_folders().await?;
                    self.state.set_folders(folders);
                    // Reload notes
                    self.reload_notes().await?;
                    self.state.set_status("Folder deleted");
                }
            }
            FocusPanel::Content => {
                self.state.set_status("Cannot delete from content panel");
            }
        }
        Ok(())
    }

    /// Reload notes for currently selected folder
    async fn reload_notes(&mut self) -> Result<()> {
        if let Some(folder) = self.state.selected_folder() {
            let notes = self.storage.list_notes(Some(&folder.id)).await?;
            self.state.set_notes(notes);
        }
        Ok(())
    }
}

/// Run the TUI application
pub async fn run_app() -> Result<()> {
    let mut app = App::new().await?;
    app.run().await
}
