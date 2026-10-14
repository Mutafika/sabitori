//! 別窓 ([`ExtraWindow`](crate::declarative::ExtraWindow)) 1 枚ぶんのポインタの状態 —
//! ホバー、押下、管理スクロール (`.scroll(id)`)、掴める帯 (`.scrollbar_grab`)。
//!
//! 主窓は同じものを `AppState` に直に持っている。判断は同じ部品
//! ([`resolve_hover`] / [`Bars`] / [`WheelLatch`] / `scroll_sync`) を通すので、
//! 別窓だけ違う動きにはならない。GPU 資源を持たないので、窓無しにテストから叩ける。
//!
//! 別窓で**まだ無い**もの: hover の ばね (`transition`)、tooltip (別窓は overlay を
//! 描かない)、ドラッグ、テキスト選択。

use std::collections::HashMap;

use sabitori_core::build::BuildResult;
use sabitori_core::{AppTheme, Cursor, Element, ScrollInfo};
use sabitori_input::WheelPhase;
use sabitori_widgets::ScrollView;

use crate::runtime_shared::{resolve_hover, Bars};
use crate::scroll_sync::{self, WheelLatch, MAX_RELAYOUT_STREAK};

#[derive(Default)]
pub(crate) struct ExtraPointer {
    /// 窓の中のポインタ位置 (論理 px)。
    pub(crate) mouse: (f32, f32),
    /// 指が乗っている要素 (`ViewContext::hovered`)。
    pub(crate) hovered: Option<String>,
    /// 押している要素 (`.active()` の見た目)。離すまで。
    pub(crate) pressed: Option<String>,
    /// 指の下の要素が求めるカーソル。`None` は矢印。
    pub(crate) cursor: Option<Cursor>,
    /// 管理スクロールの状態。主窓の `scroll_states` とは別で、id は窓ごと。
    pub(crate) scroll_states: HashMap<String, ScrollView>,
    bars: Bars,
    wheel_latch: WheelLatch,
    /// 寸法の食い違いで続けて組み直した回数 (主窓の `relayout_streak` と同じ)。
    relayout_streak: u8,
}

impl ExtraPointer {
    /// 指が動いた。見た目が変わる (描き直しが要る) なら `true`。
    pub(crate) fn moved(&mut self, build: Option<&BuildResult>, x: f32, y: f32) -> bool {
        self.mouse = (x, y);
        // 帯を掴んでいる間は帯の話だけ (主窓と同じ — 掴んだまま中身の上を横切る)。
        let (grabbed, repaint) = self.bars.moved(build, &mut self.scroll_states, x, y);
        if grabbed {
            return true;
        }
        let hit = build.map(|b| resolve_hover(b, x, y)).unwrap_or_default();
        let changed = self.hovered != hit.hovered_id;
        self.hovered = hit.hovered_id;
        self.cursor = hit.cursor;
        changed || repaint
    }

    /// 指が窓の外へ出た。ホバーを外す。描き直しが要るなら `true`。
    pub(crate) fn left(&mut self) -> bool {
        // 帯を掴んだまま外へ出ても掴んだまま (離しで放す)。
        if self.bars.held().is_some() {
            return false;
        }
        let (_, bar) = self.bars.moved(None, &mut self.scroll_states, self.mouse.0, self.mouse.1);
        self.cursor = None;
        self.hovered.take().is_some() || bar
    }

    /// 主ボタンの押しで帯を掴んだら `true`。押しはそこで終わり、アプリへ渡さない。
    pub(crate) fn grab_bar(&mut self, build: Option<&BuildResult>) -> bool {
        let (x, y) = self.mouse;
        self.bars.press(build, &mut self.scroll_states, x, y)
    }

    /// 主ボタンの離し。見た目が変わるなら `true`。
    pub(crate) fn release(&mut self) -> bool {
        let held = self.bars.release();
        self.pressed.take().is_some() || held
    }

