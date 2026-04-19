// UI rendering for NeoJoplin TUI

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::settings::{FormField, ConnectionResult};
use crate::theme::Theme;

use crate::state::{AppState, FocusPanel};
use crate::settings::SettingsTab;

/// Render the main UI
pub fn render_ui(f: &mut Frame, state: &AppState) {
    // Calculate heights for keybinding ribbon and status line
    let ribbon_height = if f.area().width < 100 { 2 } else { 1 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints(
            [
                Constraint::Min(0),  // Main content
                Constraint::Length(ribbon_height),  // Keybinding ribbon
                Constraint::Length(1),  // Status line
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
                Constraint::Percentage(25),  // Notebooks
                Constraint::Percentage(25),  // Notes
                Constraint::Percentage(50),  // Content
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
    let title = "Notebooks";
    let theme = &state.theme;

    let items: Vec<ListItem> = if state.folders.is_empty() {
        vec![
            ListItem::new("No notebooks yet").style(theme.dim()),
            ListItem::new("Press N to create one").style(theme.dim()),
        ]
    } else {
        let mut all_items = vec![];

        // Add "All Notebooks" option at the top
        let is_all_selected = state.all_notebooks_mode;
        let all_style = if is_all_selected {
            theme.selection()
        } else {
            theme.text()
        };
        all_items.push(ListItem::new("📚 All Notebooks").style(all_style));

        // Add individual notebooks
        for (i, folder) in state.folders.iter().enumerate() {
            let is_selected = state.selected_folder == Some(i) && !state.all_notebooks_mode;
            let style = if is_selected {
                theme.selection()
            } else {
                theme.text()
            };

            // Extract emoji from folder icon, or use default
            let emoji = extract_folder_emoji(&folder.icon).unwrap_or_else(|| "📁 ".to_string());

            all_items.push(ListItem::new(format!("{}{}", emoji, folder.title)).style(style));
        }

        all_items
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
    let title = if state.all_notebooks_mode {
        "Notes - All Notebooks".to_string()
    } else if let Some(folder) = state.selected_folder() {
        format!("Notes - {}", folder.title)
    } else {
        "Notes".to_string()
    };
    let theme = &state.theme;

    let items: Vec<ListItem> = if state.notes.is_empty() {
        if state.all_notebooks_mode || state.selected_folder().is_some() {
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
        state
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
                    if note.todo_completed > 0 { "󰄲" } else { "󰄱" }
                } else {
                    "📝"
                };

                ListItem::new(format!("{} {}", icon, note.title)).style(style)
            })
            .collect()
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
        note.title.clone()
    } else {
        "Content".to_string()
    };
    let theme = &state.theme;

    // Get note content as markdown
    let markdown_text = if let Some(note) = state.selected_note() {
        if note.body.is_empty() {
            "*This note is empty*".to_string()
        } else {
            note.body.clone()
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
    let visible_lines: Vec<Line> = content_lines.iter()
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

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .title(title)
                .title_bottom(Line::from(scroll_indicator).style(theme.muted()))
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
                let spans: Vec<Span<'static>> = composite.composite.compounds.iter()
                    .map(|c| {
                        let mut style = Style::default();
                        if c.bold { style = style.add_modifier(Modifier::BOLD); }
                        if c.italic { style = style.add_modifier(Modifier::ITALIC); }
                        if c.strikeout { style = style.add_modifier(Modifier::CROSSED_OUT); }
                        if c.code { style = style.fg(Color::Cyan); }
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
                        spans.into_iter()
                            .map(|s| Span::styled(s.content.into_owned(),
                                s.style.fg(color).add_modifier(Modifier::BOLD)))
                            .collect()
                    }
                    CompositeStyle::Quote => {
                        let mut result = vec![Span::styled("▌ ".to_string(), Style::default().fg(Color::DarkGray))];
                        result.extend(spans.into_iter()
                            .map(|s| Span::styled(s.content.into_owned(),
                                s.style.fg(Color::Gray))));
                        result
                    }
                    CompositeStyle::ListItem(..) => {
                        let mut result = vec![Span::styled("  • ".to_string(), Style::default().fg(Color::Yellow))];
                        result.extend(spans.into_iter()
                            .map(|s| Span::styled(s.content.into_owned(), s.style)));
                        result
                    }
                    _ => spans.into_iter()
                        .map(|s| Span::styled(s.content.into_owned(), s.style))
                        .collect(),
                };
                lines.push(Line::from(line_spans));
            }
            termimad::FmtLine::HorizontalRule => {
                let rule = "─".repeat(width.min(80));
                lines.push(Line::from(Span::styled(rule, Style::default().fg(Color::DarkGray))));
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

    // Define keybindings: (key, action)
    // All actions use the same primary color
    let bindings = &[
        ("q", "QUIT"),
        ("?", "HELP"),
        ("Tab", "PANEL"),
        ("hjkl", "NAV"),
        ("Ent", "EDIT"),
        ("n", "NOTE"),
        ("T", "TODO"),
        ("t", "TOGGLE"),
        ("N", "NOTEBOOK"),
        ("d", "DELETE"),
        ("s", "SYNC"),
        ("S", "SETTINGS"),
    ];

    let mut spans = vec![];
    let mut total_width = 0;
    let available_width = area.width as usize;

    for (key, action) in bindings.iter() {
        // Calculate segment width
        let key_width = key.chars().count();
        let action_width = action.chars().count();
        let arrow_width = arrow.chars().count();

        // Pattern: "KEY arrow ACTION arrow space" where arrows create the colored box effect
        let segment_width = key_width + arrow_width + 1 + action_width + 1 + arrow_width + 1;

        if total_width + segment_width > available_width {
            break; // Stop if we're out of space
        }

        // Use single primary color for all actions (not alternating)
        let action_bg = theme.primary;

        // Get the actual colors from the theme
        let surface_color = theme.surface; // Background color
        let key_color = theme.text; // Normal text color for keys (not grey!)
        let action_fg_color = Color::Black; // Dark text on colored background

        // Key in normal text (no trailing space, space comes after arrow)
        spans.push(Span::styled(
            *key,
            Style::default().fg(key_color).bg(surface_color),
        ));
        total_width += key_width;

        // Left arrow: NOT inverted - surface on action_bg (points TO action)
        spans.push(Span::styled(
            arrow,
            Style::default().fg(surface_color).bg(action_bg),
        ));
        total_width += arrow_width;

        // Action text: black on action background (inverted)
        spans.push(Span::styled(
            format!(" {} ", *action),
            Style::default().fg(action_fg_color).bg(action_bg).bold(),
        ));
        total_width += 1 + action_width + 1;

        // Right arrow: INVERTED - action_bg on surface (points AWAY from action)
        spans.push(Span::styled(
            arrow,
            Style::default().fg(action_bg).bg(surface_color),
        ));
        total_width += arrow_width;

        // Space after arrow before next key
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

    let status_text = if state.status_message.is_empty() {
        Line::from(vec![
            Span::from("Ready").style(theme.muted()),
        ])
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

    let tabs = ["Sync", "Encryption", "About"];
    let current_tab_idx = match state.settings.current_tab {
        SettingsTab::Sync => 0,
        SettingsTab::Encryption => 1,
        SettingsTab::About => 2,
    };

    // Create title with tabs
    let title = format!(
        "Settings - {}",
        tabs[current_tab_idx]
    );

    // Create bottom title with key hints
    let bottom_title = Line::from(vec![
        Span::styled("[", theme.muted()),
        Span::styled("<", theme.accent()),
        Span::styled("]", theme.muted()),
        Span::raw(" prev ").style(theme.text()),
        Span::styled("[", theme.muted()),
        Span::styled(">", theme.accent()),
        Span::styled("]", theme.muted()),
        Span::raw(" next ").style(theme.text()),
        Span::styled("[", theme.muted()),
        Span::styled("q", theme.accent()),
        Span::styled("]", theme.muted()),
        Span::raw(" close ").style(theme.text()),
    ]);

    // Render based on current tab
    let content = match state.settings.current_tab {
        SettingsTab::Sync => {
            // Sync tab gets special rendering with forms
            render_sync_settings_content(f, state, area);
            return;
        }
        SettingsTab::Encryption => Text::from(render_encryption_settings_inline(state)),
        SettingsTab::About => Text::from(render_about_settings_inline()),
    };

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .title(title)
                .title_bottom(bottom_title)
                .borders(Borders::ALL)
                .border_style(theme.border_focused())
        )
        .wrap(Wrap { trim: false })
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

/// Render encryption settings (inline)
fn render_encryption_settings_inline(state: &AppState) -> Vec<Line<'_>> {
    let enc = &state.settings.encryption;

    let mut lines = vec![
        Line::from("Encryption Settings").style(Style::default().bold()),
        Line::from(""),
    ];

    // Status
    lines.push(Line::from(vec![
        Span::raw("Status: "),
        Span::styled(&enc.status_message, Style::default().bold()),
    ]));

    lines.push(Line::from(""));

    // Master key info
    if let Some(ref key_id) = enc.active_master_key_id {
        lines.push(Line::from(vec![
            Span::raw("Active Key: "),
            Span::styled(&key_id[..8], Style::default().bold()),
            Span::raw("..."),
        ]));
    }

    lines.push(Line::from(format!("Available Keys: {}", enc.master_key_count)));
    lines.push(Line::from(""));

    // Actions
    if !enc.enabled {
        lines.push(Line::from(vec![
            Span::styled("[e]", Style::default().bold()),
            Span::raw(" Enable encryption with master password"),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("[d]", Style::default().bold()),
            Span::raw(" Disable encryption"),
        ]));
    }

    // Password prompt
    if enc.show_new_key_prompt {
        lines.push(Line::from(""));
        lines.push(Line::from("─────────────────────────────────").style(Style::default().bold()));
        lines.push(Line::from("Setup Master Password").style(Style::default().bold()));
        lines.push(Line::from(""));

        if !enc.password_input.is_empty() || !enc.confirm_password_input.is_empty() {
            let masked_password = "•".repeat(enc.password_input.len());
            let masked_confirm = "•".repeat(enc.confirm_password_input.len());

            lines.push(Line::from(vec![
                Span::raw("Password:      "),
                Span::styled(masked_password, Style::default().bold()),
            ]));

            lines.push(Line::from(vec![
                Span::raw("Confirm:       "),
                Span::styled(masked_confirm, Style::default().bold()),
            ]));
        } else {
            lines.push(Line::from("Type password (min 8 characters)"));
        }

        // Error message
        if let Some(ref error) = enc.password_error {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("⚠ ", Style::default().bold()),
                Span::styled(error, Style::default().bold()),
            ]));
        }

        // Success message
        if enc.password_success {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("✓ ", Style::default().bold()),
                Span::styled("Encryption enabled successfully!", Style::default().bold()),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("[", Style::default().dim()),
            Span::styled("Enter", Style::default().bold()),
            Span::styled("]", Style::default().dim()),
            Span::raw(" to confirm "),
            Span::styled("[", Style::default().dim()),
            Span::styled("Esc", Style::default().bold()),
            Span::styled("]", Style::default().dim()),
            Span::raw(" to cancel"),
        ]));
    }

    lines
}

