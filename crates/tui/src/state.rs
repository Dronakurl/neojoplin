// Application state management

use neojoplin_core::{Note, Folder};
use crate::settings::Settings;

/// Which panel has focus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPanel {
    Notebooks,
    Notes,
    Content,
}

/// Application state
#[derive(Debug, Clone)]
pub struct AppState {
    /// List of all folders
    pub folders: Vec<Folder>,
    /// Currently selected folder index
    pub selected_folder: Option<usize>,
    /// List of notes in selected folder
    pub notes: Vec<Note>,
    /// Currently selected note index
    pub selected_note: Option<usize>,
    /// Content of currently selected note
    pub current_note_content: String,
    /// Currently focused panel
    pub focus: FocusPanel,
    /// Whether to show quit confirmation
    pub show_quit_confirmation: bool,
    /// Whether to show settings
    pub show_settings: bool,
    /// Status message
    pub status_message: String,
    /// Whether settings were modified
    pub settings_modified: bool,
    /// Settings state
    pub settings: Settings,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            folders: Vec::new(),
            selected_folder: None,
            notes: Vec::new(),
            selected_note: None,
            current_note_content: String::new(),
            focus: FocusPanel::Notebooks,
            show_quit_confirmation: false,
            show_settings: false,
            status_message: String::new(),
            settings_modified: false,
            settings: Settings::new(),
        }
    }
}

impl AppState {
    /// Create new app state
    pub fn new() -> Self {
        Self::default()
    }

    /// Set folders list
    pub fn set_folders(&mut self, folders: Vec<Folder>) {
        self.folders = folders;
        // Select first folder if available
        if !self.folders.is_empty() && self.selected_folder.is_none() {
            self.selected_folder = Some(0);
        }
    }

    /// Set notes list for current folder
    pub fn set_notes(&mut self, notes: Vec<Note>) {
        self.notes = notes;
        // Select first note if available
        if !self.notes.is_empty() && self.selected_note.is_none() {
            self.selected_note = Some(0);
            self.load_note_content();
        }
    }

    /// Move selection in current panel
    pub fn move_selection(&mut self, delta: isize) {
        match self.focus {
            FocusPanel::Notebooks => {
                if let Some(ref mut idx) = self.selected_folder {
                    let len = self.folders.len();
                    if len > 0 {
                        let new_idx = (*idx as isize + delta).rem_euclid(len as isize) as usize;
                        *idx = new_idx;
                    }
                }
            }
            FocusPanel::Notes => {
                if let Some(ref mut idx) = self.selected_note {
                    let len = self.notes.len();
                    if len > 0 {
                        let new_idx = (*idx as isize + delta).rem_euclid(len as isize) as usize;
                        *idx = new_idx;
                        self.load_note_content();
                    }
                }
            }
            FocusPanel::Content => {
                // Scroll content (not implemented yet)
            }
        }
    }

    /// Switch focus to next panel
    pub fn next_panel(&mut self) {
        self.focus = match self.focus {
            FocusPanel::Notebooks => FocusPanel::Notes,
            FocusPanel::Notes => FocusPanel::Content,
            FocusPanel::Content => FocusPanel::Notebooks,
        };
    }

    /// Switch focus to previous panel
    pub fn prev_panel(&mut self) {
        self.focus = match self.focus {
            FocusPanel::Notebooks => FocusPanel::Content,
            FocusPanel::Notes => FocusPanel::Notebooks,
            FocusPanel::Content => FocusPanel::Notes,
        };
    }

    /// Load content of currently selected note
    pub fn load_note_content(&mut self) {
        if let Some(idx) = self.selected_note {
            if idx < self.notes.len() {
                self.current_note_content = self.notes[idx].body.clone();
            }
        }
    }

    /// Get currently selected folder
    pub fn selected_folder(&self) -> Option<&Folder> {
        self.selected_folder
            .and_then(|idx| self.folders.get(idx))
    }

    /// Get currently selected note
    pub fn selected_note(&self) -> Option<&Note> {
        self.selected_note
            .and_then(|idx| self.notes.get(idx))
    }

    /// Set status message
    pub fn set_status(&mut self, message: &str) {
        self.status_message = message.to_string();
    }

    /// Show quit confirmation
    pub fn show_quit(&mut self) {
        self.show_quit_confirmation = true;
    }

    /// Hide quit confirmation
    pub fn hide_quit(&mut self) {
        self.show_quit_confirmation = false;
    }

    /// Show settings
    pub fn show_settings(&mut self) {
        self.show_settings = true;
    }

    /// Hide settings
    pub fn hide_settings(&mut self) {
        self.show_settings = false;
    }

    /// Toggle settings
    pub fn toggle_settings(&mut self) {
        self.show_settings = !self.show_settings;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neojoplin_core::{now_ms};

    #[test]
    fn test_state_default() {
        let state = AppState::new();
        assert_eq!(state.focus, FocusPanel::Notebooks);
        assert_eq!(state.selected_folder, None);
        assert_eq!(state.selected_note, None);
    }

    #[test]
    fn test_next_panel() {
        let mut state = AppState::new();
        assert_eq!(state.focus, FocusPanel::Notebooks);

        state.next_panel();
        assert_eq!(state.focus, FocusPanel::Notes);

        state.next_panel();
        assert_eq!(state.focus, FocusPanel::Content);

        state.next_panel();
        assert_eq!(state.focus, FocusPanel::Notebooks);
    }

    #[test]
    fn test_prev_panel() {
        let mut state = AppState::new();
        assert_eq!(state.focus, FocusPanel::Notebooks);

        state.prev_panel();
        assert_eq!(state.focus, FocusPanel::Content);

        state.prev_panel();
        assert_eq!(state.focus, FocusPanel::Notes);
    }

    #[test]
    fn test_set_folders() {
        let mut state = AppState::new();

        let folders = vec![
            Folder {
                id: "1".to_string(),
                title: "Test Folder".to_string(),
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
            }
        ];

        state.set_folders(folders);
        assert_eq!(state.selected_folder, Some(0));
        assert_eq!(state.folders.len(), 1);
    }
}
