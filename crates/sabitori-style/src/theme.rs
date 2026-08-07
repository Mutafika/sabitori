use sabitori_core::Color;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// ANSI 16-color palette for TUI-style theming.
#[derive(Clone, Debug)]
pub struct AnsiPalette {
    pub black: Color,
    pub red: Color,
    pub green: Color,
    pub yellow: Color,
    pub blue: Color,
    pub magenta: Color,
    pub cyan: Color,
    pub white: Color,
    pub bright_black: Color,
    pub bright_red: Color,
    pub bright_green: Color,
    pub bright_yellow: Color,
    pub bright_blue: Color,
    pub bright_magenta: Color,
    pub bright_cyan: Color,
    pub bright_white: Color,
}

/// ANSI palette colors for YAML deserialization.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AnsiColors {
    pub black: String,
    pub red: String,
    pub green: String,
    pub yellow: String,
    pub blue: String,
    pub magenta: String,
    pub cyan: String,
    pub white: String,
    pub bright_black: String,
    pub bright_red: String,
    pub bright_green: String,
    pub bright_yellow: String,
    pub bright_blue: String,
    pub bright_magenta: String,
    pub bright_cyan: String,
    pub bright_white: String,
}

impl AnsiPalette {
    fn from_colors(c: &AnsiColors) -> Self {
        Self {
            black: Color::from_hex(&c.black),
            red: Color::from_hex(&c.red),
            green: Color::from_hex(&c.green),
            yellow: Color::from_hex(&c.yellow),
            blue: Color::from_hex(&c.blue),
            magenta: Color::from_hex(&c.magenta),
            cyan: Color::from_hex(&c.cyan),
            white: Color::from_hex(&c.white),
            bright_black: Color::from_hex(&c.bright_black),
            bright_red: Color::from_hex(&c.bright_red),
            bright_green: Color::from_hex(&c.bright_green),
            bright_yellow: Color::from_hex(&c.bright_yellow),
            bright_blue: Color::from_hex(&c.bright_blue),
            bright_magenta: Color::from_hex(&c.bright_magenta),
            bright_cyan: Color::from_hex(&c.bright_cyan),
            bright_white: Color::from_hex(&c.bright_white),
        }
    }
}

/// Theme color tokens.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThemeColors {
    pub surface: String,
    pub surface_elevated: String,
    pub surface_hover: String,
    pub surface_active: String,
    pub primary: String,
    pub primary_hover: String,
    pub text_primary: String,
    pub text_secondary: String,
    pub text_disabled: String,
    pub border: String,
    pub shadow: String,
    pub success: String,
    pub warning: String,
    pub error: String,
}

/// Theme shadow preset.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShadowPreset {
    pub offset: [f32; 2],
    pub blur: f32,
    pub color: String,
}

/// Theme typography settings.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Typography {
    pub font_family: Vec<String>,
    pub body_size: f32,
    pub heading_size: f32,
}

/// Theme shape settings.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shape {
    pub corner_radius: f32,
    pub border_width: f32,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
}

fn default_opacity() -> f32 { 1.0 }

/// YAML theme definition.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThemeFile {
    pub name: String,
    pub base: String,
    pub colors: ThemeColors,
    #[serde(default)]
    pub ansi: Option<AnsiColors>,
    #[serde(default)]
    pub typography: Option<Typography>,
    #[serde(default)]
    pub shape: Option<Shape>,
    #[serde(default)]
    pub shadow: Option<std::collections::HashMap<String, ShadowPreset>>,
}

/// Resolved theme with Color values.
#[derive(Clone, Debug)]
pub struct Theme {
    pub name: String,
    pub surface: Color,
    pub surface_elevated: Color,
    pub surface_hover: Color,
    pub surface_active: Color,
    pub primary: Color,
    pub primary_hover: Color,
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_disabled: Color,
    pub border: Color,
    pub shadow: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub ansi: AnsiPalette,
    pub corner_radius: f32,
    pub border_width: f32,
    pub font_size: f32,
    /// Window/surface opacity (0.0 = fully transparent, 1.0 = opaque).
    pub opacity: f32,
}

