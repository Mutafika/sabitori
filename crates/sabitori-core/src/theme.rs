use crate::Color;

/// Framework-level theme with semantic color names.
/// Provides a simple, ergonomic set of colors for app-level theming.
/// For detailed terminal/TUI theming (ANSI palette, typography, shapes),
/// use `sabitori_style::Theme` instead.
#[derive(Clone, Debug)]
pub struct AppTheme {
    pub bg: Color,
    pub surface: Color,
    pub elevated: Color,
    pub border: Color,
    pub primary: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub hover_bg: Color,
    pub select_bg: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
}

impl AppTheme {
    /// Midnight -- deep blue dark theme (default).
    pub fn midnight() -> Self {
        Self {
            bg: Color::from_hex("#1a1a2e"),
            surface: Color::from_hex("#22223a"),
            elevated: Color::from_hex("#2a2a45"),
            border: Color::from_hex("#3a3a55"),
            primary: Color::from_hex("#6c63ff"),
            text_primary: Color::from_hex("#e8e8f0"),
            text_secondary: Color::from_hex("#9090a8"),
            hover_bg: Color::from_hex("#2a2a45"),
            select_bg: Color::from_hex("#333355"),
            success: Color::from_hex("#4ade80"),
            warning: Color::from_hex("#fbbf24"),
            error: Color::from_hex("#f87171"),
        }
    }

    /// Tokyo Night -- cool blue tones, popular dev theme.
    pub fn tokyo_night() -> Self {
        Self {
            bg: Color::from_hex("#1a1b26"),
            surface: Color::from_hex("#24283b"),
            elevated: Color::from_hex("#343a52"),
            border: Color::from_hex("#414868"),
            primary: Color::from_hex("#7aa2f7"),
            text_primary: Color::from_hex("#c0caf5"),
            text_secondary: Color::from_hex("#9aa5ce"),
            hover_bg: Color::from_hex("#343a52"),
            select_bg: Color::from_hex("#414868"),
            success: Color::from_hex("#9ece6a"),
            warning: Color::from_hex("#e0af68"),
            error: Color::from_hex("#f7768e"),
        }
    }

    /// Catppuccin Mocha -- warm pastels on dark base.
    pub fn catppuccin() -> Self {
        Self {
            bg: Color::from_hex("#1e1e2e"),
            surface: Color::from_hex("#302d41"),
            elevated: Color::from_hex("#45425a"),
            border: Color::from_hex("#575268"),
            primary: Color::from_hex("#cba6f7"),
            text_primary: Color::from_hex("#cdd6f4"),
            text_secondary: Color::from_hex("#a6adc8"),
            hover_bg: Color::from_hex("#45425a"),
            select_bg: Color::from_hex("#575268"),
            success: Color::from_hex("#a6e3a1"),
            warning: Color::from_hex("#f9e2af"),
            error: Color::from_hex("#f38ba8"),
        }
    }

    /// Nord -- arctic, cool minimal palette.
    pub fn nord() -> Self {
        Self {
            bg: Color::from_hex("#2e3440"),
            surface: Color::from_hex("#3b4252"),
            elevated: Color::from_hex("#434c5e"),
            border: Color::from_hex("#4c566a"),
            primary: Color::from_hex("#88c0d0"),
            text_primary: Color::from_hex("#eceff4"),
            text_secondary: Color::from_hex("#d8dee9"),
            hover_bg: Color::from_hex("#434c5e"),
            select_bg: Color::from_hex("#4c566a"),
            success: Color::from_hex("#a3be8c"),
            warning: Color::from_hex("#ebcb8b"),
            error: Color::from_hex("#bf616a"),
        }
    }

    /// Dracula -- iconic purple dark theme.
    pub fn dracula() -> Self {
        Self {
            bg: Color::from_hex("#282a36"),
            surface: Color::from_hex("#343746"),
            elevated: Color::from_hex("#44475a"),
            border: Color::from_hex("#6272a4"),
            primary: Color::from_hex("#bd93f9"),
            text_primary: Color::from_hex("#f8f8f2"),
            text_secondary: Color::from_hex("#bfbfbf"),
            hover_bg: Color::from_hex("#44475a"),
            select_bg: Color::from_hex("#555770"),
            success: Color::from_hex("#50fa7b"),
            warning: Color::from_hex("#f1fa8c"),
            error: Color::from_hex("#ff5555"),
        }
    }
}

impl Default for AppTheme {
    fn default() -> Self {
        Self::midnight()
    }
}
