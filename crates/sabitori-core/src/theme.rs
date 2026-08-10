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


#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG AA: 本文 4.5、大きい文字と UI 部品 3.0。
    const AA_TEXT: f32 = 4.5;
    const AA_UI: f32 = 3.0;

    fn presets() -> Vec<(&'static str, AppTheme)> {
        vec![
            ("midnight", AppTheme::midnight()),
            ("tokyo_night", AppTheme::tokyo_night()),
            ("catppuccin", AppTheme::catppuccin()),
            ("nord", AppTheme::nord()),
            ("dracula", AppTheme::dracula()),
        ]
    }

    /// 主文字はどのプリセットのどの地の上でも本文コントラストを満たすこと。
    ///
    /// プリセットは 5 つあり、切り替えは 1 行で済む。読めなくなる組み合わせが
    /// 混ざっても、実際に切り替えて目で見るまで誰も気付かない — その検査を
    /// toolkit 側で持つ、というのが #7 の趣旨。
    #[test]
    fn primary_text_is_readable_on_every_surface_of_every_preset() {
        for (name, t) in presets() {
            for (label, bg) in [
                ("bg", t.bg),
                ("surface", t.surface),
                ("elevated", t.elevated),
                ("hover_bg", t.hover_bg),
                ("select_bg", t.select_bg),
            ] {
                let r = t.text_primary.contrast_ratio(bg);
                assert!(
                    r >= AA_TEXT,
                    "{name}: text_primary が {label} の上で {r:.2}:1（本文には {AA_TEXT} 要る）"
                );
            }
        }
    }

    /// 副文字は素の地・面の上では本文コントラストを満たすこと。
    #[test]
    fn secondary_text_is_readable_on_the_plain_surfaces() {
        for (name, t) in presets() {
            for (label, bg) in [("bg", t.bg), ("surface", t.surface)] {
                let r = t.text_secondary.contrast_ratio(bg);
                assert!(
                    r >= AA_TEXT,
                    "{name}: text_secondary が {label} の上で {r:.2}:1"
                );
            }
        }
    }

    /// **既知の弱点**: 選択行の上の副文字は、5 つ中 4 つで本文コントラストに届かない。
    ///
    /// | preset | text_secondary / select_bg |
    /// |---|---|
    /// | midnight | 3.86 |
    /// | tokyo_night | 3.68 |
    /// | catppuccin | 3.35 |
    /// | dracula | 3.83 |
    /// | nord | 5.46 |
    ///
    /// パレットを動かすとアプリの見た目が変わるので、ここでは**現状を固定するだけ**に
    /// して、UI 部品の下限 (3.0) を割らないことを保証する。4.5 に上げるかどうかは
    /// パレット側の判断で、`select_bg` を暗くするか `text_secondary` を明るくするか。
    /// 上げる時はこの下限を `AA_TEXT` に差し替える。
    #[test]
    fn secondary_text_on_selection_holds_at_least_the_ui_floor() {
        for (name, t) in presets() {
            let r = t.text_secondary.contrast_ratio(t.select_bg);
            assert!(
                r >= AA_UI,
                "{name}: text_secondary が select_bg の上で {r:.2}:1（UI 下限 {AA_UI} を割った）"
            );
        }
    }

    /// status 色は「地の上に置かれる印」なので UI 部品の下限で見る。
    /// nord の error は 3.05 で、ここも余裕が無い。
    #[test]
    fn status_colors_clear_the_ui_floor_on_the_base_background() {
        for (name, t) in presets() {
            for (label, c) in [
                ("primary", t.primary),
                ("success", t.success),
                ("warning", t.warning),
                ("error", t.error),
            ] {
                let r = c.contrast_ratio(t.bg);
                assert!(r >= AA_UI, "{name}: {label} が bg の上で {r:.2}:1");
            }
        }
    }
}
