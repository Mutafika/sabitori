//! README のコード例が**実際にコンパイルできる**ことを固定する。
//!
//! README は Opus がアプリを書くとき最初に読む場所で、 しかも repo の中で
//! いちばん腐りやすい。 0.4.0 より前の README は `**Status**: pre-release
//! (0.1.0)` のままで、 `.scroll()` も `testing::Harness` も role も
//! クリップボードも 1 文字も書かれていなかった — つまり**直した機構が
//! 読まれる場所に無かった**。
//!
//! ここは「文面が最新か」までは見ない。 見るのは **README に載っている形が
//! 今の API で通るか**。 API を壊したら落ちるので、 README を直し忘れて
//! マージすることが無くなる。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_widgets::{
    table, text_input, tree_view, virtual_list, Cell, TableColumn, TableState, TableStyle,
    TextInputState, TextInputStyle, TreeNode, TreeViewStyle,
};

// ---------------------------------------------------------------------------
// 「クイックスタート」
// ---------------------------------------------------------------------------

struct QuickStart {
    clicks: u32,
}

impl DeclarativeApp for QuickStart {
    fn title(&self) -> &str {
        "Hello Sabitori"
    }
    fn size(&self) -> (f32, f32) {
        (800.0, 600.0)
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        div()
            .w(Px(ctx.width))
            .h(Px(ctx.height))
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
        if id == "btn" {
            self.clicks += 1;
        }
    }
}

/// クイックスタートがそのまま動くこと。 `use sabitori::*;` だけで足りることも
/// 併せて確認している (#24 で型を統合するまでは `use sabitori::element::*;` も
/// 要ると書いてあった)。
#[test]
fn the_quick_start_compiles_and_the_button_counts() {
    let mut h = Harness::new(QuickStart { clicks: 0 }, 800.0, 600.0);
    h.frame();

    h.click("btn");

    assert_eq!(h.app().clicks, 1);
}

// ---------------------------------------------------------------------------
// 「よく間違える 4 つ」
// ---------------------------------------------------------------------------

struct FourThings {
    name: TextInputState,
    files: TableState,
    tree: TreeNode,
    lines: Vec<String>,
    pending: Option<f32>,
    saved: Option<String>,
    consumed_keys: bool,
}

const ROW_H: f32 = 28.0;

impl DeclarativeApp for FourThings {
    fn view(&self, ctx: &ViewContext) -> Element {
        // 1. スクロール — `.scroll(id)` を付けるだけ。
        let (first, count) = ctx.visible_range("file-list", ROW_H);
        let last = (first + count).min(self.lines.len());
        let rows: Vec<Element> = (first.min(last)..last)
            .map(|i| div().id(format!("row-{i}")).w_full().h(Px(ROW_H)))
            .collect();
        let scroller = div()
            .scroll("file-list")
            .flex_1()
            .flex_col()
            .children(rows);

        div().flex_col().w_full().h_full().children([
            // 2. テキスト入力
            text_input(ctx, "name", &self.name, &Self::input_style()),
            // ウィジェットは全部「自由関数 (ctx, id, &state, &style)」
            table(ctx, "files", &self.files, &TableStyle::default_dark()),
            tree_view(ctx, "tree", &self.tree, &TreeViewStyle::default_dark()),
            virtual_list(ctx, "log", &self.lines, ROW_H, |line, _i| {
                text(line.clone()).font_size(13.0)
            }),
            scroller,
            // アクセシビリティ
            div().id("close").role(Role::Button).label("閉じる"),
            text("設定").role(Role::Heading).heading(2),
            div().id("save").w(Px(80.0)).h(Px(32.0)),
        ])
    }

    fn on_click(&mut self, id: &str) {
        if id == "save" {
            self.saved = Some(self.name.text());
        }
    }

    // 3. 消費したときだけ true
    fn on_input(&mut self, _event: &InputEvent) -> bool {
        self.consumed_keys
    }

    // テキスト入力の配線は無い。 `text_input(..)` を view() に置いた時点で
    // ランタイムが面倒を見る (README 「テキスト入力と IME」節)。

    // 1. プログラム的スクロール
    fn scroll_intents(&mut self) -> Vec<(String, f32)> {
        self.pending
            .take()
            .map(|y| ("file-list".into(), y))
            .into_iter()
            .collect()
    }

}

impl FourThings {
    fn input_style() -> TextInputStyle {
        TextInputStyle {
            bg: Color::from_hex("#202020"),
            border: Color::from_hex("#404040"),
            text: Color::WHITE,
            placeholder: Color::from_hex("#808080"),
            font_size: 14.0,
            radius: 4.0,
            padding: 8.0,
            focus_border: None,
            caret: None,
            preedit: None,
        }
    }

    fn with_data() -> Self {
        let mut files = TableState::new(vec![
            TableColumn::flex("名前"),
            TableColumn::fixed("サイズ", 80.0),
        ]);
        files.set_rows(
            (0..50)
                .map(|i| vec![Cell::text(format!("f{i}")), Cell::text("1 KB")])
                .collect(),
        );
        Self {
            name: TextInputState::new("名前"),
            files,
            tree: TreeNode::new("root"),
            lines: (0..500).map(|i| format!("line {i}")).collect(),
            pending: None,
            saved: None,
            consumed_keys: false,
        }
    }
}

/// README の「テスト」節に載せた Harness の使い方が、 そのまま通ること。
#[test]
fn the_testing_section_works_as_written() {
    let mut h = Harness::new(FourThings::with_data(), 800.0, 600.0);
    h.frame();
    h.click("name");
    h.text("hello");
    h.frame();
    h.click("save");
    h.scroll("file-list", 400.0);
    h.settle();

    assert_eq!(h.app().saved.as_deref(), Some("hello"));
}

