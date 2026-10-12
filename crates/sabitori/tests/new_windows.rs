//! `DeclarativeApp::take_new_windows` — 同じプロセスで主窓を増やす。
//!
//! 本物の窓は winit のループ (`Host`) が作るので、ここでは
//! `Harness::take_new_windows` で受け取り、窓ごとに入力も状態も別なことを確かめる。
//! 窓どうしで分け合う物はアプリが `Rc` で渡す、という使い方の例も兼ねる。

use std::cell::Cell;
use std::rc::Rc;

use sabitori::testing::Harness;
use sabitori::{div, text, Element, Px, ViewContext};
use sabitori_widgets::{TextInputState, TextInputStyle};

/// メモを 1 窓に 1 枚。「new」で同じアプリをもう 1 窓開く。押した回数と打った文字は
/// 窓ごと、開いた窓の数は窓どうしで分け合う。
struct Memo {
    name: String,
    clicks: u32,
    body: TextInputState,
    opened: Rc<Cell<u32>>,
    pending: Vec<Memo>,
}

impl Memo {
    fn new(name: String, opened: Rc<Cell<u32>>) -> Self {
        Self {
            name,
            clicks: 0,
            body: TextInputState::new("memo"),
            opened,
            pending: Vec::new(),
        }
    }

    fn style() -> TextInputStyle {
        TextInputStyle {
            bg: sabitori::Color::from_hex("#202020"),
            border: sabitori::Color::from_hex("#404040"),
            text: sabitori::Color::WHITE,
            placeholder: sabitori::Color::from_hex("#808080"),
            font_size: 14.0,
            radius: 4.0,
            padding: 8.0,
            focus_border: None,
            caret: None,
            preedit: None,
            selection: None,
        }
    }
}

impl sabitori::DeclarativeApp for Memo {
    fn view(&self, ctx: &ViewContext) -> Element {
        div().flex_col().w_full().h_full().children([
            div()
                .id("new")
                .w(Px(80.0))
                .h(Px(32.0))
                .child(text("new")),
            div()
                .id("count")
                .w(Px(80.0))
                .h(Px(32.0))
                .child(text(self.clicks.to_string())),
            sabitori_widgets::text_input(ctx, "body", &self.body, &Self::style()),
        ])
    }

    fn on_click(&mut self, id: &str) {
        match id {
            "new" => {
                self.opened.set(self.opened.get() + 1);
                let name = format!("memo {}", self.opened.get());
                self.pending.push(Memo::new(name, self.opened.clone()));
            }
            "count" => self.clicks += 1,
            _ => {}
        }
    }

    fn take_new_windows(&mut self) -> Vec<Self> {
        std::mem::take(&mut self.pending)
    }
}

#[test]
fn a_new_window_is_its_own_app_with_its_own_input() {
    let opened = Rc::new(Cell::new(0));
    let mut first = Harness::new(Memo::new("memo 0".into(), opened.clone()), 400.0, 300.0);
    first.frame();
    assert!(first.take_new_windows().is_empty(), "頼むまでは開かない");

    first.click("new");
    let mut windows = first.take_new_windows();
    assert_eq!(windows.len(), 1);
    assert!(first.take_new_windows().is_empty(), "頼んだ分は 1 度で空になる");
    let mut second = windows.pop().unwrap();
    second.frame();
    assert_eq!(second.app().name, "memo 1");

    // 押した回数・焦点・打った文字は窓ごと
    second.click("count");
    second.click("body");
    second.text("hello");
    assert_eq!(second.app().clicks, 1);
    assert_eq!(first.app().clicks, 0);
    assert_eq!(second.app().body.text(), "hello");
    assert_eq!(first.app().body.text(), "");
    assert_eq!(second.focused_id(), Some("body"));
    assert_eq!(first.focused_id(), None);

    // 分け合う物は、アプリが新しい窓に渡した Rc で
    second.click("new");
    assert_eq!(opened.get(), 2);
    let third = second.take_new_windows().pop().unwrap();
    assert_eq!(third.app().name, "memo 2");
}
