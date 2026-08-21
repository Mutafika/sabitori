//! README のコード例を、 **README から逐語で写して**コンパイルする。
//!
//! # なぜ逐語なのか
//!
//! 0.4.0 でこのファイルを足したとき、 スタイルだけ自前に構築していた —
//! README は `TextInputStyle::default_dark()` を書いているのに、 テストは
//! フィールドを 1 個ずつ並べていた。 **その関数は存在しなかった。** つまり
//! 「README のコードが通ること」を見ているつもりで見ていなかった。
//!
//! 同じ回に、 README のテキスト入力の例が `struct App { name: TextInputState }`
//! と書きながら `self.saved` を触っている (コンパイルできない) ことも見つかった。
//!
//! なので今は **README のブロックをそのまま貼る**。 貼れる形でないブロックは
//! README 側を直した (単発の式を `let` 束縛にする等) — 読者がコピペして動かない
//! なら、 それはドキュメントとして壊れている。
//!
//! 貼り付けは列 0 のまま。 Rust はインデントを見ないので、 逐語性を優先した。
//!
//! 最後の 2 つのテストが輪を閉じる。 README を編集してここを直し忘れると落ち、
//! 日本語版だけコードがずれても落ちる。

#![allow(dead_code, unused_variables, unused_imports, unused_mut)]

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_widgets::{
    table, text_area, text_input, tree_view, virtual_list, Cell, TableColumn, TableState,
    TableStyle, TextInputState, TextInputStyle, TreeNode, TreeViewStyle,
};

/// README.md から抜いた rust ブロック。 テスト側で使う。
fn readme_blocks(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut it = markdown.split("```rust\n");
    it.next();
    for chunk in it {
        if let Some(end) = chunk.find("```") {
            out.push(chunk[..end].trim_end().to_string());
        }
    }
    out
}
// ===== README [0] クイックスタート =====
mod quick_start {
use sabitori::*;

struct App { clicks: u32 }

impl DeclarativeApp for App {
    fn title(&self) -> &str { "Hello Sabitori" }
    fn size(&self) -> (f32, f32) { (800.0, 600.0) }

    fn view(&self, ctx: &ViewContext) -> Element {
        div()
            .w(Px(ctx.width)).h(Px(ctx.height))
            .bg(Color::from_hex("#1a1b26"))
            .flex_col()
            .items_center()
            .justify_center()
            .gap(16.0)
            .children([
                text(&format!("Clicks: {}", self.clicks))
                    .font_size(24.0)
                    .color(Color::from_hex("#c0caf5")),
                button("Click Me")
                    .id("btn")
                    .accent(Color::from_hex("#7aa2f7")),
            ])
    }

    fn on_click(&mut self, id: &str) {
        if id == "btn" { self.clicks += 1; }
    }
}

fn main() {
    sabitori::run_declarative(App { clicks: 0 });
}

    // 私有な型を外から触らずに済むよう、 挙動のテストはモジュールの中に置く
    // (`struct App` に `pub` を足すと README と逐語でなくなる)。
    #[test]
    fn the_quick_start_button_counts() {
        use sabitori::testing::Harness;
        let mut h = Harness::new(App { clicks: 0 }, 800.0, 600.0);
        h.frame();
        h.click("btn");
        assert_eq!(h.app().clicks, 1);
    }
}

// ===== README [1] [2] click =====
mod click_form {
    use super::*;

    #[derive(Default)]
    pub struct App {
        pub saved: bool,
        pub selected: Option<usize>,
    }

    impl DeclarativeApp for App {
        fn view(&self, ctx: &ViewContext) -> Element {
            let i = 0usize;
            let save_button =
div().click(ctx, "save", |app: &mut App| app.saved = true)
                .id("save")
                .w(Px(80.0))
                .h(Px(30.0));
            let row =
div().click(ctx, format!("row-{i}"), move |app: &mut App| app.selected = Some(i))
                .w(Px(80.0))
                .h(Px(30.0));
            div().w_full().h_full().flex_col().children([save_button, row])
        }
    }
}

// ===== README [3] [4] [5] スクロール =====
mod scrolling {
    use super::*;

    pub const ROW_H: f32 = 28.0;

    pub struct App {
        pub lines: Vec<String>,
        pub pending: Option<f32>,
    }

    impl Default for App {
        fn default() -> Self {
            Self { lines: (0..500).map(|i| format!("line {i}")).collect(), pending: None }
        }
    }