    /// ホイール。カーソルの下の管理スクロールが受けたら `true` (アプリには渡さない)。
    /// 受け方は主窓と同じ: その向きに動ける内側から、トラックパッドは 1 ジェスチャの
    /// 間だけ届け先を固定、Shift + 刻みホイールは横へ。
    pub(crate) fn wheel(
        &mut self,
        build: Option<&BuildResult>,
        delta: (f32, f32),
        precise: bool,
        phase: WheelPhase,
        shift: bool,
    ) -> bool {
        let Some(build) = build else { return false };
        let (x, y) = self.mouse;
        let (dx, dy) = crate::input_router::shift_wheel_sideways(delta.0, delta.1, precise, shift);
        self.wheel_latch
            .route(build, &mut self.scroll_states, x, y, dx, dy, precise, phase)
    }

    pub(crate) fn advance(&mut self, dt: f32) {
        scroll_sync::tick_all(&mut self.scroll_states, dt);
    }

    /// スクロールのばねが動いている (毎フレーム描き直す)。
    pub(crate) fn animating(&self) -> bool {
        self.scroll_states.values().any(|sv| sv.is_animating())
    }

    /// `ViewContext::scroll_states` に渡す値。
    pub(crate) fn scroll_info(&self) -> HashMap<String, ScrollInfo> {
        self.scroll_states
            .iter()
            .map(|(id, sv)| {
                (
                    id.clone(),
                    ScrollInfo {
                        scroll_x: sv.scroll_x.value(),
                        scroll_y: sv.scroll_y.value(),
                        viewport_width: sv.viewport_width,
                        viewport_height: sv.viewport_height,
                        content_width: sv.content_width,
                        content_height: sv.content_height,
                    },
                )
            })
            .collect()
    }

    /// `view_for` の木を組む前に: hover / active の見た目、管理スクロールの位置、
    /// 帯の色。順番は主窓と同じ (テーマの塗りは状態の見た目より先)。
    pub(crate) fn before_build(&mut self, root: &mut Element, theme: &AppTheme) {
        sabitori_core::element::resolve_theme_tints(root, theme);
        sabitori_core::element::apply_state_styles(root, &self.hovered, &self.pressed);
        scroll_sync::patch_scroll_offsets(root, &mut self.scroll_states);
        scroll_sync::paint_bar_state(root, self.bars.held(), self.bars.hover());
    }

