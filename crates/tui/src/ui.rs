// UI rendering for NeoJoplin TUI

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::state::{AppState, FocusPanel};

/// Render the main UI
pub fn render_ui(f: &mut Frame, state: &AppState) {
    // Use 2 lines for status bar on narrow terminals, 1 line on wide terminals
    let status_bar_height = if f.area().width < 100 { 2 } else { 1 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints(
            [
                Constraint::Min(0),  // Main content
                Constraint::Length(status_bar_height),  // Status bar
            ]
            .as_ref(),
        )
        .split(f.area());

    // Render main content
    render_main_content(f, state, chunks[0]);

    // Render status bar
    render_status_bar(f, state, chunks[1]);
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

    let items: Vec<ListItem> = state
        .folders
        .iter()
        .enumerate()
        .map(|(i, folder)| {
            let is_selected = state.selected_folder == Some(i);
            let style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(format!("📁 {}", folder.title)).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(if state.focus == FocusPanel::Notebooks {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                }),
        )
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::REVERSED)
                .fg(Color::Yellow),
        );

    f.render_widget(list, area);
}

/// Render notes panel
fn render_notes_panel(f: &mut Frame, state: &AppState, area: Rect) {
    let title = if let Some(folder) = state.selected_folder() {
        format!("Notes - {}", folder.title)
    } else {
        "Notes".to_string()
    };

    let items: Vec<ListItem> = state
        .notes
        .iter()
        .enumerate()
        .map(|(i, note)| {
            let is_selected = state.selected_note == Some(i);
            let style = if is_selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(format!("📝 {}", note.title)).style(style)
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(if state.focus == FocusPanel::Notes {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                }),
        )
        .highlight_style(
            Style::default()
                .add_modifier(Modifier::REVERSED)
                .fg(Color::Yellow),
        );

    f.render_widget(list, area);
}

/// Render note content panel
fn render_content_panel(f: &mut Frame, state: &AppState, area: Rect) {
    let title = if let Some(note) = state.selected_note() {
        note.title.clone()
    } else {
        "No note selected".to_string()
    };

    let content = if let Some(note) = state.selected_note() {
        note.body.clone()
    } else {
        "Select a note to view its content".to_string()
    };

    let paragraph = Paragraph::new(content)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(if state.focus == FocusPanel::Content {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                }),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

/// Render status bar with keybinding help
fn render_status_bar(f: &mut Frame, state: &AppState, area: Rect) {
    let use_two_lines = area.height > 1;

    let help_text = if use_two_lines {
        vec![
            Line::from(vec![
                Span::styled("q", Style::default().fg(Color::Yellow)),
                Span::raw(":quit "),
                Span::styled("Tab", Style::default().fg(Color::Yellow)),
                Span::raw(":panel "),
                Span::styled("hjkl", Style::default().fg(Color::Yellow)),
                Span::raw(":nav "),
                Span::styled("Ent", Style::default().fg(Color::Yellow)),
                Span::raw(":edit "),
                Span::styled("n", Style::default().fg(Color::Yellow)),
                Span::raw(":new "),
            ]),
            Line::from(vec![
                Span::styled("N", Style::default().fg(Color::Yellow)),
                Span::raw(":folder "),
                Span::styled("d", Style::default().fg(Color::Yellow)),
                Span::raw(":del "),
                Span::styled("s", Style::default().fg(Color::Yellow)),
                Span::raw(":sync "),
                Span::styled("?", Style::default().fg(Color::Yellow)),
                Span::raw(":help "),
                Span::styled(&state.status_message, Style::default().fg(Color::Cyan)),
            ]),
        ]
    } else {
        vec![Line::from(vec![
            Span::styled("q", Style::default().fg(Color::Yellow)),
            Span::raw(":quit "),
            Span::styled("Tab", Style::default().fg(Color::Yellow)),
            Span::raw(":panel "),
            Span::styled("hjkl", Style::default().fg(Color::Yellow)),
            Span::raw(":nav "),
            Span::styled("Ent", Style::default().fg(Color::Yellow)),
            Span::raw(":edit "),
            Span::styled("n", Style::default().fg(Color::Yellow)),
            Span::raw(":new "),
            Span::styled("N", Style::default().fg(Color::Yellow)),
            Span::raw(":fldr "),
            Span::styled("d", Style::default().fg(Color::Yellow)),
            Span::raw(":del "),
            Span::styled("s", Style::default().fg(Color::Yellow)),
            Span::raw(":sync "),
            Span::styled("?", Style::default().fg(Color::Yellow)),
            Span::raw(":help "),
            Span::styled(&state.status_message, Style::default().fg(Color::Cyan)),
        ])]
    };

    let paragraph = Paragraph::new(help_text)
        .alignment(Alignment::Left)
        .block(Block::default().bg(Color::DarkGray));

    f.render_widget(paragraph, area);
}

/// Render help popup
pub fn render_help(f: &mut Frame, scroll: u16) {
    let area = centered_rect(80, 80, f.area());

    let text = Text::from(vec![
        Line::from("NEOJOPLIN").style(Style::default().fg(Color::Cyan).bold()),
        Line::from(""),
        Line::from("Joplin-compatible terminal note-taking client").style(Style::default().fg(Color::Gray)),
        Line::from(""),
        Line::from("Keybindings").style(Style::default().fg(Color::Yellow).bold()),
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
                .border_style(Style::default().fg(Color::Yellow)),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0))
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

/// Render quit confirmation popup
pub fn render_quit_confirmation(f: &mut Frame) {
    let area = centered_rect(40, 20, f.area());

    let text = Text::from(vec![
        Line::from("Quit NeoJoplin?").style(Style::default().fg(Color::Yellow)),
        Line::from(""),
        Line::from("Press q or y to confirm, any other key to cancel"),
    ]);

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .title("Confirm")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
        )
        .wrap(Wrap { trim: true })
        .alignment(Alignment::Center);

    f.render_widget(paragraph, area);
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
