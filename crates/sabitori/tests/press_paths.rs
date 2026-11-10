//! 押下の判定はマウス・タッチ・別窓で 1 本 (`runtime_shared::press_hit`)。
//! 前はタッチの道だけ、テキスト欄のキャレットが触れた所に来なかった
//! (本文中のリンクをタップしても鳴らなかった件は、文字の当たり箱が描画で
//! 作られる都合で Harness には出ないので、`declarative.rs` の中で確かめる)。

use sabitori::testing::Harness;
use sabitori::*;

struct App {
    field: TextInputState,
    clicked: Vec<String>,
}

impl App {
    fn new() -> Self {
        let field = TextInputState::new("");
        field.set_text("abcdef");
        Self { field, clicked: Vec::new() }
    }
}

impl DeclarativeApp for App {
    fn view(&self, ctx: &ViewContext) -> Element {
        div().w_full().h_full().flex_col().gap(20.0).p(Px(20.0)).children([
            text_input(ctx, "field", &self.field, &TextInputStyle::default_dark()).w(Px(300.0)),
        ])
    }

    fn on_click(&mut self, id: &str) {
        self.clicked.push(id.to_string());
    }

    fn on_focused_input(&mut self, _id: &str, _event: &InputEvent) -> bool {
        false
    }
}

fn open() -> Harness<App> {
    let mut h = Harness::new(App::new(), 500.0, 300.0);
    h.frame();
    h
}

#[test]
fn tapping_a_text_field_puts_the_caret_where_the_finger_touched() {
    let mut h = open();
    let field = h.rect_of("field").expect("欄");
    // 欄の左端 (文字の前) を触る。キャレットが末尾のままなら "abcdefX" になる。
    h.tap_at(field.origin.x + 2.0, field.origin.y + field.size.height / 2.0);
    h.frame();
    assert_eq!(h.focused_id(), Some("field"));
    h.text("X");
    h.frame();
    assert_eq!(h.app().field.text(), "Xabcdef");
}
