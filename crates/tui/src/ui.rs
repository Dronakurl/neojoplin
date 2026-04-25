// UI rendering for NeoJoplin TUI

use neojoplin_core::timestamp_to_datetime;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Tabs, Wrap},
    Frame,
};

use crate::settings::{ConnectionResult, EncryptionField, FormField};
use crate::theme::Theme;

use crate::settings::SettingsTab;
use crate::state::{AppState, FocusPanel, NoteFilterMode};

const HELP_LINES: &[&str] = &[
    "Navigation",
    "  Tab / Shift-Tab    Switch panels (Notebooks -> Notes -> Content)",
    "  h / l / <- ->      Switch panels left / right",
    "  j / k / Up Down    Move selection or scroll content",
    "  gg                 Jump to the top of the focused list or preview",
    "  ge / G             Jump to the end of the focused list or preview",
    "  Enter              Open notebook (in Notebooks panel)",
    "",
    "Notes & Notebooks",
    "  n      New note in current notebook",
    "  N      New notebook",
    "  t      New to-do in current notebook",
    "  T      Convert selected note <-> to-do",
    "  m      Move the selected note or notebook to another notebook",
    "  r      Rename selected note or notebook",
    "  f      Filter the focused list (notes: title only, #tag still works)",
    "  F      Full-text filter across note contents",
    "  ,      Open sort help for the focused list",
    "  d      Delete selected note or notebook (with confirmation)",
    "  D      Delete selected note immediately",
    "  R      Restore selected trashed note",
    "",
    "Filtering",
    "  f / F              Filter the focused list (F searches note contents too)",
    "  =text              Disable fuzzy matching and use a literal substring search",
    "  #=tag              Match a tag literally instead of fuzzily",
    "",
    "Commands",
    "  :move <notebook>       Move the selected note to a notebook",
    "  :mv <notebook>         Alias for :move",
    "  :delete-orphaned       Delete notes whose notebook no longer exists",
    "  :read <file>           Create a note from a file",
    "  :tag <tag-name>        Add a tag to the selected note",
    "  :import                Import from ~/.config/joplin/database.sqlite",
    "  :import <db>           Import from an explicit Joplin SQLite file",
    "  :import-desktop        Import from ~/.config/joplin-desktop/database.sqlite",
    "  :import-jex <file>     Import a JEX archive",
    "  :export-jex <file>     Export notes to a JEX archive",
    "  :mknote <title>        Create a new note",
    "  :mktodo <title>        Create a new to-do",
    "  :mkbook <title>        Create a new notebook",
    "  :quit / :q             Quit immediately",
    "  Tab                    Complete commands, notebooks, tags, and file paths",
    "",
    "Todos & Trash",
    "  Space    Toggle todo completed / unchecked",
    "  Trash    Select the Trash notebook entry to browse deleted notes",
    "  Orphaned Select the Orphaned notebook entry to browse notes without a notebook",
    "",
    "Other",
    "  /          Search inside help",
    "  :          Open the command line",
    "  Enter      Edit selected note in $EDITOR",
    "  s          Sync with WebDAV",
    "  S          Open settings",
    "  q          Quit",
];

pub fn help_search_lines() -> &'static [&'static str] {
    HELP_LINES
}

/// Strip a leading Markdown H1 prefix ("# ") from a title for display in lists/borders.
/// The full title (including "# ") is preserved in the data model and shown in the preview.
fn display_title(title: &str) -> &str {
    title.strip_prefix("# ").unwrap_or(title)
}

