//! **窓を縮めたときの崩れを、テストで止められること** ([#95](https://github.com/Mutafika/sabitori/issues/95))。
//!
//! sabitori-renta の車両一覧: 検索欄 (伸びる) と状態の切り替え (幅 460 固定) を
//! 横一列に `.wrap()` 無しで並べていたので、狭い窓で切り替えが右へはみ出して
//! いた。描画もレイアウトも正常に終わるので、何も出なかった。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_core::element::{div, Px};

struct Vehicles {
    wrap: bool,
    /// 外付けの木 (`overlay_view`) に、はみ出す中身を出すか。
    menu: bool,
}

impl DeclarativeApp for Vehicles {
    fn view(&self, _ctx: &ViewContext) -> Element {
        let mut filters = div().id("filters").w_full().flex_row().gap(8.0).children([
            div().id("search").grow(1.0).h(Px(32.0)),
            div().id("status").w(Px(460.0)).h(Px(32.0)).shrink(0.0),
        ]);
        if self.wrap {
            filters = filters.wrap();
        }
        div()
            .id("app")
            .w_full()
            .h_full()
            .flex_row()
            .children([
                div().id("sidebar").w(Px(210.0)).h_full().shrink(0.0),
                div().id("main").grow(1.0).p_px(16.0).flex_col().child(filters),
            ])
    }

    fn overlay_view(&self, _ctx: &ViewContext) -> Option<Element> {
        self.menu.then(|| {
            div()
                .id("menu")
                .w(Px(120.0))
                .h(Px(80.0))
                .child(div().id("item").w(Px(200.0)).h(Px(24.0)).shrink(0.0))
        })
    }
}

fn harness(wrap: bool, width: f32) -> Harness<Vehicles> {
    let mut h = Harness::new(Vehicles { wrap, menu: false }, width, 700.0);
    h.settle();
    h
}

#[test]
fn a_wide_window_has_nothing_sticking_out() {
    assert!(harness(false, 1320.0).overflows().is_empty());
}

/// 640px: 本文 = 640 - 210 = 430、padding 16 の内側で絞り込みの行は 398。
/// 伸びる検索欄は 0 まで潰れ、間 8 + 切り替え 460 = 468 で 70px 右へ出る。
#[test]
fn a_narrow_window_reports_the_filter_that_sticks_out() {
    let h = harness(false, 640.0);
    let found: Vec<_> = h.overflows().iter().map(|o| (o.id.clone(), o.by.right)).collect();
    assert_eq!(found, vec![(Some("status".to_string()), 70.0)], "{:?}", h.overflows());
}

#[test]
fn wrapping_the_row_does_not_fix_a_child_wider_than_the_row() {
    // 折り返しても 460 の子は 1 行に収まらない。直すべきは子の幅の方、と教える。
    let h = harness(true, 640.0);
    assert_eq!(h.overflows().len(), 1, "{:?}", h.overflows());
}

#[test]
fn resizing_updates_what_is_reported() {
    let mut h = harness(false, 640.0);
    assert!(!h.overflows().is_empty());
    h.resize(1320.0, 700.0);
    h.settle();
    assert!(h.overflows().is_empty(), "広げたら消える: {:?}", h.overflows());
}

/// `overlay_view` の中身も同じく拾う。
#[test]
fn the_overlay_tree_is_checked_too() {
    let mut h = Harness::new(Vehicles { wrap: false, menu: true }, 1320.0, 700.0);
    h.settle();
    let ids: Vec<_> = h.overflows().iter().map(|o| o.id.clone()).collect();
    assert_eq!(ids, vec![Some("item".to_string())], "{:?}", h.overflows());
}