/// Default ANSI palette (xterm-256 standard colors).
fn default_ansi() -> AnsiPalette {
    AnsiPalette {
        black:          Color::from_hex("#000000"),
        red:            Color::from_hex("#cd3131"),
        green:          Color::from_hex("#0dbc79"),
        yellow:         Color::from_hex("#e5e510"),
        blue:           Color::from_hex("#2472c8"),
        magenta:        Color::from_hex("#bc3fbc"),
        cyan:           Color::from_hex("#11a8cd"),
        white:          Color::from_hex("#e5e5e5"),
        bright_black:   Color::from_hex("#666666"),
        bright_red:     Color::from_hex("#f14c4c"),
        bright_green:   Color::from_hex("#23d18b"),
        bright_yellow:  Color::from_hex("#f5f543"),
        bright_blue:    Color::from_hex("#3b8eea"),
        bright_magenta: Color::from_hex("#d670d6"),
        bright_cyan:    Color::from_hex("#29b8db"),
        bright_white:   Color::from_hex("#ffffff"),
    }
}

impl Theme {
    pub fn from_file(file: &ThemeFile) -> Self {
        let shape = file.shape.as_ref();
        let typo = file.typography.as_ref();
        let ansi = file.ansi.as_ref()
            .map(AnsiPalette::from_colors)
            .unwrap_or_else(default_ansi);
        Self {
            name: file.name.clone(),
            surface: Color::from_hex(&file.colors.surface),
            surface_elevated: Color::from_hex(&file.colors.surface_elevated),
            surface_hover: Color::from_hex(&file.colors.surface_hover),
            surface_active: Color::from_hex(&file.colors.surface_active),
            primary: Color::from_hex(&file.colors.primary),
            primary_hover: Color::from_hex(&file.colors.primary_hover),
            text_primary: Color::from_hex(&file.colors.text_primary),
            text_secondary: Color::from_hex(&file.colors.text_secondary),
            text_disabled: Color::from_hex(&file.colors.text_disabled),
            border: Color::from_hex(&file.colors.border),
            shadow: Color::from_hex(&file.colors.shadow),
            success: Color::from_hex(&file.colors.success),
            warning: Color::from_hex(&file.colors.warning),
            error: Color::from_hex(&file.colors.error),
            ansi,
            corner_radius: shape.map_or(8.0, |s| s.corner_radius),
            border_width: shape.map_or(1.0, |s| s.border_width),
            font_size: typo.map_or(14.0, |t| t.body_size),
            opacity: shape.map_or(1.0, |s| s.opacity),
        }
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let file: ThemeFile = serde_yaml::from_str(&content)?;
        Ok(Self::from_file(&file))
    }

    /// Default midnight theme.
    pub fn midnight() -> Self {
        Self {
            name: "Midnight".into(),
            surface: Color::from_hex("#1a1a2e"),
            surface_elevated: Color::from_hex("#22223a"),
            surface_hover: Color::from_hex("#2a2a45"),
            surface_active: Color::from_hex("#333355"),
            primary: Color::from_hex("#6c63ff"),
            primary_hover: Color::from_hex("#8a82ff"),
            text_primary: Color::from_hex("#e8e8f0"),
            text_secondary: Color::from_hex("#9090a8"),
            text_disabled: Color::from_hex("#555568"),
            border: Color::from_hex("#3a3a55"),
            shadow: Color::from_hex("#00000060"),
            success: Color::from_hex("#4ade80"),
            warning: Color::from_hex("#fbbf24"),
            error: Color::from_hex("#f87171"),
            ansi: default_ansi(),
            corner_radius: 8.0,
            border_width: 1.0,
            font_size: 14.0,
            opacity: 1.0,
        }
    }