/// Render the main UI
pub fn render_ui(f: &mut Frame, state: &AppState) {
    // Calculate heights for keybinding ribbon and status line
    let ribbon_height = if f.area().width < 100 { 2 } else { 1 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints(
            [
                Constraint::Min(0),                // Main content
                Constraint::Length(ribbon_height), // Keybinding ribbon
                Constraint::Length(1),             // Status line
            ]
            .as_ref(),
        )
        .split(f.area());

    // Render main content
    render_main_content(f, state, chunks[0]);

    // Render keybinding ribbon
    render_keybinding_ribbon(f, state, chunks[1]);

    // Render status line
    render_status_line(f, state, chunks[2]);
}

/// Render main content area with split panes
fn render_main_content(f: &mut Frame, state: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .margin(0)
        .constraints(
            [
                Constraint::Percentage(25), // Notebooks
                Constraint::Percentage(25), // Notes
                Constraint::Percentage(50), // Content
            ]
            .as_ref(),
        )
        .split(area);

    render_notebooks_panel(f, state, chunks[0]);
    render_notes_panel(f, state, chunks[1]);
    render_content_panel(f, state, chunks[2]);
}

/// Render notebooks (folders) panel
fn render_notebooks_panel(f: &mut Frame, state: &AppState, area: Rect) {
    let title = if state.has_active_filter(FocusPanel::Notebooks) {
        format!(
            "Notebooks [{}] / {}",
            state.notebook_sort.label(),
            state.notebook_filter_query
        )
    } else {
        format!("Notebooks [{}]", state.notebook_sort.label())
    };
    let theme = &state.theme;

    let items: Vec<ListItem> = if state.folders.is_empty()
        && state.orphan_note_count == 0
        && state.trash_note_count == 0
    {
        if state.has_active_filter(FocusPanel::Notebooks) {
            vec![ListItem::new("No matching notebooks").style(theme.dim())]
        } else {
            vec![
                ListItem::new("No notebooks yet").style(theme.dim()),
                ListItem::new("Press N to create one").style(theme.dim()),
            ]
        }
    } else {
        let mut all_items = vec![];
        let folder_depths = build_folder_depths(state);

        // Add "All Notebooks" option at the top
        let is_all_selected = state.all_notebooks_mode && !state.orphan_mode && !state.trash_mode;
        let all_style = if is_all_selected {
            theme.selection()
        } else {
            theme.text()
        };
        all_items.push(ListItem::new("📚 All Notes").style(all_style));

        // Add individual notebooks
        for (i, folder) in state.folders.iter().enumerate() {
            let is_selected =
                state.selected_folder == Some(i) && !state.all_notebooks_mode && !state.trash_mode;
            let style = if is_selected {
                theme.selection()
            } else {
                theme.text()
            };

            // Extract emoji from folder icon, or use default
            let emoji = extract_folder_emoji(&folder.icon).unwrap_or_else(|| "📁 ".to_string());

            // Use disambiguated display name (with grey suffix for duplicates)
            let display_name = state
                .folder_display_names
                .get(&folder.id)
                .map(String::as_str)
                .unwrap_or(&folder.title);
            let indent = "  ".repeat(*folder_depths.get(folder.id.as_str()).unwrap_or(&0));
            let base_name: &str;
            let suffix: Option<&str>;
            if let Some(paren_pos) = display_name.rfind(" (") {
                base_name = &display_name[..paren_pos];
                suffix = Some(&display_name[paren_pos..]);
            } else {
                base_name = display_name;
                suffix = None;
            }

            let label = if let Some(suf) = suffix {
                Line::from(vec![
                    Span::raw(format!("{}{}{}", indent, emoji, base_name)),
                    Span::styled(suf.to_string(), theme.dim()),
                ])
            } else {
                Line::from(format!("{}{}{}", indent, emoji, base_name))
            };

            all_items.push(ListItem::new(label).style(style));
        }

        let orphan_style = if state.orphan_mode {
            theme.selection()
        } else {
            theme.text()
        };
        all_items.push(ListItem::new(Line::from(vec![
            Span::styled("🔎 Orphaned", orphan_style),
            Span::styled(format!(" ({})", state.orphan_note_count), theme.dim()),
        ])));

        // Add Trash entry at the bottom
        let trash_style = if state.trash_mode && !state.all_notebooks_mode {
            theme.selection()
        } else {
            // Use a more visible error color for trash entries
            theme.error()
        };
        all_items.push(ListItem::new(Line::from(vec![
            Span::styled("🗑 Trash", trash_style),
            Span::styled(format!(" ({})", state.trash_note_count), theme.dim()),
        ])));

        let selected_row = selected_notebook_row(state);
        let visible_rows = area.height.saturating_sub(2) as usize;
        slice_visible_items(all_items, selected_row, visible_rows)
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(if state.focus == FocusPanel::Notebooks {
                    theme.border_focused()
                } else {
                    theme.border_normal()
                }),
        )
        .highlight_style(theme.selection());

    f.render_widget(list, area);
}

/// Render notes panel
fn render_notes_panel(f: &mut Frame, state: &AppState, area: Rect) {
    let title = if state.trash_mode {
        format_with_filter(
            format!("🗑 Trash [{}]", state.note_sort.label()),
            &state.note_filter_query,
        )
    } else if state.orphan_mode {
        format_with_filter(
            format!("Notes - Orphaned [{}]", state.note_sort.label()),
            &state.note_filter_query,
        )
    } else if state.all_notebooks_mode {
        format_with_filter(
            format!("Notes - All Notebooks [{}]", state.note_sort.label()),
            &state.note_filter_query,
        )
    } else if let Some(folder) = state.selected_folder() {
        format_with_filter(
            format!("Notes - {} [{}]", folder.title, state.note_sort.label()),
            &state.note_filter_query,
        )
    } else {
        format_with_filter(
            format!("Notes [{}]", state.note_sort.label()),
            &state.note_filter_query,
        )
    };
    let theme = &state.theme;

    let items: Vec<ListItem> = if state.notes.is_empty() {
        if state.has_active_filter(FocusPanel::Notes) {
            vec![ListItem::new("No matching notes").style(theme.dim())]
        } else if state.trash_mode {
            vec![ListItem::new("Trash is empty").style(theme.dim())]
        } else if state.all_notebooks_mode || state.selected_folder().is_some() {
            vec![
                ListItem::new("No notes in this notebook").style(theme.dim()),
                ListItem::new("Press n to create one").style(theme.dim()),
            ]
        } else {
            vec![
                ListItem::new("No notebook selected").style(theme.dim()),
                ListItem::new("Select a notebook first").style(theme.dim()),
            ]
        }
    } else {
        let all_items: Vec<ListItem> = state
            .notes
            .iter()
            .enumerate()
            .map(|(i, note)| {
                let is_selected = state.selected_note == Some(i);
                let style = if is_selected {
                    theme.selection()
                } else {
                    theme.text()
                };

                let icon = if note.is_todo == 1 {
                    if note.todo_completed > 0 {
                        "󰄲"
                    } else {
                        "󰄱"
                    }
                } else {
                    "📝"
                };

                ListItem::new(format!("{} {}", icon, display_title(&note.title))).style(style)
            })
            .collect();
        slice_visible_items(
            all_items,
            state.selected_note.unwrap_or(0),
            area.height.saturating_sub(2) as usize,
        )
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(if state.focus == FocusPanel::Notes {
                    theme.border_focused()
                } else {
                    theme.border_normal()
                }),
        )
        .highlight_style(theme.selection());

    f.render_widget(list, area);
}

/// Render note content panel
fn render_content_panel(f: &mut Frame, state: &AppState, area: Rect) {
    let title = if let Some(note) = state.selected_note() {
        display_title(&note.title).to_string()
    } else {
        "Content".to_string()
    };
    let theme = &state.theme;
    let notebook_name = current_notebook_label(state);
    let tag_summary = current_tag_summary(state, area.width.saturating_sub(18) as usize);

    // Get note content as markdown. If the title starts with "# ", prepend it so
    // the heading is visible in the rendered preview (the title is stored separately
    // from the body, so we need to inject it here for correct markdown display).
    let markdown_text = if let Some(note) = state.selected_note() {
        let body = if note.body.is_empty() {
            "*This note is empty*".to_string()
        } else {
            note.body.clone()
        };
        if note.title.starts_with("# ") && !note.body.starts_with(&note.title) {
            format!("{}\n\n{}", note.title, body)
        } else {
            body
        }
    } else {
        "*Select a note to view its content*".to_string()
    };

    // Render markdown with termimad → ratatui native Text
    let content_width = area.width.saturating_sub(2) as usize;
    let content_lines: Vec<Line> = termimad_to_ratatui_lines(&markdown_text, content_width);

    // Calculate visible area for scrolling
    let visible_height = area.height.saturating_sub(2) as usize; // Subtract border and padding
    let total_lines = content_lines.len();

    // Ensure scroll offset is valid
    let max_scroll = total_lines.saturating_sub(visible_height);

    let scroll_offset = state.content_scroll_offset.min(max_scroll);

    // Get visible lines based on scroll offset
    let visible_lines: Vec<Line> = content_lines
        .iter()
        .skip(scroll_offset)
        .take(visible_height)
        .cloned()
        .collect();

    let content = Text::from(visible_lines);

    // Create scroll indicator
    let scroll_indicator = if total_lines > visible_height {
        let position = if visible_height >= total_lines {
            100
        } else {
            ((scroll_offset * 100) / (total_lines - visible_height)).min(100)
        };
        format!(" {}% ", position)
    } else {
        " All ".to_string()
    };

    let bottom_indicator = if let Some(note) = state.selected_note() {
        if let Some(tags) = state.note_tags.get(&note.id) {
            if !tags.is_empty() {
                format!("{} • tags: {}", scroll_indicator.trim(), tags.join(", "))
            } else {
                scroll_indicator
            }
        } else {
            scroll_indicator
        }
    } else {
        scroll_indicator
    };

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .title(title)
                .title_alignment(Alignment::Left)
                .title(
                    Line::from(notebook_name)
                        .alignment(Alignment::Right)
                        .style(theme.muted()),
                )
                .title_bottom(Line::from(bottom_indicator).style(theme.muted()))
                .title_bottom(
                    Line::from(tag_summary)
                        .alignment(Alignment::Right)
                        .style(theme.dim()),
                )
                .borders(Borders::ALL)
                .border_style(if state.focus == FocusPanel::Content {
                    theme.border_focused()
                } else {
                    theme.border_normal()
                }),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

/// Convert termimad FmtText to ratatui Lines (native conversion, no ANSI round-trip)
fn termimad_to_ratatui_lines(text: &str, width: usize) -> Vec<Line<'static>> {
    let skin = termimad::MadSkin::default();
    let fmt_text = skin.text(text, Some(width));
    let mut lines: Vec<Line<'static>> = Vec::new();

    for fmt_line in &fmt_text.lines {
        match fmt_line {
            termimad::FmtLine::Normal(composite) => {
                let spans: Vec<Span<'static>> = composite
                    .composite
                    .compounds
                    .iter()
                    .map(|c| {
                        let mut style = Style::default();
                        if c.bold {
                            style = style.add_modifier(Modifier::BOLD);
                        }
                        if c.italic {
                            style = style.add_modifier(Modifier::ITALIC);
                        }
                        if c.strikeout {
                            style = style.add_modifier(Modifier::CROSSED_OUT);
                        }
                        if c.code {
                            style = style.fg(Color::Cyan);
                        }
                        Span::styled(c.src.to_string(), style)
                    })
                    .collect();
                // Apply heading style based on composite type
                use minimad::CompositeStyle;
                let line_spans = match composite.composite.style {
                    CompositeStyle::Header(level) => {
                        let color = match level {
                            1 => Color::Yellow,
                            2 => Color::Green,
                            _ => Color::Cyan,
                        };
                        spans
                            .into_iter()
                            .map(|s| {
                                Span::styled(
                                    s.content.into_owned(),
                                    s.style.fg(color).add_modifier(Modifier::BOLD),
                                )
                            })
                            .collect()
                    }
                    CompositeStyle::Quote => {
                        let mut result = vec![Span::styled(
                            "▌ ".to_string(),
                            Style::default().fg(Color::DarkGray),
                        )];
                        result.extend(spans.into_iter().map(|s| {
                            Span::styled(s.content.into_owned(), s.style.fg(Color::Gray))
                        }));
                        result
                    }
                    CompositeStyle::ListItem(..) => {
                        let mut result = vec![Span::styled(
                            "  • ".to_string(),
                            Style::default().fg(Color::Yellow),
                        )];
                        result.extend(
                            spans
                                .into_iter()
                                .map(|s| Span::styled(s.content.into_owned(), s.style)),
                        );
                        result
                    }
                    _ => spans
                        .into_iter()
                        .map(|s| Span::styled(s.content.into_owned(), s.style))
                        .collect(),
                };
                lines.push(Line::from(line_spans));
            }
            termimad::FmtLine::HorizontalRule => {
                let rule = "─".repeat(width.min(80));
                lines.push(Line::from(Span::styled(
                    rule,
                    Style::default().fg(Color::DarkGray),
                )));
            }
            _ => {
                // Table rows: render as plain text for now
                lines.push(Line::from(""));
            }
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(""));
    }
    lines
}

/// Render keybinding ribbon (show available keybindings) - Zellij-style arrow separators
fn render_keybinding_ribbon(f: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;
    let arrow = ""; // Powerline separator

    // Build dynamic list of bindings based on state and availability
    let mut bindings: Vec<(String, String, bool)> = Vec::new();
    let filters_active = state.has_active_filter(FocusPanel::Notebooks)
        || state.has_active_filter(FocusPanel::Notes);

    // Always available
    bindings.push(("q".to_string(), "QUIT".to_string(), false));
    bindings.push(("hjkl".to_string(), "NAV".to_string(), false));

    // Contextual bindings
    if state.trash_mode {
        bindings.push(("R".to_string(), "RESTORE".to_string(), false));
        bindings.push(("d".to_string(), "DELETE".to_string(), false));
    } else {
        // Only show New when in notebooks/notes
        if matches!(
            state.focus,
            crate::state::FocusPanel::Notebooks | crate::state::FocusPanel::Notes
        ) {
            bindings.push(("n".to_string(), "NEW".to_string(), false));
        }
        bindings.push(("d".to_string(), "DELETE".to_string(), false));
        bindings.push(("t".to_string(), "TODO".to_string(), false));
        bindings.push((",".to_string(), "SORT".to_string(), false));
        bindings.push(("s".to_string(), "SYNC".to_string(), false));
    }

    // Filter available when focus is list-based
    if matches!(
        state.focus,
        crate::state::FocusPanel::Notebooks | crate::state::FocusPanel::Notes
    ) || state.trash_mode
    {
        bindings.push(("f".to_string(), "FILTER".to_string(), filters_active));
    }

    // Settings always available
    bindings.push(("S".to_string(), "SETTINGS".to_string(), false));
    bindings.push(("?".to_string(), "HELP".to_string(), false));

    // Hidden bindings (not in ribbon) are not included here

    let mut spans = vec![];
    let mut total_width = 0;
    let available_width = area.width as usize;

    for (key, action, highlighted) in bindings.iter() {
        let key_width = key.chars().count();
        let action_width = action.chars().count();
        let arrow_width = arrow.chars().count();
        let segment_width = key_width + 1 + arrow_width + 1 + action_width + 1 + arrow_width + 1;
        if total_width + segment_width > available_width {
            break;
        }

        let action_bg = if *highlighted {
            theme.warning
        } else {
            theme.primary
        };
        let surface_color = theme.surface;
        let key_color = theme.text;
        let action_fg_color = Color::Black;

        spans.push(Span::styled(
            format!("{} ", key),
            Style::default().fg(key_color).bg(surface_color),
        ));
        total_width += key_width;

        spans.push(Span::styled(
            arrow,
            Style::default().fg(surface_color).bg(action_bg),
        ));
        total_width += arrow_width;

        spans.push(Span::styled(
            format!(" {} ", action),
            Style::default().fg(action_fg_color).bg(action_bg).bold(),
        ));
        total_width += 1 + action_width + 1;

        spans.push(Span::styled(
            arrow,
            Style::default().fg(action_bg).bg(surface_color),
        ));
        total_width += arrow_width;

        spans.push(Span::styled(
            " ",
            Style::default().fg(key_color).bg(surface_color),
        ));
        total_width += 1;
    }

    let help_text = vec![Line::from(spans)];

    let paragraph = Paragraph::new(help_text)
        .alignment(Alignment::Left)
        .block(Block::default().bg(theme.surface));

    f.render_widget(paragraph, area);
}

/// Render status line (show current status message)
fn render_status_line(f: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;

    let status_text = if state.show_filter_prompt {
        let target = match state.filter_target {
            FocusPanel::Notebooks => "Filter notebooks: ",
            FocusPanel::Notes | FocusPanel::Content => match state.note_filter_mode {
                NoteFilterMode::TitleOnly => "Filter note titles (#tag): ",
                NoteFilterMode::FullText => "Full-text filter (#tag): ",
            },
        };
        Line::from(vec![
            Span::styled(target, theme.muted()),
            Span::styled(&state.filter_input, theme.primary()),
            Span::styled("█", theme.muted()),
            Span::styled("  [= literal] [Enter confirm] [Esc cancel]", theme.muted()),
        ])
    } else if state.command_prompt.visible {
        let mut spans = vec![
            Span::styled(":", theme.muted()),
            Span::styled(&state.command_prompt.input, theme.primary()),
            Span::styled("█", theme.muted()),
        ];

        if let Some(completion) = state.command_prompt.completion.as_ref() {
            if !completion.items.is_empty() {
                let preview = completion
                    .items
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("  ");
                spans.push(Span::styled("  [Tab cycle] ", theme.muted()));
                spans.push(Span::styled(preview, theme.text()));
            }
        } else {
            spans.push(Span::styled(
                "  [Tab complete] [Up/Down history] [Enter run] [Esc cancel]",
                theme.muted(),
            ));
        }

        Line::from(spans)
    } else if state.status_message.is_empty() {
        Line::from(vec![Span::from("Ready").style(theme.muted())])
    } else {
        Line::from(vec![
            Span::from("→ ").style(theme.muted()),
            Span::styled(&state.status_message, theme.primary()),
        ])
    };

    let paragraph = Paragraph::new(status_text)
        .alignment(Alignment::Left)
        .block(Block::default().bg(theme.surface));

    f.render_widget(paragraph, area);
}

/// Render settings menu
pub fn render_settings(f: &mut Frame, state: &AppState) {
    let area = centered_rect(70, 80, f.area());
    let theme = &state.theme;

    // Build contextual bottom hints
    let bottom_hints = settings_bottom_hints(state, theme);

    // Outer block shared across all tabs
    let outer_block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .border_style(theme.border_focused())
        .title_bottom(bottom_hints);

    let inner = outer_block.inner(area);
    f.render_widget(outer_block, area);

    // Layout: tabs row + content
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    // Tab bar
    let tab_names = ["Sync", "Auto-sync", "Status", "Encryption"];
    let current_tab_idx = match state.settings.current_tab {
        SettingsTab::Sync => 0,
        SettingsTab::AutoSync => 1,
        SettingsTab::Status => 2,
        SettingsTab::Encryption => 3,
    };
    let tabs = Tabs::new(tab_names)
        .select(current_tab_idx)
        .style(theme.muted())
        .highlight_style(theme.primary().bold());
    f.render_widget(tabs, layout[0]);

    // Content area for the active tab
    match state.settings.current_tab {
        SettingsTab::Sync => render_sync_settings_content(f, state, layout[1]),
        SettingsTab::AutoSync => render_auto_sync_settings(f, state, layout[1]),
        SettingsTab::Status => render_sync_status_settings(f, state, layout[1]),
        SettingsTab::Encryption => render_encryption_settings(f, state, layout[1]),
    }

    // Delete confirmation overlay
    if state.settings.sync.confirm_delete {
        let target_name = state
            .settings
            .sync
            .current_target_index
            .and_then(|i| state.settings.sync.targets.get(i))
            .map(|t| t.name.as_str())
            .unwrap_or("this target");
        render_delete_confirm_overlay(f, target_name, area, theme);
    }
}

/// Build contextual bottom hints for the settings panel
fn settings_bottom_hints<'a>(state: &'a AppState, theme: &'a Theme) -> Line<'a> {
    fn kh<'a>(theme: &'a Theme, key: &'static str, desc: &'static str) -> Vec<Span<'a>> {
        vec![
            Span::styled("[", theme.muted()),
            Span::styled(key, theme.accent()),
            Span::styled("]", theme.muted()),
            Span::raw(format!(" {} ", desc)).style(theme.text()),
        ]
    }

    let sync = &state.settings.sync;
    let enc = &state.settings.encryption;
    let mut spans: Vec<Span<'_>> = Vec::new();

    if sync.show_add_form || sync.show_edit_form {
        for s in kh(theme, "Tab", "next field") {
            spans.push(s);
        }
        for s in kh(theme, "Enter", "save") {
            spans.push(s);
        }
        for s in kh(theme, "Esc", "cancel") {
            spans.push(s);
        }
        for s in kh(theme, "Ctrl+T", "test") {
            spans.push(s);
        }
    } else if sync.confirm_delete {
        for s in kh(theme, "y/Enter", "confirm") {
            spans.push(s);
        }
        for s in kh(theme, "n/Esc", "cancel") {
            spans.push(s);
        }
    } else if enc.show_new_key_prompt {
        for s in kh(theme, "Tab", "switch field") {
            spans.push(s);
        }
        for s in kh(theme, "Enter", "confirm") {
            spans.push(s);
        }
        for s in kh(theme, "Esc", "cancel") {
            spans.push(s);
        }
    } else {
        for s in kh(theme, "h/l", "switch tab") {
            spans.push(s);
        }
        for s in kh(theme, "q", "close") {
            spans.push(s);
        }
        match state.settings.current_tab {
            SettingsTab::Sync => {
                for s in kh(theme, "n", "add") {
                    spans.push(s);
                }
                if !sync.targets.is_empty() {
                    for s in kh(theme, "e", "edit") {
                        spans.push(s);
                    }
                    for s in kh(theme, "d", "delete") {
                        spans.push(s);
                    }
                }
            }
            SettingsTab::AutoSync => {
                for s in kh(theme, "j/k", "change interval") {
                    spans.push(s);
                }
            }
            SettingsTab::Status => {
                for s in kh(theme, "r", "refresh") {
                    spans.push(s);
                }
            }
            SettingsTab::Encryption => {
                if enc.enabled {
                    for s in kh(theme, "d", "disable") {
                        spans.push(s);
                    }
                } else {
                    for s in kh(theme, "e", "enable") {
                        spans.push(s);
                    }
                }
            }
        }
    }

    Line::from(spans)
}

