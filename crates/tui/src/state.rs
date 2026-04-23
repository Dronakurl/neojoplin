// Application state management

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::settings::Settings;
use crate::theme::Theme;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use joplin_domain::{Folder, Note};

/// Which panel has focus
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPanel {
    Notebooks,
    Notes,
    Content,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingDelete {
    Note { id: String, title: String },
    Notebook { id: String, title: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteSortMode {
    TimeAsc,
    TimeDesc,
    NameAsc,
}

impl NoteSortMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::TimeAsc => "time",
            Self::TimeDesc => "time desc",
            Self::NameAsc => "name",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotebookSortMode {
    TimeAsc,
    TimeDesc,
    NameAsc,
    RecentNote,
}

impl NotebookSortMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::TimeAsc => "time",
            Self::TimeDesc => "time desc",
            Self::NameAsc => "name",
            Self::RecentNote => "recent note",
        }
    }
}

/// Application state
#[derive(Debug, Clone)]
pub struct AppState {
    /// List of all folders
    pub folders: Vec<Folder>,
    /// Currently selected folder index (None = "All Notebooks")
    pub selected_folder: Option<usize>,
    /// Whether "All Notebooks" mode is active
    pub all_notebooks_mode: bool,
    /// List of notes in selected folder
    pub notes: Vec<Note>,
    /// Currently selected note index
    pub selected_note: Option<usize>,
    /// Content of currently selected note
    pub current_note_content: String,
    /// Content scroll offset
    pub content_scroll_offset: usize,
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
    /// Whether rename mode is active
    pub show_rename_prompt: bool,
    /// Current rename input
    pub rename_input: String,
    /// Whether filter prompt is active
    pub show_filter_prompt: bool,
    /// Which panel the filter prompt is editing
    pub filter_target: FocusPanel,
    /// Current filter prompt input
    pub filter_input: String,
    /// Filter input value before opening the prompt
    pub filter_original_input: String,
    /// Active notebook filter query
    pub notebook_filter_query: String,
    /// Active note filter query
    pub note_filter_query: String,
    /// Newly created note kept at end of the list until renamed
    pub new_note_id: Option<String>,
    /// Newly created notebook kept at end of the list until renamed
    pub new_folder_id: Option<String>,
    /// Pending note or notebook deletion
    pub pending_delete: Option<PendingDelete>,
    /// Whether sort help is active
    pub show_sort_popup: bool,
    /// Current note sort mode
    pub note_sort: NoteSortMode,
    /// Current notebook sort mode
    pub notebook_sort: NotebookSortMode,
    /// Color theme
    pub theme: Theme,
    /// Whether to show error dialog
    pub show_error_dialog: bool,
    /// Current error message to display
    pub error_message: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            folders: Vec::new(),
            selected_folder: None,
            all_notebooks_mode: false,
            notes: Vec::new(),
            selected_note: None,
            current_note_content: String::new(),
            content_scroll_offset: 0,
            focus: FocusPanel::Notebooks,
            show_quit_confirmation: false,
            show_settings: false,
            status_message: String::new(),
            settings_modified: false,
            settings: Settings::new(),
            show_rename_prompt: false,
            rename_input: String::new(),
            show_filter_prompt: false,
            filter_target: FocusPanel::Notes,
            filter_input: String::new(),
            filter_original_input: String::new(),
            notebook_filter_query: String::new(),
            note_filter_query: String::new(),
            new_note_id: None,
            new_folder_id: None,
            pending_delete: None,
            show_sort_popup: false,
            note_sort: NoteSortMode::TimeAsc,
            notebook_sort: NotebookSortMode::TimeAsc,
            theme: crate::theme::default_theme(),
            show_error_dialog: false,
            error_message: String::new(),
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

        if self.folders.is_empty() {
            self.selected_folder = None;
            self.all_notebooks_mode = false;
            return;
        }

