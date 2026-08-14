//! **クリック処理をその場に書く** (`Element::click`) の固定。
//!
//! ## 何が問題だったか
//!
//! 従来の書き方は `.id("save")` を置いて `DeclarativeApp::on_click` で文字列を
//! 突き合わせるもの。 id を書く場所と受ける場所が離れていて、 型が繋いでいない。
//!
//! ```ignore
//! fn view(..) { div().id("save") }
//! fn on_click(&mut self, id: &str) {
//!     if id == "sav" { self.saved = true; }   // ← タイプミス
//! }
//! ```
//!
//! **コンパイルは通り、 押しても何も起きない。** 0.4.0 で潰し続けたのと同じ形の
//! 失敗が、 いちばん中心の経路に残っていた。
//!
//! `click(ctx, id, handler)` なら文字列が 1 回しか出てこないので、 食い違う場所が
//! 存在しない。

use sabitori::testing::Harness;
use sabitori::*;

// ---------------------------------------------------------------------------
// 基本
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Counter {
    clicks: u32,
    resets: u32,
}

impl DeclarativeApp for Counter {
    fn view(&self, ctx: &ViewContext) -> Element {
        div().flex_col().w_full().h_full().children([
            div()
                .click(ctx, "inc", |app: &mut Counter| app.clicks += 1)
                .w(Px(80.0))
                .h(Px(32.0)),
            div()
                .click(ctx, "reset", |app: &mut Counter| {
                    app.clicks = 0;
                    app.resets += 1;
                })
                .w(Px(80.0))
                .h(Px(32.0)),
        ])
    }
}

/// その場に書いた処理が走ること。
#[test]
fn a_click_handler_written_in_place_runs() {
    let mut h = Harness::new(Counter::default(), 400.0, 300.0);
    h.frame();

    h.click("inc");
    h.click("inc");

    assert_eq!(h.app().clicks, 2);
}

/// 別の要素の処理は走らないこと (id ごとに分かれている)。
#[test]
fn handlers_are_per_element() {
    let mut h = Harness::new(Counter::default(), 400.0, 300.0);
    h.frame();

    h.click("inc");
    h.frame();
    h.click("reset");

    assert_eq!(h.app().clicks, 0, "reset が効いた");
    assert_eq!(h.app().resets, 1);
    assert_eq!(h.app().clicks, 0, "inc は巻き添えにならない");
}

// ---------------------------------------------------------------------------
// 動的な一覧 — 添字を捕まえる
// ---------------------------------------------------------------------------

struct Rows {
    selected: Option<usize>,
    count: usize,
}

impl DeclarativeApp for Rows {
    fn view(&self, ctx: &ViewContext) -> Element {
        let rows: Vec<Element> = (0..self.count)
            .map(|i| {
                div()
                    // 添字は**捕まえる**。 id から切り出して parse しない。
                    .click(ctx, format!("row-{i}"), move |app: &mut Rows| {
                        app.selected = Some(i);
                    })
                    .w_full()
                    .h(Px(24.0))
                    .shrink(0.0)
            })
            .collect();
        div().flex_col().w_full().h_full().children(rows)
    }
}

/// 一覧の各行が、 自分の添字を正しく持つこと。
///
/// 従来は `id.strip_prefix("row-")?.parse()` を書く必要があり、 接頭辞の
/// 打ち間違いも parse 失敗も黙って無視された。
#[test]
fn each_row_carries_its_own_index() {
    let mut h = Harness::new(Rows { selected: None, count: 10 }, 400.0, 400.0);
    h.frame();

    h.click("row-7");
    assert_eq!(h.app().selected, Some(7));

    h.click("row-0");
    assert_eq!(h.app().selected, Some(0));
}

/// 行数が変わっても、 その時点のツリーの処理が使われること。
///
/// 登録は毎フレームやり直されるので、 前フレームの古いハンドラは残らない。
#[test]
fn handlers_are_rebuilt_every_frame() {
    let mut h = Harness::new(Rows { selected: None, count: 3 }, 400.0, 400.0);
    h.frame();
    h.click("row-2");
    assert_eq!(h.app().selected, Some(2));

    // 行を増やす。
    h.app_mut().count = 20;
    h.frame();

    h.click("row-15");
    assert_eq!(h.app().selected, Some(15), "新しい行にも処理が付いている");
}

// ---------------------------------------------------------------------------
// 併用
// ---------------------------------------------------------------------------

/// 従来の `on_click(id)` と混ぜても両方走ること。
///
/// 既存のコードを書き換えなくていい、 という保証。
#[test]
fn the_old_and_new_paths_coexist() {
    #[derive(Default)]
    struct Both {
        from_action: bool,
        from_on_click: bool,
    }
    impl DeclarativeApp for Both {
        fn view(&self, ctx: &ViewContext) -> Element {
            div()
                .click(ctx, "go", |app: &mut Both| app.from_action = true)
                .w(Px(80.0))
                .h(Px(32.0))
        }
        fn on_click(&mut self, id: &str) {
            if id == "go" {
                self.from_on_click = true;
            }
        }
    }

    let mut h = Harness::new(Both::default(), 400.0, 300.0);
    h.frame();
    h.click("go");

    assert!(h.app().from_action, "その場に書いた処理");
    assert!(h.app().from_on_click, "従来のハンドラ");
}

/// `.click` を使った要素は、 スクロールやフォーカスと同じく id を持つこと。
///
/// `click` は id の割り当ても兼ねるので、 `.id()` を別に書く必要は無い。
#[test]
fn click_also_assigns_the_id() {
    let mut h = Harness::new(Counter::default(), 400.0, 300.0);
    h.frame();

    assert!(h.rect_of("inc").is_some(), "id が付いているので矩形が引ける");
    assert!(
        h.visible_ids().iter().any(|id| id == "reset"),
        "hit_regions にも載る"
    );
}
