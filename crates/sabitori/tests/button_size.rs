//! **ボタンの大きさは「ラベル + padding」で、ラベルは箱の中央に描かれる**
//! ([#112](https://github.com/Mutafika/sabitori/issues/112))。
//!
//! 測る物があるとき、ボタンの padding が 2 回数えられていた。`measure_text_leaf`
//! が「ラベル + padding」を返し、taffy がその外にもう一度ノードの padding を
//! 足すため (taffy の葉は measure の戻り値を content-box として扱う)。14px の
//! `button("カード追加")` が 36px ではなく 52px、横並びの `button("JSON")` が
//! 左右 32px ずつ太っていた。
//!
//! 描く側は、ラベルを内側の箱の左上に置いていた。ボタンが中身より大きい
//! (`.w_full()`・`.h()`・stretch) とラベルが左上に寄る。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_core::element::{button, div, text, Px, TextAlign};
use sabitori_core::render_list::{RenderCommand, TextDraw};

/// `button()` の既定の padding (縦 8 / 横 16)。
const PAD_Y: f32 = 8.0;
const PAD_X: f32 = 16.0;

/// 同じ文字を `text()` と `button()` で並べる。`text()` の箱 = ラベルだけの大きさ。
struct Pair {
    label: &'static str,
}

impl DeclarativeApp for Pair {
    fn view(&self, _ctx: &ViewContext) -> Element {
        let col = div().w(Px(196.0)).flex_col().items_start().children([
            div().id("t").flex_row().child(text(self.label).font_size(14.0)),
            button(self.label).id("b").font_size(14.0),
        ]);
        div().w(Px(800.0)).h(Px(600.0)).flex_col().child(col)
    }
}

fn label_draw<A: DeclarativeApp>(h: &Harness<A>, needle: &str, nth: usize) -> TextDraw {
    h.build()
        .render_list
        .commands
        .iter()
        .filter_map(|c| match c {
            RenderCommand::Text(t) if t.content.contains(needle) => Some(t.clone()),
            _ => None,
        })
        .nth(nth)
        .unwrap_or_else(|| panic!("{needle:?} の {nth} 番目が描かれていない"))
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() <= 0.5
}

/// **実物の計測: 高さ = 1 行 + 上下の padding、幅 = ラベル + 左右の padding。**
#[test]
fn a_button_is_its_label_plus_its_padding_with_real_text() {
    for label in ["カード追加", "JSON"] {
        let mut h = Harness::with_real_text(Pair { label }, 800.0, 600.0);
        h.frame();
        let t = h.rect_of("t").unwrap().size;
        let b = h.rect_of("b").unwrap().size;
        assert!(
            close(b.height, t.height + PAD_Y * 2.0),
            "{label}: ボタンの高さ {} (1 行 {} + {})",
            b.height,
            t.height,
            PAD_Y * 2.0
        );
        assert!(
            close(b.width, t.width + PAD_X * 2.0),
            "{label}: ボタンの幅 {} (ラベル {} + {})",
            b.width,
            t.width,
            PAD_X * 2.0
        );
        // ラベルを描く箱 (padding の内側) = ラベルの大きさ。
        let draw = label_draw(&h, label, 1);
        assert!(close(draw.max_height, t.height), "{label}: ラベルの箱の高さ {}", draw.max_height);
    }
}

/// **スタブの計測でも同じ** (1 文字 = 7px 幅・1 行 = 14px)。px で書ける。
#[test]
fn a_button_is_its_label_plus_its_padding_with_the_stub() {
    let mut h = Harness::new(Pair { label: "JSON" }, 800.0, 600.0);
    h.frame();
    let b = h.rect_of("b").unwrap().size;
    assert_eq!((b.width, b.height), (4.0 * 7.0 + PAD_X * 2.0, 14.0 + PAD_Y * 2.0));
}

/// 中身より大きいボタン。
struct Wide;

impl DeclarativeApp for Wide {
    fn view(&self, _ctx: &ViewContext) -> Element {
        div().w(Px(800.0)).h(Px(600.0)).flex_col().children([
            div().w(Px(196.0)).flex_col().child(button("カード追加").id("full").font_size(14.0).w_full()),
            button("OK").id("tall").font_size(14.0).w(Px(200.0)).h(Px(60.0)),
        ])
    }
}

fn assert_centered<A: DeclarativeApp>(h: &Harness<A>, id: &str, label: &str, line: f32) {
    let b = h.rect_of(id).unwrap();
    let d = label_draw(h, label, 0);
    assert_eq!(d.typo.align, TextAlign::Center, "{id}: 横に中央揃えで描かれていない");
    // 横: 揃える幅 (padding の内側) がボタンの中心に対して左右対称。
    let b_cx = b.origin.x + b.size.width * 0.5;
    let d_cx = d.position.x + d.max_width * 0.5;
    assert!(close(d_cx, b_cx), "{id}: 横の中心 {d_cx} / ボタン {b_cx}");
    assert!(close(d.max_width, b.size.width - PAD_X * 2.0), "{id}: 揃える幅 {}", d.max_width);
    // 縦: 1 行の中心がボタンの中心。
    let b_cy = b.origin.y + b.size.height * 0.5;
    let d_cy = d.position.y + line * 0.5;
    assert!(close(d_cy, b_cy), "{id}: ラベルの縦の中心 {d_cy} / ボタン {b_cy} ({d:?})");
    // ボタンの外へは出ない。
    assert!(d.position.y + d.max_height <= b.origin.y + b.size.height - PAD_Y + 0.5, "{id}: {d:?}");
}

/// **中身より大きいボタンでも、ラベルは中央 (スタブ)。**
#[test]
fn the_label_is_centered_inside_a_bigger_button_with_the_stub() {
    let mut h = Harness::new(Wide, 800.0, 600.0);
    h.frame();
    assert_eq!(h.rect_of("full").unwrap().size.width, 196.0);
    assert_centered(&h, "full", "カード追加", 14.0);
    assert_centered(&h, "tall", "OK", 14.0);
}

/// **中身より大きいボタンでも、ラベルは中央 (実物の計測)。**
#[test]
fn the_label_is_centered_inside_a_bigger_button_with_real_text() {
    let mut line = Harness::with_real_text(Pair { label: "カード追加" }, 800.0, 600.0);
    line.frame();
    let line = line.rect_of("t").unwrap().size.height;

    let mut h = Harness::with_real_text(Wide, 800.0, 600.0);
    h.frame();
    assert_centered(&h, "full", "カード追加", line);
    assert_centered(&h, "tall", "OK", line);
}

/// 中身どおりの大きさのボタンは、ラベルが padding のすぐ内側 (以前と同じ位置)。
#[test]
fn a_snug_button_draws_its_label_right_inside_the_padding() {
    let mut h = Harness::new(Pair { label: "JSON" }, 800.0, 600.0);
    h.frame();
    let b = h.rect_of("b").unwrap();
    let d = label_draw(&h, "JSON", 1);
    assert_eq!((d.position.x, d.position.y), (b.origin.x + PAD_X, b.origin.y + PAD_Y));
    assert_eq!((d.max_width, d.max_height), (28.0, 14.0));
}
