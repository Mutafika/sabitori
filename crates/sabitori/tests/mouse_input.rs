//! #58: マウス入力が OS の作法で届くことを、 ヘッドレスの `Harness` で固定する。
//! クリック回数 / 右ボタン / ホイールの配り順 / 端に達したスクロールの伝播。
//!
//! ここで叩いているのは実ランタイムと同じ入口 (`press_primary` /
//! `press_secondary` / `wheel`) なので、 winit の変換だけが検証の外にある。

use sabitori::testing::Harness;
use sabitori::*;

#[derive(Default)]
struct Rec {
    nested: bool,
    consume_right: bool,
    zoom_on_meta: bool,
    clicks: Vec<String>,
    doubles: Vec<(String, f32, f32)>,
    rights: Vec<(String, f32, f32)>,
    presses: Vec<(Option<MouseButton>, u32)>,
    releases: Vec<Option<MouseButton>>,
    wheels: Vec<(Point, f32, f32, bool, WheelPhase, Modifiers)>,
    scrolled_xy: Vec<(f32, f32)>,
}

fn rows(n: usize) -> Vec<Element> {
    (0..n).map(|_| div().w_full().h(Px(40.0))).collect()
}

impl DeclarativeApp for Rec {
    fn view(&self, _ctx: &ViewContext) -> Element {
        if self.nested {
            // 外側 400x300 (ヘッダ 100 + 内側 150 + フッタ 1000)。 内側の矩形は y 100..250。
            div().w(Px(800.0)).h(Px(600.0)).flex_col().child(
                div().scroll("outer").w(Px(400.0)).h(Px(300.0)).flex_col().children([
                    div().w_full().h(Px(100.0)),
                    div().scroll("inner").w_full().h(Px(150.0)).flex_col().children(rows(20)),
                    div().w_full().h(Px(1000.0)),
                ]),
            )
        } else {
            div().w(Px(800.0)).h(Px(600.0)).flex_col().children([
                button("Open").id("open"),
                button("Other").id("other"),
                div().scroll("list").w(Px(400.0)).h(Px(300.0)).flex_col().children(rows(50)),
            ])
        }
    }
    fn on_click(&mut self, id: &str) {
        self.clicks.push(id.to_owned());
    }
    fn on_double_click(&mut self, id: &str, x: f32, y: f32) {
        self.doubles.push((id.to_owned(), x, y));
    }
    fn on_right_click(&mut self, id: &str, x: f32, y: f32) {
        self.rights.push((id.to_owned(), x, y));
    }
    fn on_scroll_xy(&mut self, dx: f32, dy: f32) {
        self.scrolled_xy.push((dx, dy));
    }
    fn on_input(&mut self, ev: &InputEvent) -> bool {
        match ev {
            InputEvent::PointerPressed { button, click_count, .. } => {
                self.presses.push((*button, *click_count));
                self.consume_right && *button == Some(MouseButton::Right)
            }
            InputEvent::PointerReleased { button, .. } => {
                self.releases.push(*button);
                false
            }
            InputEvent::Wheel { position, delta_x, delta_y, precise, phase, modifiers } => {
                self.wheels
                    .push((*position, *delta_x, *delta_y, *precise, *phase, *modifiers));
                self.zoom_on_meta && modifiers.meta
            }
            _ => false,
        }
    }
}

fn harness(app: Rec) -> Harness<Rec> {
    let mut h = Harness::new(app, 800.0, 600.0);
    h.frame();
    h
}

fn counts(h: &Harness<Rec>) -> Vec<u32> {
    h.app().presses.iter().map(|p| p.1).collect()
}

// ---- クリック回数 ---------------------------------------------------------

/// 同じ要素を続けて 2 回: `on_click` は 2 回、 `on_double_click` は 1 回、
/// `PointerPressed::click_count` は 1, 2。
#[test]
fn second_rapid_click_on_the_same_element_is_a_double_click() {
    let mut h = harness(Rec::default());
    h.double_click("open");
    assert_eq!(h.app().clicks, vec!["open", "open"], "on_click は毎回鳴る (click → dblclick の順)");
    assert_eq!(h.app().doubles.len(), 1);
    assert_eq!(h.app().doubles[0].0, "open");
    assert_eq!(counts(&h), vec![1, 2]);
}

