// UI rendering for NeoJoplin TUI

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

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

    let items: Vec<ListItem> = if state.folders.is_empty() {
        vec![
            ListItem::new("No folders yet").style(Style::default().dim()),
            ListItem::new("Press N to create one").style(Style::default().dim()),
        ]
    } else {
        state
            .folders
            .iter()
            .enumerate()
            .map(|(i, folder)| {
                let is_selected = state.selected_folder == Some(i);
                let style = if is_selected {
                    Style::default().bold()
                } else {
                    Style::default()
                };

                // Extract emoji from folder icon, or use default
                let emoji = extract_folder_emoji(&folder.icon).unwrap_or_else(|| "📁 ".to_string());

                ListItem::new(format!("{}{}", emoji, folder.title)).style(style)
            })
            .collect()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(if state.focus == FocusPanel::Notebooks {
                    Style::default().bold()
                } else {
                    Style::default()
                }),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED).bold());

    f.render_widget(list, area);
}

/// Render notes panel
fn render_notes_panel(f: &mut Frame, state: &AppState, area: Rect) {
    let title = if let Some(folder) = state.selected_folder() {
        format!("Notes - {}", folder.title)
    } else {
        "Notes".to_string()
    };

    let items: Vec<ListItem> = if state.notes.is_empty() {
        if state.selected_folder().is_some() {
            vec![
                ListItem::new("No notes in this folder").style(Style::default().dim()),
                ListItem::new("Press n to create one").style(Style::default().dim()),
            ]
        } else {
            vec![
                ListItem::new("No folder selected").style(Style::default().dim()),
                ListItem::new("Select a folder first").style(Style::default().dim()),
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
                    Style::default().bold()
                } else {
                    Style::default()
                };

                ListItem::new(format!("📝 {}", note.title)).style(style)
            })
            .collect()
    };

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(if state.focus == FocusPanel::Notes {
                    Style::default().bold()
                } else {
                    Style::default()
                }),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED).bold());

    f.render_widget(list, area);
}

