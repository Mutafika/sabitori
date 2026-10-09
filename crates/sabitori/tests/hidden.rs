//! **幅で要素を隠す** ([#106](https://github.com/Mutafika/sabitori/issues/106))。
//!
//! sabitori-renta の稼働率の画面「車両別収益」: 固定幅の列だけで 600px ある行で、
//! 狭い窓では「ナンバー」「費用」を隠す (React 版の `hidden md:table-cell`)。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_core::element::{button, div, polyline, text, Px, Track};
use sabitori_core::render_list::RenderCommand;

#[derive(Default)]
struct Revenue {
    clicked: Vec<&'static str>,
}

fn col(id: &str, w: f32, label: &str) -> Element {
    div().id(id).w(Px(w)).h(Px(32.0)).shrink(0.0).child(text(label.to_string()))
}

impl DeclarativeApp for Revenue {
    fn view(&self, ctx: &ViewContext) -> Element {
        div().w_full().h_full().flex_col().children([
            div().id("row").flex_row().children([
                col("plate", 180.0, "品川 500 あ 12-34").at(SizeClass::Compact, |e| e.hidden()),
                col("count", 90.0, "貸出回数"),
                col("sales", 110.0, "売上"),
                col("cost", 110.0, "費用").at(SizeClass::Compact, |e| e.hidden()),
                col("profit", 110.0, "利益"),
            ]),
            // 広い窓でだけ出す補助ボタン。隠している間は押せない。
            button("CSV")
                .click(ctx, "csv", |a: &mut Revenue| a.clicked.push("csv"))
                .w(Px(80.0))
                .h(Px(32.0))
                .hidden()
                .at_least(SizeClass::Medium, |e| e.shown()),
            // 並べ方 (grid) は隠しても覚えている。
            div()
                .id("cards")
                .w(Px(400.0))
                .grid_cols([Track::fr(1.0), Track::fr(1.0)])
                .children([div().id("c1").h(Px(20.0)), div().id("c2").h(Px(20.0))])
                .hidden()
                .at_least(SizeClass::Medium, |e| e.shown()),
            // 線は箱が 0 でも点で描けるので、隠したら中身ごと止める必要がある。
            div()
                .id("trend")
                .child(polyline().points([(0.0, 0.0), (120.0, 40.0)]).stroke_width(2.0))
                .at(SizeClass::Compact, |e| e.hidden()),
        ])
    }
}

fn harness(width: f32) -> Harness<Revenue> {
    let mut h = Harness::new(Revenue::default(), width, 600.0);
    h.settle();
    h
}

#[test]
fn a_narrow_window_drops_the_hidden_columns_and_closes_the_gap() {
    let h = harness(500.0);
    assert_eq!(h.rect_of("plate"), None);
    assert_eq!(h.rect_of("cost"), None);
    assert!(h.text_rect("費用").is_none(), "中身の文字も描かない");
    assert!(h.text_rect("品川").is_none());
    // 残りの列が詰めて並ぶ。
    assert_eq!(h.rect_of("count").unwrap().origin.x, 0.0);
    assert_eq!(h.rect_of("sales").unwrap().origin.x, 90.0);
    assert_eq!(h.rect_of("profit").unwrap().origin.x, 200.0);
    assert!(h.overflows().is_empty(), "{:?}", h.overflows());
}

#[test]
fn a_wide_window_shows_everything() {
    let h = harness(1000.0);
    assert_eq!(h.rect_of("plate").unwrap().size.width, 180.0);
    assert_eq!(h.rect_of("cost").unwrap().origin.x, 380.0);
    assert!(h.text_rect("費用").is_some());
    assert_eq!(h.rect_of("csv").unwrap().size.width, 80.0);
}

#[test]
fn a_hidden_button_takes_no_space_and_cannot_be_pressed() {
    let mut h = harness(500.0);
    assert_eq!(h.rect_of("csv"), None);
    // 大きさ 0 の当たり領域も残さない — 残るとタブで移れてしまう。
    assert!(
        !h.build().hit_regions.iter().any(|r| r.id.as_deref() == Some("csv")),
        "隠したボタンの当たり領域が残っている"
    );
    h.key(Key::Tab, Modifiers::default());
    assert_eq!(h.focused_id(), None, "隠したボタンへタブで移れる");
    // 広い窓なら CSV が居た場所 (行のすぐ下) を押しても何も起きない。
    h.click_at(40.0, 48.0);
    assert!(h.app().clicked.is_empty());

    h.resize(1000.0, 600.0);
    h.settle();
    h.click("csv");
    assert_eq!(h.app().clicked, ["csv"]);
}

fn polylines(h: &Harness<Revenue>) -> usize {
    let b = h.build();
    b.all_commands()
        .filter(|c| matches!(c, RenderCommand::Polyline(_)))
        .count()
}

#[test]
fn a_hidden_chart_draws_no_line() {
    assert_eq!(polylines(&harness(1000.0)), 1);
    assert_eq!(polylines(&harness(500.0)), 0, "隠した枠の中の線が描かれた");
}

#[test]
fn shown_keeps_the_grid() {
    let h = harness(1000.0);
    let (c1, c2) = (h.rect_of("c1").unwrap(), h.rect_of("c2").unwrap());
    assert_eq!(c1.origin.y, c2.origin.y, "2 列のまま");
    assert_eq!(c2.origin.x, 200.0);
    assert_eq!(harness(500.0).rect_of("c1"), None, "中身ごと隠れる");
}
