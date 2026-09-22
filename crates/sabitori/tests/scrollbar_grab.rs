//! **掴める帯が、幕の下で動かないこと。**
//!
//! `.scrollbar_grab` の押しは `press_primary` の**一番先**で食う（帯は中身の上に
//! 重なっているので、後ろへ渡すと掴んだだけで下の物が押される）。その代わり、
//! 手前に幕がある時は譲らなければならない ── menu が開いている間の押しは
//! menu を閉じる物であって、帯を掴む物ではない。
//!
//! 最初の版は `element_index >= OVERLAY_INDEX_BASE` で幕を見ていた。あの帯が
//! 付くのは `overlay_view` (外付け) だけで、`.overlay()` (内側) は普通の連番の
//! まま素通りする。**組み込みの menu / modal / dropdown / context menu / toast は
//! 全部内側**なので、代用ではどれ 1 つ止まらなかった。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_core::element::{div, text, Px};

/// 200 行の一覧。`open` で全画面の幕を内側の `.overlay()` で被せる
/// （組み込みウィジェットと同じ形）。
struct App {
    menu_open: bool,
    external: bool,
}

impl App {
    fn list(&self) -> Element {
        let rows: Vec<Element> =
            (0..200).map(|i| text(format!("row {i}")).font_size(13.0)).collect();
        div()
            .id("list")
            .scroll("list")
            .flex_col()
            .w(Px(400.0))
            .h(Px(300.0))
            .scrollbar(Color::new(0.5, 0.5, 0.5, 1.0))
            .scrollbar_grab(14.0)
            .children(rows)
    }

    fn curtain(id: &str) -> Element {
        div()
            .id(id)
            .pos(0.0, 0.0)
            .w(Px(800.0))
            .h(Px(600.0))
            .bg(Color::new(0.0, 0.0, 0.0, 0.5))
            .child(text("menu"))
    }
}

impl DeclarativeApp for App {
    fn view(&self, _ctx: &ViewContext) -> Element {
        let mut root = div().w(Px(800.0)).h(Px(600.0)).flex_col().child(self.list());
        if self.menu_open && !self.external {
            root = root.child(App::curtain("menu").overlay());
        }
        root
    }

    fn overlay_view(&self, _ctx: &ViewContext) -> Option<Element> {
        (self.menu_open && self.external).then(|| App::curtain("ext-menu"))
    }
}

/// 帯の上（右端から 14px 以内）。
const ON_BAR: (f32, f32) = (396.0, 250.0);

/// **前提: 幕が無ければ掴める。** これが落ちていたら以下は何も言えない。
#[test]
fn a_bar_is_grabbable_when_nothing_is_in_front() {
    let mut h = Harness::new(App { menu_open: false, external: false }, 800.0, 600.0);
    h.frame();
    let before = h.scroll_y("list").unwrap();

    h.press_at(ON_BAR.0, ON_BAR.1);
    h.frame();

    assert_ne!(h.scroll_y("list").unwrap(), before, "帯を掴めていない");
}

/// **内側の `.overlay()` が開いている間は掴まない。**
#[test]
fn a_bar_under_an_inner_overlay_is_not_grabbable() {
    let mut h = Harness::new(App { menu_open: true, external: false }, 800.0, 600.0);
    h.frame();
    let before = h.scroll_y("list").unwrap();

    h.press_at(ON_BAR.0, ON_BAR.1);
    h.frame();

    assert_eq!(
        h.scroll_y("list").unwrap(),
        before,
        "menu が開いているのに帯が掴めた (幕の下で面が動く)"
    );
}

/// **外付け `overlay_view` でも同じ。**
#[test]
fn a_bar_under_an_external_overlay_is_not_grabbable() {
    let mut h = Harness::new(App { menu_open: true, external: true }, 800.0, 600.0);
    h.frame();
    let before = h.scroll_y("list").unwrap();

    h.press_at(ON_BAR.0, ON_BAR.1);
    h.frame();

    assert_eq!(h.scroll_y("list").unwrap(), before, "外付け overlay の下で掴めた");
}