/// Render a small delete confirmation overlay
fn render_delete_confirm_overlay(f: &mut Frame, target_name: &str, parent: Rect, theme: &Theme) {
    let area = centered_rect(50, 20, parent);
    let msg = format!("Delete '{}'?", target_name);
    let hints = Line::from(vec![
        Span::styled("[y/Enter]", theme.accent()),
        Span::raw(" yes  ").style(theme.text()),
        Span::styled("[n/Esc]", theme.muted()),
        Span::raw(" no").style(theme.text()),
    ]);
    let content = Paragraph::new(vec![
        Line::from(""),
        Line::from(msg).alignment(Alignment::Center),
        Line::from(""),
    ])
    .block(
        Block::default()
            .title("")
            .borders(Borders::ALL)
            .border_style(theme.error())
            .title_bottom(hints),
    )
    .alignment(Alignment::Center);
    f.render_widget(content, area);
}

/// Render encryption settings tab content
fn render_encryption_settings(f: &mut Frame, state: &AppState, area: Rect) {
    let enc = &state.settings.encryption;
    let theme = &state.theme;

    if enc.show_new_key_prompt {
        render_encryption_password_form(f, state, area);
        return;
    }

    // Status overview
    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("Status: "),
            Span::styled(&enc.status_message, Style::default().bold()),
        ]),
        Line::from(""),
    ];

    if let Some(ref key_id) = enc.active_master_key_id {
        lines.push(Line::from(vec![
            Span::styled("Active Key: ", theme.muted()),
            Span::styled(&key_id[..8.min(key_id.len())], Style::default().bold()),
            Span::raw("…"),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled("Master Keys: ", theme.muted()),
        Span::raw(enc.master_key_count.to_string()),
    ]));
    lines.push(Line::from(""));

    if enc.password_success {
        lines.push(Line::from(vec![
            Span::styled("✓ ", theme.success()),
            Span::styled(
                if enc.enabled {
                    "Encryption enabled"
                } else {
                    "Encryption disabled"
                },
                theme.success(),
            ),
        ]));
    }

    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .alignment(Alignment::Left);
    f.render_widget(paragraph, area);
}