        if let Some(idx) = self.selected_folder {
            if idx >= self.folders.len() {
                self.selected_folder = Some(self.folders.len() - 1);
            }
        } else if !self.all_notebooks_mode {
            self.selected_folder = Some(0);
        }
    }

    /// Set selected folder by index and return whether it changed
    pub fn set_folder(&mut self, index: Option<usize>) -> bool {
        let old_folder = self.selected_folder;
        self.selected_folder = index;
        self.all_notebooks_mode = index.is_none();
        old_folder != index
    }

    /// Check if folder selection changed and needs notes reload
    pub fn has_folder_changed(&self, old_folder: Option<usize>) -> bool {
        self.selected_folder != old_folder || self.all_notebooks_mode
    }

    /// Set notes list for current folder
    pub fn set_notes(&mut self, notes: Vec<Note>) {
        self.notes = notes;

        if self.notes.is_empty() {
            self.selected_note = None;
            self.current_note_content.clear();
            self.content_scroll_offset = 0;
            return;
        }

        if let Some(idx) = self.selected_note {
            if idx >= self.notes.len() {
                self.selected_note = Some(self.notes.len() - 1);
                self.load_note_content();
            }
        } else {
            self.selected_note = Some(0);
            self.load_note_content();
        }
    }

    /// Move selection in current panel, returns true if folder changed
    pub fn move_selection(&mut self, delta: isize) -> bool {
        let mut folder_changed = false;

        match self.focus {
            FocusPanel::Notebooks => {
                let len = self.folders.len();
                if len > 0 {
                    if self.all_notebooks_mode {
                        if delta > 0 {
                            self.all_notebooks_mode = false;
                            self.selected_folder = Some(0);
                            folder_changed = true;
                        }
                    } else if let Some(ref mut idx) = self.selected_folder {
                        let old_idx = *idx;
                        let new_idx_raw = (*idx as isize + delta).rem_euclid(len as isize) as usize;

                        if delta < 0 && *idx == 0 {
                            self.all_notebooks_mode = true;
                            self.selected_folder = None;
                            folder_changed = true;
                        } else {
                            *idx = new_idx_raw;
                            folder_changed = old_idx != new_idx_raw;
                        }
                    } else {
                        self.selected_folder = Some(0);
                        self.all_notebooks_mode = false;
                        folder_changed = true;
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
            FocusPanel::Content => {}
        }

        folder_changed
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
                self.content_scroll_offset = 0;
            }
        }
    }

    /// Get currently selected folder
    pub fn selected_folder(&self) -> Option<&Folder> {
        self.selected_folder.and_then(|idx| self.folders.get(idx))
    }

    /// Get currently selected folder id
    pub fn selected_folder_id(&self) -> Option<&str> {
        self.selected_folder().map(|folder| folder.id.as_str())
    }

    /// Get currently selected note
    pub fn selected_note(&self) -> Option<&Note> {
        self.selected_note.and_then(|idx| self.notes.get(idx))
    }

    /// Get currently selected note id
    pub fn selected_note_id(&self) -> Option<&str> {
        self.selected_note().map(|note| note.id.as_str())
    }

    /// Select folder by id
    pub fn select_folder_by_id(&mut self, folder_id: &str) -> bool {
        if let Some(idx) = self
            .folders
            .iter()
            .position(|folder| folder.id == folder_id)
        {
            self.selected_folder = Some(idx);
            self.all_notebooks_mode = false;
            true
        } else {
            false
        }
    }

    /// Select note by id
    pub fn select_note_by_id(&mut self, note_id: &str) -> bool {
        if let Some(idx) = self.notes.iter().position(|note| note.id == note_id) {
            self.selected_note = Some(idx);
            self.load_note_content();
            true
        } else {
            false
        }
    }

    /// Sort notes for the current sort mode
    pub fn sort_notes(&self, notes: &mut [Note]) {
        notes.sort_by(|left, right| match self.note_sort {
            NoteSortMode::TimeAsc => compare_note_time(left, right),
            NoteSortMode::TimeDesc => compare_note_time(right, left),
            NoteSortMode::NameAsc => {
                compare_note_name(left, right).then_with(|| compare_note_time(left, right))
            }
        });
        move_note_to_end(notes, self.new_note_id.as_deref());
    }

    /// Sort notebooks for the current sort mode
    pub fn sort_folders(&self, folders: &mut [Folder], notes: &[Note]) {
        let recent_note_times = folder_recent_note_times(notes);

        folders.sort_by(|left, right| match self.notebook_sort {
            NotebookSortMode::TimeAsc => compare_folder_time(left, right),
            NotebookSortMode::TimeDesc => compare_folder_time(right, left),
            NotebookSortMode::NameAsc => {
                compare_folder_name(left, right).then_with(|| compare_folder_time(left, right))
            }
            NotebookSortMode::RecentNote => {
                compare_folder_recent_note(left, right, &recent_note_times)
            }
        });
        move_folder_to_end(folders, self.new_folder_id.as_deref());
    }

    /// Apply the active notebook filter query to a list of folders.
    pub fn filter_folders(&self, folders: Vec<Folder>) -> Vec<Folder> {
        fuzzy_filter_by_query(folders, &self.notebook_filter_query, |folder| &folder.title)
    }

    /// Apply the active note filter query to a list of notes.
    pub fn filter_notes(&self, notes: Vec<Note>) -> Vec<Note> {
        fuzzy_filter_by_query(notes, &self.note_filter_query, |note| &note.title)
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

    /// Show error dialog with message
    pub fn show_error(&mut self, error: &str) {
        self.error_message = error.to_string();
        self.show_error_dialog = true;
    }

    /// Hide error dialog
    pub fn hide_error(&mut self) {
        self.show_error_dialog = false;
        self.error_message.clear();
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

    /// Show rename prompt
    pub fn show_rename_prompt(&mut self) {
        self.show_rename_prompt = true;
        self.rename_input.clear();
    }

    /// Hide rename prompt
    pub fn hide_rename_prompt(&mut self) {
        self.show_rename_prompt = false;
        self.rename_input.clear();
    }

    /// Show sort popup
    pub fn open_sort_popup(&mut self) {
        self.show_sort_popup = true;
    }

    /// Hide sort popup
    pub fn close_sort_popup(&mut self) {
        self.show_sort_popup = false;
    }

    /// Open the filter prompt for the active list panel.
    pub fn open_filter_prompt(&mut self) {
        self.filter_target = if self.focus == FocusPanel::Notebooks {
            FocusPanel::Notebooks
        } else {
            FocusPanel::Notes
        };
        self.filter_input = self.current_filter_query().to_string();
        self.filter_original_input = self.filter_input.clone();
        self.show_filter_prompt = true;
    }

    /// Close the filter prompt and optionally restore the original query.
    pub fn close_filter_prompt(&mut self, restore_original: bool) {
        if restore_original {
            let original = self.filter_original_input.clone();
            self.set_filter_query(original);
        }
        self.show_filter_prompt = false;
        self.filter_original_input.clear();
    }

    /// Add a character to the live filter query.
    pub fn add_filter_char(&mut self, c: char) {
        self.filter_input.push(c);
        self.set_filter_query(self.filter_input.clone());
    }

    /// Remove a character from the live filter query.
    pub fn remove_filter_char(&mut self) {
        self.filter_input.pop();
        self.set_filter_query(self.filter_input.clone());
    }

    /// Return the filter query for the active filter target.
    pub fn current_filter_query(&self) -> &str {
        match self.filter_target {
            FocusPanel::Notebooks => &self.notebook_filter_query,
            FocusPanel::Notes | FocusPanel::Content => &self.note_filter_query,
        }
    }

    /// Set the filter query for the current filter target.
    pub fn set_filter_query(&mut self, query: String) {
        match self.filter_target {
            FocusPanel::Notebooks => self.notebook_filter_query = query,
            FocusPanel::Notes | FocusPanel::Content => self.note_filter_query = query,
        }
    }

    /// Whether any panel filter is currently active.
    pub fn has_active_filter(&self, panel: FocusPanel) -> bool {
        match panel {
            FocusPanel::Notebooks => !self.notebook_filter_query.is_empty(),
            FocusPanel::Notes => !self.note_filter_query.is_empty(),
            FocusPanel::Content => false,
        }
    }

    /// Open a delete confirmation dialog.
    pub fn confirm_delete(&mut self, pending_delete: PendingDelete) {
        self.pending_delete = Some(pending_delete);
    }

    /// Close the delete confirmation dialog.
    pub fn clear_pending_delete(&mut self) {
        self.pending_delete = None;
    }

    /// Keep a newly created note at the end of the list until it is renamed.
    pub fn mark_new_note(&mut self, note_id: String) {
        self.new_note_id = Some(note_id);
    }

    /// Keep a newly created notebook at the end of the list until it is renamed.
    pub fn mark_new_folder(&mut self, folder_id: String) {
        self.new_folder_id = Some(folder_id);
    }

    /// Clear the note marker once the note is no longer "new".
    pub fn clear_new_note_marker_if(&mut self, note_id: &str) {
        if self.new_note_id.as_deref() == Some(note_id) {
            self.new_note_id = None;
        }
    }

    /// Clear the notebook marker once the notebook is no longer "new".
    pub fn clear_new_folder_marker_if(&mut self, folder_id: &str) {
        if self.new_folder_id.as_deref() == Some(folder_id) {
            self.new_folder_id = None;
        }
    }

    /// Add character to rename input
    pub fn add_rename_char(&mut self, c: char) {
        self.rename_input.push(c);
    }

    /// Remove last character from rename input
    pub fn remove_rename_char(&mut self) {
        self.rename_input.pop();
    }
}

fn compare_note_time(left: &Note, right: &Note) -> Ordering {
    left.updated_time
        .cmp(&right.updated_time)
        .then_with(|| left.created_time.cmp(&right.created_time))
        .then_with(|| compare_note_name(left, right))
        .then_with(|| left.id.cmp(&right.id))
}

fn compare_note_name(left: &Note, right: &Note) -> Ordering {
    normalized_name(&left.title)
        .cmp(&normalized_name(&right.title))
        .then_with(|| left.title.cmp(&right.title))
        .then_with(|| left.id.cmp(&right.id))
}

fn compare_folder_time(left: &Folder, right: &Folder) -> Ordering {
    left.updated_time
        .cmp(&right.updated_time)
        .then_with(|| left.created_time.cmp(&right.created_time))
        .then_with(|| compare_folder_name(left, right))
        .then_with(|| left.id.cmp(&right.id))
}

fn compare_folder_name(left: &Folder, right: &Folder) -> Ordering {
    normalized_name(&left.title)
        .cmp(&normalized_name(&right.title))
        .then_with(|| left.title.cmp(&right.title))
        .then_with(|| left.id.cmp(&right.id))
}

fn compare_folder_recent_note(
    left: &Folder,
    right: &Folder,
    recent_note_times: &HashMap<&str, i64>,
) -> Ordering {
    let left_recent = recent_note_times.get(left.id.as_str()).copied();
    let right_recent = recent_note_times.get(right.id.as_str()).copied();

    right_recent
        .cmp(&left_recent)
        .then_with(|| compare_folder_name(left, right))
        .then_with(|| compare_folder_time(right, left))
}

fn folder_recent_note_times(notes: &[Note]) -> HashMap<&str, i64> {
    let mut recent_times = HashMap::new();

    for note in notes {
        recent_times
            .entry(note.parent_id.as_str())
            .and_modify(|current: &mut i64| *current = (*current).max(note.updated_time))
            .or_insert(note.updated_time);
    }

    recent_times
}

fn normalized_name(name: &str) -> String {
    name.to_lowercase()
}

fn fuzzy_filter_by_query<T, F>(items: Vec<T>, query: &str, text_fn: F) -> Vec<T>
where
    T: Clone,
    F: Fn(&T) -> &str,
{
    let query = query.trim();
    if query.is_empty() {
        return items;
    }

    let matcher = SkimMatcherV2::default().smart_case();
    let mut matches: Vec<(usize, i64, T)> = items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            matcher
                .fuzzy_match(text_fn(item), query)
                .map(|score| (idx, score, item.clone()))
        })
        .collect();

    matches.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

    matches.into_iter().map(|(_, _, item)| item).collect()
}

