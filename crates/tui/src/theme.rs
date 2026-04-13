// Theme system for NeoJoplin TUI
// Uses semantic colors that work on both dark and light terminal backgrounds

use ratatui::style::{Color, Style, Stylize};

/// Semantic theme colors for the TUI
///
/// Uses named semantic colors instead of raw color names to ensure
/// consistent visual hierarchy across different terminal backgrounds.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Background color (use Reset for terminal default)
    pub background: Color,

    /// Surface color for panels and containers
    pub surface: Color,

    /// Primary text color
    pub text: Color,

    /// Muted text color for secondary information
    pub text_muted: Color,

    /// Primary accent color for important elements
    pub primary: Color,

    /// Secondary accent color
    pub secondary: Color,

    /// Highlight color for emphasis
    pub accent: Color,

    /// Background color for selected items
    pub selection_bg: Color,

    /// Foreground color for selected items
    pub selection_fg: Color,

    /// Error color
    pub error: Color,

    /// Warning color
    pub warning: Color,

    /// Success color
    pub success: Color,

    /// Border color for focused panels
    pub border_focused: Color,

    /// Border color for unfocused panels
    pub border_normal: Color,
}

impl Theme {
    /// Get style for normal text
    pub fn text(&self) -> Style {
        Style::default().fg(self.text)
    }

    /// Get style for muted text
    pub fn muted(&self) -> Style {
        Style::default().fg(self.text_muted)
    }

    /// Get style for primary text
    pub fn primary(&self) -> Style {
        Style::default().fg(self.primary)
    }

    /// Get style for secondary text
    pub fn secondary(&self) -> Style {
        Style::default().fg(self.secondary)
    }

    /// Get style for accent text
    pub fn accent(&self) -> Style {
        Style::default().fg(self.accent)
    }

    /// Get style for selected items
    pub fn selection(&self) -> Style {
        Style::default()
            .fg(self.selection_fg)
            .bg(self.selection_bg)
            .bold()
    }

    /// Get style for error text
    pub fn error(&self) -> Style {
        Style::default().fg(self.error).bold()
    }

    /// Get style for warning text
    pub fn warning(&self) -> Style {
        Style::default().fg(self.warning).bold()
    }

    /// Get style for success text
    pub fn success(&self) -> Style {
        Style::default().fg(self.success).bold()
    }

    /// Get style for focused panel border
    pub fn border_focused(&self) -> Style {
        Style::default().fg(self.border_focused).bold()
    }

    /// Get style for normal panel border
    pub fn border_normal(&self) -> Style {
        Style::default().fg(self.border_normal)
    }

    /// Get style for dimmed text (helper text, placeholders)
    pub fn dim(&self) -> Style {
        Style::default().fg(self.text_muted).dim()
    }
}

/// Dark theme (default)
///
/// Optimized for dark terminal backgrounds. Most terminals default to dark,
/// so this is the default theme.
pub fn dark_theme() -> Theme {
    Theme {
        background: Color::Reset,
        surface: Color::Black,
        text: Color::White,
        text_muted: Color::DarkGray,
        primary: Color::Cyan,
        secondary: Color::Blue,
        accent: Color::Magenta,
        selection_bg: Color::Blue,
        selection_fg: Color::White,
        error: Color::Red,
        warning: Color::Yellow,
        success: Color::Green,
        border_focused: Color::Cyan,
        border_normal: Color::DarkGray,
    }
}

/// Light theme
///
/// Optimized for light terminal backgrounds. Users can switch to this
/// via configuration if their terminal uses a light background.
pub fn light_theme() -> Theme {
    Theme {
        background: Color::Reset,
        surface: Color::White,
        text: Color::Black,
        text_muted: Color::Gray,
        primary: Color::Blue,
        secondary: Color::Cyan,
        accent: Color::Magenta,
        selection_bg: Color::LightBlue,
        selection_fg: Color::Black,
        error: Color::Red,
        warning: Color::Yellow,
        success: Color::Green,
        border_focused: Color::Blue,
        border_normal: Color::Gray,
    }
}

/// Get the default theme
///
/// Currently returns the dark theme. In the future, this could detect
/// terminal background or read from user configuration.
pub fn default_theme() -> Theme {
    dark_theme()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dark_theme_colors() {
        let theme = dark_theme();

        // Verify semantic colors are set
        assert_eq!(theme.primary, Color::Cyan);
        assert_eq!(theme.selection_bg, Color::Blue);
        assert_eq!(theme.error, Color::Red);
    }

    #[test]
    fn test_light_theme_colors() {
        let theme = light_theme();

        // Light theme should use different colors
        assert_eq!(theme.primary, Color::Blue);
        assert_eq!(theme.selection_bg, Color::LightBlue);
        assert_eq!(theme.text, Color::Black);
    }

    #[test]
    fn test_theme_style_methods() {
        let theme = dark_theme();

        // Test style methods
        let text_style = theme.text();
        assert_eq!(text_style.fg, Some(Color::White));

        let selection_style = theme.selection();
        assert_eq!(selection_style.fg, Some(Color::White));
        assert_eq!(selection_style.bg, Some(Color::Blue));
    }
}