    /// Warp-inspired dark theme with modern ANSI colors.
    pub fn warp_dark() -> Self {
        Self {
            name: "Warp Dark".into(),
            surface: Color::from_hex("#0e0e12"),
            surface_elevated: Color::from_hex("#171720"),
            surface_hover: Color::from_hex("#1f1f2e"),
            surface_active: Color::from_hex("#28283d"),
            primary: Color::from_hex("#01a4ef"),
            primary_hover: Color::from_hex("#38bdf8"),
            text_primary: Color::from_hex("#d4d4e4"),
            text_secondary: Color::from_hex("#7a7a8e"),
            text_disabled: Color::from_hex("#44445a"),
            border: Color::from_hex("#2a2a3c"),
            shadow: Color::from_hex("#00000080"),
            success: Color::from_hex("#34d399"),
            warning: Color::from_hex("#fbbf24"),
            error: Color::from_hex("#f87171"),
            ansi: AnsiPalette {
                black:          Color::from_hex("#0e0e12"),
                red:            Color::from_hex("#ff6b6b"),
                green:          Color::from_hex("#34d399"),
                yellow:         Color::from_hex("#fbbf24"),
                blue:           Color::from_hex("#01a4ef"),
                magenta:        Color::from_hex("#c084fc"),
                cyan:           Color::from_hex("#22d3ee"),
                white:          Color::from_hex("#d4d4e4"),
                bright_black:   Color::from_hex("#44445a"),
                bright_red:     Color::from_hex("#fca5a5"),
                bright_green:   Color::from_hex("#6ee7b7"),
                bright_yellow:  Color::from_hex("#fde68a"),
                bright_blue:    Color::from_hex("#7dd3fc"),
                bright_magenta: Color::from_hex("#d8b4fe"),
                bright_cyan:    Color::from_hex("#67e8f9"),
                bright_white:   Color::from_hex("#f5f5f5"),
            },
            corner_radius: 6.0,
            border_width: 1.0,
            font_size: 13.0,
            opacity: 1.0,
        }
    }

    /// Classic green-on-black retro terminal theme.
    pub fn retro_green() -> Self {
        let green = Color::from_hex("#00ff41");
        let dim_green = Color::from_hex("#00a82a");
        Self {
            name: "Retro Green".into(),
            surface: Color::from_hex("#0a0a0a"),
            surface_elevated: Color::from_hex("#111111"),
            surface_hover: Color::from_hex("#1a1a1a"),
            surface_active: Color::from_hex("#222222"),
            primary: green,
            primary_hover: Color::from_hex("#33ff66"),
            text_primary: green,
            text_secondary: dim_green,
            text_disabled: Color::from_hex("#1a4a1a"),
            border: Color::from_hex("#00551a"),
            shadow: Color::from_hex("#00ff4120"),
            success: green,
            warning: Color::from_hex("#ffaa00"),
            error: Color::from_hex("#ff3333"),
            ansi: AnsiPalette {
                black:          Color::from_hex("#0a0a0a"),
                red:            Color::from_hex("#ff3333"),
                green,
                yellow:         Color::from_hex("#ffaa00"),
                blue:           Color::from_hex("#0077ff"),
                magenta:        Color::from_hex("#cc44cc"),
                cyan:           Color::from_hex("#00cccc"),
                white:          Color::from_hex("#cccccc"),
                bright_black:   Color::from_hex("#444444"),
                bright_red:     Color::from_hex("#ff6666"),
                bright_green:   Color::from_hex("#66ff88"),
                bright_yellow:  Color::from_hex("#ffcc44"),
                bright_blue:    Color::from_hex("#4499ff"),
                bright_magenta: Color::from_hex("#ee77ee"),
                bright_cyan:    Color::from_hex("#44eeee"),
                bright_white:   Color::from_hex("#ffffff"),
            },
            corner_radius: 0.0,
            border_width: 1.0,
            font_size: 13.0,
            opacity: 1.0,
        }
    }