fn move_note_to_end(notes: &mut [Note], note_id: Option<&str>) {
    if let Some(note_id) = note_id {
        if let Some(idx) = notes.iter().position(|note| note.id == note_id) {
            notes[idx..].rotate_left(1);
        }
    }
}

fn move_folder_to_end(folders: &mut [Folder], folder_id: Option<&str>) {
    if let Some(folder_id) = folder_id {
        if let Some(idx) = folders.iter().position(|folder| folder.id == folder_id) {
            folders[idx..].rotate_left(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use joplin_domain::now_ms;

    #[test]
    fn test_state_default() {
        let state = AppState::new();
        assert_eq!(state.focus, FocusPanel::Notebooks);
        assert_eq!(state.selected_folder, None);
        assert_eq!(state.selected_note, None);
        assert!(!state.all_notebooks_mode);
        assert_eq!(state.note_sort, NoteSortMode::TimeAsc);
        assert_eq!(state.notebook_sort, NotebookSortMode::TimeAsc);
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

        let folders = vec![Folder {
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
        }];

        state.set_folders(folders);
        assert_eq!(state.selected_folder, Some(0));
        assert_eq!(state.folders.len(), 1);
    }

    #[test]
    fn test_all_notebooks_mode() {
        let mut state = AppState::new();

        state.all_notebooks_mode = true;
        state.selected_folder = None;

        assert!(state.all_notebooks_mode);
        assert_eq!(state.selected_folder, None);

        state.set_folder(Some(0));
        assert!(!state.all_notebooks_mode);
        assert_eq!(state.selected_folder, Some(0));
    }

    #[test]
    fn test_move_selection_with_all_notebooks() {
        let mut state = AppState::new();

        let folders = vec![
            Folder {
                id: "1".to_string(),
                title: "Folder 1".to_string(),
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
            },
            Folder {
                id: "2".to_string(),
                title: "Folder 2".to_string(),
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
            },
        ];

        state.set_folders(folders);
        state.focus = FocusPanel::Notebooks;
        state.all_notebooks_mode = true;
        state.selected_folder = None;

        let folder_changed = state.move_selection(1);
        assert!(folder_changed);
        assert!(!state.all_notebooks_mode);
        assert_eq!(state.selected_folder, Some(0));

        let folder_changed = state.move_selection(-1);
        assert!(folder_changed);
        assert!(state.all_notebooks_mode);
        assert_eq!(state.selected_folder, None);
    }

    #[test]
    fn test_sort_notes_by_name() {
        let mut state = AppState::new();
        state.note_sort = NoteSortMode::NameAsc;

        let mut notes = vec![
            Note {
                id: "2".to_string(),
                title: "zeta".to_string(),
                updated_time: 2,
                ..Note::default()
            },
            Note {
                id: "1".to_string(),
                title: "Alpha".to_string(),
                updated_time: 3,
                ..Note::default()
            },
        ];

        state.sort_notes(&mut notes);

        assert_eq!(notes[0].title, "Alpha");
        assert_eq!(notes[1].title, "zeta");
    }

    #[test]
    fn test_sort_folders_by_recent_note() {
        let mut state = AppState::new();
        state.notebook_sort = NotebookSortMode::RecentNote;

        let mut folders = vec![
            Folder {
                id: "folder-a".to_string(),
                title: "Archive".to_string(),
                ..Folder::default()
            },
            Folder {
                id: "folder-b".to_string(),
                title: "Inbox".to_string(),
                ..Folder::default()
            },
            Folder {
                id: "folder-c".to_string(),
                title: "Empty".to_string(),
                ..Folder::default()
            },
        ];
        let notes = vec![
            Note {
                id: "n1".to_string(),
                parent_id: "folder-a".to_string(),
                updated_time: 10,
                ..Note::default()
            },
            Note {
                id: "n2".to_string(),
                parent_id: "folder-b".to_string(),
                updated_time: 20,
                ..Note::default()
            },
        ];

        state.sort_folders(&mut folders, &notes);

        assert_eq!(folders[0].id, "folder-b");
        assert_eq!(folders[1].id, "folder-a");
        assert_eq!(folders[2].id, "folder-c");
    }

    #[test]
    fn test_select_note_by_id() {
        let mut state = AppState::new();
        state.set_notes(vec![
            Note {
                id: "a".to_string(),
                title: "First".to_string(),
                body: "first".to_string(),
                ..Note::default()
            },
            Note {
                id: "b".to_string(),
                title: "Second".to_string(),
                body: "second".to_string(),
                ..Note::default()
            },
        ]);

        assert!(state.select_note_by_id("b"));
        assert_eq!(state.selected_note, Some(1));
        assert_eq!(state.current_note_content, "second");
    }
}
