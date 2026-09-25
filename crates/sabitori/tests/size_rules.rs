//! **幅で変わるスタイルを、その要素の上に書ける** ([#97](https://github.com/Mutafika/sabitori/issues/97))。
//!
//! rentacar-node の `grid grid-cols-1 md:grid-cols-2` (30 か所) を、`ctx` を部品の
//! 奥まで渡さずに書く。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_core::element::{div, Px, Track};

/// `ctx` を受け取らない部品。幅を知らないまま、幅で形が変わる。
fn field(id: &str) -> Element {
    div().id(id).h(Px(36.0))
}

fn customer_form() -> Element {
    div()
        .id("form")
        .w_full()
        .p_px(16.0)
        .grid_cols([Track::fr(1.0)])
        .gap(8.0)
        .at_least(SizeClass::Medium, |e| e.grid_cols([Track::fr(1.0), Track::fr(1.0)]).gap(16.0))
        .children([field("name"), field("kana"), field("phone"), field("mail")])
}

struct Form;

impl DeclarativeApp for Form {
    fn view(&self, _ctx: &ViewContext) -> Element {
        div().w_full().h_full().flex_col().child(customer_form())
    }

    fn overlay_view(&self, _ctx: &ViewContext) -> Option<Element> {
        Some(
            div()
                .id("sheet")
                .w(Px(300.0))
                .h(Px(100.0))
                .at(SizeClass::Compact, |e| e.w(Px(200.0))),
        )
    }
}

fn harness(width: f32) -> Harness<Form> {
    let mut h = Harness::new(Form, width, 700.0);
    h.settle();
    h
}

#[test]
fn a_wide_window_lays_the_fields_out_in_two_columns() {
    let h = harness(1000.0);
    let (name, kana) = (h.rect_of("name").unwrap(), h.rect_of("kana").unwrap());
    assert_eq!(name.origin.y, kana.origin.y, "横に並ぶ");
    assert_eq!(kana.origin.x - (name.origin.x + name.size.width), 16.0, "間は 16");
}

#[test]
fn a_narrow_window_stacks_them() {
    let h = harness(500.0);
    let (name, kana) = (h.rect_of("name").unwrap(), h.rect_of("kana").unwrap());
    assert_eq!(name.origin.x, kana.origin.x, "縦に積む");
    assert_eq!(kana.origin.y - (name.origin.y + name.size.height), 8.0, "間は 8");
    assert_eq!(name.size.width, 500.0 - 32.0);
}

#[test]
fn resizing_switches_on_the_spot() {
    let mut h = harness(1000.0);
    h.resize(500.0, 700.0);
    h.settle();
    assert_eq!(h.rect_of("name").unwrap().origin.x, h.rect_of("kana").unwrap().origin.x);
    h.resize(1000.0, 700.0);
    h.settle();
    assert_eq!(h.rect_of("name").unwrap().origin.y, h.rect_of("kana").unwrap().origin.y);
}

#[test]
fn the_overlay_tree_follows_the_window_width_too() {
    assert_eq!(harness(1000.0).rect_of("sheet").unwrap().size.width, 300.0);
    assert_eq!(harness(500.0).rect_of("sheet").unwrap().size.width, 200.0);
}
