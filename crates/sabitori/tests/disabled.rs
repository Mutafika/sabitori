//! 無効状態 (`.disabled(true)`) の固定 (#62)。
//!
//! ## なぜ要るか
//!
//! 「送信中は押せない」をアプリ側で書くと、こうなる:
//!
//! ```ignore
//! if busy { return base.id(id).bg(DISABLED); }   // click を付けない
//! base.click(ctx, id, on)
//! ```
//!
//! フォームが増えるとこの分岐が全ボタンに散り、**1 か所忘れた所が二重送信**に
//! なる。しかも「押せない見た目」と「押しても鳴らない」が別々の手当てなので、
//! 片方だけ書いた所も静かに生まれる。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_input::{Key, Modifiers};

#[derive(Default)]
struct Form {
    saving: bool,
    saves: u32,
    card_clicks: u32,
}

impl DeclarativeApp for Form {
    fn view(&self, ctx: &ViewContext) -> Element {
        div().flex_col().w_full().h_full().children([
            // 無効なボタンを、クリックできる親の中に置く。
            div()
                .click(ctx, "card", |app: &mut Form| app.card_clicks += 1)
                .w(Px(200.0))
                .h(Px(60.0))
                .child(
                    button("保存")
                        .disabled(self.saving)
                        .w(Px(120.0))
                        .h(Px(40.0))
                        .click(ctx, "save", |app: &mut Form| app.saves += 1),
                ),
        ])
    }
}

/// 送信中は鳴らない。フラグを下ろせば鳴る。
#[test]
fn a_disabled_button_does_not_fire_and_recovers() {
    let mut h = Harness::new(Form::default(), 400.0, 300.0);
    h.frame();

    h.click("save");
    assert_eq!(h.app().saves, 1, "有効なうちは鳴る");

    h.app_mut().saving = true;
    h.frame();
    h.click("save");
    h.click("save");
    assert_eq!(h.app().saves, 1, "無効な間は鳴らない");

    h.app_mut().saving = false;
    h.frame();
    h.click("save");
    assert_eq!(h.app().saves, 2, "戻せば鳴る");
}

/// 押下は**吸う**。無効なボタンを押して、下に居る親のクリックが代わりに
/// 鳴ってはいけない (ブラウザと同じ)。ここを素通しにすると、無効化した
/// ボタンを押した人が、意図しない行選択や画面遷移を踏む。
#[test]
fn a_disabled_button_absorbs_the_press() {
    let mut h = Harness::new(Form::default(), 400.0, 300.0);
    h.app_mut().saving = true;
    h.frame();

    h.click("save");

    assert_eq!(h.app().saves, 0);
    assert_eq!(h.app().card_clicks, 0, "親のクリックが代わりに鳴っている");
}

// ---------------------------------------------------------------------------
// フォーカス
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Fields {
    lock_second: bool,
}

impl DeclarativeApp for Fields {
    fn view(&self, _ctx: &ViewContext) -> Element {
        div().flex_col().w_full().h_full().children([
            div().id("a").w(Px(100.0)).h(Px(30.0)).focusable(),
            div()
                .id("b")
                .w(Px(100.0))
                .h(Px(30.0))
                .focusable()
                .disabled(self.lock_second),
            div().id("c").w(Px(100.0)).h(Px(30.0)).focusable(),
        ])
    }
}

/// Tab は無効な欄を飛ばす。
#[test]
fn tab_skips_a_disabled_element() {
    let mut h = Harness::new(Fields::default(), 400.0, 300.0);
    h.frame();

    h.key(Key::Tab, Modifiers::default());
    h.key(Key::Tab, Modifiers::default());
    assert_eq!(h.focused_id(), Some("b"), "有効なら 2 番目に入る");

    let mut h = Harness::new(Fields { lock_second: true }, 400.0, 300.0);
    h.frame();

    h.key(Key::Tab, Modifiers::default());
    h.key(Key::Tab, Modifiers::default());
    assert_eq!(h.focused_id(), Some("c"), "無効な b は飛ばす");
}

/// クリックでもフォーカスは入らない。
#[test]
fn clicking_a_disabled_element_does_not_focus_it() {
    let mut h = Harness::new(Fields { lock_second: true }, 400.0, 300.0);
    h.frame();

    h.click("b");

    assert_eq!(h.focused_id(), None);
}

// ---------------------------------------------------------------------------
// 見た目・カーソル・継承
// ---------------------------------------------------------------------------

struct Looks {
    disabled: bool,
}

