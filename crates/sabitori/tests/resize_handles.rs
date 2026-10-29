//! 窓の縁と角の内側の、大きさを変える持ち手 (`DeclarativeApp::resize_handles`)。
//!
//! ## なぜ要るか
//!
//! 中身を窓の上端まで広げた窓は、macOS で縁の外側からつかみにくい (Finder は縁に
//! 吸い付くのに、こちらは内側の細い所でしか効かない)。持ち手を頼んだ窓では、縁の
//! 内側の帯と下の角の内側の四角をランタイムが大きさ変えに使う。持ち手の上では
//! 下の部品はホバーも押下も受けない — 受けると、角をつかんだつもりで一覧の行が
//! 選ばれたり、ボタンが押されたりする。

use sabitori::testing::Harness;
use sabitori::*;

#[derive(Default)]
struct Win {
    handles: bool,
    body_clicks: u32,
    /// アプリに届いた `PointerPressed` の数。
    pressed: u32,
}

impl DeclarativeApp for Win {
    fn titlebar(&self) -> Titlebar {
        Titlebar::Custom { height: 38.0 }
    }

    fn resize_handles(&self) -> Option<ResizeHandles> {
        self.handles.then(ResizeHandles::default)
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        // 窓いっぱいの押せる物 (一覧の地のようなもの)
        div()
            .w_full()
            .h_full()
            .cursor(Cursor::Pointer)
            .click(ctx, "body", |app: &mut Win| app.body_clicks += 1)
    }

    fn on_input(&mut self, event: &InputEvent) -> bool {
        if let InputEvent::PointerPressed { .. } = event {
            self.pressed += 1;
        }
        false
    }
}

fn win(handles: bool) -> Harness<Win> {
    let mut h = Harness::new(
        Win {
            handles,
            ..Win::default()
        },
        400.0,
        300.0,
    );
    h.frame();
    h
}

/// 右下の角の内側 (縁の帯より内側でも、角の四角の中) を押すと、そこから大きさを
/// 変える。押下はアプリにも要素にも届かない。
#[test]
fn pressing_inside_the_bottom_right_corner_resizes_the_window() {
    let mut h = win(true);
    h.click_at(390.0, 290.0);
    assert_eq!(
        h.take_window_gesture(),
        Some(WindowGesture::Resize(ResizeEdge::SouthEast))
    );
    assert_eq!(h.app().pressed, 0, "押下がアプリに届いている");
    assert_eq!(h.app().body_clicks, 0, "角をつかんだのに下の物が押された");
}

/// 縁の細い帯は縁ごとの向き。帯の外は普通に押せる。
#[test]
fn the_edges_are_a_thin_band() {
    let mut h = win(true);
    h.click_at(398.0, 150.0);
    assert_eq!(
        h.take_window_gesture(),
        Some(WindowGesture::Resize(ResizeEdge::East))
    );
    h.click_at(200.0, 1.0);
    assert_eq!(
        h.take_window_gesture(),
        Some(WindowGesture::Resize(ResizeEdge::North))
    );
    h.click_at(390.0, 150.0);
    assert_eq!(h.take_window_gesture(), None, "帯の外なのに大きさ変えになった");
    assert_eq!(h.app().body_clicks, 1);
}

/// 上の角は広げない (帯の部品 — 閉じるボタン等 — が角まで来ることがある)。
#[test]
fn the_top_corners_are_not_widened() {
    let mut h = win(true);
    h.click_at(390.0, 10.0);
    assert_eq!(h.take_window_gesture(), None);
    assert_eq!(h.app().body_clicks, 1);
}

/// 持ち手の上ではカーソルが大きさ変えの矢印になり、下の物はホバーしない。
#[test]
fn hovering_a_handle_shows_the_resize_cursor_not_the_content() {
    let mut h = win(true);
    h.move_to(390.0, 290.0);
    h.frame();
    assert_eq!(h.hovered_id(), None, "持ち手の上で下の物がホバーした");
    h.move_to(200.0, 150.0);
    h.frame();
    assert_eq!(h.hovered_id(), Some("body"));
}

/// 頼まなければ置かない (既定)。縁も角も普通に押せる。
#[test]
fn without_handles_the_edges_are_plain_content() {
    let mut h = win(false);
    h.click_at(390.0, 290.0);
    h.click_at(398.0, 150.0);
    assert_eq!(h.take_window_gesture(), None);
    assert_eq!(h.app().body_clicks, 2);
}