/// Render about settings (inline)
fn render_about_settings_inline() -> Vec<Line<'static>> {
    vec![
        Line::from("About NeoJoplin").bold(),
        Line::from(""),
        Line::from("Version: 0.1.0-alpha"),
        Line::from(""),
        Line::from("A fast, memory-safe Joplin-compatible").bold(),
        Line::from("terminal note-taking client in Rust."),
        Line::from(""),
        Line::from("Features:"),
        Line::from("  • 100% Joplin sync compatibility"),
        Line::from("  • End-to-end encryption (E2EE)"),
        Line::from("  • WebDAV synchronization"),
        Line::from("  • External editor integration"),
        Line::from("  • Terminal UI (TUI)"),
        Line::from(""),
        Line::from("Repository:"),
        Line::from("  https://github.com/Dronakurl/neojoplin"),
        Line::from(""),
        Line::from("Based on Joplin by Laurent Cozic"),
        Line::from(""),
        Line::from("Press 'q' to close settings"),
    ]
}

/// Render sync settings content (special handling for forms)
fn render_sync_settings_content(f: &mut Frame, state: &AppState, area: Rect) {
    let theme = &state.theme;

    // Create a block for sync settings
    let block = Block::default()
        .title(" Sync Settings ")
        .borders(Borders::ALL)
        .border_style(theme.border_focused());

    // Get inner area for content
    let content_area = block.inner(area);

    f.render_widget(block, area);

    // Split into target list (left) and form/details (right)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)].as_ref())
        .margin(1)
        .split(content_area);

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
            ListItem::new(Line::from(vec![
                Span::styled("No sync targets configured", theme.muted()),
            ])),
            ListItem::new(Line::from(vec![
                Span::raw("Press "),
                Span::styled("'n'", theme.accent()),
                Span::raw(" to add one"),
            ])),
        ]
    } else {
        sync.targets.iter().enumerate().map(|(i, target)| {
            let is_active = sync.current_target_index == Some(i);
            let prefix = if is_active { "● " } else { "○ " };
            let style = if is_active { theme.primary() } else { theme.text() };

            ListItem::new(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(&target.name, style),
            ]))
        }).collect()
    };

    let list = List::new(items)
        .block(Block::default()
            .title(" Sync Targets ")
            .borders(Borders::ALL)
            .border_style(theme.border_normal()))
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
            .block(Block::default()
                .title(" Instructions ")
                .borders(Borders::ALL)
                .border_style(theme.border_normal()))
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
                    Span::styled(if target.username.is_empty() { "(none)" } else { &target.username }, theme.text()),
                ]),
                Line::from(vec![
                    Span::styled("Password: ", theme.muted()),
                    Span::styled(if target.password.is_empty() { "(not set)" } else { "•••• (set)" }, theme.text()),
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
                .block(Block::default()
                    .title(" Selected Target ")
                    .borders(Borders::ALL)
                    .border_style(theme.border_normal()))
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

    let _title = if sync.show_edit_form { "Edit Target" } else { "Add Target" };

    // Form layout with input fields
    let form_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Name
            Constraint::Length(3), // URL
            Constraint::Length(3), // Username
            Constraint::Length(3), // Password
            Constraint::Length(3), // Path
            Constraint::Length(2), // Error message
            Constraint::Length(3), // Buttons
        ].as_ref())
        .split(area);

    // Highlight active field
    let is_name_active = sync.active_field == Some(FormField::Name);
    let is_url_active = sync.active_field == Some(FormField::Url);
    let is_username_active = sync.active_field == Some(FormField::Username);
    let is_password_active = sync.active_field == Some(FormField::Password);
    let is_path_active = sync.active_field == Some(FormField::Path);

    // Render each form field
    render_form_field(f, "Name:", &sync.name_input, form_chunks[0], theme, is_name_active);
    render_form_field(f, "URL:", &sync.url_input, form_chunks[1], theme, is_url_active);
    render_form_field(f, "Username:", &sync.username_input, form_chunks[2], theme, is_username_active);
    render_form_field_with_placeholder(f, "Password:", &sync.password_input, form_chunks[3], theme, is_password_active);
    render_form_field(f, "Path:", &sync.path_input, form_chunks[4], theme, is_path_active);

    // Error message
    if let Some(ref error) = sync.form_error {
        let error_text = Paragraph::new(error.clone())
            .style(theme.error())
            .alignment(Alignment::Center);
        f.render_widget(error_text, form_chunks[5]);
    } else if let Some(ref result) = sync.connection_result {
        match result {
            ConnectionResult::Success => {
                let success_text = Paragraph::new("✓ Connection successful!")
                    .style(theme.success())
                    .alignment(Alignment::Center);
                f.render_widget(success_text, form_chunks[5]);
            }
            ConnectionResult::Failed(err) => {
                let error_text = Paragraph::new(format!("✗ Connection failed: {}", err))
                    .style(theme.error())
                    .alignment(Alignment::Center);
                f.render_widget(error_text, form_chunks[5]);
            }
        }
    }

    // Buttons
    let buttons = Line::from(vec![
        Span::styled("[Enter]", theme.accent()),
        Span::raw(" Save "),
        Span::styled("[Esc]", theme.muted()),
        Span::raw(" Cancel "),
        Span::styled("[Tab]", theme.accent()),
        Span::raw(" Next field "),
        Span::styled("[Ctrl+T]", theme.accent()),
        Span::raw(" Test "),
    ]);

    let button_paragraph = Paragraph::new(buttons)
        .alignment(Alignment::Center);
    f.render_widget(button_paragraph, form_chunks[6]);
}