/// 3 打目はダブルクリックではない (回数 3)。 4 打目で再び鳴る (偶数回目)。
#[test]
fn third_click_is_not_a_double_click_but_the_fourth_is() {
    let mut h = harness(Rec::default());
    for _ in 0..3 {
        h.click("open");
    }
    assert_eq!(h.app().doubles.len(), 1);
    assert_eq!(counts(&h), vec![1, 2, 3]);
    h.click("open");
    assert_eq!(h.app().doubles.len(), 2);
}

/// 別の要素を続けて押しても組にならない (位置が離れるので回数も 1 に戻る)。
#[test]
fn clicks_on_different_elements_do_not_pair_up() {
    let mut h = harness(Rec::default());
    h.click("open");
    h.click("other");
    assert!(h.app().doubles.is_empty());
    assert_eq!(counts(&h), vec![1, 1]);
}

/// 空白のダブルクリックは `""` と座標で届く (`on_right_click` と同じ規約)。
/// `on_click` は鳴らない。
#[test]
fn double_click_on_empty_space_reports_an_empty_id_with_the_position() {
    let mut h = harness(Rec::default());
    h.double_click_at(700.0, 500.0);
    assert!(h.app().clicks.is_empty());
    assert_eq!(h.app().doubles, vec![(String::new(), 700.0, 500.0)]);
}

// ---- 右ボタン -------------------------------------------------------------

/// 右ボタンは押下・解放とも `on_input` に届き、 消費されなければ `on_right_click`。
#[test]
fn right_button_reaches_on_input_and_then_on_right_click() {
    let mut h = harness(Rec::default());
    h.right_click("open");
    assert_eq!(h.app().presses, vec![(Some(MouseButton::Right), 1)]);
    assert_eq!(h.app().releases, vec![Some(MouseButton::Right)]);
    assert_eq!(h.app().rights.len(), 1);
    assert_eq!(h.app().rights[0].0, "open");
}

/// 押下を `on_input` で消費したら `on_right_click` は鳴らない (右ドラッグの合図)。
/// 解放は変わらず届く。
#[test]
fn consuming_the_right_press_suppresses_on_right_click() {
    let mut h = harness(Rec { consume_right: true, ..Default::default() });
    h.right_click("open");
    assert!(h.app().rights.is_empty(), "消費したのにコンテキストメニューが開く");
    assert_eq!(h.app().releases, vec![Some(MouseButton::Right)]);
}

#[test]
fn right_click_on_empty_space_reports_an_empty_id() {
    let mut h = harness(Rec::default());
    h.right_click_at(700.0, 500.0);
    assert_eq!(h.app().rights.len(), 1);
    assert_eq!(h.app().rights[0].0, "");
}

// ---- ホイール -------------------------------------------------------------

fn inside(h: &Harness<Rec>, id: &str) -> (f32, f32) {
    let r = h.rect_of(id).expect("見えているはず");
    (r.origin.x + 100.0, r.origin.y + 100.0)
}

/// ホイールは管理コンテナより先に `on_input` へ (位置・精度・位相つき)。
/// 消費しなければ管理コンテナが動き、 `on_scroll_xy` は鳴らない。
#[test]
fn wheel_reaches_on_input_before_the_managed_container() {
    let mut h = harness(Rec::default());
    let (x, y) = inside(&h, "list");
    h.wheel_at(x, y, 0.0, -60.0);
    h.settle();

    let w = &h.app().wheels;
    assert_eq!(w.len(), 1);
    assert_eq!((w[0].0.x, w[0].0.y), (x, y), "カーソル位置が載る");
    assert_eq!((w[0].1, w[0].2), (0.0, -60.0));
    assert!(w[0].3, "Harness のホイールは精密入力");
    assert_eq!(w[0].4, WheelPhase::Moved);
    assert!(h.scroll_y("list").unwrap() > 0.0, "管理コンテナが動いた");
    assert!(h.app().scrolled_xy.is_empty(), "消費されたのに on_scroll_xy が鳴った");
}