    /// 組んだあと: 測れた寸法を管理スクロールへ。`seen` (`view_for` に渡した寸法) と
    /// 食い違えば、もう 1 枚組む (`true`)。続けては [`MAX_RELAYOUT_STREAK`] 回まで。
    pub(crate) fn after_build(
        &mut self,
        build: &BuildResult,
        seen: &HashMap<String, ScrollInfo>,
    ) -> bool {
        scroll_sync::apply_scroll_measures(build, &mut self.scroll_states);
        if !scroll_sync::measures_moved(seen, &self.scroll_states) {
            self.relayout_streak = 0;
            return false;
        }
        if self.relayout_streak >= MAX_RELAYOUT_STREAK {
            return false;
        }
        self.relayout_streak += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::StubMeasure;
    use sabitori_core::build::build_tree_measured;
    use sabitori_core::{div, text, Color, Dimension, Px};

    const W: f32 = 400.0;
    const H: f32 = 300.0;

    /// `view_for` を組む 1 周 (ランタイムの `redraw_extra` と同じ並び)。
    fn frame(p: &mut ExtraPointer, view: impl Fn(&HashMap<String, ScrollInfo>) -> Element) -> BuildResult {
        let seen = p.scroll_info();
        let mut root = view(&seen);
        p.before_build(&mut root, &AppTheme::default());
        let build = build_tree_measured(&root, W, H, &StubMeasure);
        p.after_build(&build, &seen);
        build
    }

    fn button_view() -> Element {
        div().w(Px(W)).h(Px(H)).child(
            div()
                .id("close")
                .w(Px(40.0))
                .h(Px(40.0))
                .bg(Color::from_hex("#202020"))
                .hover(|s| s.bg(Color::from_hex("#404040")))
                .cursor(Cursor::Pointer),
        )
    }

    fn list_view() -> Element {
        let rows: Vec<Element> = (0..50).map(|i| text(format!("row {i}")).h(Px(20.0))).collect();
        div().w(Px(W)).h(Px(H)).flex_col().child(
            div()
                .scroll("pane")
                .flex_col()
                .h(Dimension::Px(200.0))
                .children(rows),
        )
    }

    #[test]
    fn hovering_a_button_reports_it_and_its_cursor() {
        let mut p = ExtraPointer::default();
        let build = frame(&mut p, |_| button_view());
        assert!(p.moved(Some(&build), 20.0, 20.0), "乗ったら描き直す");
        assert_eq!(p.hovered.as_deref(), Some("close"));
        assert_eq!(p.cursor, Some(Cursor::Pointer));
        assert!(!p.moved(Some(&build), 22.0, 22.0), "乗ったまま動いても描き直さない");
        assert!(p.moved(Some(&build), 200.0, 200.0), "外れたら描き直す");
        assert_eq!(p.hovered, None);
        assert_eq!(p.cursor, None);
    }

    #[test]
    fn the_hover_style_is_folded_in_before_the_build() {
        let mut p = ExtraPointer::default();
        let build = frame(&mut p, |_| button_view());
        p.moved(Some(&build), 20.0, 20.0);
        let mut root = button_view();
        p.before_build(&mut root, &AppTheme::default());
        assert_eq!(root.children[0].style.background, Color::from_hex("#404040"));
    }

    #[test]
    fn leaving_the_window_drops_the_hover() {
        let mut p = ExtraPointer::default();
        let build = frame(&mut p, |_| button_view());
        p.moved(Some(&build), 20.0, 20.0);
        assert!(p.left());
        assert_eq!(p.hovered, None);
        assert_eq!(p.cursor, None);
        assert!(!p.left(), "2 回目は何も変わらない");
    }

    #[test]
    fn a_pressed_element_stays_pressed_until_release() {
        let mut p = ExtraPointer::default();
        let build = frame(&mut p, |_| button_view());
        p.moved(Some(&build), 20.0, 20.0);
        p.pressed = crate::runtime_shared::hit_id_at(&build, 20.0, 20.0);
        assert_eq!(p.pressed.as_deref(), Some("close"));
        assert!(p.release());
        assert_eq!(p.pressed, None);
    }

    #[test]
    fn the_wheel_scrolls_the_container_under_the_pointer() {
        let mut p = ExtraPointer::default();
        let build = frame(&mut p, |_| list_view());
        let pane = &p.scroll_states["pane"];
        assert_eq!(pane.viewport_height, 200.0);
        assert!(pane.content_height > 200.0, "50 行は 200px に入らない");

        p.moved(Some(&build), 100.0, 100.0);
        assert!(p.wheel(Some(&build), (0.0, -120.0), false, WheelPhase::Moved, false));
        for _ in 0..120 {
            p.advance(1.0 / 60.0);
        }
        assert!(!p.animating(), "落ち着く");
        assert_eq!(p.scroll_states["pane"].scroll_y.value(), 120.0);
        // 次に組む木には、その位置が入る
        let seen = p.scroll_info();
        assert_eq!(seen["pane"].scroll_y, 120.0);
    }

    #[test]
    fn the_wheel_outside_any_container_is_left_to_the_app() {
        let mut p = ExtraPointer::default();
        let build = frame(&mut p, |_| list_view());
        p.moved(Some(&build), 100.0, 250.0); // 枠 (高さ 200) の下
        assert!(!p.wheel(Some(&build), (0.0, -120.0), false, WheelPhase::Moved, false));
        assert!(!p.wheel(None, (0.0, -120.0), false, WheelPhase::Moved, false), "まだ組んでいない");
    }

    #[test]
    fn a_changed_extent_asks_for_one_more_build() {
        let mut p = ExtraPointer::default();
        // 1 枚目: view はまだ枠を知らない → 測れたので、もう 1 枚
        let seen = p.scroll_info();
        let mut root = list_view();
        p.before_build(&mut root, &AppTheme::default());
        let build = build_tree_measured(&root, W, H, &StubMeasure);
        assert!(p.after_build(&build, &seen));
        // 2 枚目: 同じ寸法なら要らない
        let seen = p.scroll_info();
        let mut root = list_view();
        p.before_build(&mut root, &AppTheme::default());
        let build = build_tree_measured(&root, W, H, &StubMeasure);
        assert!(!p.after_build(&build, &seen));
    }
}