/// Render the password input form for enabling encryption
fn render_encryption_password_form(f: &mut Frame, state: &AppState, area: Rect) {
    let enc = &state.settings.encryption;
    let theme = &state.theme;

    // Layout: header + 2 input fields + error/hint
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(2), // header
            Constraint::Length(3), // Password field
            Constraint::Length(3), // Confirm field
            Constraint::Min(1),    // error / hint
        ])
        .split(area);

    let header =
        Paragraph::new(vec![Line::from("Set Master Password").bold()]).alignment(Alignment::Center);
    f.render_widget(header, chunks[0]);

    let is_pw_active = enc.active_field == EncryptionField::Password;
    let is_confirm_active = enc.active_field == EncryptionField::Confirm;

    render_form_field_password(
        f,
        "Password:",
        &enc.password_input,
        chunks[1],
        theme,
        is_pw_active,
    );
    render_form_field_password(
        f,
        "Confirm: ",
        &enc.confirm_password_input,
        chunks[2],
        theme,
        is_confirm_active,
    );

    // Error or hint
    if let Some(ref error) = enc.password_error {
        let err = Paragraph::new(vec![Line::from(vec![
            Span::styled("⚠ ", theme.error()),
            Span::styled(error.clone(), theme.error()),
        ])]);
        f.render_widget(err, chunks[3]);
    } else {
        let hint = Paragraph::new("Type password, Tab to switch fields, Enter to confirm")
            .style(theme.muted())
            .alignment(Alignment::Center);
        f.render_widget(hint, chunks[3]);
    }
}