    impl DeclarativeApp for App {
        fn view(&self, ctx: &ViewContext) -> Element {
let (first, count) = ctx.visible_range("file-list", ROW_H);
            let last = (first + count).min(self.lines.len());
            let first = first.min(last);
            let mut rows: Vec<Element> = Vec::new();
            if first > 0 {
                rows.push(div().h(Px(first as f32 * ROW_H)).shrink(0.0));
            }
            rows.extend(
                (first..last)
                    .map(|i| div().id(format!("row-{i}")).w_full().h(Px(ROW_H)).shrink(0.0)),
            );
            let tail = self.lines.len().saturating_sub(last);
            if tail > 0 {
                rows.push(div().h(Px(tail as f32 * ROW_H)).shrink(0.0));
            }
div().scroll("file-list").flex_1().flex_col().children(rows)
            .w_full()
            .h_full()
        }

fn scroll_intents(&mut self) -> Vec<(String, f32)> {
    self.pending.take().map(|y| ("file-list".into(), y)).into_iter().collect()
}
    }
}

// ===== README [6] テキスト入力 =====
mod text_field {
    use super::*;

struct App { name: TextInputState, saved: Option<String> }

impl DeclarativeApp for App {
    fn view(&self, ctx: &ViewContext) -> Element {
        text_input(ctx, "name", &self.name, &TextInputStyle::default_dark())
    }
    fn on_click(&mut self, id: &str) {
        if id == "save" { self.saved = Some(self.name.text()); }
    }
}

    impl Default for App {
        fn default() -> Self {
            Self { name: TextInputState::new("Name"), saved: None }
        }
    }

    /// テキスト入力が `view()` に置くだけで打てること。 配線メソッドは 1 つも
    /// 実装していない。 README [7] の形もここで通す。
    #[test]
    fn the_text_field_needs_no_wiring() {
        let mut h = Harness::new(App::default(), 800.0, 600.0);
        h.frame();
        h.click("name");
        h.text("hello");
        assert_eq!(h.app().name.text(), "hello");
assert!(h.unrouted_text_inputs().is_empty());
    }
}

// ===== README [8] text_area =====
mod multiline {
    use super::*;

    pub struct App {
        pub memo: TextInputState,
    }

    impl Default for App {
        fn default() -> Self {
            Self { memo: TextInputState::new("Memo") }
        }
    }

    impl DeclarativeApp for App {
        fn view(&self, ctx: &ViewContext) -> Element {
text_area(ctx, "memo", &self.memo, &TextInputStyle::default_dark(), 6)  // 6 lines tall
        }
    }
}

// ===== README [9] テスト =====
mod testing_section {
    use super::*;

    pub struct App {
        pub name: TextInputState,
        pub lines: Vec<String>,
        pub saved: Option<String>,
    }

    impl Default for App {
        fn default() -> Self {
            Self {
                name: TextInputState::new("Name"),
                lines: (0..500).map(|i| format!("line {i}")).collect(),
                saved: None,
            }
        }
    }

    impl DeclarativeApp for App {
        fn view(&self, ctx: &ViewContext) -> Element {
            let rows: Vec<Element> = (0..self.lines.len())
                .map(|i| div().id(format!("row-{i}")).w_full().h(Px(28.0)).shrink(0.0))
                .collect();
            div().w_full().h_full().flex_col().children([
                text_input(ctx, "name", &self.name, &TextInputStyle::default_dark()),
                div().id("save").w(Px(80.0)).h(Px(32.0)).shrink(0.0),
                div().scroll("file-list").flex_1().flex_col().children(rows),
            ])
        }

        fn on_click(&mut self, id: &str) {
            if id == "save" {
                self.saved = Some(self.name.text());
            }
        }
    }

    /// README の「テスト」節が、 **書いてあるとおりに**通ること。
    #[test]
    fn the_testing_section_works_as_written() {
        use testing_section::App;
use sabitori::testing::Harness;

let mut h = Harness::new(App::default(), 800.0, 600.0);
h.frame();                  // build + layout
h.click("name");            // focus the field by id
h.text("hello");            // typed input goes to the focused element
h.click("save");            // now the handler sees the typed value
h.scroll("file-list", 400.0);
h.settle();                 // let springs finish (needed for scroll_intents)
assert_eq!(h.app().saved.as_deref(), Some("hello"));
    }
}

// ===== README [10] tick で動かすなら名乗る =====
mod animating_section {
    use super::*;

    /// README の 5 番目の落とし穴そのもの — `tick` が絵を動かすアプリ。
    #[derive(Default)]
    pub struct Spinner {
        pub t: f32,
    }

    impl DeclarativeApp for Spinner {
        fn view(&self, _ctx: &ViewContext) -> Element {
            div().id("dot").w(Px(10.0)).h(Px(10.0))
        }

fn is_animating(&self) -> bool { true }   // particles, spinners, a clock
fn tick(&mut self, dt: f32) { self.t += dt; }
    }