/// Render note content panel
fn render_content_panel(f: &mut Frame, state: &AppState, area: Rect) {
    let title = if let Some(note) = state.selected_note() {
        note.title.clone()
    } else {
        "Content".to_string()
    };

    let content = if let Some(note) = state.selected_note() {
        if note.body.is_empty() {
            Text::from(vec![
                Line::from("This note is empty").style(Style::default().dim()),
                Line::from(""),
                Line::from("Press Enter to edit this note").style(Style::default().bold()),
            ])
        } else {
            Text::from(note.body.clone())
        }
    } else {
        Text::from(vec![
            Line::from("No note selected").style(Style::default().dim()),
            Line::from(""),
            Line::from("Select a note to view its content").style(Style::default().dim()),
            Line::from(""),
            Line::from("Keybindings:").style(Style::default().bold()),
            Line::from("  Tab/Shift-Tab - Switch panels"),
            Line::from("  hjkl/Arrows     - Move selection"),
            Line::from("  Enter           - Edit selected note"),
            Line::from("  n               - New note"),
            Line::from("  N               - New folder"),
            Line::from("  d               - Delete selected"),
        ])
    };

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(if state.focus == FocusPanel::Content {
                    Style::default().bold()
                } else {
                    Style::default()
                }),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

/// Render keybinding ribbon (show available keybindings)
fn render_keybinding_ribbon(f: &mut Frame, _state: &AppState, area: Rect) {
    let use_two_lines = area.height > 1;

    let key_style = Style::default().bold();

    let help_text = if use_two_lines {
        vec![
            Line::from(vec![
                Span::styled("q", key_style),
                Span::raw(":quit "),
                Span::styled("?", key_style),
                Span::raw(":help "),
                Span::styled("Tab", key_style),
                Span::raw(":panel "),
                Span::styled("hjkl", key_style),
                Span::raw(":nav "),
                Span::styled("Ent", key_style),
                Span::raw(":edit "),
                Span::styled("n", key_style),
                Span::raw(":new "),
            ]),
            Line::from(vec![
                Span::styled("N", key_style),
                Span::raw(":folder "),
                Span::styled("d", key_style),
                Span::raw(":del "),
                Span::styled("s", key_style),
                Span::raw(":sync "),
                Span::styled("S", key_style),
                Span::raw(":settings "),
            ]),
        ]
    } else {
        vec![Line::from(vec![
            Span::styled("q", key_style),
            Span::raw(":quit "),
            Span::styled("?", key_style),
            Span::raw(":help "),
            Span::styled("Tab", key_style),
            Span::raw(":panel "),
            Span::styled("hjkl", key_style),
            Span::raw(":nav "),
            Span::styled("Ent", key_style),
            Span::raw(":edit "),
            Span::styled("n", key_style),
            Span::raw(":new "),
            Span::styled("N", key_style),
            Span::raw(":fldr "),
            Span::styled("d", key_style),
            Span::raw(":del "),
            Span::styled("s", key_style),
            Span::raw(":sync "),
            Span::styled("S", key_style),
            Span::raw(":set "),
        ])]
    };

    let paragraph = Paragraph::new(help_text)
        .alignment(Alignment::Left)
        .block(Block::default().style(Style::default().dim()));

    f.render_widget(paragraph, area);
}

/// Render status line (show current status message)
fn render_status_line(f: &mut Frame, state: &AppState, area: Rect) {
    let status_text = if state.status_message.is_empty() {
        Line::from(vec![
            Span::from("Ready").style(Style::default().dim()),
        ])
    } else {
        Line::from(vec![
            Span::from("→ ").style(Style::default().dim()),
            Span::styled(&state.status_message, Style::default().bold()),
        ])
    };

    let paragraph = Paragraph::new(status_text)
        .alignment(Alignment::Left)
        .block(Block::default().style(Style::default().dim()));

    f.render_widget(paragraph, area);
}

/// Render settings menu
pub fn render_settings(f: &mut Frame, state: &AppState) {
    let area = centered_rect(70, 80, f.area());

    let tabs = vec!["General", "Encryption", "About"];
    let current_tab_idx = match state.settings.current_tab {
        SettingsTab::General => 0,
        SettingsTab::Encryption => 1,
        SettingsTab::About => 2,
    };

    // Create title with tabs
    let title = format!(
        "Settings - {}",
        tabs[current_tab_idx]
    );

    // Render based on current tab
    let content = match state.settings.current_tab {
        SettingsTab::General => Text::from(render_general_settings_inline(state)),
        SettingsTab::Encryption => Text::from(render_encryption_settings_inline(state)),
        SettingsTab::About => Text::from(render_about_settings_inline()),
    };

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().bold())
        )
        .wrap(Wrap { trim: false })
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);

    // Render tab navigation hint at bottom
    let hint_area = Rect {
        x: area.x,
        y: area.bottom() - 3,
        width: area.width,
        height: 3,
    };

    let hint_text = Text::from(vec![
        Line::from(vec![
            Span::styled("[", Style::default().dim()),
            Span::styled("<", Style::default().bold()),
            Span::styled("]", Style::default().dim()),
            Span::raw(" prev tab "),
            Span::styled("[", Style::default().dim()),
            Span::styled(">", Style::default().bold()),
            Span::styled("]", Style::default().dim()),
            Span::raw(" next tab "),
            Span::styled("[", Style::default().dim()),
            Span::styled("q", Style::default().bold()),
            Span::styled("]", Style::default().dim()),
            Span::raw(" close "),
        ]),
        Line::from(vec![
            Span::styled("[", Style::default().dim()),
            Span::styled("e", Style::default().bold()),
            Span::styled("]", Style::default().dim()),
            Span::raw(" enable encryption "),
            Span::styled("[", Style::default().dim()),
            Span::styled("d", Style::default().bold()),
            Span::styled("]", Style::default().dim()),
            Span::raw(" disable encryption "),
        ]),
    ]);

    let hint_paragraph = Paragraph::new(hint_text)
        .alignment(Alignment::Center)
        .block(Block::default().style(Style::default().dim()));

    f.render_widget(hint_paragraph, hint_area);
}

