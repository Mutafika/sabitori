//! **文字の折り返しに依る崩れを、Harness で止められること** (#91)。
//!
//! 接続できないときのログイン画面で、エラーの赤い文字がカードの外まで伸びて
//! いた。文字に `.min_w(Px(0.0))` が無く、flex の自動の最小幅 (= 1 行の幅) で
//! 押し広げていたため。実機では直すと 1 行 → 3 行に折り返すのに、スタブの
//! Harness は折り返さないので**直す前も後も同じ寸法**を返し、テストで
//! 止められなかった。
//!
//! 実物の計測は入っている書体で寸法が変わるので、px ではなく関係を見る。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_core::element::{div, text, Px};

const MESSAGE: &str = "サーバーに接続できません。ネットワークの設定を確認してから、もう一度ログインしてください。";

struct Login {
    /// 文字に `.min_w(Px(0.0))` を付けるか (= 直した後か)。
    fixed: bool,
}

impl DeclarativeApp for Login {
    fn view(&self, _ctx: &ViewContext) -> Element {
        let mut msg = text(MESSAGE).font_size(13.0);
        if self.fixed {
            msg = msg.min_w(Px(0.0));
        }
        let banner = div().id("banner").w_full().p_px(10.0).flex_row().child(msg);
        let card = div().id("card").w(Px(316.0)).flex_col().child(banner);
        div().w(Px(800.0)).h(Px(600.0)).flex_col().items_center().child(card)
    }
}

fn right(r: Rect) -> f32 {
    r.origin.x + r.size.width
}

/// **直す前: 文字がカードからはみ出していることが見える。**
#[test]
fn text_that_does_not_wrap_is_seen_overflowing_its_card() {
    let mut h = Harness::with_real_text(Login { fixed: false }, 800.0, 600.0);
    h.frame();
    let card = h.rect_of("card").unwrap();
    let msg = h.text_rect("サーバー").expect("文字が描かれていない");
    assert!(
        right(msg) > right(card) + 1.0,
        "折り返していない文字がカードに収まって見える: msg {msg:?} card {card:?}"
    );
}

/// **直した後: 文字は折り返してカードに収まり、帯が行数ぶん伸びる。**
#[test]
fn text_that_wraps_stays_inside_and_grows_its_banner() {
    let mut before = Harness::with_real_text(Login { fixed: false }, 800.0, 600.0);
    before.frame();
    let mut after = Harness::with_real_text(Login { fixed: true }, 800.0, 600.0);
    after.frame();

    let card = after.rect_of("card").unwrap();
    let msg = after.text_rect("サーバー").unwrap();
    assert!(right(msg) <= right(card) + 0.5, "折り返してもはみ出す: {msg:?} {card:?}");

    let one_line = before.text_rect("サーバー").unwrap().size.height;
    assert!(
        msg.size.height >= one_line * 2.0 - 0.5,
        "2 行以上になっていない: {} (1 行 {one_line})",
        msg.size.height
    );
    assert!(
        after.rect_of("banner").unwrap().size.height > before.rect_of("banner").unwrap().size.height,
        "行が増えたのに帯の高さが変わらない"
    );
}

/// スタブは折り返さない — 上の 2 つがスタブでは書けなかった理由。
/// これが落ちたら (スタブが折り返すようになったら) モジュールの doc を直すこと。
#[test]
fn the_stub_does_not_wrap() {
    let mut before = Harness::new(Login { fixed: false }, 800.0, 600.0);
    before.frame();
    let mut after = Harness::new(Login { fixed: true }, 800.0, 600.0);
    after.frame();
    assert_eq!(
        before.rect_of("banner").unwrap().size.height,
        after.rect_of("banner").unwrap().size.height
    );
}
