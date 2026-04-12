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

use neojoplin_core::Storage;
use neojoplin_storage::SqliteStorage;

use crate::config::Config;
use crate::state::{AppState, FocusPanel};
use crate::ui;

/// Main TUI application
pub struct App {
    state: AppState,
    storage: Arc<SqliteStorage>,
    config: Config,
    show_help: bool,
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
                    ui::render_help(f);
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
                if key.code == KeyCode::Char('q') {
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
            self.show_help = false;
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
                    self.state.set_status("Syncing...");
                    // TODO: Implement sync
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
                        self.state.set_status(&format!("Editing: {}", note.title));
                        // TODO: Launch external editor
                    }
                }
            }

            // New note
            KeyCode::Char('n') => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    // N - New folder
                    self.state.set_status("New folder not implemented yet");
                } else {
                    // n - New note
                    self.state.set_status("New note not implemented yet");
                }
            }

            // Delete
            KeyCode::Char('d') => {
                self.state.set_status("Delete not implemented yet");
            }

            _ => {}
        }

        Ok(false)
    }
}

/// Run the TUI application
pub async fn run_app() -> Result<()> {
    let mut app = App::new().await?;
    app.run().await
}
