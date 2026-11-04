//! 自前のタイトルバーに並べる窓のボタン (`titlebar_buttons` と `.window_action(..)`)。
//!
//! ## なぜ要るか
//!
//! `Titlebar::Custom` の窓は信号ボタンを消すので、窓の操作ボタンはアプリが描く。
//! 以前は − □ ✕ の見た目・フルスクリーン中に隠す判定・クリックを
//! `set_minimized` / `set_maximized` / 閉じる旗へ振り分ける処理を、アプリごとに
//! 同じものを書き写していた。ここでは「並べるだけで窓が動く」と「絵も機能も
//! 差し替えられる」を固定する。

use std::cell::RefCell;

use sabitori::render_list::RenderCommand;
use sabitori::testing::Harness;
use sabitori::*;

const BAR_H: f32 = 38.0;

struct Bar {
    /// 帯に並べるボタン。テーマから組むプリセットは `view()` で作る。
    buttons: fn(&AppTheme) -> TitlebarButtons,
    /// アプリに届いた `on_click` の id。
    clicks: RefCell<Vec<String>>,
}

impl Bar {
    fn new(buttons: fn(&AppTheme) -> TitlebarButtons) -> Self {
        Self { buttons, clicks: RefCell::default() }
    }
}

impl DeclarativeApp for Bar {
    fn titlebar(&self) -> Titlebar {
        Titlebar::Custom { height: BAR_H }
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        let buttons = (self.buttons)(&ctx.theme);
        let bar = div()
            .window_drag()
            .w_full()
            .h(Px(BAR_H))
            .flex_row()
            .items_center()
            .children([div().flex_1(), titlebar_buttons(ctx, "win", &buttons)]);
        div().w_full().h_full().flex_col().child(bar)
    }

    fn on_click(&mut self, id: &str) {
        self.clicks.borrow_mut().push(id.to_string());
    }
}

fn windows(theme: &AppTheme) -> TitlebarButtons {
    TitlebarButtons::windows(theme, BAR_H)
}

fn harness(buttons: fn(&AppTheme) -> TitlebarButtons) -> Harness<Bar> {
    let mut h = Harness::new(Bar::new(buttons), 600.0, 400.0);
    h.frame();
    h
}

/// `rect` の中に引かれた線 (線で描く絵) の太さ。
fn strokes_in(h: &Harness<Bar>, id: &str) -> Vec<f32> {
    let r = h.rect_of(id).unwrap_or_else(|| panic!("{id} が描かれていない"));
    h.build()
        .all_commands()
        .filter_map(|c| match c {
            RenderCommand::Polyline(p) if p.points.iter().all(|pt| r.contains(*pt)) => Some(p.width),
            _ => None,
        })
        .collect()
}

/// − □ ✕ は押すだけで窓に掛かる。アプリは窓のハンドルもクリックの振り分けも
/// 持たない。クリック自体はいつも通りアプリへも届く。
#[test]
fn the_windows_preset_drives_the_window_by_itself() {
    let mut h = harness(windows);
    for (key, action) in [
        ("minimize", WindowAction::Minimize),
        ("maximize", WindowAction::ToggleMaximize),
        ("close", WindowAction::Close),
    ] {
        let id = titlebar_button_id("win", key);
        h.click(&id);
        assert_eq!(h.take_window_action(), Some(action), "{id}");
        assert_eq!(h.take_window_gesture(), None, "{id} を押したのに窓をつかんだ");
        assert_eq!(h.app().clicks.borrow().last().map(String::as_str), Some(id.as_str()));
        h.frame();
    }
    // 右端に、左から − □ ✕ の順で並ぶ。
    let x = |key| h.rect_of(&titlebar_button_id("win", key)).unwrap().origin.x;
    assert!(x("minimize") < x("maximize") && x("maximize") < x("close"));
    let close = h.rect_of("win:close").unwrap();
    assert_eq!(close.origin.x + close.size.width, 600.0, "右端に付いていない");
    assert_eq!(close.size.height, BAR_H, "帯の高さいっぱいでない");
}

/// ボタンの間 (帯の空いた所) は今まで通り窓のつかみどころ。
#[test]
fn the_bar_around_the_buttons_still_drags_the_window() {
    let mut h = harness(windows);
    h.click_at(100.0, BAR_H * 0.5);
    assert_eq!(h.take_window_gesture(), Some(WindowGesture::Drag));
    assert_eq!(h.take_window_action(), None);
}

/// 機能は自由: 足したボタンはアプリが `on_click` で受け、窓には何もしない。
/// 既にあるボタンの機能も差し替えられる。
#[test]
fn buttons_can_be_added_and_rewired() {
    let mut h = harness(|theme| {
        TitlebarButtons::windows(theme, BAR_H)
            .insert(0, TitlebarButton::new("pin", TitlebarIcon::glyph("📌")))
            // □ を「フルスクリーン」に付け替える
            .edit("maximize", |b| b.action(WindowAction::ToggleFullscreen))
            .remove("minimize")
    });
    h.click("win:pin");
    assert_eq!(h.take_window_action(), None, "アプリのボタンが窓を触った");
    assert_eq!(h.app().clicks.borrow().as_slice(), ["win:pin"]);

    h.frame();
    h.click("win:maximize");
    assert_eq!(h.take_window_action(), Some(WindowAction::ToggleFullscreen));
    assert!(h.rect_of("win:minimize").is_none(), "外したボタンが残っている");
    let pin = h.rect_of("win:pin").unwrap();
    let max = h.rect_of("win:maximize").unwrap();
    assert!(pin.origin.x < max.origin.x, "差し込んだ所に居ない");
}