/// 「スクロール」節の形だけを取り出したアプリ。 README のスニペットそのまま。
struct ScrollSnippet {
    lines: Vec<String>,
    pending: Option<f32>,
}

impl DeclarativeApp for ScrollSnippet {
    fn view(&self, ctx: &ViewContext) -> Element {
        let (first, count) = ctx.visible_range("file-list", ROW_H);
        let last = (first + count).min(self.lines.len());
        let first = first.min(last);

        let mut rows: Vec<Element> = Vec::new();
        if first > 0 {
            rows.push(div().h(Px(first as f32 * ROW_H)).shrink(0.0));
        }
        rows.extend(
            (first..last).map(|i| div().id(format!("row-{i}")).w_full().h(Px(ROW_H)).shrink(0.0)),
        );
        let tail = self.lines.len().saturating_sub(last);
        if tail > 0 {
            rows.push(div().h(Px(tail as f32 * ROW_H)).shrink(0.0));
        }

        div()
            .scroll("file-list")
            .flex_1()
            .flex_col()
            .w_full()
            .h_full()
            .children(rows)
    }

    fn scroll_intents(&mut self) -> Vec<(String, f32)> {
        self.pending
            .take()
            .map(|y| ("file-list".into(), y))
            .into_iter()
            .collect()
    }
}

fn scroll_app() -> ScrollSnippet {
    ScrollSnippet {
        lines: (0..500).map(|i| format!("line {i}")).collect(),
        pending: None,
    }
}

/// 「`on_scroll` を実装してはいけない」の裏返し — `.scroll(id)` を書くだけで
/// 実際にホイールが届くこと。
#[test]
fn declaring_scroll_is_the_whole_wiring() {
    let mut h = Harness::new(scroll_app(), 800.0, 600.0);
    h.frame();

    h.scroll("file-list", 500.0);
    h.frame();

    assert!(
        h.scroll_y("file-list").unwrap_or(0.0) > 0.0,
        "`on_scroll` を書かなくてもホイールが届いていること"
    );
}

/// `scroll_intents` が README のとおりの形で効くこと (`settle()` が要ること込み)。
#[test]
fn scroll_intents_work_as_documented() {
    let mut h = Harness::new(scroll_app(), 800.0, 600.0);
    h.frame();
    h.scroll("file-list", 500.0);
    h.frame();
    assert!(h.scroll_y("file-list").unwrap_or(0.0) > 0.0);

    h.app_mut().pending = Some(0.0);
    h.frame();
    h.settle();

    assert!(
        h.scroll_y("file-list").unwrap_or(-1.0) < 1.0,
        "intent で先頭へ戻ること"
    );
}

/// `frame()` だけでは ばねが動かないこと — README が `settle()` を要ると
/// 書いている根拠。 これが崩れたら README の記述のほうを直すこと。
#[test]
fn frame_alone_does_not_advance_springs() {
    let mut h = Harness::new(scroll_app(), 800.0, 600.0);
    h.frame();
    h.scroll("file-list", 500.0);
    h.frame();
    let before = h.scroll_y("file-list").unwrap_or(0.0);

    h.app_mut().pending = Some(0.0);
    h.frame();
    h.frame();

    assert_eq!(
        h.scroll_y("file-list").unwrap_or(0.0),
        before,
        "時間を進めない限り位置は変わらない"
    );
}

/// README 「テキスト入力と IME」節のコードが、書いてあるとおりに動くこと。
///
/// 「`view()` に置く。配線はこれで全部」という記述の根拠。 他のトレイトメソッドを
/// 1 つも実装していないアプリで、打鍵から日本語変換まで通ることを見る。
#[test]
fn the_text_input_section_needs_no_wiring_as_documented() {
    struct App {
        name: TextInputState,
        saved: Option<String>,
    }

    impl DeclarativeApp for App {
        fn view(&self, ctx: &ViewContext) -> Element {
            div().flex_col().w_full().h_full().children([
                text_input(ctx, "name", &self.name, &FourThings::input_style()),
                div().id("save").w(Px(80.0)).h(Px(32.0)),
            ])
        }
        fn on_click(&mut self, id: &str) {
            if id == "save" {
                self.saved = Some(self.name.text());
            }
        }
    }

    let mut h = Harness::new(
        App { name: TextInputState::new("名前"), saved: None },
        400.0,
        300.0,
    );
    h.frame();

    h.click("name");
    h.text("k");
    h.ime_preedit("にほん", None);
    h.ime_commit("日本");
    h.frame();
    h.click("save");

    assert_eq!(h.app().saved.as_deref(), Some("k日本"));
}

/// README 冒頭の「押したら何が起きるかは、押される要素のところに書く」が
/// 書いてあるとおりに動くこと。 一覧で添字を捕まえる形も含む。
#[test]
fn the_click_form_works_as_documented() {
    struct App {
        saved: bool,
        selected: Option<usize>,
    }
    impl DeclarativeApp for App {
        fn view(&self, ctx: &ViewContext) -> Element {
            let rows: Vec<Element> = (0..5)
                .map(|i| {
                    div()
                        .click(ctx, format!("row-{i}"), move |app: &mut App| {
                            app.selected = Some(i)
                        })
                        .w_full()
                        .h(Px(24.0))
                        .shrink(0.0)
                })
                .collect();
            div().flex_col().w_full().h_full().children([
                div()
                    .click(ctx, "save", |app: &mut App| app.saved = true)
                    .w(Px(80.0))
                    .h(Px(32.0)),
                div().flex_col().children(rows),
            ])
        }
    }

    let mut h = Harness::new(App { saved: false, selected: None }, 400.0, 400.0);
    h.frame();

    h.click("save");
    assert!(h.app().saved);

    h.click("row-3");
    assert_eq!(h.app().selected, Some(3));
}
