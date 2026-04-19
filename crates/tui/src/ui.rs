// UI rendering for NeoJoplin TUI

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Style, Stylize},
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
    let theme = &state.theme;

    let items: Vec<ListItem> = if state.folders.is_empty() {
        vec![
            ListItem::new("No notebooks yet").style(theme.dim()),
            ListItem::new("Press N to create one").style(theme.dim()),
        ]
    } else {
        state
            .folders
            .iter()
            .enumerate()
            .map(|(i, folder)| {
                let is_selected = state.selected_folder == Some(i);
                let style = if is_selected {
                    theme.selection()
                } else {
                    theme.text()
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
    let title = if let Some(folder) = state.selected_folder() {
        format!("Notes - {}", folder.title)
    } else {
        "Notes".to_string()
    };
    let theme = &state.theme;

    let items: Vec<ListItem> = if state.notes.is_empty() {
        if state.selected_folder().is_some() {
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

    let content = if let Some(note) = state.selected_note() {
        if note.body.is_empty() {
            Text::from(vec![
                Line::from("This note is empty").style(theme.dim()),
                Line::from(""),
                Line::from("Press Enter to edit this note").style(theme.primary()),
            ])
        } else {
            Text::from(note.body.clone())
        }
    } else {
        Text::from(vec![
            Line::from("No note selected").style(theme.dim()),
            Line::from(""),
            Line::from("Select a note to view its content").style(theme.dim()),
            Line::from(""),
            Line::from("Keybindings:").style(theme.primary()),
            Line::from("  Tab/Shift-Tab - Switch panels").style(theme.text()),
            Line::from("  hjkl/Arrows     - Move selection").style(theme.text()),
            Line::from("  Enter           - Edit selected note").style(theme.text()),
            Line::from("  n               - New note").style(theme.text()),
            Line::from("  N               - New notebook").style(theme.text()),
            Line::from("  d               - Delete selected").style(theme.text()),
        ])
    };

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .title(title)
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

/// Render keybinding ribbon (show available keybindings)
fn render_keybinding_ribbon(f: &mut Frame, state: &AppState, area: Rect) {
    let use_two_lines = area.height > 1;
    let theme = &state.theme;

    let key_style = theme.accent();

    let help_text = if use_two_lines {
        vec![
            Line::from(vec![
                Span::styled("q", key_style),
                Span::raw(":quit ").style(theme.muted()),
                Span::styled("?", key_style),
                Span::raw(":help ").style(theme.muted()),
                Span::styled("Tab", key_style),
                Span::raw(":panel ").style(theme.muted()),
                Span::styled("hjkl", key_style),
                Span::raw(":nav ").style(theme.muted()),
                Span::styled("Ent", key_style),
                Span::raw(":edit ").style(theme.muted()),
                Span::styled("n", key_style),
                Span::raw(":new ").style(theme.muted()),
            ]),
            Line::from(vec![
                Span::styled("N", key_style),
                Span::raw(":folder ").style(theme.muted()),
                Span::styled("d", key_style),
                Span::raw(":del ").style(theme.muted()),
                Span::styled("s", key_style),
                Span::raw(":sync ").style(theme.muted()),
                Span::styled("S", key_style),
                Span::raw(":settings ").style(theme.muted()),
            ]),
        ]
    } else {
        vec![Line::from(vec![
            Span::styled("q", key_style),
            Span::raw(":quit ").style(theme.muted()),
            Span::styled("?", key_style),
            Span::raw(":help ").style(theme.muted()),
            Span::styled("Tab", key_style),
            Span::raw(":panel ").style(theme.muted()),
            Span::styled("hjkl", key_style),
            Span::raw(":nav ").style(theme.muted()),
            Span::styled("Ent", key_style),
            Span::raw(":edit ").style(theme.muted()),
            Span::styled("n", key_style),
            Span::raw(":new ").style(theme.muted()),
            Span::styled("N", key_style),
            Span::raw(":nbk ").style(theme.muted()),
            Span::styled("d", key_style),
            Span::raw(":del ").style(theme.muted()),
            Span::styled("s", key_style),
            Span::raw(":sync ").style(theme.muted()),
            Span::styled("S", key_style),
            Span::raw(":set ").style(theme.muted()),
        ])]
    };

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
                .border_style(theme.border_focused())
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
            Span::styled("[", theme.muted()),
            Span::styled("<", theme.accent()),
            Span::styled("]", theme.muted()),
            Span::raw(" prev tab "),
            Span::styled("[", theme.muted()),
            Span::styled(">", theme.accent()),
            Span::styled("]", theme.muted()),
            Span::raw(" next tab "),
            Span::styled("[", theme.muted()),
            Span::styled("q", theme.accent()),
            Span::styled("]", theme.muted()),
            Span::raw(" close "),
        ]),
        Line::from(vec![
            Span::styled("[", theme.muted()),
            Span::styled("e", theme.accent()),
            Span::styled("]", theme.muted()),
            Span::raw(" enable encryption "),
            Span::styled("[", theme.muted()),
            Span::styled("d", theme.accent()),
            Span::styled("]", theme.muted()),
            Span::raw(" disable encryption "),
        ]),
    ]);

    let hint_paragraph = Paragraph::new(hint_text)
        .alignment(Alignment::Center)
        .block(Block::default().bg(theme.surface));

    f.render_widget(hint_paragraph, hint_area);
}

/// Render general settings (inline)
fn render_general_settings_inline(state: &AppState) -> Vec<Line<'_>> {
    let enc = &state.settings.encryption;
    let theme = &state.theme;

    let mut lines = vec![
        Line::from("End-to-End Encryption").style(theme.primary()),
        Line::from(""),
    ];

    // Status
    lines.push(Line::from(vec![
        Span::raw("Status: ").style(theme.text()),
        Span::styled(&enc.status_message, theme.primary()),
    ]));

    lines.push(Line::from(""));

    // Master key info
    if let Some(ref key_id) = enc.active_master_key_id {
        lines.push(Line::from(vec![
            Span::raw("Active Key: ").style(theme.text()),
            Span::styled(&key_id[..8], theme.accent()),
            Span::raw("...").style(theme.text()),
        ]));
    }

    lines.push(Line::from(format!("Available Keys: {}", enc.master_key_count)).style(theme.text()));
    lines.push(Line::from(""));

    // Actions
    if !enc.enabled {
        lines.push(Line::from(vec![
            Span::styled("[e]", theme.accent()),
            Span::raw(" Enable encryption with master password").style(theme.text()),
        ]));
    } else {
        lines.push(Line::from(vec![
            Span::styled("[d]", theme.accent()),
            Span::raw(" Disable encryption").style(theme.text()),
        ]));
    }

    // Password prompt
    if enc.show_new_key_prompt {
        lines.push(Line::from(""));
        lines.push(Line::from("─────────────────────────────────").style(theme.primary()));
        lines.push(Line::from("Setup Master Password").style(theme.primary()));
        lines.push(Line::from(""));

        if !enc.password_input.is_empty() || !enc.confirm_password_input.is_empty() {
            let masked_password = "•".repeat(enc.password_input.len());
            let masked_confirm = "•".repeat(enc.confirm_password_input.len());

            lines.push(Line::from(vec![
                Span::raw("Password:      ").style(theme.text()),
                Span::styled(masked_password, theme.primary()),
            ]));

            lines.push(Line::from(vec![
                Span::raw("Confirm:       ").style(theme.text()),
                Span::styled(masked_confirm, theme.primary()),
            ]));
        } else {
            lines.push(Line::from("Type password (min 8 characters)").style(theme.muted()));
        }

        // Error message
        if let Some(ref error) = enc.password_error {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("⚠ ", theme.warning()),
                Span::styled(error, theme.error()),
            ]));
        }

        // Success message
        if enc.password_success {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("✓ ", theme.success()),
                Span::styled("Encryption enabled successfully!", theme.success()),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("[", theme.muted()),
            Span::styled("Enter", theme.accent()),
            Span::styled("]", theme.muted()),
            Span::raw(" to confirm ").style(theme.text()),
            Span::styled("[", theme.muted()),
            Span::styled("Esc", theme.accent()),
            Span::styled("]", theme.muted()),
            Span::raw(" to cancel").style(theme.text()),
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

/// Render help popup
pub fn render_help(f: &mut Frame, scroll: u16) {
    let area = centered_rect(80, 80, f.area());

    let text = Text::from(vec![
        Line::from("NEOJOPLIN").bold(),
        Line::from(""),
        Line::from("Joplin-compatible terminal note-taking client").dim(),
        Line::from(""),
        Line::from("Keybindings").bold(),
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
        Line::from("  N      New notebook"),
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
        Line::from("Quit NeoJoplin?").bold(),
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
    let area = centered_rect(40, 15, f.area());
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

    // Input field with visual highlighting
    let input_text = format!("New name: {}", state.rename_input);
    let input_paragraph = Paragraph::new(input_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_focused())
                .style(theme.primary()),
        )
        .padding(Padding::horizontal(1));

    // Main dialog content
    let text = Text::from(vec![
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

    // Render input field inside the dialog
    let input_area = centered_rect(38, 13, f.area());
    f.render_widget(input_paragraph, input_area);
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