/// Render sync settings content (no outer block — provided by render_settings)
fn render_sync_settings_content(f: &mut Frame, state: &AppState, area: Rect) {
    // Split into target list (left) and form/details (right)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
        .margin(1)
        .split(area);

    // Render target list
    render_target_list(f, state, chunks[0]);

    // Render form or details based on state
    if state.settings.sync.show_add_form || state.settings.sync.show_edit_form {
        render_target_form(f, state, chunks[1]);
    } else {
        render_target_details(f, state, chunks[1]);
    }
}

/// Render target list
fn render_target_list(f: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;
    let sync = &state.settings.sync;

    let items: Vec<ListItem> = if sync.targets.is_empty() {
        vec![
            ListItem::new(Line::from(vec![Span::styled(
                "No sync targets configured",
                theme.muted(),
            )])),
            ListItem::new(Line::from(vec![
                Span::raw("Press "),
                Span::styled("'n'", theme.accent()),
                Span::raw(" to add one"),
            ])),
        ]
    } else {
        sync.targets
            .iter()
            .enumerate()
            .map(|(i, target)| {
                let is_active = sync.current_target_index == Some(i);
                let prefix = if is_active { "● " } else { "○ " };
                let style = if is_active {
                    theme.primary()
                } else {
                    theme.text()
                };

                ListItem::new(Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(&target.name, style),
                ]))
            })
            .collect()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Sync Targets ")
                .borders(Borders::ALL)
                .border_style(theme.border_normal()),
        )
        .highlight_style(theme.selection());

    f.render_widget(list, area);
}

/// Render target details (when no form is active)
fn render_target_details(f: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;
    let sync = &state.settings.sync;

    if sync.targets.is_empty() {
        let help_text = vec![
            Line::from("Sync Target Management").bold(),
            Line::from(""),
            Line::from("Configure your WebDAV sync targets here."),
            Line::from(""),
            Line::from("Key bindings:"),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("'n'", theme.accent()),
                Span::raw(" - Add new target"),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("'e'", theme.accent()),
                Span::raw(" - Edit selected target"),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("'d'", theme.accent()),
                Span::raw(" - Delete selected target"),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("Enter", theme.accent()),
                Span::raw(" - Set as active"),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("↑/↓", theme.accent()),
                Span::raw(" - Navigate targets"),
            ]),
        ];

        let paragraph = Paragraph::new(help_text)
            .block(
                Block::default()
                    .title(" Instructions ")
                    .borders(Borders::ALL)
                    .border_style(theme.border_normal()),
            )
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);

        f.render_widget(paragraph, area);
        return;
    }

    // Show details of current target
    if let Some(idx) = sync.current_target_index {
        if let Some(target) = sync.targets.get(idx) {
            let details = vec![
                Line::from(vec![
                    Span::styled("Target Details: ", theme.primary()),
                    Span::styled(&target.name, theme.text()).bold(),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Type: ", theme.muted()),
                    Span::styled(format!("{:?}", target.target_type), theme.text()),
                ]),
                Line::from(vec![
                    Span::styled("URL: ", theme.muted()),
                    Span::styled(&target.url, theme.text()),
                ]),
                Line::from(vec![
                    Span::styled("Username: ", theme.muted()),
                    Span::styled(
                        if target.username.is_empty() {
                            "(none)"
                        } else {
                            &target.username
                        },
                        theme.text(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Password: ", theme.muted()),
                    Span::styled(
                        if target.password.is_empty() {
                            "(not set)"
                        } else {
                            "•••• (set)"
                        },
                        theme.text(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Path: ", theme.muted()),
                    Span::styled(&target.remote_path, theme.text()),
                ]),
                Line::from(""),
                Line::from("Actions:"),
                Line::from(vec![
                    Span::raw("  Press "),
                    Span::styled("'e'", theme.accent()),
                    Span::raw(" to edit"),
                ]),
                Line::from(vec![
                    Span::raw("  Press "),
                    Span::styled("'d'", theme.accent()),
                    Span::raw(" to delete"),
                ]),
            ];

            let paragraph = Paragraph::new(details)
                .block(
                    Block::default()
                        .title(" Selected Target ")
                        .borders(Borders::ALL)
                        .border_style(theme.border_normal()),
                )
                .wrap(Wrap { trim: false })
                .alignment(Alignment::Left);

            f.render_widget(paragraph, area);
        }
    }
}

/// Render target form (add/edit)
fn render_target_form(f: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;
    let sync = &state.settings.sync;

    // Form layout with input fields (URL = full WebDAV URL including remote path)
    let form_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3), // Name
                Constraint::Length(3), // URL (full WebDAV URL)
                Constraint::Length(3), // Username
                Constraint::Length(3), // Password
                Constraint::Length(2), // Error message
                Constraint::Length(3), // Hints
            ]
            .as_ref(),
        )
        .split(area);

    let is_name_active = sync.active_field == Some(FormField::Name);
    let is_url_active = sync.active_field == Some(FormField::Url);
    let is_username_active = sync.active_field == Some(FormField::Username);
    let is_password_active = sync.active_field == Some(FormField::Password);

    render_form_field(
        f,
        "Name:",
        &sync.name_input,
        form_chunks[0],
        theme,
        is_name_active,
    );
    render_form_field(
        f,
        "URL: ",
        &sync.url_input,
        form_chunks[1],
        theme,
        is_url_active,
    );
    render_form_field(
        f,
        "User:",
        &sync.username_input,
        form_chunks[2],
        theme,
        is_username_active,
    );
    render_form_field_password(
        f,
        "Pass:",
        &sync.password_input,
        form_chunks[3],
        theme,
        is_password_active,
    );

    // Error / connection result
    if let Some(ref error) = sync.form_error {
        let error_text = Paragraph::new(error.clone())
            .style(theme.error())
            .alignment(Alignment::Center);
        f.render_widget(error_text, form_chunks[4]);
    } else if let Some(ref result) = sync.connection_result {
        match result {
            ConnectionResult::Success(message) => {
                let success_text = Paragraph::new(format!("✓ {}", message))
                    .style(theme.success())
                    .alignment(Alignment::Center);
                f.render_widget(success_text, form_chunks[4]);
            }
            ConnectionResult::Failed(err) => {
                let error_text = Paragraph::new(format!("✗ {}", err))
                    .style(theme.error())
                    .alignment(Alignment::Center);
                f.render_widget(error_text, form_chunks[4]);
            }
        }
    }

    let hint = Paragraph::new(
        Line::from("Enter URL including remote path, e.g. http://host/webdav/sync")
            .style(theme.muted()),
    )
    .alignment(Alignment::Center);
    f.render_widget(hint, form_chunks[5]);
}