    /// 名乗ったアプリは `settle` が打ち切れない = 描き続ける側に居る。
    /// README が「止まりません」と言っているのはこの状態。
    #[test]
    fn naming_yourself_animating_keeps_the_runtime_awake() {
        let mut h = Harness::new(Spinner::default(), 200.0, 200.0);
        h.frame();
        assert_eq!(h.settle_for(3), 3, "is_animating が true なら落ち着かない");
        assert!(h.app().t > 0.0, "tick で時間が進んでいる");
    }
}

// ===== README [11] レイアウト =====
mod layout_section {
    use super::*;

    pub fn build() -> Element {
        let sidebar = div().id("sidebar");
        let body = div().id("body");
        let header = div().id("header");
        let (a, b, c) = (div().id("a"), div().id("b"), div().id("c"));

// Flex
let toolbar = div().flex_row().items_center().justify_between().gap(8.0);

// Grid — a fixed sidebar and a body that takes the rest
let shell = grid()
    .grid_cols([Track::px(240.0), Track::fr(1.0)])
    .gap(12.0)
    .children([sidebar, body]);

// A header spanning every column
let sheet = grid()
    .grid_cols(Track::repeat(3, Track::fr(1.0)))
    .children([header.col_span(3), a, b, c]);

        div()
            .w(Px(800.0))
            .h(Px(600.0))
            .flex_col()
            .children([toolbar.h(Px(40.0)), shell.h(Px(200.0)), sheet.h(Px(200.0))])
    }
}

// ===== README [12] ウィジェット =====
mod widget_section {
    use super::*;

    pub struct App {
        pub name: TextInputState,
        pub files: TableState,
        pub tree: TreeNode,
    }

    impl Default for App {
        fn default() -> Self {
            let mut files = TableState::new(vec![
                TableColumn::flex("Name"),
                TableColumn::fixed("Size", 80.0),
            ]);
            files.set_rows(
                (0..20)
                    .map(|i| vec![Cell::text(format!("f{i}")), Cell::text("1 KB")])
                    .collect(),
            );
            Self { name: TextInputState::new("Name"), files, tree: TreeNode::new("root") }
        }
    }

    impl DeclarativeApp for App {
        fn view(&self, ctx: &ViewContext) -> Element {
div().flex_col().children([
    text_input(ctx, "name", &self.name, &TextInputStyle::default_dark()),
    table(ctx, "files", &self.files, &TableStyle::default_dark()),
    tree_view(ctx, "tree", &self.tree, &TreeViewStyle::default_dark()),
])
            .w_full()
            .h_full()
        }
    }
}

// ===== README [13] アクセシビリティ =====
mod a11y_section {
    use super::*;

    pub fn build() -> Element {
let close = div().id("close").role(Role::Button).label("Close");   // icon-only button
let heading = text("Settings").role(Role::Heading).heading(2);

        div().w(Px(200.0)).h(Px(100.0)).flex_col().children([close.h(Px(40.0)), heading])
    }
}

// ===========================================================================
// 挙動 — 「コンパイルが通る」だけでなく、 README が言っているとおりに動くこと
// ===========================================================================

/// `click(ctx, id, handler)` が id の打ち間違い無しに繋がること。
#[test]
fn the_click_form_needs_no_second_place() {
    let mut h = Harness::new(click_form::App::default(), 800.0, 600.0);
    h.frame();
    h.click("save");
    assert!(h.app().saved);
    h.click("row-0");
    assert_eq!(h.app().selected, Some(0));
}

/// `.scroll(id)` を書くだけでホイールが届くこと。 `on_scroll` は実装していない。
#[test]
fn declaring_scroll_is_the_whole_wiring() {
    let mut h = Harness::new(scrolling::App::default(), 800.0, 600.0);
    h.frame();
    h.scroll("file-list", 300.0);
    h.settle();
    assert!(h.scroll_y("file-list").unwrap_or(0.0) > 0.0);
}

/// `scroll_intents` がばねを進めた後に効くこと (`frame()` だけでは動かない)。
#[test]
fn scroll_intents_need_time_to_pass() {
    let mut h = Harness::new(scrolling::App::default(), 800.0, 600.0);
    h.frame();
    h.scroll("file-list", 500.0);
    h.settle();
    h.app_mut().pending = Some(0.0);
    h.frame();
    assert!(h.scroll_y("file-list").unwrap_or(1.0) > 0.0, "まだ時間を進めていない");
    h.settle();
    assert!(h.scroll_y("file-list").unwrap_or(1.0).abs() < 1.0);
}