    /// Tokyo Night theme — cool blues, popular with devs.
    pub fn tokyo_night() -> Self {
        Self {
            name: "Tokyo Night".into(),
            surface: Color::from_hex("#1a1b26"),
            surface_elevated: Color::from_hex("#24283b"),
            surface_hover: Color::from_hex("#343a52"),
            surface_active: Color::from_hex("#414868"),
            primary: Color::from_hex("#7aa2f7"),
            primary_hover: Color::from_hex("#89b4fa"),
            text_primary: Color::from_hex("#c0caf5"),
            text_secondary: Color::from_hex("#9aa5ce"),
            text_disabled: Color::from_hex("#565f89"),
            border: Color::from_hex("#414868"),
            shadow: Color::from_hex("#00000060"),
            success: Color::from_hex("#9ece6a"),
            warning: Color::from_hex("#e0af68"),
            error: Color::from_hex("#f7768e"),
            ansi: AnsiPalette {
                black:          Color::from_hex("#15161e"),
                red:            Color::from_hex("#f7768e"),
                green:          Color::from_hex("#9ece6a"),
                yellow:         Color::from_hex("#e0af68"),
                blue:           Color::from_hex("#7aa2f7"),
                magenta:        Color::from_hex("#bb9af7"),
                cyan:           Color::from_hex("#7dcfff"),
                white:          Color::from_hex("#c0caf5"),
                bright_black:   Color::from_hex("#414868"),
                bright_red:     Color::from_hex("#f7768e"),
                bright_green:   Color::from_hex("#9ece6a"),
                bright_yellow:  Color::from_hex("#e0af68"),
                bright_blue:    Color::from_hex("#7aa2f7"),
                bright_magenta: Color::from_hex("#bb9af7"),
                bright_cyan:    Color::from_hex("#7dcfff"),
                bright_white:   Color::from_hex("#c0caf5"),
            },
            corner_radius: 6.0,
            border_width: 1.0,
            font_size: 14.0,
            opacity: 1.0,
        }
    }

    /// Catppuccin Mocha — warm pastels on dark base.
    pub fn catppuccin_mocha() -> Self {
        Self {
            name: "Catppuccin Mocha".into(),
            surface: Color::from_hex("#1e1e2e"),
            surface_elevated: Color::from_hex("#302d41"),
            surface_hover: Color::from_hex("#45425a"),
            surface_active: Color::from_hex("#575268"),
            primary: Color::from_hex("#cba6f7"),
            primary_hover: Color::from_hex("#d4b8fa"),
            text_primary: Color::from_hex("#cdd6f4"),
            text_secondary: Color::from_hex("#a6adc8"),
            text_disabled: Color::from_hex("#6c7086"),
            border: Color::from_hex("#575268"),
            shadow: Color::from_hex("#00000060"),
            success: Color::from_hex("#a6e3a1"),
            warning: Color::from_hex("#f9e2af"),
            error: Color::from_hex("#f38ba8"),
            ansi: AnsiPalette {
                black:          Color::from_hex("#11111b"),
                red:            Color::from_hex("#f38ba8"),
                green:          Color::from_hex("#a6e3a1"),
                yellow:         Color::from_hex("#f9e2af"),
                blue:           Color::from_hex("#89b4fa"),
                magenta:        Color::from_hex("#cba6f7"),
                cyan:           Color::from_hex("#94e2d5"),
                white:          Color::from_hex("#cdd6f4"),
                bright_black:   Color::from_hex("#585b70"),
                bright_red:     Color::from_hex("#f38ba8"),
                bright_green:   Color::from_hex("#a6e3a1"),
                bright_yellow:  Color::from_hex("#f9e2af"),
                bright_blue:    Color::from_hex("#89b4fa"),
                bright_magenta: Color::from_hex("#cba6f7"),
                bright_cyan:    Color::from_hex("#94e2d5"),
                bright_white:   Color::from_hex("#a6adc8"),
            },
            corner_radius: 8.0,
            border_width: 1.0,
            font_size: 14.0,
            opacity: 1.0,
        }
    }

