// Application state management

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::command_line::{CommandPromptState, CompletionState};
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
    Note {
        id: String,
        title: String,
        permanent: bool,
    },
    Notebook {
        id: String,
        title: String,
        note_count: usize,
    },
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteFilterMode {
    TitleOnly,
    FullText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TagPopupFocus {
    #[default]
    List,
    Input,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagPopupItem {
    pub id: String,
    pub title: String,
    pub attached: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TagPopupState {
    pub visible: bool,
    pub items: Vec<TagPopupItem>,
    pub selected_index: usize,
    pub input: String,
    pub focus: TagPopupFocus,
    pub pending_delete_tag: Option<(String, String)>,
}

impl TagPopupState {
    pub fn open(&mut self, items: Vec<TagPopupItem>) {
        self.visible = true;
        self.items = items;
        self.selected_index = self
            .items
            .iter()
            .position(|item| item.attached)
            .unwrap_or(0)
            .min(self.items.len().saturating_sub(1));
        self.input.clear();
        self.focus = if self.items.is_empty() {
            TagPopupFocus::Input
        } else {
            TagPopupFocus::List
        };
        self.pending_delete_tag = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.items.clear();
        self.selected_index = 0;
        self.input.clear();
        self.focus = TagPopupFocus::List;
        self.pending_delete_tag = None;
    }

    pub fn current_item(&self) -> Option<&TagPopupItem> {
        self.items.get(self.selected_index)
    }

    pub fn move_selection(&mut self, forward: bool) {
        if self.items.is_empty() {
            self.selected_index = 0;
            return;
        }

        self.selected_index = if forward {
            (self.selected_index + 1) % self.items.len()
        } else if self.selected_index == 0 {
            self.items.len() - 1
        } else {
            self.selected_index - 1
        };
    }
}

impl NoteFilterMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::TitleOnly => "title",
            Self::FullText => "full text",
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
    /// Tag completion state for filter prompt
    pub filter_completion: Option<CompletionState>,
    /// Filter input value before opening the prompt
    pub filter_original_input: String,
    /// Active notebook filter query
    pub notebook_filter_query: String,
    /// Active note filter query
    pub note_filter_query: String,
    /// Whether note filtering searches titles only or full text
    pub note_filter_mode: NoteFilterMode,
    /// Newly created note kept at end of the list until renamed
    pub new_note_id: Option<String>,
    /// Newly created notebook kept at end of the list until renamed
    pub new_folder_id: Option<String>,
    /// Pending note or notebook deletion
    pub pending_delete: Option<PendingDelete>,
    /// Whether "Trash" mode is active
    pub trash_mode: bool,
    /// Whether "Orphaned" mode is active
    pub orphan_mode: bool,
    /// Number of currently orphaned notes
    pub orphan_note_count: usize,
    /// Number of currently trashed notes
    pub trash_note_count: usize,
    /// Display names for folders (with disambiguation suffixes)
    pub folder_display_names: HashMap<String, String>,
    /// Whether sort help is active
    pub show_sort_popup: bool,
    /// Current note sort mode
    pub note_sort: NoteSortMode,
    /// Current notebook sort mode
    pub notebook_sort: NotebookSortMode,
    /// Vim-style command prompt state
    pub command_prompt: CommandPromptState,
    /// Tag selection popup state
    pub tag_popup: TagPopupState,
    /// Tag names keyed by note ID for filtering
    pub note_tags: HashMap<String, Vec<String>>,
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
            filter_completion: None,
            filter_original_input: String::new(),
            notebook_filter_query: String::new(),
            note_filter_query: String::new(),
            note_filter_mode: NoteFilterMode::TitleOnly,
            new_note_id: None,
            new_folder_id: None,
            pending_delete: None,
            trash_mode: false,
            orphan_mode: false,
            orphan_note_count: 0,
            trash_note_count: 0,
            folder_display_names: HashMap::new(),
            show_sort_popup: false,
            note_sort: NoteSortMode::TimeAsc,
            notebook_sort: NotebookSortMode::TimeAsc,
            command_prompt: CommandPromptState::default(),
            tag_popup: TagPopupState::default(),
            note_tags: HashMap::new(),
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
        self.rebuild_folder_display_names();

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
        self.orphan_mode = false;
        self.trash_mode = false;
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
                if self.trash_mode {
                    if delta < 0 {
                        self.trash_mode = false;
                        if self.orphan_note_count > 0 {
                            self.orphan_mode = true;
                            self.selected_folder = None;
                            self.all_notebooks_mode = false;
                        } else if len > 0 {
                            self.selected_folder = Some(len - 1);
                            self.all_notebooks_mode = false;
                            self.orphan_mode = false;
                        } else {
                            self.all_notebooks_mode = true;
                        }
                        folder_changed = true;
                    }
                } else if self.orphan_mode {
                    if delta < 0 {
                        self.orphan_mode = false;
                        if len > 0 {
                            self.selected_folder = Some(len - 1);
                            self.all_notebooks_mode = false;
                        } else {
                            self.all_notebooks_mode = true;
                            self.selected_folder = None;
                        }
                        folder_changed = true;
                    } else if delta > 0 {
                        self.orphan_mode = false;
                        self.selected_folder = None;
                        self.all_notebooks_mode = false;
                        if self.trash_note_count > 0 {
                            self.trash_mode = true;
                        } else if len == 0 {
                            self.all_notebooks_mode = true;
                        }
                        folder_changed = true;
                    }
                } else if self.all_notebooks_mode {
                    if delta > 0 {
                        self.all_notebooks_mode = false;
                        if len > 0 {
                            self.selected_folder = Some(0);
                        } else if self.orphan_note_count > 0 {
                            self.orphan_mode = true;
                            self.selected_folder = None;
                        } else if self.trash_note_count > 0 {
                            self.trash_mode = true;
                        }
                        folder_changed = true;
                    }
                } else if len > 0 {
                    if let Some(ref mut idx) = self.selected_folder {
                        let old_idx = *idx;

                        if delta < 0 && *idx == 0 {
                            self.selected_folder = None;
                            self.all_notebooks_mode = true;
                            folder_changed = true;
                        } else if delta > 0 && *idx == len - 1 {
                            self.selected_folder = None;
                            self.all_notebooks_mode = false;
                            if self.orphan_note_count > 0 {
                                self.orphan_mode = true;
                            } else if self.trash_note_count > 0 {
                                self.trash_mode = true;
                            } else {
                                self.all_notebooks_mode = true;
                            }
                            folder_changed = true;
                        } else {
                            let new_idx_raw =
                                (*idx as isize + delta).rem_euclid(len as isize) as usize;
                            *idx = new_idx_raw;
                            folder_changed = old_idx != new_idx_raw;
                        }
                    } else {
                        self.selected_folder = Some(0);
                        self.all_notebooks_mode = false;
                        self.orphan_mode = false;
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
            self.orphan_mode = false;
            self.trash_mode = false;
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
        let existing_ids: HashSet<String> =
            folders.iter().map(|folder| folder.id.clone()).collect();
        let mut grouped: HashMap<String, Vec<Folder>> = HashMap::new();
        let mut roots = Vec::new();

        for folder in folders.iter().cloned() {
            if folder.parent_id.is_empty() || !existing_ids.contains(&folder.parent_id) {
                roots.push(folder);
            } else {
                grouped
                    .entry(folder.parent_id.clone())
                    .or_default()
                    .push(folder);
            }
        }

        sort_folder_group(&mut roots, self.notebook_sort, &recent_note_times);

        let mut ordered = Vec::with_capacity(folders.len());
        flatten_folder_group(
            &roots,
            &mut grouped,
            &mut ordered,
            self.notebook_sort,
            &recent_note_times,
        );

        move_folder_vec_to_end(&mut ordered, self.new_folder_id.as_deref());
        for (target, source) in folders.iter_mut().zip(ordered.into_iter()) {
            *target = source;
        }
    }

    /// Apply the active notebook filter query to a list of folders.
    pub fn filter_folders(&self, folders: Vec<Folder>) -> Vec<Folder> {
        let query = self.notebook_filter_query.trim();
        if query.is_empty() {
            return folders;
        }

        let matched_ids: HashSet<String> =
            fuzzy_filter_by_query(folders.clone(), query, |folder| folder.title.clone())
                .into_iter()
                .map(|folder| folder.id)
                .collect();

        if matched_ids.is_empty() {
            return Vec::new();
        }

        let parent_by_id: HashMap<&str, &str> = folders
            .iter()
            .map(|folder| (folder.id.as_str(), folder.parent_id.as_str()))
            .collect();

        let mut visible_ids = matched_ids.clone();
        for matched_id in matched_ids {
            let mut current = matched_id.as_str();
            while let Some(parent_id) = parent_by_id.get(current).copied() {
                if parent_id.is_empty() || !visible_ids.insert(parent_id.to_string()) {
                    break;
                }
                current = parent_id;
            }
        }

        folders
            .into_iter()
            .filter(|folder| visible_ids.contains(&folder.id))
            .collect()
    }

    /// Apply the active note filter query to a list of notes.
    pub fn filter_notes(&self, notes: Vec<Note>) -> Vec<Note> {
        let (text_query, tag_terms) = split_note_filter_query(&self.note_filter_query);
        if text_query.text.is_empty() && tag_terms.is_empty() {
            return notes;
        }

        let matcher = SkimMatcherV2::default().smart_case();
        let mut matches: Vec<(usize, i64, Note)> = notes
            .iter()
            .enumerate()
            .filter_map(|(idx, note)| {
                let tags = self.note_tags.get(&note.id).cloned().unwrap_or_default();
                if !note_matches_tag_terms(&matcher, &tags, &tag_terms) {
                    return None;
                }

                let searchable_text = match self.note_filter_mode {
                    NoteFilterMode::TitleOnly => note.title.clone(),
                    NoteFilterMode::FullText => {
                        if tags.is_empty() {
                            format!("{} {}", note.title, note.body)
                        } else {
                            format!("{} {} {}", note.title, note.body, tags.join(" "))
                        }
                    }
                };

                let score = if text_query.text.is_empty() {
                    0
                } else if text_query.exact {
                    if searchable_text
                        .to_lowercase()
                        .contains(&text_query.text.to_lowercase())
                    {
                        1
                    } else {
                        return None;
                    }
                } else {
                    matcher.fuzzy_match(&searchable_text, &text_query.text)?
                };

                Some((idx, score, note.clone()))
            })
            .collect();

        matches.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
        matches.into_iter().map(|(_, _, note)| note).collect()
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
    pub fn open_filter_prompt(&mut self, full_text: bool) {
        self.filter_target = if self.focus == FocusPanel::Notebooks {
            FocusPanel::Notebooks
        } else {
            FocusPanel::Notes
        };
        if self.filter_target == FocusPanel::Notes {
            self.note_filter_mode = if full_text {
                NoteFilterMode::FullText
            } else {
                NoteFilterMode::TitleOnly
            };
        }
        self.filter_input = self.current_filter_query().to_string();
        self.filter_completion = None;
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
        self.filter_completion = None;
        self.filter_original_input.clear();
    }

    /// Add a character to the live filter query.
    pub fn add_filter_char(&mut self, c: char) {
        self.filter_input.push(c);
        self.filter_completion = None;
        self.set_filter_query(self.filter_input.clone());
    }

    /// Remove a character from the live filter query.
    pub fn remove_filter_char(&mut self) {
        self.filter_input.pop();
        self.filter_completion = None;
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

    /// The active note text filter query and whether it is exact.
    pub fn note_text_filter_query(&self) -> Option<(String, bool)> {
        let (text_query, _) = split_note_filter_query(&self.note_filter_query);
        if text_query.text.is_empty() {
            None
        } else {
            Some((text_query.text, text_query.exact))
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

    /// Replace the tag cache for the currently loaded notes.
    pub fn set_note_tags(&mut self, note_tags: HashMap<String, Vec<String>>) {
        self.note_tags = note_tags;
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

    /// Open the vim-style command prompt.
    pub fn open_command_prompt(&mut self, initial_input: impl Into<String>) {
        self.command_prompt.open(initial_input);
    }

    /// Close the command prompt.
    pub fn close_command_prompt(&mut self) {
        self.command_prompt.close();
    }

    pub fn open_tag_popup(&mut self, items: Vec<TagPopupItem>) {
        self.tag_popup.open(items);
    }

    pub fn close_tag_popup(&mut self) {
        self.tag_popup.close();
    }

    pub fn rebuild_folder_display_names(&mut self) {
        self.folder_display_names = build_folder_display_names(&self.folders);
    }

    pub fn set_trash_mode(&mut self, enabled: bool) {
        self.trash_mode = enabled;
        if enabled {
            self.all_notebooks_mode = false;
            self.orphan_mode = false;
            self.selected_folder = None;
        }
    }

    pub fn is_trash_mode(&self) -> bool {
        self.trash_mode
    }

    pub fn set_orphan_mode(&mut self, enabled: bool) {
        self.orphan_mode = enabled;
        if enabled {
            self.all_notebooks_mode = false;
            self.trash_mode = false;
            self.selected_folder = None;
        }
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
    F: Fn(&T) -> String,
{
    let query = query.trim();
    if query.is_empty() {
        return items;
    }

    let (query, exact) = parse_exact_query(query);

    let matcher = SkimMatcherV2::default().smart_case();
    let mut matches: Vec<(usize, i64, T)> = items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| {
            let text = text_fn(item);
            if exact {
                text.to_lowercase()
                    .contains(&query.to_lowercase())
                    .then_some((idx, 1, item.clone()))
            } else {
                matcher
                    .fuzzy_match(&text, query)
                    .map(|score| (idx, score, item.clone()))
            }
        })
        .collect();

    matches.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));

    matches.into_iter().map(|(_, _, item)| item).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TextQuery {
    text: String,
    exact: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TagQuery {
    text: String,
    exact: bool,
}

fn split_note_filter_query(query: &str) -> (TextQuery, Vec<TagQuery>) {
    let mut text_terms = Vec::new();
    let mut tag_terms = Vec::new();
    let trimmed = query.trim();
    let exact_text = trimmed.starts_with('=');

    for token in trimmed.trim_start_matches('=').split_whitespace() {
        if let Some(tag) = token.strip_prefix("#=") {
            if !tag.is_empty() {
                tag_terms.push(TagQuery {
                    text: tag.to_string(),
                    exact: true,
                });
            }
        } else if let Some(tag) = token.strip_prefix('#') {
            if !tag.is_empty() {
                tag_terms.push(TagQuery {
                    text: tag.to_string(),
                    exact: false,
                });
            }
        } else if let Some(tag) = token.strip_prefix("tag:=") {
            if !tag.is_empty() {
                tag_terms.push(TagQuery {
                    text: tag.to_string(),
                    exact: true,
                });
            }
        } else if let Some(tag) = token.strip_prefix("tag:") {
            if !tag.is_empty() {
                tag_terms.push(TagQuery {
                    text: tag.to_string(),
                    exact: false,
                });
            }
        } else {
            text_terms.push(token.to_string());
        }
    }

    (
        TextQuery {
            text: text_terms.join(" "),
            exact: exact_text,
        },
        tag_terms,
    )
}

fn note_matches_tag_terms(
    matcher: &SkimMatcherV2,
    tags: &[String],
    tag_terms: &[TagQuery],
) -> bool {
    if tag_terms.is_empty() {
        return true;
    }

    tag_terms.iter().all(|term| {
        tags.iter().any(|tag| {
            if term.exact {
                tag.eq_ignore_ascii_case(&term.text)
                    || tag.to_lowercase().contains(&term.text.to_lowercase())
            } else {
                tag.to_lowercase().contains(&term.text.to_lowercase())
                    || matcher.fuzzy_match(tag, &term.text).is_some()
            }
        })
    })
}

fn move_note_to_end(notes: &mut [Note], note_id: Option<&str>) {
    if let Some(note_id) = note_id {
        if let Some(idx) = notes.iter().position(|note| note.id == note_id) {
            notes[idx..].rotate_left(1);
        }
    }
}

fn move_folder_vec_to_end(folders: &mut Vec<Folder>, folder_id: Option<&str>) {
    if let Some(folder_id) = folder_id {
        if let Some(idx) = folders.iter().position(|folder| folder.id == folder_id) {
            let folder = folders.remove(idx);
            folders.push(folder);
        }
    }
}

fn sort_folder_group(
    folders: &mut [Folder],
    mode: NotebookSortMode,
    recent_note_times: &HashMap<&str, i64>,
) {
    folders.sort_by(|left, right| match mode {
        NotebookSortMode::TimeAsc => compare_folder_time(left, right),
        NotebookSortMode::TimeDesc => compare_folder_time(right, left),
        NotebookSortMode::NameAsc => {
            compare_folder_name(left, right).then_with(|| compare_folder_time(left, right))
        }
        NotebookSortMode::RecentNote => compare_folder_recent_note(left, right, recent_note_times),
    });
}

fn flatten_folder_group(
    folders: &[Folder],
    grouped: &mut HashMap<String, Vec<Folder>>,
    ordered: &mut Vec<Folder>,
    mode: NotebookSortMode,
    recent_note_times: &HashMap<&str, i64>,
) {
    for folder in folders {
        ordered.push(folder.clone());
        if let Some(mut children) = grouped.remove(&folder.id) {
            sort_folder_group(&mut children, mode, recent_note_times);
            flatten_folder_group(&children, grouped, ordered, mode, recent_note_times);
        }
    }
}

fn parse_exact_query(query: &str) -> (&str, bool) {
    if let Some(rest) = query.strip_prefix('=') {
        (rest.trim(), true)
    } else {
        (query, false)
    }
}

/// Build a map of `folder_id -> display_name`.
/// Duplicate folder titles get a grey `(N)` suffix (N = 1, 2, …).
/// Non-duplicate folders use their plain title.
pub fn build_folder_display_names(folders: &[Folder]) -> HashMap<String, String> {
    // Count occurrences of each title
    let mut title_count: HashMap<&str, usize> = HashMap::new();
    for f in folders {
        *title_count.entry(f.title.as_str()).or_insert(0) += 1;
    }

    // Sort by created_time so duplicate disambiguation is stable
    let mut sorted: Vec<&Folder> = folders.iter().collect();
    sorted.sort_by_key(|f| f.created_time);

    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut result = HashMap::new();
    for f in sorted {
        let display = if *title_count.get(f.title.as_str()).unwrap_or(&0) > 1 {
            let n = seen.entry(f.title.as_str()).or_insert(0);
            *n += 1;
            format!("{} ({})", f.title, n)
        } else {
            f.title.clone()
        };
        result.insert(f.id.clone(), display);
    }
    result
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

        let folder_changed = state.move_selection(1);
        assert!(folder_changed);
        assert_eq!(state.selected_folder, Some(0));
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

    #[test]
    fn test_filter_notes_by_tag_query() {
        let mut state = AppState::new();
        state.note_filter_query = "#work".to_string();
        state.note_tags.insert(
            "1".to_string(),
            vec!["work".to_string(), "urgent".to_string()],
        );
        state
            .note_tags
            .insert("2".to_string(), vec!["home".to_string()]);

        let notes = vec![
            Note {
                id: "1".to_string(),
                title: "Project".to_string(),
                ..Note::default()
            },
            Note {
                id: "2".to_string(),
                title: "Groceries".to_string(),
                ..Note::default()
            },
        ];

        let filtered = state.filter_notes(notes);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "1");
    }
}
