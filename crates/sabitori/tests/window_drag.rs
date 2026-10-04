//! 窓のつかみどころ (`.window_drag()`) と、中身と一体のタイトルバー
//! (`Titlebar::Unified`)。
//!
//! ## なぜ要るか
//!
//! タイトルバーを中身と一体にすると (Warp・Safari の形)、OS のタイトルバーの
//! 代わりにアプリの帯が窓の上端に来る。帯に「ここをつかめば窓が動く」印が
//! 無いと窓を動かせず、印が強すぎると**帯の中のタブを押しても窓が動く**。
//! どちらに当たったかはランタイムの当たり判定で決めるので、ここで固定する。

use std::cell::Cell;

use sabitori::testing::Harness;
use sabitori::*;

const BAR_H: f32 = 38.0;

#[derive(Default)]
struct Chrome {
    tab_clicks: u32,
    body_clicks: u32,
    /// アプリに届いた `PointerPressed` の数。
    pressed: u32,
    /// 帯の上に手前の層 (menu・modal の幕) を出す。
    scrim: bool,
    /// 直前の `view()` が受け取った `ctx.window_controls`。
    controls: Cell<Option<Rect>>,
}

impl DeclarativeApp for Chrome {
    fn titlebar(&self) -> Titlebar {
        Titlebar::Unified { height: BAR_H }
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        self.controls.set(ctx.window_controls);
        let bar = div()
            .window_drag()
            .w_full()
            .h(Px(BAR_H))
            .flex_row()
            .pl(Px(80.0))
            .child(
                div()
                    .w(Px(100.0))
                    .h(Px(BAR_H))
                    .click(ctx, "tab", |app: &mut Chrome| app.tab_clicks += 1),
            );
        let body = div()
            .w_full()
            .h(Px(200.0))
            .click(ctx, "body", |app: &mut Chrome| app.body_clicks += 1);
        let mut root = div().w_full().h_full().flex_col().children([bar, body]);
        if self.scrim {
            root = root.child(
                div()
                    .overlay()
                    .absolute()
                    .pos(0.0, 0.0)
                    .w(Px(400.0))
                    .h(Px(300.0))
                    .click(ctx, "scrim", |_: &mut Chrome| {}),
            );
        }
        root
    }

    fn on_input(&mut self, event: &InputEvent) -> bool {
        if let InputEvent::PointerPressed { .. } = event {
            self.pressed += 1;
        }
        false
    }
}

/// 帯の空いた所を押すと窓のドラッグになる。押下はアプリにも要素にも届かない
/// (OS のタイトルバーと同じ。ドラッグ中は OS が離しまで引き取るので、届けると
/// 押下だけが届いて離しが来ない形になる)。
#[test]
fn pressing_the_empty_part_of_the_bar_drags_the_window() {
    let mut h = Harness::new(Chrome::default(), 400.0, 300.0);
    h.frame();

    h.click_at(300.0, BAR_H / 2.0);

    assert_eq!(h.take_window_gesture(), Some(WindowGesture::Drag));
    assert_eq!(h.app().pressed, 0, "押下がアプリに届いている");
    assert_eq!(h.app().tab_clicks, 0);
}

/// 帯の中の押せる物 (タブ) は普通に押せて、窓は動かない。
#[test]
fn a_tab_in_the_bar_is_clicked_not_dragged() {
    let mut h = Harness::new(Chrome::default(), 400.0, 300.0);
    h.frame();

    h.click_at(130.0, BAR_H / 2.0);

    assert_eq!(h.take_window_gesture(), None, "タブを押したのに窓が動く");
    assert_eq!(h.app().tab_clicks, 1);
    assert_eq!(h.app().pressed, 1);
}

/// ダブルクリックはダブルクリックの動作 (macOS は拡大 / しまう、他は最大化) を頼む。
#[test]
fn double_clicking_the_bar_asks_for_the_double_click_action() {
    let mut h = Harness::new(Chrome::default(), 400.0, 300.0);
    h.frame();

    h.click_at(300.0, BAR_H / 2.0);
    assert_eq!(h.take_window_gesture(), Some(WindowGesture::Drag), "1 回目は普通につかむ");
    h.click_at(300.0, BAR_H / 2.0);
    assert_eq!(h.take_window_gesture(), Some(WindowGesture::DoubleClick));
}

/// 帯の上に手前の層が出ている間は、そちらが押下を受ける。幕の上から窓を
/// つかめてはいけない。
#[test]
fn an_overlay_over_the_bar_takes_the_press() {
    let mut h = Harness::new(Chrome { scrim: true, ..Default::default() }, 400.0, 300.0);
    h.frame();

    h.click_at(300.0, BAR_H / 2.0);

    assert_eq!(h.take_window_gesture(), None);
}

/// 帯の外 (中身) は今までどおり。
#[test]
fn the_body_below_the_bar_is_untouched() {
    let mut h = Harness::new(Chrome::default(), 400.0, 300.0);
    h.frame();

    h.click_at(200.0, 100.0);

    assert_eq!(h.take_window_gesture(), None);
    assert_eq!(h.app().body_clicks, 1);
}

/// 信号ボタンの場所は、macOS では帯の縦の真ん中・左端から 12px。帯の部品は
/// この右から並べる。macOS 以外はタイトルバーが標準のままなので `None`。
#[test]
fn window_controls_sit_in_the_middle_of_the_bar_on_macos() {
    let mut h = Harness::new(Chrome::default(), 400.0, 300.0);
    h.frame();

    let controls = h.app().controls.get();
    if cfg!(target_os = "macos") {
        let r = controls.expect("macOS なのに信号ボタンの場所が無い");
        assert_eq!((r.origin.x, r.size.width, r.size.height), (12.0, 54.0, 14.0));
        assert_eq!(r.origin.y + r.size.height / 2.0, BAR_H / 2.0, "帯の縦の真ん中でない");
    } else {
        assert_eq!(controls, None);
    }
}