/// Helper function to render a form field
fn render_form_field(
    f: &mut Frame,
    label: &str,
    value: &str,
    area: Rect,
    theme: &Theme,
    is_active: bool,
) {
    let cursor = if is_active { "█" } else { "" };
    let style = if is_active {
        theme.primary()
    } else {
        theme.text()
    };

    let text = Line::from(vec![
        Span::styled(label, theme.muted()),
        Span::styled(value, style),
        Span::styled(cursor, theme.primary()),
    ]);

    let border_style = if is_active {
        theme.border_focused()
    } else {
        theme.border_normal()
    };

    let paragraph = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style),
    );
    f.render_widget(paragraph, area);
}

/// Helper function to render a password form field (masked)
fn render_form_field_password(
    f: &mut Frame,
    label: &str,
    value: &str,
    area: Rect,
    theme: &Theme,
    is_active: bool,
) {
    let display_value = if value.is_empty() {
        if is_active {
            String::new()
        } else {
            "(not set)".to_string()
        }
    } else if is_active {
        "•".repeat(value.len())
    } else {
        "•••• (set)".to_string()
    };
    render_form_field(f, label, &display_value, area, theme, is_active);
}

/// Render help popup
pub fn render_help(
    f: &mut Frame,
    scroll: u16,
    state: &AppState,
    search_query: Option<&str>,
    search_input: Option<&str>,
) {
    let area = centered_rect(80, 80, f.area());
    let theme = &state.theme;

    let bottom_title = Line::from(vec![
        Span::styled("[", theme.muted()),
        Span::styled("q", theme.accent()),
        Span::styled("]", theme.muted()),
        Span::raw(" close ").style(theme.text()),
        Span::styled("[", theme.muted()),
        Span::styled("j/k", theme.accent()),
        Span::styled("]", theme.muted()),
        Span::raw(" scroll ").style(theme.text()),
        Span::styled("[", theme.muted()),
        Span::styled("/", theme.accent()),
        Span::styled("]", theme.muted()),
        Span::raw(" search ").style(theme.text()),
        Span::styled(
            match search_input {
                Some(input) => format!(" /{}", input),
                None => search_query
                    .filter(|query| !query.is_empty())
                    .map(|query| format!(" query: {}", query))
                    .unwrap_or_default(),
            },
            theme.muted(),
        ),
    ]);

    let search_query = search_query
        .filter(|query| !query.is_empty())
        .map(|q| q.to_lowercase());
    let text = Text::from(
        HELP_LINES
            .iter()
            .map(|line| {
                let is_heading = !line.is_empty() && !line.starts_with(' ');
                let matches = search_query
                    .as_ref()
                    .map(|query| line.to_lowercase().contains(query))
                    .unwrap_or(false);

                if matches {
                    Line::from(Span::styled(*line, theme.accent().bold()))
                } else if is_heading {
                    Line::from(Span::styled(*line, theme.primary()))
                } else {
                    Line::from(*line)
                }
            })
            .collect::<Vec<_>>(),
    );

    // Clamp scroll to the available content height to avoid infinite scrolling
    let total_lines = text.lines.len();
    let visible_height = area.height.saturating_sub(2) as usize; // border + padding
    let max_scroll = if total_lines > visible_height {
        (total_lines - visible_height) as u16
    } else {
        0u16
    };
    let scroll = std::cmp::min(scroll, max_scroll);

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title("Help")
                .title_bottom(bottom_title)
                .borders(Borders::ALL)
                .border_style(theme.border_focused()),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

fn render_auto_sync_settings(f: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;
    let settings = &state.settings.auto_sync;
    let options: Vec<ListItem> = crate::settings::AUTO_SYNC_INTERVAL_OPTIONS
        .iter()
        .map(|interval| {
            let is_selected = *interval == settings.interval_seconds;
            let style = if is_selected {
                theme.selection()
            } else {
                theme.text()
            };
            ListItem::new(format_auto_sync_interval(*interval)).style(style)
        })
        .collect();

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .margin(1)
        .split(area);

    let list = List::new(options).block(
        Block::default()
            .title(" Interval ")
            .borders(Borders::ALL)
            .border_style(theme.border_normal()),
    );
    f.render_widget(list, chunks[0]);

    let details = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Current: ", theme.muted()),
            Span::styled(
                format_auto_sync_interval(settings.interval_seconds),
                theme.primary(),
            ),
        ]),
        Line::from(""),
        Line::from("NeoJoplin will automatically run a background sync on this interval."),
        Line::from("The timer is reset after each sync attempt."),
    ])
    .block(
        Block::default()
            .title(" Auto-sync ")
            .borders(Borders::ALL)
            .border_style(theme.border_normal()),
    )
    .wrap(Wrap { trim: false });
    f.render_widget(details, chunks[1]);
}

fn render_sync_status_settings(f: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;
    let status = &state.settings.status;

    let last_sync = status
        .last_sync_time
        .map(|value| {
            timestamp_to_datetime(value)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| "Never".to_string());
    let result_style = if status.last_sync_success {
        theme.success()
    } else {
        theme.error()
    };
    let result_text = if status.last_sync_time.is_none() {
        "No sync has completed yet".to_string()
    } else if status.last_sync_success {
        "Success".to_string()
    } else {
        status
            .last_sync_error
            .clone()
            .unwrap_or_else(|| "Failed".to_string())
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Last sync: ", theme.muted()),
            Span::styled(last_sync, theme.text()),
        ]),
        Line::from(vec![
            Span::styled("Last target: ", theme.muted()),
            Span::styled(
                status
                    .last_sync_target_name
                    .clone()
                    .unwrap_or_else(|| "(none)".to_string()),
                theme.text(),
            ),
        ]),
        Line::from(vec![
            Span::styled("Last result: ", theme.muted()),
            Span::styled(result_text, result_style),
        ]),
        Line::from(vec![
            Span::styled("Conflicts: ", theme.muted()),
            Span::styled(status.current_conflict_count.to_string(), theme.text()),
        ]),
        Line::from(vec![
            Span::styled("Encryption now: ", theme.muted()),
            Span::styled(
                if status.current_encryption_enabled {
                    "Enabled"
                } else {
                    "Disabled"
                },
                if status.current_encryption_enabled {
                    theme.success()
                } else {
                    theme.warning()
                },
            ),
        ]),
        Line::from(vec![
            Span::styled("Encryption on last sync: ", theme.muted()),
            Span::styled(
                if status.last_sync_encryption_enabled {
                    "Enabled"
                } else {
                    "Disabled"
                },
                if status.last_sync_encryption_enabled {
                    theme.success()
                } else {
                    theme.warning()
                },
            ),
        ]),
    ];

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Sync Status ")
                .borders(Borders::ALL)
                .border_style(theme.border_normal()),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);
}