    /// Dracula — iconic purple dark theme.
    pub fn dracula() -> Self {
        Self {
            name: "Dracula".into(),
            surface: Color::from_hex("#282a36"),
            surface_elevated: Color::from_hex("#343746"),
            surface_hover: Color::from_hex("#44475a"),
            surface_active: Color::from_hex("#555770"),
            primary: Color::from_hex("#bd93f9"),
            primary_hover: Color::from_hex("#caa9fa"),
            text_primary: Color::from_hex("#f8f8f2"),
            text_secondary: Color::from_hex("#bfbfbf"),
            text_disabled: Color::from_hex("#6272a4"),
            border: Color::from_hex("#6272a4"),
            shadow: Color::from_hex("#00000060"),
            success: Color::from_hex("#50fa7b"),
            warning: Color::from_hex("#f1fa8c"),
            error: Color::from_hex("#ff5555"),
            ansi: AnsiPalette {
                black:          Color::from_hex("#21222c"),
                red:            Color::from_hex("#ff5555"),
                green:          Color::from_hex("#50fa7b"),
                yellow:         Color::from_hex("#f1fa8c"),
                blue:           Color::from_hex("#bd93f9"),
                magenta:        Color::from_hex("#ff79c6"),
                cyan:           Color::from_hex("#8be9fd"),
                white:          Color::from_hex("#f8f8f2"),
                bright_black:   Color::from_hex("#6272a4"),
                bright_red:     Color::from_hex("#ff6e6e"),
                bright_green:   Color::from_hex("#69ff94"),
                bright_yellow:  Color::from_hex("#ffffa5"),
                bright_blue:    Color::from_hex("#d6acff"),
                bright_magenta: Color::from_hex("#ff92df"),
                bright_cyan:    Color::from_hex("#a4ffff"),
                bright_white:   Color::from_hex("#ffffff"),
            },
            corner_radius: 6.0,
            border_width: 1.0,
            font_size: 14.0,
            opacity: 1.0,
        }
    }

    /// Nord — arctic, cool minimal palette.
    pub fn nord() -> Self {
        Self {
            name: "Nord".into(),
            surface: Color::from_hex("#2e3440"),
            surface_elevated: Color::from_hex("#3b4252"),
            surface_hover: Color::from_hex("#434c5e"),
            surface_active: Color::from_hex("#4c566a"),
            primary: Color::from_hex("#88c0d0"),
            primary_hover: Color::from_hex("#8fbcbb"),
            text_primary: Color::from_hex("#eceff4"),
            text_secondary: Color::from_hex("#d8dee9"),
            text_disabled: Color::from_hex("#616e88"),
            border: Color::from_hex("#4c566a"),
            shadow: Color::from_hex("#00000050"),
            success: Color::from_hex("#a3be8c"),
            warning: Color::from_hex("#ebcb8b"),
            error: Color::from_hex("#bf616a"),
            ansi: AnsiPalette {
                black:          Color::from_hex("#3b4252"),
                red:            Color::from_hex("#bf616a"),
                green:          Color::from_hex("#a3be8c"),
                yellow:         Color::from_hex("#ebcb8b"),
                blue:           Color::from_hex("#81a1c1"),
                magenta:        Color::from_hex("#b48ead"),
                cyan:           Color::from_hex("#88c0d0"),
                white:          Color::from_hex("#e5e9f0"),
                bright_black:   Color::from_hex("#4c566a"),
                bright_red:     Color::from_hex("#bf616a"),
                bright_green:   Color::from_hex("#a3be8c"),
                bright_yellow:  Color::from_hex("#ebcb8b"),
                bright_blue:    Color::from_hex("#81a1c1"),
                bright_magenta: Color::from_hex("#b48ead"),
                bright_cyan:    Color::from_hex("#8fbcbb"),
                bright_white:   Color::from_hex("#eceff4"),
            },
            corner_radius: 4.0,
            border_width: 1.0,
            font_size: 14.0,
            opacity: 1.0,
        }
    }