/// `text_area` が Enter で改行すること。
#[test]
fn the_text_area_takes_newlines() {
    let mut h = Harness::new(multiline::App::default(), 800.0, 600.0);
    h.frame();
    h.click("memo");
    h.text("a");
    h.key(Key::Enter, Modifiers::default());
    h.text("b");
    h.frame();
    assert_eq!(h.app().memo.text(), "a\nb");
}

/// レイアウト節の grid が、 書いてあるとおりに割れること。
#[test]
fn the_layout_section_splits_as_documented() {
    struct A;
    impl DeclarativeApp for A {
        fn view(&self, _ctx: &ViewContext) -> Element {
            layout_section::build()
        }
    }
    let mut h = Harness::new(A, 800.0, 600.0);
    h.frame();
    let r = |id: &str| h.build().region_rect(id).unwrap_or_else(|| panic!("{id} が無い"));
    assert_eq!(r("sidebar").size.width, 240.0);
    assert_eq!(r("header").size.width, 800.0, "col_span(3) が全幅を取る");
}

/// ウィジェット節の 3 つが同じ形 (ctx, id, &state, &style) で並ぶこと。
#[test]
fn the_widget_section_composes() {
    let mut h = Harness::new(widget_section::App::default(), 800.0, 600.0);
    h.frame();
    assert!(h.build().region_rect("name").is_some());
    assert!(h.build().region_rect("files").is_some());
}

/// アクセシビリティ節の役割とラベルがビルド結果に載ること。
#[test]
fn the_a11y_section_reaches_the_build() {
    struct A;
    impl DeclarativeApp for A {
        fn view(&self, _ctx: &ViewContext) -> Element {
            a11y_section::build()
        }
    }
    let mut h = Harness::new(A, 200.0, 100.0);
    h.frame();
    let region = h
        .build()
        .hit_regions
        .iter()
        .find(|r| r.id.as_deref() == Some("close"))
        .expect("close が居ない");
    assert_eq!(region.role, Some(Role::Button));
    assert_eq!(region.label.as_deref(), Some("Close"));
}

// ===========================================================================
// 輪を閉じる 2 つ
// ===========================================================================

/// **README の rust ブロックが、 1 つ残らずこのファイルに逐語で入っていること。**
///
/// これが無いと、 README を直したのにテストは古い形のまま通り続ける。 実際
/// それが起きた — README が `TextInputStyle::default_dark()` を書いている
/// 一方でテストはスタイルを自前に構築しており、 **その関数は存在しなかった**。
///
/// 落ちたら、 README のブロックをこのファイルへ写すこと。 写せない形なら、
/// それはコピペして動かないということなので README 側を直す。
#[test]
fn every_readme_block_appears_here_verbatim() {
    let readme = include_str!("../../../README.md");
    let me = include_str!("readme_examples.rs");
    let blocks = readme_blocks(readme);

    assert!(!blocks.is_empty(), "README から rust ブロックが 1 つも抜けていない");
    let mut missing = Vec::new();
    for b in &blocks {
        if !me.contains(b.as_str()) {
            missing.push(b.clone());
        }
    }
    assert!(
        missing.is_empty(),
        "README.md の {} ブロック中 {} 個がこのファイルに無い:\n\n{}",
        blocks.len(),
        missing.len(),
        missing.join("\n\n---\n\n")
    );
}

/// **日本語版のコードが英語版とずれていないこと。**
///
/// 比べるのは**構造だけ** — コメントは落とし、 文字列リテラルの中身は伏せる。
/// 説明文もラベルも訳されていて当然だが (`"Close"` / `"閉じる"`)、 呼んでいる
/// メソッドや引数の形が違ったら、 **どちらかの README が嘘**になる。
#[test]
fn the_japanese_readme_carries_the_same_code() {
    /// コメントを落とし、 文字列リテラルの中身を `""` に潰す。
    fn code_only(block: &str) -> Vec<String> {
        block
            .lines()
            .map(|l| l.split("//").next().unwrap_or(""))
            .map(|l| {
                let mut out = String::new();
                let mut in_str = false;
                for ch in l.chars() {
                    match ch {
                        '"' => {
                            in_str = !in_str;
                            out.push('"');
                        }
                        _ if in_str => {}
                        _ => out.push(ch),
                    }
                }
                out.trim_end().to_string()
            })
            .filter(|l| !l.trim().is_empty())
            .collect()
    }

    let en = readme_blocks(include_str!("../../../README.md"));
    let ja = readme_blocks(include_str!("../../../README.ja.md"));
    assert_eq!(en.len(), ja.len(), "ブロック数が違う (en {} / ja {})", en.len(), ja.len());

    for (i, (a, b)) in en.iter().zip(&ja).enumerate() {
        assert_eq!(
            code_only(a),
            code_only(b),
            "ブロック {i} のコードが英語版と日本語版でずれている"
        );
    }
}