fn format_auto_sync_interval(interval_seconds: u64) -> String {
    match interval_seconds {
        0 => "Disabled".to_string(),
        300 => "Every 5 minutes".to_string(),
        600 => "Every 10 minutes".to_string(),
        1800 => "Every 30 minutes".to_string(),
        3600 => "Every hour".to_string(),
        43200 => "Every 12 hours".to_string(),
        86400 => "Every 24 hours".to_string(),
        value => format!("Every {} seconds", value),
    }
}

/// Render quit confirmation popup
pub fn render_quit_confirmation(f: &mut Frame, state: &AppState) {
    let area = centered_rect(35, 15, f.area()); // Smaller: 35% width, 15% height
    let theme = &state.theme;

    let bottom_title = Line::from(vec![
        Span::styled("[", theme.muted()),
        Span::styled("q", theme.accent()),
        Span::styled("]", theme.muted()),
        Span::raw(" or ").style(theme.text()),
        Span::styled("[", theme.muted()),
        Span::styled("y", theme.accent()),
        Span::styled("]", theme.muted()),
        Span::raw(" quit ").style(theme.text()),
        Span::styled("[", theme.muted()),
        Span::styled("any", theme.accent()),
        Span::styled("]", theme.muted()),
        Span::raw(" cancel ").style(theme.text()),
    ]);

    let text = Text::from(vec![
        Line::from(""),
        Line::from("Quit NeoJoplin?").style(theme.primary()),
        Line::from(""),
    ]);

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title("Confirm Quit")
                .title_bottom(bottom_title)
                .borders(Borders::ALL)
                .border_style(theme.border_focused()),
        )
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Center);

    f.render_widget(paragraph, area);
}

pub fn render_delete_confirmation(f: &mut Frame, state: &AppState) {
    let area = centered_rect_with_min_width(42, 22, 52, f.area());
    let theme = &state.theme;

    let (question, item_label) = match state.pending_delete.as_ref() {
        Some(crate::state::PendingDelete::Note {
            title,
            permanent: true,
            ..
        }) => ("Permanently delete note?", title.as_str()),
        Some(crate::state::PendingDelete::Note { title, .. }) => {
            ("Move note to trash?", title.as_str())
        }
        Some(crate::state::PendingDelete::Notebook { title, .. }) => {
            ("Delete notebook?", title.as_str())
        }
        None => ("Delete?", ""),
    };

    let bottom_title = match state.pending_delete.as_ref() {
        Some(crate::state::PendingDelete::Notebook { .. }) => Line::from(vec![
            Span::styled("[", theme.muted()),
            Span::styled("y/Enter", theme.accent()),
            Span::styled("]", theme.muted()),
            Span::raw(" notebook only ").style(theme.text()),
            Span::styled("[", theme.muted()),
            Span::styled("Y", theme.accent()),
            Span::styled("]", theme.muted()),
            Span::raw(" delete notebook + notes ").style(theme.text()),
            Span::styled("[", theme.muted()),
            Span::styled("n/Esc", theme.accent()),
            Span::styled("]", theme.muted()),
            Span::raw(" cancel ").style(theme.text()),
        ]),
        _ => Line::from(vec![
            Span::styled("[", theme.muted()),
            Span::styled("y/Enter", theme.accent()),
            Span::styled("]", theme.muted()),
            Span::raw(" delete ").style(theme.text()),
            Span::styled("[", theme.muted()),
            Span::styled("n/Esc", theme.accent()),
            Span::styled("]", theme.muted()),
            Span::raw(" cancel ").style(theme.text()),
        ]),
    };

    let mut text_lines = vec![
        Line::from(""),
        Line::from(question).style(theme.error()),
        Line::from(""),
        Line::from(item_label).style(theme.primary()),
        Line::from(""),
    ];
    if let Some(crate::state::PendingDelete::Notebook { note_count, .. }) =
        state.pending_delete.as_ref()
    {
        text_lines.push(Line::from(""));
        text_lines.push(Line::from(format!(
            "{} notes will become orphaned if you keep them.",
            note_count
        )));
    }
    text_lines.push(Line::from(""));

    let text = Text::from(text_lines);

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title("")
                .title_bottom(bottom_title)
                .borders(Borders::ALL)
                .border_style(theme.error()),
        )
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Center);

    f.render_widget(paragraph, area);
}

fn build_folder_depths(state: &AppState) -> std::collections::HashMap<&str, usize> {
    let parent_by_id: std::collections::HashMap<&str, &str> = state
        .folders
        .iter()
        .map(|folder| (folder.id.as_str(), folder.parent_id.as_str()))
        .collect();

    state
        .folders
        .iter()
        .map(|folder| {
            let mut depth = 0usize;
            let mut parent_id = folder.parent_id.as_str();
            while !parent_id.is_empty() {
                depth += 1;
                parent_id = parent_by_id.get(parent_id).copied().unwrap_or("");
            }
            (folder.id.as_str(), depth)
        })
        .collect()
}

fn selected_notebook_row(state: &AppState) -> usize {
    if state.trash_mode {
        state.folders.len() + 2
    } else if state.orphan_mode {
        state.folders.len() + 1
    } else if state.all_notebooks_mode {
        0
    } else {
        state.selected_folder.map(|index| index + 1).unwrap_or(0)
    }
}

fn slice_visible_items(
    items: Vec<ListItem>,
    selected: usize,
    visible_rows: usize,
) -> Vec<ListItem> {
    if visible_rows == 0 || items.len() <= visible_rows {
        return items;
    }

    let selected = selected.min(items.len() - 1);
    let start = if selected >= visible_rows {
        selected + 1 - visible_rows
    } else {
        0
    };
    items.into_iter().skip(start).take(visible_rows).collect()
}

fn current_notebook_label(state: &AppState) -> String {
    let Some(note) = state.selected_note() else {
        return String::new();
    };

    if let Some(folder) = state
        .folders
        .iter()
        .find(|folder| folder.id == note.parent_id)
    {
        state
            .folder_display_names
            .get(&folder.id)
            .cloned()
            .unwrap_or_else(|| folder.title.clone())
    } else if note.deleted_time > 0 {
        "Trash".to_string()
    } else {
        "Orphaned".to_string()
    }
}

fn current_tag_summary(state: &AppState, max_width: usize) -> String {
    let Some(note) = state.selected_note() else {
        return String::new();
    };
    let Some(tags) = state.note_tags.get(&note.id) else {
        return String::new();
    };
    if tags.is_empty() {
        return String::new();
    }

    truncate_text(&format!("tags: {}", tags.join(", ")), max_width)
}

fn truncate_text(text: &str, max_width: usize) -> String {
    if max_width == 0 || text.chars().count() <= max_width {
        return text.to_string();
    }
    if max_width <= 1 {
        return "…".to_string();
    }
    let mut truncated: String = text.chars().take(max_width - 1).collect();
    truncated.push('…');
    truncated
}