    /// Gruvbox Dark — warm retro earth tones.
    pub fn gruvbox_dark() -> Self {
        Self {
            name: "Gruvbox Dark".into(),
            surface: Color::from_hex("#282828"),
            surface_elevated: Color::from_hex("#3c3836"),
            surface_hover: Color::from_hex("#504945"),
            surface_active: Color::from_hex("#665c54"),
            primary: Color::from_hex("#fe8019"),
            primary_hover: Color::from_hex("#fabd2f"),
            text_primary: Color::from_hex("#ebdbb2"),
            text_secondary: Color::from_hex("#bdae93"),
            text_disabled: Color::from_hex("#665c54"),
            border: Color::from_hex("#665c54"),
            shadow: Color::from_hex("#00000060"),
            success: Color::from_hex("#b8bb26"),
            warning: Color::from_hex("#fabd2f"),
            error: Color::from_hex("#fb4934"),
            ansi: AnsiPalette {
                black:          Color::from_hex("#282828"),
                red:            Color::from_hex("#cc241d"),
                green:          Color::from_hex("#98971a"),
                yellow:         Color::from_hex("#d79921"),
                blue:           Color::from_hex("#458588"),
                magenta:        Color::from_hex("#b16286"),
                cyan:           Color::from_hex("#689d6a"),
                white:          Color::from_hex("#a89984"),
                bright_black:   Color::from_hex("#928374"),
                bright_red:     Color::from_hex("#fb4934"),
                bright_green:   Color::from_hex("#b8bb26"),
                bright_yellow:  Color::from_hex("#fabd2f"),
                bright_blue:    Color::from_hex("#83a598"),
                bright_magenta: Color::from_hex("#d3869b"),
                bright_cyan:    Color::from_hex("#8ec07c"),
                bright_white:   Color::from_hex("#ebdbb2"),
            },
            corner_radius: 2.0,
            border_width: 1.0,
            font_size: 14.0,
            opacity: 1.0,
        }
    }

    /// Rose Pine — elegant pastels on deep purple.
    pub fn rose_pine() -> Self {
        Self {
            name: "Rose Pine".into(),
            surface: Color::from_hex("#191724"),
            surface_elevated: Color::from_hex("#1f1d2e"),
            surface_hover: Color::from_hex("#26233a"),
            surface_active: Color::from_hex("#403d52"),
            primary: Color::from_hex("#c4a7e7"),
            primary_hover: Color::from_hex("#ebbcba"),
            text_primary: Color::from_hex("#e0def4"),
            text_secondary: Color::from_hex("#908caa"),
            text_disabled: Color::from_hex("#6e6a86"),
            border: Color::from_hex("#403d52"),
            shadow: Color::from_hex("#00000060"),
            success: Color::from_hex("#9ccfd8"),
            warning: Color::from_hex("#f6c177"),
            error: Color::from_hex("#eb6f92"),
            ansi: AnsiPalette {
                black:          Color::from_hex("#26233a"),
                red:            Color::from_hex("#eb6f92"),
                green:          Color::from_hex("#31748f"),
                yellow:         Color::from_hex("#f6c177"),
                blue:           Color::from_hex("#9ccfd8"),
                magenta:        Color::from_hex("#c4a7e7"),
                cyan:           Color::from_hex("#ebbcba"),
                white:          Color::from_hex("#e0def4"),
                bright_black:   Color::from_hex("#6e6a86"),
                bright_red:     Color::from_hex("#eb6f92"),
                bright_green:   Color::from_hex("#31748f"),
                bright_yellow:  Color::from_hex("#f6c177"),
                bright_blue:    Color::from_hex("#9ccfd8"),
                bright_magenta: Color::from_hex("#c4a7e7"),
                bright_cyan:    Color::from_hex("#ebbcba"),
                bright_white:   Color::from_hex("#e0def4"),
            },
            corner_radius: 8.0,
            border_width: 1.0,
            font_size: 14.0,
            opacity: 1.0,
        }
    }

