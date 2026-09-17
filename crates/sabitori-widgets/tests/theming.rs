//! ウィジェットの既定スタイルが [`AppTheme`] に追従すること (#65)。
//!
//! v0.11.2 では `default_dark()` を持つ style が 9 型あって、**どれも
//! `AppTheme` と繋がっていなかった**。テーマを差し替えてもウィジェットは
//! 追従せず、何も指定しないアプリは全部「いつもの紫」になる。明るくしたい
//! アプリは部品ごとに色を配り直すことになっていた。
//!
//! ここで見るのは 2 つ:
//!
//! 1. **型を 1 つも置き去りにしていないこと** — 9 型のうち 1 つだけ忘れる、が
//!    この手の作業で必ず起きる (実際 `default_light` は text_input にしか
//!    無かった)。ソースを読んで数える。
//! 2. **本当にテーマを見ていること** — 明色テーマを渡したら明るくなる。
//!    `from_theme` を生やしただけで中身が `default_dark()` のままでも
//!    コンパイルは通るので、色で確かめる。

use sabitori_core::{AppTheme, Color};
use sabitori_widgets::{
    ColorPickerStyle, DatePickerStyle, DropdownStyle, MenuBarStyle, ModalStyle, SplitPaneStyle,
    TableStyle, TextInputStyle, TreeViewStyle,
};

/// `default_dark()` を持つ型は、全部 `from_theme()` も持つこと。
#[test]
fn every_themed_style_can_be_built_from_a_theme() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let mut missing = Vec::new();
    let mut found = 0;

    for entry in std::fs::read_dir(dir).expect("src/") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&path).expect("read");
        if !src.contains("pub fn default_dark") {
            continue;
        }
        found += 1;
        if !src.contains("pub fn from_theme") {
            missing.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }

    assert!(found >= 9, "default_dark を持つ型が {found} 個しか見つからない");
    assert!(
        missing.is_empty(),
        "from_theme が無い: {missing:?} — テーマを差し替えてもここだけ追従しない"
    );
}

/// 明色テーマを渡したら、地が明るく文字が暗くなること。
///
/// 「地の方が文字より明るい」を全型で見る。`from_theme` を生やしただけで
/// 中身が `default_dark()` のままなら、ここで落ちる。
#[test]
fn a_light_theme_produces_light_widgets() {
    let t = AppTheme::light();

    let pairs: Vec<(&str, Color, Color)> = vec![
        ("text_input", TextInputStyle::from_theme(&t).bg, TextInputStyle::from_theme(&t).text),
        ("table", TableStyle::from_theme(&t).row_bg, TableStyle::from_theme(&t).fg),
        ("tree_view", TreeViewStyle::from_theme(&t).bg_selected, TreeViewStyle::from_theme(&t).fg),
        ("dropdown", DropdownStyle::from_theme(&t).bg, DropdownStyle::from_theme(&t).fg),
        ("menu_bar", MenuBarStyle::from_theme(&t).menu_bg, MenuBarStyle::from_theme(&t).item_fg),
        ("date_picker", DatePickerStyle::from_theme(&t).bg, DatePickerStyle::from_theme(&t).day_fg),
        ("color_picker", ColorPickerStyle::from_theme(&t).bg, ColorPickerStyle::from_theme(&t).text),
    ];

    for (name, bg, fg) in pairs {
        assert!(
            bg.luminance() > fg.luminance(),
            "{name}: 明色テーマなのに地 ({:.2}) が文字 ({:.2}) より暗い",
            bg.luminance(),
            fg.luminance()
        );
    }

    // 色を 1 つしか持たない型も、テーマの値になっていること。
    assert_eq!(SplitPaneStyle::from_theme(&t).divider, t.border);
    assert_eq!(ModalStyle::from_theme(&t).bg, t.elevated);
}

/// 暗色テーマでは暗いまま (追従の向きが逆になっていない)。
#[test]
fn a_dark_theme_still_produces_dark_widgets() {
    for t in [AppTheme::midnight(), AppTheme::nord(), AppTheme::dracula()] {
        let s = TextInputStyle::from_theme(&t);
        assert!(
            s.bg.luminance() < s.text.luminance(),
            "暗色テーマなのに地の方が明るい"
        );
        assert!(t.is_dark(), "暗色プリセットが is_dark() で false");
    }
}

/// **読める組み合わせであること。** 業務画面は 1 日中見るので、
/// 本文が地に対して WCAG AA (4.5:1) を下回るテーマを既定で配らない。
#[test]
fn every_preset_keeps_body_text_readable() {
    let presets = [
        ("light", AppTheme::light()),
        ("midnight", AppTheme::midnight()),
        ("tokyo_night", AppTheme::tokyo_night()),
        ("catppuccin", AppTheme::catppuccin()),
        ("nord", AppTheme::nord()),
        ("dracula", AppTheme::dracula()),
    ];
    for (name, t) in presets {
        for (what, fg, bg) in [
            ("本文 on 地", t.text_primary, t.bg),
            ("本文 on surface", t.text_primary, t.surface),
            ("primary の上の文字", t.on_primary(), t.primary),
        ] {
            let ratio = fg.contrast_ratio(bg);
            assert!(
                ratio >= 4.5,
                "{name}: {what} のコントラストが {ratio:.2}:1 (4.5 未満)"
            );
        }
    }
}