/// **2 つの引き方が同じ答えを出すこと。**
///
/// 押しは `scroll_bars()` から、hover は確保しない `scroll_bar_id_at()` から
/// 引く。ここが食い違うと、**光っていない帯を掴む**（あるいは光っているのに
/// 掴めない）。重なった時にどれを選ぶかまで含めて揃っている必要がある。
#[test]
fn the_two_lookups_agree_on_which_bar_is_under_the_point() {
    let mut h = Harness::new(App { menu_open: false, external: false }, 800.0, 600.0);
    h.frame();
    let build = h.build();

    for y in [0.0, 100.0, 250.0, 299.0, 400.0] {
        for x in [0.0, 200.0, 385.0, 390.0, 396.0, 400.0, 401.0] {
            let via_vec = build
                .scroll_bars()
                .into_iter()
                .find(|b| b.lane_has(x, y))
                .map(|b| b.id);
            let direct = build.scroll_bar_id_at(x, y).map(str::to_owned);
            assert_eq!(via_vec, direct, "({x}, {y}) で食い違った");
        }
    }
}

/// **横の帯も共有の式から出ていること。**
///
/// 縦だけ `scrollbar::thumb` に寄せて横を書き下したままにすると、`MIN_THUMB`
/// を直しても横が付いてこない。
#[test]
fn the_horizontal_bar_comes_from_the_shared_geometry() {
    struct Wide;
    impl DeclarativeApp for Wide {
        fn view(&self, _ctx: &ViewContext) -> Element {
            div()
                .id("strip")
                .scroll("strip")
                .flex_row()
                .w(Px(400.0))
                .h(Px(120.0))
                .scrollbar(Color::new(0.5, 0.5, 0.5, 1.0))
                .children(
                    (0..20)
                        .map(|i| div().id(format!("cell{i}")).w(Px(200.0)).h(Px(80.0)))
                        .collect::<Vec<_>>(),
                )
        }
    }

    let mut h = Harness::new(Wide, 800.0, 600.0);
    h.frame();
    let m = h.build().scroll_measures["strip"].clone();
    let (want_left, want_w) =
        sabitori_core::scrollbar::thumb(m.viewport_width, m.content_width, 0.0);

    // 横の帯 = 高さが BAR_W の矩形。
    let bar = h
        .build()
        .render_list
        .commands
        .iter()
        .filter_map(|c| match c {
            sabitori_core::RenderCommand::Rect(r) => Some(r),
            _ => None,
        })
        .find(|r| (r.rect.size.height - sabitori_core::scrollbar::BAR_W).abs() < 0.01)
        .expect("横の帯が描かれていない");

    assert!(
        (bar.rect.size.width - want_w).abs() < 0.01,
        "つまみの幅が共有の式と違う: {} vs {want_w}",
        bar.rect.size.width
    );
    assert!(
        (bar.rect.origin.x - (m.rect.origin.x + want_left)).abs() < 0.01,
        "つまみの位置が共有の式と違う"
    );
    assert!(
        (bar.rect.origin.y - (m.rect.origin.y + m.viewport_height - sabitori_core::scrollbar::BAR_INSET)).abs() < 0.01,
        "帯の下端が BAR_INSET から出ていない"
    );
}