/// 拡大中の □ は「元に戻す」(重なった □ = 線 2 本) に変わる。OS 側の操作
/// (ダブルクリック等) で拡大しても `ctx.window_state` で追従する。
#[test]
fn maximize_turns_into_restore_while_maximized() {
    let mut h = harness(windows);
    assert_eq!(strokes_in(&h, "win:maximize").len(), 1, "□ は 1 筆");
    let mut state = WindowState::default();
    state.maximized = true;
    h.set_window_state(state);
    h.frame();
    assert_eq!(strokes_in(&h, "win:maximize").len(), 2, "元に戻すは □ 2 つ");
}

/// フルスクリーン中、macOS では最小化・拡大・閉じるを隠す (OS の帯に信号ボタンが
/// 戻るので)。他の OS には OS の帯が無いので隠さない — 隠すと閉じる手段が消える。
/// アプリのボタンはどちらでも残る。
#[test]
fn fullscreen_hides_the_window_controls_only_where_the_os_brings_them_back() {
    let mut h = harness(|theme| {
        TitlebarButtons::windows(theme, BAR_H)
            .push(TitlebarButton::new("pin", TitlebarIcon::glyph("📌")))
    });
    let mut state = WindowState::default();
    state.fullscreen = true;
    h.set_window_state(state);
    h.frame();
    let mac = cfg!(target_os = "macos");
    for key in ["minimize", "maximize", "close"] {
        assert_eq!(h.rect_of(&titlebar_button_id("win", key)).is_none(), mac, "{key}");
    }
    assert!(h.rect_of("win:pin").is_some(), "アプリのボタンまで隠れた");
}

/// フルスクリーン中に隠すかは機能で決まる。□ をフルスクリーンに付け替えたら、
/// フルスクリーン中も残る (残らないと、抜けるボタンが消える)。明示すれば上書きできる。
#[test]
fn whether_a_button_hides_in_fullscreen_follows_its_action() {
    let mut h = harness(|theme| {
        TitlebarButtons::windows(theme, BAR_H)
            .edit("maximize", |b| b.action(WindowAction::ToggleFullscreen))
            .push(TitlebarButton::new("pin", TitlebarIcon::glyph("📌")).hide_in_fullscreen(true))
    });
    let mut state = WindowState::default();
    state.fullscreen = true;
    h.set_window_state(state);
    h.frame();
    assert!(h.rect_of("win:maximize").is_some(), "フルスクリーンを抜けるボタンが隠れた");
    assert!(h.rect_of("win:pin").is_none(), "明示した隠すが効いていない");
    h.click("win:maximize");
    assert_eq!(h.take_window_action(), Some(WindowAction::ToggleFullscreen));
}

/// 信号の丸は、どれかに指が乗っている間だけ絵を出す。線の太さは座標系
/// (`view_box`) によらず `stroke_width` px。
#[test]
fn traffic_lights_show_their_icons_on_hover() {
    let mut h = harness(|_| TitlebarButtons::traffic_lights());
    assert!(strokes_in(&h, "win:close").is_empty(), "指が乗る前から絵が出ている");
    let r = h.rect_of("win:minimize").unwrap();
    h.move_to(r.origin.x + r.size.width * 0.5, r.origin.y + r.size.height * 0.5);
    h.frame();
    // 乗っているのは黄だけでも、3 つとも絵を出す (macOS と同じ)。
    for key in ["close", "minimize", "fullscreen"] {
        let widths = strokes_in(&h, &titlebar_button_id("win", key));
        assert!(!widths.is_empty(), "{key} の絵が出ていない");
        let want = TitlebarButtonsStyle::traffic_lights().stroke_width;
        assert!(widths.iter().all(|w| (w - want).abs() < 1e-3), "{key}: {widths:?}");
    }
    h.click("win:fullscreen");
    assert_eq!(h.take_window_action(), Some(WindowAction::ToggleFullscreen));
}

/// 差し替えた SVG の絵も、座標系の大きさによらず同じ太さの線になる
/// (Lucide のような 24 四方の絵をそのまま貼れる)。
#[test]
fn a_swapped_svg_icon_keeps_the_stroke_width() {
    let h = harness(|theme| {
        TitlebarButtons::windows(theme, BAR_H)
            .edit("close", |b| b.icon(TitlebarIcon::svg("M6 6 L18 18 M18 6 L6 18", 24.0)))
    });
    let widths = strokes_in(&h, "win:close");
    assert!(!widths.is_empty(), "差し替えた絵が描かれていない");
    let want = TitlebarButtonsStyle::windows(&AppTheme::default(), BAR_H).stroke_width;
    assert!(widths.iter().all(|w| (w - want).abs() < 1e-3), "{widths:?}");
}