impl DeclarativeApp for Looks {
    fn view(&self, _ctx: &ViewContext) -> Element {
        div().flex_col().w_full().h_full().child(
            div()
                .id("tile")
                .w(Px(100.0))
                .h(Px(40.0))
                .bg(Color::linear(0.2, 0.2, 0.2, 1.0))
                .cursor(Cursor::Pointer)
                .hover(|s| s.bg(Color::linear(0.9, 0.9, 0.9, 1.0)))
                .disabled_style(|s| s.bg(Color::linear(0.5, 0.0, 0.0, 1.0)))
                .disabled(self.disabled),
        )
    }
}

fn tile_bg(h: &Harness<Looks>) -> Color {
    h.build()
        .render_list
        .rects()
        .find(|r| r.rect.size.width == 100.0 && r.rect.size.height == 40.0)
        .expect("tile")
        .fill_color
}

/// 無効な要素に hover のスタイルは当たらない。押せないものが押せそうに
/// 見えるのが一番悪い。代わりに `disabled_style` が当たる。
#[test]
fn hover_does_not_light_up_a_disabled_element() {
    let mut h = Harness::new(Looks { disabled: false }, 400.0, 300.0);
    h.frame();
    let c = h.rect_of("tile").expect("tile").center();
    h.move_to(c.x, c.y);
    h.frame();
    assert_eq!(tile_bg(&h).r, 0.9, "有効なら hover が当たる");

    let mut h = Harness::new(Looks { disabled: true }, 400.0, 300.0);
    h.frame();
    let c = h.rect_of("tile").expect("tile").center();
    h.move_to(c.x, c.y);
    h.frame();
    let bg = tile_bg(&h);
    assert_eq!((bg.r, bg.g), (0.5, 0.0), "無効なら disabled_style");
}

/// カーソルは NotAllowed。明示した `.cursor(Pointer)` より優先する。
#[test]
fn a_disabled_element_shows_the_not_allowed_cursor() {
    let mut h = Harness::new(Looks { disabled: true }, 400.0, 300.0);
    h.frame();

    let region = h
        .build()
        .hit_regions
        .iter()
        .find(|r| r.id.as_deref() == Some("tile"))
        .expect("tile");
    assert_eq!(region.cursor, Some(Cursor::NotAllowed));
    assert!(region.disabled);
    assert!(!region.focusable);
}

// ---------------------------------------------------------------------------
// 継承 — 送信中にフォームごと止める
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Nested {
    busy: bool,
    inner: u32,
}

impl DeclarativeApp for Nested {
    fn view(&self, ctx: &ViewContext) -> Element {
        div()
            .flex_col()
            .w_full()
            .h_full()
            .disabled(self.busy)
            .child(
                div()
                    .w(Px(100.0))
                    .h(Px(40.0))
                    .click(ctx, "inner", |app: &mut Nested| app.inner += 1),
            )
    }
}

/// 無効な入れ物の中身も無効 (`<fieldset disabled>` と同じ)。フォームごと
/// 止められないと、結局ボタン 1 個ずつに書くことになる。
#[test]
fn disabling_a_container_disables_what_is_inside_it() {
    let mut h = Harness::new(Nested::default(), 400.0, 300.0);
    h.frame();
    h.click("inner");
    assert_eq!(h.app().inner, 1);

    h.app_mut().busy = true;
    h.frame();
    h.click("inner");
    assert_eq!(h.app().inner, 1, "入れ物ごと無効なら中身も鳴らない");
}

// ---------------------------------------------------------------------------
// button() の既定の薄さ
// ---------------------------------------------------------------------------

struct Buttons {
    disabled: bool,
}

impl DeclarativeApp for Buttons {
    fn view(&self, _ctx: &ViewContext) -> Element {
        div().flex_col().w_full().h_full().child(
            button("保存")
                .id("save")
                .w(Px(120.0))
                .h(Px(40.0))
                .bg(Color::linear(0.2, 0.4, 0.8, 1.0))
                .disabled(self.disabled),
        )
    }
}

fn save_alpha(h: &Harness<Buttons>) -> f32 {
    h.build()
        .render_list
        .rects()
        .find(|r| r.rect.size.width == 120.0)
        .expect("save")
        .fill_color
        .a
}

/// `button()` は無効になると既定で薄くなる。何も書かなくても「押せない」
/// ことが見て分かること — 見た目の手当てを毎回書かせると、書き忘れた所が
/// 「押せるのに鳴らないボタン」になる。
#[test]
fn a_disabled_button_dims_itself_without_being_asked() {
    let mut h = Harness::new(Buttons { disabled: false }, 400.0, 300.0);
    h.frame();
    let full = save_alpha(&h);
    assert_eq!(full, 1.0);

    let mut h = Harness::new(Buttons { disabled: true }, 400.0, 300.0);
    h.frame();
    let dim = save_alpha(&h);
    assert!(dim < full, "無効なボタンが薄くなっていない (alpha {dim})");
}