    /// Kanagawa — Japanese ink painting inspired.
    pub fn kanagawa() -> Self {
        Self {
            name: "Kanagawa".into(),
            surface: Color::from_hex("#1f1f28"),
            surface_elevated: Color::from_hex("#2a2a37"),
            surface_hover: Color::from_hex("#363646"),
            surface_active: Color::from_hex("#54546d"),
            primary: Color::from_hex("#7e9cd8"),
            primary_hover: Color::from_hex("#7fb4ca"),
            text_primary: Color::from_hex("#dcd7ba"),
            text_secondary: Color::from_hex("#727169"),
            text_disabled: Color::from_hex("#54546d"),
            border: Color::from_hex("#54546d"),
            shadow: Color::from_hex("#00000060"),
            success: Color::from_hex("#98bb6c"),
            warning: Color::from_hex("#e6c384"),
            error: Color::from_hex("#c34043"),
            ansi: AnsiPalette {
                black:          Color::from_hex("#16161d"),
                red:            Color::from_hex("#c34043"),
                green:          Color::from_hex("#76946a"),
                yellow:         Color::from_hex("#c0a36e"),
                blue:           Color::from_hex("#7e9cd8"),
                magenta:        Color::from_hex("#957fb8"),
                cyan:           Color::from_hex("#6a9589"),
                white:          Color::from_hex("#c8c093"),
                bright_black:   Color::from_hex("#727169"),
                bright_red:     Color::from_hex("#e82424"),
                bright_green:   Color::from_hex("#98bb6c"),
                bright_yellow:  Color::from_hex("#e6c384"),
                bright_blue:    Color::from_hex("#7fb4ca"),
                bright_magenta: Color::from_hex("#938aa9"),
                bright_cyan:    Color::from_hex("#7aa89f"),
                bright_white:   Color::from_hex("#dcd7ba"),
            },
            corner_radius: 4.0,
            border_width: 1.0,
            font_size: 14.0,
            opacity: 1.0,
        }
    }

    /// SynthWave 84 — neon retro cyberpunk.
    pub fn synthwave() -> Self {
        Self {
            name: "SynthWave 84".into(),
            surface: Color::from_hex("#262335"),
            surface_elevated: Color::from_hex("#2a2139"),
            surface_hover: Color::from_hex("#34294f"),
            surface_active: Color::from_hex("#3e3350"),
            primary: Color::from_hex("#ff7edb"),
            primary_hover: Color::from_hex("#fede5d"),
            text_primary: Color::from_hex("#ffffff"),
            text_secondary: Color::from_hex("#b4a8c8"),
            text_disabled: Color::from_hex("#5a4e6e"),
            border: Color::from_hex("#3e3350"),
            shadow: Color::from_hex("#00000070"),
            success: Color::from_hex("#72f1b8"),
            warning: Color::from_hex("#fede5d"),
            error: Color::from_hex("#fe4450"),
            ansi: AnsiPalette {
                black:          Color::from_hex("#1e1a2b"),
                red:            Color::from_hex("#fe4450"),
                green:          Color::from_hex("#72f1b8"),
                yellow:         Color::from_hex("#fede5d"),
                blue:           Color::from_hex("#03edf9"),
                magenta:        Color::from_hex("#ff7edb"),
                cyan:           Color::from_hex("#03edf9"),
                white:          Color::from_hex("#ffffff"),
                bright_black:   Color::from_hex("#5a4e6e"),
                bright_red:     Color::from_hex("#fe6e7a"),
                bright_green:   Color::from_hex("#8af8cc"),
                bright_yellow:  Color::from_hex("#fef08a"),
                bright_blue:    Color::from_hex("#36f9f6"),
                bright_magenta: Color::from_hex("#ff9eeb"),
                bright_cyan:    Color::from_hex("#36f9f6"),
                bright_white:   Color::from_hex("#ffffff"),
            },
            corner_radius: 6.0,
            border_width: 1.0,
            font_size: 14.0,
            opacity: 1.0,
        }
    }

    /// Make any theme transparent with the given opacity (0.0–1.0).
    pub fn with_opacity(mut self, opacity: f32) -> Self {
        self.opacity = opacity.clamp(0.0, 1.0);
        self
    }

    /// Returns all built-in theme presets.
    pub fn all_presets() -> Vec<Self> {
        vec![
            Self::warp_dark(),
            Self::midnight(),
            Self::tokyo_night(),
            Self::catppuccin_mocha(),
            Self::dracula(),
            Self::nord(),
            Self::gruvbox_dark(),
            Self::rose_pine(),
            Self::kanagawa(),
            Self::synthwave(),
            Self::retro_green(),
        ]
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::midnight()
    }
}