/// Render error dialog popup
pub fn render_error_dialog(f: &mut Frame, state: &AppState) {
    let area = centered_rect(60, 25, f.area()); // Wider for error messages
    let theme = &state.theme;

    let bottom_title = Line::from(vec![
        Span::styled("[", theme.muted()),
        Span::styled("Enter", theme.accent()),
        Span::styled("]", theme.muted()),
        Span::raw(" close ").style(theme.text()),
    ]);

    // Split error message into multiple lines if it's too long
    let error_lines = split_error_text(&state.error_message, 50);

    let mut text_lines = vec![
        Line::from(""),
        Line::from("⚠ Error").style(theme.error()),
        Line::from(""),
    ];

    for line in error_lines {
        text_lines.push(Line::from(line).style(theme.text()));
    }

    text_lines.push(Line::from(""));
    text_lines.push(Line::from(""));

    let text = Text::from(text_lines);

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title("Error")
                .title_bottom(bottom_title)
                .borders(Borders::ALL)
                .border_style(theme.error()),
        )
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

/// Split error text into multiple lines for better display
fn split_error_text(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = vec![];
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else if current_line.len() + 1 + word.len() <= max_width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(text.to_string());
    }

    lines
}

/// Render rename prompt
pub fn render_rename_prompt(f: &mut Frame, state: &AppState) {
    let area = centered_rect_with_min_width(40, 15, 50, f.area());
    let theme = &state.theme;

    let item_name = if state.focus == FocusPanel::Notes {
        state
            .selected_note()
            .map(|n| n.title.as_str())
            .unwrap_or("note")
    } else {
        state
            .selected_folder()
            .map(|f| f.title.as_str())
            .unwrap_or("notebook")
    };

    let title = format!("Rename: {}", item_name);
    let bottom_title = Line::from(vec![
        Span::styled("[", theme.muted()),
        Span::styled("Enter", theme.accent()),
        Span::styled("]", theme.muted()),
        Span::raw(" confirm ").style(theme.text()),
        Span::styled("[", theme.muted()),
        Span::styled("Esc", theme.accent()),
        Span::styled("]", theme.muted()),
        Span::raw(" cancel ").style(theme.text()),
    ]);

    // Input field with visual highlighting using a styled paragraph
    let input_text = vec![
        Span::styled("New name: ", theme.text()),
        Span::styled(&state.rename_input, theme.primary()),
        Span::styled("█", theme.muted()), // Cursor indicator
    ];

    // Main dialog content with centered input
    let text = Text::from(vec![Line::from(""), Line::from(input_text), Line::from("")]);

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title(title)
                .title_bottom(bottom_title)
                .borders(Borders::ALL)
                .border_style(theme.border_focused()),
        )
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

/// Render sort help popup
pub fn render_sort_popup(f: &mut Frame, state: &AppState) {
    let area = centered_rect_with_min_width(52, 22, 56, f.area());
    let theme = &state.theme;

    let (title, current_sort, lines) = match state.focus {
        FocusPanel::Notebooks => (
            "Sort Notebooks",
            state.notebook_sort.label(),
            vec![
                Line::from("t  Sort by time (oldest first)").style(theme.text()),
                Line::from("T  Sort by time (newest first)").style(theme.text()),
                Line::from("a  Sort by name").style(theme.text()),
                Line::from("m  Sort by newest changed note in notebook").style(theme.text()),
            ],
        ),
        FocusPanel::Notes => (
            "Sort Notes",
            state.note_sort.label(),
            vec![
                Line::from("t  Sort by time (oldest first)").style(theme.text()),
                Line::from("T  Sort by time (newest first)").style(theme.text()),
                Line::from("a  Sort by name").style(theme.text()),
            ],
        ),
        FocusPanel::Content => (
            "Sort",
            "n/a",
            vec![Line::from("Focus notebooks or notes to change sorting").style(theme.text())],
        ),
    };

    let bottom_title = Line::from(vec![
        Span::styled("[", theme.muted()),
        Span::styled("Esc", theme.accent()),
        Span::styled("]", theme.muted()),
        Span::raw(" close ").style(theme.text()),
    ]);

    let mut text_lines = vec![
        Line::from(format!("Current sort: {}", current_sort)).style(theme.primary()),
        Line::from(""),
    ];
    text_lines.extend(lines);

    let paragraph = Paragraph::new(Text::from(text_lines))
        .block(
            Block::default()
                .title(title)
                .title_bottom(bottom_title)
                .borders(Borders::ALL)
                .border_style(theme.border_focused()),
        )
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

fn format_with_filter(base_title: String, filter_query: &str) -> String {
    if filter_query.is_empty() {
        base_title
    } else {
        format!("{base_title} / {filter_query}")
    }
}

/// Extract emoji from folder icon JSON field
fn extract_folder_emoji(icon: &str) -> Option<String> {
    if icon.is_empty() {
        return None;
    }

    // Try to parse as JSON: {"emoji":"📝"}
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(icon) {
        if let Some(emoji) = json.get("emoji").and_then(|e| e.as_str()) {
            return Some(format!("{} ", emoji));
        }
    }

    // If JSON parsing fails, try to use the string directly if it looks like an emoji
    if icon.chars().count() <= 4
        && icon
            .chars()
            .all(|c| c.is_alphanumeric() || c == ':' || c == ' ')
    {
        return None; // Don't show non-emoji strings
    }

    None
}

#[allow(dead_code)]
fn strip_disambiguation_suffix(name: &str) -> Option<&str> {
    if let Some(pos) = name.rfind(" (") {
        let suffix = &name[pos..];
        if suffix.ends_with(')')
            && suffix[2..suffix.len() - 1]
                .chars()
                .all(|c| c.is_ascii_digit())
            && !suffix[2..suffix.len() - 1].is_empty()
        {
            return Some(&name[..pos]);
        }
    }
    None
}

/// Helper to create centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}

/// Helper to create centered rectangle with minimum width
fn centered_rect_with_min_width(percent_x: u16, percent_y: u16, min_width: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    // Calculate the width as percentage, but ensure minimum width
    let calculated_width = (r.width * percent_x) / 100;
    let actual_width = calculated_width.max(min_width);

    // If the calculated width is smaller than minimum, we need a different approach
    if actual_width > calculated_width {
        // Use fixed width centered
        let horizontal_padding = r.width.saturating_sub(actual_width) / 2;
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Length(horizontal_padding),
                    Constraint::Length(actual_width),
                    Constraint::Length(horizontal_padding),
                ]
                .as_ref(),
            )
            .split(popup_layout[1])[1]
    } else {
        // Use percentage-based layout
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints(
                [
                    Constraint::Percentage((100 - percent_x) / 2),
                    Constraint::Percentage(percent_x),
                    Constraint::Percentage((100 - percent_x) / 2),
                ]
                .as_ref(),
            )
            .split(popup_layout[1])[1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_centered_rect() {
        let size = Rect::new(0, 0, 100, 100);
        let centered = centered_rect(50, 50, size);

        assert_eq!(centered.x, 25);
        assert_eq!(centered.y, 25);
        assert_eq!(centered.width, 50);
        assert_eq!(centered.height, 50);
    }
}
