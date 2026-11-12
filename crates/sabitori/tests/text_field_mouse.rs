//! テキスト欄をマウスで選ぶ: ドラッグで範囲、ダブルクリックで語、トリプルクリックで行。
//!
//! 以前は押した所にキャレットを置くだけで、ドラッグしても選べなかった
//! (⇧+矢印と ⌘A でしか選べなかった)。

use sabitori::testing::Harness;
use sabitori::*;

struct Form {
    subject: TextInputState,
    body: TextInputState,
}

impl DeclarativeApp for Form {
    fn view(&self, ctx: &ViewContext) -> Element {
        let style = TextInputStyle::default_dark();
        div().w_full().h_full().flex_col().gap(12.0).children([
            text_input(ctx, "subject", &self.subject, &style),
            text_area(ctx, "body", &self.body, &style, 6),
        ])
    }
}

fn app() -> Harness<Form> {
    let form = Form {
        subject: TextInputState::with_text("件名", "hello world"),
        body: TextInputState::with_text("本文", "一行目\n二行目\n三行目"),
    };
    let mut h = Harness::new(form, 600.0, 400.0);
    h.frame();
    h
}

/// 前の押下の続き (ダブルクリック) に数えられないよう、間を空ける。
fn pause() {
    std::thread::sleep(MULTI_CLICK_INTERVAL + std::time::Duration::from_millis(100));
}

/// 欄の中の、文字列の先頭からの割合 `t` の所 (縦は欄の真ん中)。
fn at(h: &Harness<Form>, id: &str, needle: &str, t: f32) -> (f32, f32) {
    let field = h.rect_of(id).unwrap();
    let r = h.text_rect(needle).unwrap();
    (r.origin.x + r.size.width * t, field.origin.y + field.size.height / 2.0)
}

#[test]
fn dragging_across_a_field_selects_the_text() {
    let mut h = app();
    let (x0, y) = at(&h, "subject", "hello world", 0.0);
    let field = h.rect_of("subject").unwrap();
    h.press_at(x0 - 2.0, y);
    h.move_to(x0 + 30.0, y);
    // 欄の右の外まで払っても、末尾まで選んで止まる
    h.move_to(field.origin.x + field.size.width + 50.0, y);
    h.release();
    h.frame();
    assert_eq!(h.focused_id(), Some("subject"));
    assert_eq!(h.app().subject.selection_range(), Some((0, "hello world".len())));
}

#[test]
fn a_plain_click_only_places_the_caret() {
    let mut h = app();
    let (x, y) = at(&h, "subject", "hello world", 0.5);
    h.press_at(x, y);
    // 手のぶれ (数 px) は選択にしない
    h.move_to(x + 2.0, y);
    h.release();
    h.frame();
    assert_eq!(h.app().subject.selection_range(), None);
    let caret = h.app().subject.cursor_pos();
    assert!(caret > 0 && caret < "hello world".len(), "押した所に置く: {caret}");
}

#[test]
fn double_click_selects_a_word_and_a_wobble_keeps_it() {
    let mut h = app();
    let (x, y) = at(&h, "subject", "hello world", 0.85);
    h.click_at(x, y);
    h.press_at(x, y);
    h.move_to(x + 2.0, y);
    h.move_to(x + 6.0, y);
    h.release();
    h.frame();
    assert_eq!(h.app().subject.selection_range(), Some((6, 11)), "world を選ぶ");
}

#[test]
fn triple_click_selects_the_line_in_a_text_area() {
    let mut h = app();
    let text = "一行目\n二行目\n三行目";
    // 欄の中ほどを押す。どの行に当たるかは字の測り方しだいなので、まず 1 回押して
    // キャレットの来た行を見ておき、同じ所のトリプルクリックがその行を選ぶかを見る。
    let r = h.text_rect("二行目").unwrap();
    let (x, y) = (r.origin.x + r.size.width * 0.5, r.origin.y + r.size.height * 0.25);
    h.click_at(x, y);
    h.frame();
    let caret = h.app().body.cursor_pos();
    let line_start = text[..caret].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[caret..].find('\n').map_or(text.len(), |i| caret + i);
    pause();
    h.click_at(x, y);
    h.click_at(x, y);
    h.click_at(x, y);
    h.frame();
    let (lo, hi) = h.app().body.selection_range().unwrap();
    assert_eq!((lo, hi), (line_start, line_end), "押した行 {:?}", &text[line_start..line_end]);
    assert!(!text[lo..hi].contains('\n'), "改行は含めない");
}

#[test]
fn a_drag_right_after_focusing_starts_from_where_it_was_pressed() {
    let mut h = app();
    let (x0, y) = at(&h, "subject", "hello world", 0.0);
    // 焦点を当てて、キャレットを末尾へ
    h.click("subject");
    h.key(Key::End, Modifiers::default());
    pause();
    // 押してすぐ (描き直す前に) 動かしても、前のキャレット (末尾) ではなく押した所から選ぶ
    h.press_at(x0 - 2.0, y);
    let (x1, _) = at(&h, "subject", "hello world", 0.4);
    h.move_to(x1, y);
    h.release();
    h.frame();
    let (lo, hi) = h.app().subject.selection_range().unwrap();
    assert_eq!(lo, 0);
    assert!(hi > 0 && hi < "hello world".len(), "押した所から動かした所まで: {hi}");
}