/// `on_input` が `true` を返せば管理コンテナは動かない (Cmd+ホイールでズーム)。
#[test]
fn consuming_the_wheel_in_on_input_stops_the_managed_scroll() {
    let mut h = harness(Rec { zoom_on_meta: true, ..Default::default() });
    let (x, y) = inside(&h, "list");
    h.set_modifiers(Modifiers { meta: true, ..Default::default() });
    h.wheel_at(x, y, 0.0, -60.0);
    h.settle();

    assert_eq!(h.app().wheels.len(), 1);
    assert!(h.app().wheels[0].5.meta, "修飾キーが載る");
    assert_eq!(h.scroll_y("list"), Some(0.0), "消費したのにスクロールした");
    assert!(h.app().scrolled_xy.is_empty());
}

/// 管理コンテナの外なら従来どおり `on_scroll_xy` へ。 `on_input` にも届いている。
#[test]
fn wheel_outside_any_container_falls_through_to_on_scroll_xy() {
    let mut h = harness(Rec::default());
    h.wheel_at(700.0, 500.0, 0.0, -60.0);
    assert_eq!(h.app().scrolled_xy, vec![(0.0, -60.0)]);
    assert_eq!(h.app().wheels.len(), 1);
}

/// 刻みホイールは `precise = false` で、 行数 × `LINE_DELTA_PX` の px で届く。
#[test]
fn discrete_wheel_arrives_in_pixels_flagged_as_imprecise() {
    let mut h = harness(Rec::default());
    h.wheel_lines_at(700.0, 500.0, 0.0, -1.0);
    let w = &h.app().wheels;
    assert_eq!(w.len(), 1);
    assert!(!w[0].3);
    assert_eq!(w[0].2, -LINE_DELTA_PX);
}

/// 内側のリストが下端なら、 その上のホイールで外側が動く (以前は内側が飲んで
/// 何も起きなかった)。
#[test]
fn wheel_at_the_end_of_an_inner_list_scrolls_the_outer_one() {
    let mut h = harness(Rec { nested: true, ..Default::default() });
    h.scroll("inner", 100_000.0);
    h.wheel_at(200.0, 175.0, 0.0, -60.0);
    h.settle();
    assert!(h.scroll_y("outer").unwrap() > 0.0, "外側が動いていない");
    assert!(h.app().scrolled_xy.is_empty(), "管理コンテナが消費すべきところでアプリへ落ちた");
}

/// トラックパッドの 1 ジェスチャは届け先を固定する: 内側を端まで払った続きで
/// 外側は動かない。 次のジェスチャで外側へ。
#[test]
fn a_trackpad_gesture_does_not_jump_to_the_outer_list_mid_gesture() {
    let mut h = harness(Rec { nested: true, ..Default::default() });
    h.wheel_phase_at(200.0, 175.0, 0.0, 0.0, WheelPhase::Started);
    h.wheel_phase_at(200.0, 175.0, 0.0, -100_000.0, WheelPhase::Moved);
    h.wheel_phase_at(200.0, 175.0, 0.0, -60.0, WheelPhase::Moved);
    h.settle();
    assert_eq!(h.scroll_y("outer"), Some(0.0), "ジェスチャ中に外側へ跳ねた");

    h.wheel_phase_at(200.0, 175.0, 0.0, 0.0, WheelPhase::Ended);
    h.wheel_phase_at(200.0, 175.0, 0.0, 0.0, WheelPhase::Started);
    h.wheel_phase_at(200.0, 175.0, 0.0, -60.0, WheelPhase::Moved);
    h.settle();
    assert!(h.scroll_y("outer").unwrap() > 0.0, "次のジェスチャで外側が動かない");
}
