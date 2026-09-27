//! **横並びの文字は、幅が足りなければ縮んで「…」になる**
//! ([#109](https://github.com/Mutafika/sabitori/issues/109))。
//!
//! kasane の断片パネル: 1 マス 162px に `costume`・`ajisaifuku`・鍵 (26px)・× (24px)
//! を並べたら、文字が縮まずに × が隣のマスへ押し出され、押せなくなった。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_core::element::{button, div, text, Px};
use sabitori_core::render_list::RenderCommand;

struct Panel;

impl DeclarativeApp for Panel {
    fn view(&self, _ctx: &ViewContext) -> Element {
        let cell = |id: &str, with: bool| {
            let mut kids = vec![
                text("costume").font_size(12.0),
                div().flex_1(),
                text("ajisaifuku").font_size(12.0),
            ];
            if with {
                kids.push(button("L").id(format!("{id}-lock")).px_pad(Px(6.0)).py(Px(3.0)).w(Px(26.0)).h(Px(30.0)));
                kids.push(button("×").id(format!("{id}-x")).px_pad(Px(6.0)).py(Px(3.0)).w(Px(24.0)).h(Px(30.0)));
            }
            div().flex_1().flex_row().gap(2.0).children(vec![
                div().id(id).flex_1().h(Px(30.0)).flex_row().items_center().px_pad(Px(8.0)).gap(6.0).children(kids),
            ])
        };
        let row = div().w(Px(328.0)).flex_row().gap(4.0).children(vec![cell("a", true), cell("b", false)]);
        div().w(Px(400.0)).flex_col().children([
            row,
            // 幅 120 のボタン。文字が折り返して下の行に重なっていた。
            button("Browse all (3751)").id("browse").w(Px(120.0)),
            button("OK").id("ok").w(Px(120.0)),
            div().id("below").h(Px(20.0)),
        ])
    }
}

fn harnesses() -> [(&'static str, Harness<Panel>); 2] {
    let mut stub = Harness::new(Panel, 400.0, 200.0);
    stub.settle();
    let mut real = Harness::with_real_text(Panel, 400.0, 200.0);
    real.settle();
    [("stub", stub), ("real", real)]
}

fn right(r: sabitori_core::Rect) -> f32 {
    r.origin.x + r.size.width
}

#[test]
fn the_close_button_stays_inside_its_cell() {
    for (name, mut h) in harnesses() {
        let a = h.rect_of("a").unwrap();
        let x = h.rect_of("a-x").unwrap();
        assert!(right(x) <= right(a) + 0.5, "{name}: × が行の外 {x:?} / {a:?}");
        assert!(x.size.width >= 20.0, "{name}: × が潰れている {x:?}");
        assert!(right(h.rect_of("a-lock").unwrap()) <= x.origin.x, "{name}: 鍵と × が重なる");
        assert!(h.overflows().is_empty(), "{name}: {:?}", h.overflows());
        h.click("a-x");
    }
}

/// 縮んだ文字は 1 行で「…」の指定で描く (測った 1 行の箱に 2 行描かない)。
#[test]
fn the_squeezed_labels_are_drawn_as_one_line() {
    for (name, h) in harnesses() {
        let lines: Vec<_> = h
            .build()
            .render_list
            .commands
            .iter()
            .filter_map(|c| match c {
                RenderCommand::Text(t) if &*t.content == "ajisaifuku" => Some(t.max_lines),
                _ => None,
            })
            .collect();
        assert_eq!(lines, [Some(1), Some(1)], "{name}");
    }
}

#[test]
fn a_fixed_width_button_keeps_its_label_on_one_line() {
    for (name, h) in harnesses() {
        let browse = h.rect_of("browse").unwrap();
        let below = h.rect_of("below").unwrap();
        assert!(below.origin.y >= browse.origin.y + browse.size.height, "{name}: 下の行に重なる");
        let label = h.text_rect("Browse").unwrap();
        // 1 行のボタン (OK) と同じ高さ = 折れていない。
        assert_eq!(browse.size.height, h.rect_of("ok").unwrap().size.height, "{name}: 2 行に折れている");
        assert_eq!(label.size.width, 120.0 - 32.0, "{name}: padding の内側に切れる");
    }
}

/// 書いたとおりにしたい所は、書けば元の動き。
struct Opted;

impl DeclarativeApp for Opted {
    fn view(&self, _ctx: &ViewContext) -> Element {
        let long = "The quick brown fox jumps over the lazy dog, and then the lazy dog gets up and chases the fox all the way home";
        div().w(Px(200.0)).flex_col().children([
            div().id("r0").flex_row().child(text("x")),
            div().id("r1").flex_row().child(text(long).id("wrap-n").max_lines(2)),
            div().id("r2").flex_row().child(text(long).id("wrap-all").min_w(Px(0.0))),
            div().id("r3").flex_row().child(text(long).id("fixed").w(Px(150.0))),
            div().id("r4").flex_col().child(text(long).id("in-col")),
        ])
    }
}

#[test]
fn writing_a_size_or_line_count_keeps_the_text_wrapping() {
    let mut h = Harness::with_real_text(Opted, 400.0, 400.0);
    h.settle();
    let height = |id: &str| h.rect_of(id).map(|r| r.size.height).unwrap_or_else(|| panic!("{id}"));
    let line = height("r0");
    assert!(height("r1") > line * 1.5 && height("r1") < line * 2.5, "max_lines(2) で 2 行: {}", height("r1"));
    assert!(height("r2") > line * 2.5, "min_w(0) で全部折り返す: {}", height("r2"));
    assert!(height("r3") > line * 2.5, "幅を書けばその幅で折り返す: {}", height("r3"));
    assert!(height("r4") > line * 2.5, "縦並びは今までどおり折り返す: {}", height("r4"));
}

/// 改行を含む文字は切らない — 2 行目から先が消える (コードブロック・メモ)。
/// 浮かせた箱 (`.absolute()`) の中の文も、以前と同じ形で出す。
struct Lines;

impl DeclarativeApp for Lines {
    fn view(&self, _ctx: &ViewContext) -> Element {
        div().w(Px(400.0)).h(Px(400.0)).flex_col().children([
            div().id("one").child(text("x")),
            div().id("three").child(text("first line\nsecond line\nthird line")),
            div()
                .id("pop")
                .absolute()
                .pos(0.0, 200.0)
                .w(Px(160.0))
                .child(text("A popover explains what this button does in a few words").id("pop-text")),
        ])
    }
}

#[test]
fn text_with_newlines_keeps_every_line() {
    let mut h = Harness::with_real_text(Lines, 400.0, 400.0);
    h.settle();
    let one = h.rect_of("one").unwrap().size.height;
    let three = h.rect_of("three").unwrap().size.height;
    assert!(three > one * 2.5, "3 行が 1 行に切れた: {three} (1 行 {one})");
    let drawn = h.build().render_list.commands.iter().find_map(|c| match c {
        RenderCommand::Text(t) if t.content.contains("second") => Some(t.max_lines),
        _ => None,
    });
    assert_eq!(drawn, Some(None));
}

#[test]
fn a_fixed_width_popover_stays_inside_its_box() {
    let mut h = Harness::with_real_text(Lines, 400.0, 400.0);
    h.settle();
    let pop = h.rect_of("pop").unwrap();
    let t = h.text_rect("popover").unwrap();
    assert!(right(t) <= right(pop) + 0.5, "{t:?} / {pop:?}");
}