/// **padding のある枠でも帯が見えること** ([#87])。
///
/// 帯は border box の右端から `BAR_INSET`(6px) 内側に描くのに、クリップは
/// content box (padding を引いた内側) だった。右 padding が 6px より大きいと
/// 帯の x 範囲が丸ごとクリップの外に落ちて、**コマンドは出ているのに 1px も
/// 見えない**。業務画面の本文は `px_pad(Px(28.0))` のような枠なので、素直に
/// 書くとまず当たる。手掛かりは「出ない」以外に無い。
///
/// 見えるかどうかは、帯がその時点で効いているクリップに入っているかで判定する
/// （描画コマンドが在るかだけでは、まさにこのバグを見逃す）。
///
/// [#87]: https://github.com/Mutafika/sabitori/issues/87
#[test]
fn a_padded_scroll_pane_still_shows_its_bar() {
    use sabitori_core::RenderCommand;

    struct Padded {
        pad: f32,
    }
    impl DeclarativeApp for Padded {
        fn view(&self, _ctx: &ViewContext) -> Element {
            div()
                .id("page")
                .scroll("page")
                .flex_col()
                .w(Px(400.0))
                .h(Px(300.0))
                .px_pad(Px(self.pad))
                .py(Px(self.pad))
                .scrollbar(Color::new(1.0, 0.0, 0.0, 1.0))
                .children(
                    (0..200)
                        .map(|i| text(format!("row {i}")).font_size(13.0))
                        .collect::<Vec<_>>(),
                )
        }
    }

    // padding 28 は `BAR_INSET`(6) より大きい = 壊れていた側。
    for pad in [0.0, 6.0, 28.0] {
        let mut h = Harness::new(Padded { pad }, 800.0, 600.0);
        h.frame();

        // 効いているクリップを追いながら、帯（幅 BAR_W の赤い矩形）を探す。
        let mut clips: Vec<sabitori_core::Rect> = Vec::new();
        let mut visible = false;
        for cmd in &h.build().render_list.commands {
            match cmd {
                RenderCommand::PushClip(r) => clips.push(*r),
                RenderCommand::PopClip => {
                    clips.pop();
                }
                RenderCommand::Rect(r)
                    if (r.rect.size.width - sabitori_core::scrollbar::BAR_W).abs() < 0.01
                        && r.fill_color.r > 0.9 =>
                {
                    let inside = clips.iter().all(|c| {
                        r.rect.origin.x >= c.origin.x
                            && r.rect.origin.x + r.rect.size.width
                                <= c.origin.x + c.size.width
                    });
                    visible |= inside;
                }
                _ => {}
            }
        }
        assert!(visible, "padding {pad}: 帯がクリップの外に落ちている (1px も見えない)");
    }
}

/// **`.scale()` を書いた面でも帯の長さと位置が合うこと。**
///
/// `w` / `h` は画面 px (scale 済み)、`max_child_*` と `scroll_*` は taffy の
/// 素の px。混ぜて渡していたので、`scale > 1` の面では「溢れていない」と
/// 見なされて**帯が丸ごと消え**、`scale < 1` では逆に長さが狂っていた。
/// (#87 を直す時に同じ 10 行で見つかったもの。)
#[test]
fn a_scaled_pane_still_gets_a_correctly_sized_bar() {
    use sabitori_core::RenderCommand;

    struct Scaled {
        factor: f32,
    }
    impl DeclarativeApp for Scaled {
        fn view(&self, _ctx: &ViewContext) -> Element {
            div().w(Px(800.0)).h(Px(600.0)).child(
                div()
                    .id("pane")
                    .scroll("pane")
                    .flex_col()
                    .w(Px(400.0))
                    .h(Px(300.0))
                    .scaled(self.factor)
                    .scrollbar(Color::new(1.0, 0.0, 0.0, 1.0))
                    .children(
                        (0..100)
                            .map(|i| div().id(format!("r{i}")).w(Px(200.0)).h(Px(30.0)))
                            .collect::<Vec<_>>(),
                    ),
            )
        }
    }

    for factor in [1.0f32, 2.0, 0.5] {
        let mut h = Harness::new(Scaled { factor }, 800.0, 600.0);
        h.frame();
        let bar = h
            .build()
            .render_list
            .commands
            .iter()
            .filter_map(|c| match c {
                RenderCommand::Rect(r) => Some(r),
                _ => None,
            })
            .find(|r| (r.rect.size.width - sabitori_core::scrollbar::BAR_W).abs() < 0.01
                && r.fill_color.r > 0.9)
            .unwrap_or_else(|| panic!("scale {factor}: 帯が描かれていない"));

        // 枠 300 * factor に対し、中身は 3000 * factor。つまみは 10 分の 1。
        let track = 300.0 * factor;
        let want = (track / (3000.0 * factor) * track).max(sabitori_core::scrollbar::MIN_THUMB);
        assert!(
            (bar.rect.size.height - want).abs() < 0.5,
            "scale {factor}: つまみの高さが {} (期待 {want})",
            bar.rect.size.height
        );
    }
}