/// Render general settings (inline)
fn render_general_settings_inline(state: &AppState) -> Vec<Line<'_>> {
    let enc = &state.settings.encryption;

    let mut lines = vec![
        Line::from("End-to-End Encryption").style(Style::default().bold()),
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
        Line::from("About NeoJoplin").style(Style::default().bold()),
        Line::from(""),
        Line::from("Version: 0.1.0-alpha"),
        Line::from(""),
        Line::from("A fast, memory-safe Joplin-compatible").style(Style::default().bold()),
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

/// Render help popup
pub fn render_help(f: &mut Frame, scroll: u16) {
    let area = centered_rect(80, 80, f.area());

    let text = Text::from(vec![
        Line::from("NEOJOPLIN").style(Style::default().bold()),
        Line::from(""),
        Line::from("Joplin-compatible terminal note-taking client").style(Style::default().dim()),
        Line::from(""),
        Line::from("Keybindings").style(Style::default().bold()),
        Line::from(""),
        Line::from("Navigation:"),
        Line::from("  Tab/Shift-Tab  Switch panels"),
        Line::from("  hjkl/Arrows    Move selection"),
        Line::from("  j/k (in help)  Scroll help"),
        Line::from(""),
        Line::from("Actions:"),
        Line::from("  q      Quit"),
        Line::from("  s      Sync with WebDAV"),
        Line::from("  S      Open settings"),
        Line::from("  ?      Show this help"),
        Line::from("  Enter  Edit selected note"),
        Line::from("  n      New note"),
        Line::from("  N      New folder"),
        Line::from("  d      Delete selected"),
    ]);

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title("Help (q: close, j/k: scroll)")
                .borders(Borders::ALL)
                .border_style(Style::default().bold()),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

/// Render quit confirmation popup
pub fn render_quit_confirmation(f: &mut Frame) {
    let area = centered_rect(50, 25, f.area());

    let text = Text::from(vec![
        Line::from("Quit NeoJoplin?").style(Style::default().bold()),
        Line::from(""),
        Line::from(""),
        Line::from(vec![
            Span::styled("[", Style::default().dim()),
            Span::styled("q", Style::default().bold()),
            Span::styled("]", Style::default().dim()),
            Span::raw(" or "),
            Span::styled("[", Style::default().dim()),
            Span::styled("y", Style::default().bold()),
            Span::styled("]", Style::default().dim()),
            Span::raw(" to quit "),
        ]),
        Line::from(vec![
            Span::styled("[", Style::default().dim()),
            Span::styled("any", Style::default().bold()),
            Span::styled("]", Style::default().dim()),
            Span::raw(" other key to cancel "),
        ]),
    ]);

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title("Confirm Quit")
                .borders(Borders::ALL)
                .border_style(Style::default().bold()),
        )
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Center);

    f.render_widget(paragraph, area);
}

/// Render rename prompt
pub fn render_rename_prompt(f: &mut Frame, state: &AppState) {
    let area = centered_rect(60, 20, f.area());

    let item_name = if state.focus == FocusPanel::Notes {
        state.selected_note().map(|n| n.title.as_str()).unwrap_or("note")
    } else {
        state.selected_folder().map(|f| f.title.as_str()).unwrap_or("folder")
    };

    let text = Text::from(vec![
        Line::from("Rename").style(Style::default().bold()),
        Line::from(format!("Renaming: {}", item_name)).style(Style::default().dim()),
        Line::from(""),
        Line::from(format!("New name: {}", state.rename_input)).style(Style::default().bold()),
        Line::from(""),
        Line::from(vec![
            Span::styled("[", Style::default().dim()),
            Span::styled("Enter", Style::default().bold()),
            Span::styled("]", Style::default().dim()),
            Span::raw(" to confirm "),
        ]),
        Line::from(vec![
            Span::styled("[", Style::default().dim()),
            Span::styled("Esc", Style::default().bold()),
            Span::styled("]", Style::default().dim()),
            Span::raw(" to cancel "),
        ]),
        Line::from(vec![
            Span::styled("[", Style::default().dim()),
            Span::styled("Backspace", Style::default().bold()),
            Span::styled("]", Style::default().dim()),
            Span::raw(" to delete character "),
        ]),
    ]);

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title("Rename Item")
                .borders(Borders::ALL)
                .border_style(Style::default().bold()),
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
