//! `DeclarativeApp::scroll_shifts` — スクロール位置をその場で差分ずらす口。
//!
//! 仮想化したリストで画面より上の行の高さが実測で変わったとき、読んでいる行を
//! 動かさないために使う。`scroll_intents` では代わりにならない理由が 2 つあり、
//! それぞれをここで固定する:
//!
//! 1. `view()` の**前**に効く（intents はレイアウトの後 = 次の 1 枚にずれが出る）
//! 2. 進行中のばね・慣性を**止めない**（intents は目標を置き直す）

use std::cell::Cell;

use sabitori::testing::Harness;
use sabitori::{div, DeclarativeApp, Element, Px, ScrollIntent, ScrollShift, ViewContext};

struct List {
    shifts: Vec<ScrollShift>,
    intents: Vec<ScrollIntent>,
    /// `view()` が見た `scroll_y`。ずらしが view より前に当たったかを見る。
    seen: Cell<f32>,
}

impl List {
    fn new() -> Self {
        Self { shifts: Vec::new(), intents: Vec::new(), seen: Cell::new(-1.0) }
    }
}

impl DeclarativeApp for List {
    fn view(&self, ctx: &ViewContext) -> Element {
        self.seen.set(ctx.scroll_info("list").map(|s| s.scroll_y).unwrap_or(-1.0));
        let rows: Vec<Element> = (0..100).map(|i| div().id(format!("row{i}")).h(Px(40.0)).shrink(0.0)).collect();
        div().w_full().h_full().flex_col().children([div().scroll("list").flex_1().flex_col().children(rows)])
    }

    fn scroll_shifts(&mut self) -> Vec<ScrollShift> {
        std::mem::take(&mut self.shifts)
    }

    fn scroll_intents(&mut self) -> Vec<ScrollIntent> {
        std::mem::take(&mut self.intents)
    }
}

/// 同じフレームの `view()` がもうずれた位置を見ている。
#[test]
fn a_shift_is_visible_to_the_very_next_view() {
    let mut h = Harness::new(List::new(), 400.0, 300.0);
    h.frame();
    h.scroll("list", 500.0);
    h.frame();
    assert_eq!(h.app().seen.get(), 500.0);

    h.app_mut().shifts.push(ScrollShift::y("list", 30.0));
    h.frame();
    assert_eq!(h.app().seen.get(), 530.0, "ずらしは view の前に当たる");
    assert_eq!(h.scroll_y("list"), Some(530.0));
}

/// ばねで向かっている途中にずらしても、アニメは続き、行き先も同じだけずれる。
/// `ScrollIntent` で同じことをすると目標の置き直し＝速度 0 からやり直しになる。
#[test]
fn a_shift_carries_a_running_spring_along() {
    let mut h = Harness::new(List::new(), 400.0, 300.0);
    h.frame();
    h.app_mut().intents.push(ScrollIntent::y("list", 1000.0));
    h.frame();
    // 途中まで進める（まだ着いていない）。
    for _ in 0..3 {
        h.tick(0.016);
        h.frame();
    }
    let mid = h.scroll_y("list").unwrap();
    assert!(mid > 0.0 && mid < 1000.0, "まだ向かっている途中: {mid}");

    h.app_mut().shifts.push(ScrollShift::y("list", 40.0));
    h.frame();
    assert!((h.scroll_y("list").unwrap() - (mid + 40.0)).abs() < 0.01, "値はその場で 40 ずれる");

    h.settle();
    assert!((h.scroll_y("list").unwrap() - 1040.0).abs() < 0.5, "行き先も 40 ずれている: {:?}", h.scroll_y("list"));
}

/// 上端より上へはずらさない（負の位置はゴム引きの領域）。
#[test]
fn a_shift_stops_at_the_top() {
    let mut h = Harness::new(List::new(), 400.0, 300.0);
    h.frame();
    h.scroll("list", 20.0);
    h.frame();
    h.app_mut().shifts.push(ScrollShift::y("list", -100.0));
    h.frame();
    assert_eq!(h.scroll_y("list"), Some(0.0));
}