/// Helper function to render a form field
fn render_form_field(f: &mut Frame, label: &str, value: &str, area: Rect, theme: &Theme, is_active: bool) {
    let cursor = if is_active { "█" } else { "" };
    let style = if is_active { theme.primary() } else { theme.text() };

    let text = Line::from(vec![
        Span::styled(label, theme.muted()),
        Span::styled(value, style),
        Span::styled(cursor, theme.primary()),
    ]);

    let border_style = if is_active { theme.border_focused() } else { theme.border_normal() };

    let paragraph = Paragraph::new(text)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(border_style));
    f.render_widget(paragraph, area);
}

/// Helper function to render a password field (masked with placeholder)
fn render_form_field_with_placeholder(f: &mut Frame, label: &str, value: &str, area: Rect, theme: &Theme, is_active: bool) {
    let display_value = if value.is_empty() {
        "(not set)".to_string()
    } else if is_active {
        "•".repeat(value.len())
    } else {
        "•••• (set)".to_string()
    };

    let cursor = if is_active { "█" } else { "" };
    let style = if is_active { theme.primary() } else { theme.text() };

    let text = Line::from(vec![
        Span::styled(label, theme.muted()),
        Span::styled(&display_value, style),
        Span::styled(cursor, theme.primary()),
    ]);

    let border_style = if is_active { theme.border_focused() } else { theme.border_normal() };

    let paragraph = Paragraph::new(text)
        .block(Block::default()
            .borders(Borders::ALL)
            .border_style(border_style));
    f.render_widget(paragraph, area);
}

