//! `examples/` が `fonts()` で積んでいるフォントで、 その example が実際に
//! 描く文字を全部描けるか。
//!
//! ## なぜ要るか
//!
//! native にはシステムフォントがある。 だから `fonts()` に日本語フォントを
//! 入れ忘れても、 **画面はふつうに日本語で出る** — OS が拾ってしまうので。
//! 足りないことに気づくのは wasm に持っていった時で、 そこには
//! システムフォントが無いから、 日本語だけが一斉に豆腐になる。
//!
//! `tui_demo` と `tui_gallery` が実際にそうなっていた。 Hack (Latin のみ) を
//! 2 つ積んだきりで、 日本語のラベルは全部システムフォント任せ。 native で
//! 動かしている限り永久に分からない形。
//!
//! ここでは **wasm と同じ条件** — システムフォントを一切使わず、 その example が
//! `include_bytes!` している物だけを積んだ状態 — を native で作って、
//! ソースの文字列リテラルに出てくる日本語が全部描けるかを見る。
//!
//! ## 何を見ていないか
//!
//! 実行時に組み立てられる文字列 (`format!` の結果、 外部から来るデータ) は
//! 見ていない。 ソースに literal で書いてある字だけ。 それでも、 UI の
//! ラベルはほぼ literal なので、 この種の抜けはここで止まる。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use sabitori_core::build::TextShape;
use sabitori_core::Typography;
use sabitori_text::TextShaper;

/// ワークスペースのルート。 integration test の CWD はパッケージのルート
/// (`crates/sabitori-text/`) なので 2 つ上がる。
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/<name>/ の 2 つ上")
        .to_path_buf()
}

/// example のソースから `include_bytes!("../assets/fonts/X")` を拾う。
fn bundled_font_paths(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    const MARK: &str = "assets/fonts/";
    for (i, _) in src.match_indices(MARK) {
        let name: String = src[i + MARK.len()..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
            .collect();
        if (name.ends_with(".ttf") || name.ends_with(".otf")) && !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// 行コメントを落とす。 コメントの日本語は描かれないので、 対象から外す。
///
/// `//` が文字列の中に居る場合 (URL など) も落ちるが、 URL は ASCII なので
/// ここで見たい CJK は失わない。
fn strip_line_comments(src: &str) -> String {
    src.lines()
        .map(|line| match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 描画対象になりうる CJK / かな。 記号や Latin は Hack 側が持っているので
/// ここでは見ない (足りなければどのみち別のテストで落ちる)。
fn cjk_chars(src: &str) -> BTreeSet<char> {
    src.chars()
        .filter(|c| {
            matches!(*c as u32,
                0x3040..=0x309F   // ひらがな
                | 0x30A0..=0x30FF // カタカナ
                | 0x3400..=0x4DBF // CJK 拡張 A
                | 0x4E00..=0x9FFF // CJK 統合漢字
                | 0xFF00..=0xFF60 // 全角英数・記号
            )
        })
        .collect()
}

fn shape() -> TextShape<'static> {
    TextShape {
        font_size: 14.0,
        bold: false,
        monospace: true,
        font_family: None,
        wrap_width: None,
        typo: Typography::default(),
    }
}

/// **これが本題。** `fonts()` を持つ example は、 自分が描く日本語を
/// 自分の積んだフォントだけで描けること。
#[test]
fn every_example_can_draw_its_own_japanese_with_the_fonts_it_bundles() {
    let root = workspace_root();
    let mut checked = 0usize;
    let mut failures: Vec<String> = Vec::new();

    let mut entries: Vec<_> = std::fs::read_dir(root.join("examples"))
        .expect("examples/ が読めること")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs"))
        .collect();
    entries.sort();

    for path in entries {
        let src = std::fs::read_to_string(&path).expect("example のソース");
        let fonts = bundled_font_paths(&src);
        if fonts.is_empty() {
            // `fonts()` を実装していない example は、 native の
            // システムフォント前提。 wasm には元から持っていけないので対象外。
            continue;
        }
        let wanted = cjk_chars(&strip_line_comments(&src));
        if wanted.is_empty() {
            continue;
        }

        let data: Vec<Vec<u8>> = fonts
            .iter()
            .map(|name| {
                std::fs::read(root.join("assets/fonts").join(name))
                    .unwrap_or_else(|e| panic!("{name} が読めない: {e}"))
            })
            .collect();

        let mut shaper = TextShaper::with_fonts_only("ja", &data);
        let text: String = wanted.iter().collect();
        let missing = shaper.missing_glyphs(&text, shape());
        checked += 1;

        if !missing.is_empty() {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let sample: String = missing.iter().take(20).collect();
            failures.push(format!(
                "{name}: {} 文字が描けない (積んでいるのは {fonts:?})\n    例: {sample}",
                missing.len()
            ));
        }
    }

    assert!(
        checked >= 2,
        "検査対象が {checked} 件しか無い。 example の `fonts()` の書き方が変わって\n\
         パスを拾えていない可能性がある — 黙って 0 件を通すとこのテストは意味を失う"
    );
    assert!(
        failures.is_empty(),
        "wasm に持っていくと日本語が豆腐になる example がある:\n{}",
        failures.join("\n")
    );
}

/// 抽出そのものが動いていること。 上のテストが「0 件検査して合格」に
/// 化けるのを防ぐ。
#[test]
fn the_font_path_extraction_actually_finds_things() {
    let src = r#"
        fn fonts(&self) -> Vec<Vec<u8>> {
            vec![
                include_bytes!("../assets/fonts/HackGen-Regular.ttf").to_vec(),
                include_bytes!("../assets/fonts/HackGen-Bold.ttf").to_vec(),
            ]
        }
    "#;
    assert_eq!(
        bundled_font_paths(src),
        vec!["HackGen-Regular.ttf", "HackGen-Bold.ttf"]
    );
}

/// コメントの日本語は対象外であること。 コメントに珍しい漢字を書いただけで
/// 落ちるなら、 このテストは無視されるようになる。
#[test]
fn japanese_in_comments_is_not_counted() {
    let src = "// 齟齬\nlet s = \"設定\";";
    let found = cjk_chars(&strip_line_comments(src));
    assert!(found.contains(&'設'), "文字列リテラルは拾う");
    assert!(!found.contains(&'齟'), "コメントは拾わない");
}