/// Render help popup
pub fn render_help(f: &mut Frame, scroll: u16, state: &AppState) {
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
    ]);

    let text = Text::from(vec![
        Line::from("NEOJOPLIN").style(theme.primary()),
        Line::from(""),
        Line::from("Joplin-compatible terminal note-taking client").style(theme.muted()),
        Line::from(""),
        Line::from("Navigation").style(theme.primary()),
        Line::from("  Tab / Shift-Tab    Switch panels (Notebooks → Notes → Content)"),
        Line::from("  h / l              Switch panels left / right"),
        Line::from("  j / k / ↑↓        Move selection or scroll content"),
        Line::from("  Enter              Open notebook (in Notebooks panel)"),
        Line::from(""),
        Line::from("Notes & Notebooks").style(theme.primary()),
        Line::from("  n      New note in current notebook"),
        Line::from("  N      New notebook"),
        Line::from("  r      Rename selected note or notebook"),
        Line::from("  d      Delete selected note or notebook"),
        Line::from(""),
        Line::from("Todos").style(theme.primary()),
        Line::from("  T        Create new todo"),
        Line::from("  Space    Toggle todo completed / unchecked"),
        Line::from(""),
        Line::from("Other").style(theme.primary()),
        Line::from("  e / Enter  Edit selected note in $EDITOR"),
        Line::from("  s          Sync with WebDAV"),
        Line::from("  S          Open settings"),
        Line::from("  q          Quit"),
    ]);

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
        state.selected_note().map(|n| n.title.as_str()).unwrap_or("note")
    } else {
        state.selected_folder().map(|f| f.title.as_str()).unwrap_or("notebook")
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
        Span::styled("New name: ", theme.muted()),
        Span::styled(&state.rename_input, theme.primary()),
        Span::styled("█", theme.muted()), // Cursor indicator
    ];

    // Main dialog content with centered input
    let text = Text::from(vec![
        Line::from(""),
        Line::from(input_text),
        Line::from(""),
    ]);

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
    if icon.chars().count() <= 4 && icon.chars().all(|c| c.is_alphanumeric() || c == ':' || c == ' ') {
        return None; // Don't show non-emoji strings
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
