//! ボタンの hover / active の塗りがテーマから来ること。
//!
//! ## なぜ要るか
//!
//! `button()` の既定は拡大縮小 (1.02 / 0.96) だけで、色は変わらなかった。
//! 地が透明なボタンは、ホバーしてもほとんど分からない。色を固定で入れると
//! テーマや `.accent()` とぶつかるので、ランタイムが `AppTheme` と
//! ボタン自身の地の色から決める (`StateStyle::theme_tint`)。
//!
//! ついでに塞いだ穴: `.accent()` 付きのボタンは描画時に accent が
//! `background` より優先されていたので、`.hover(|s| s.bg(..))` を書いても
//! ホバー色が出なかった。

use sabitori::testing::Harness;
use sabitori::*;

const ACCENT: Color = Color::new(0.2, 0.1, 0.8, 1.0);
const CUSTOM: Color = Color::new(0.9, 0.0, 0.0, 1.0);

enum Kind {
    Plain,
    Accent,
    AccentCustomHover,
    ScaleOnlyHover,
}

struct App {
    kind: Kind,
    theme: AppTheme,
}

impl App {
    fn new(kind: Kind) -> Self {
        Self { kind, theme: AppTheme::default() }
    }
}

impl DeclarativeApp for App {
    fn theme(&self) -> AppTheme {
        self.theme.clone()
    }

    fn view(&self, _ctx: &ViewContext) -> Element {
        let b = button("保存").id("b").w(Px(120.0)).h(Px(40.0));
        let b = match self.kind {
            Kind::Plain => b,
            Kind::Accent => b.accent(ACCENT),
            Kind::AccentCustomHover => b.accent(ACCENT).hover(|s| s.bg(CUSTOM)),
            Kind::ScaleOnlyHover => b.accent(ACCENT).hover(|s| s.scale(1.1)),
        };
        div().w_full().h_full().p(Px(20.0)).child(b)
    }
}

/// ボタンの中心にある、塗りのある一番上の矩形の色。塗りが無ければ `None`。
fn fill(h: &Harness<App>) -> Option<Color> {
    let c = h.rect_of("b").expect("button").center();
    h.build()
        .render_list
        .rects()
        .filter(|r| r.rect.contains(c) && r.fill_color.a > 0.0)
        .last()
        .map(|r| r.fill_color)
}

fn close(a: Color, b: Color) -> bool {
    (a.r - b.r).abs() < 0.01
        && (a.g - b.g).abs() < 0.01
        && (a.b - b.b).abs() < 0.01
        && (a.a - b.a).abs() < 0.01
}

fn hover(h: &mut Harness<App>) {
    let c = h.rect_of("b").expect("button").center();
    h.move_to(c.x, c.y);
    h.frame();
    h.settle();
}

fn start(kind: Kind) -> Harness<App> {
    let mut h = Harness::new(App::new(kind), 400.0, 200.0);
    h.frame();
    h
}

/// 地が透明な素の `button()` は、ホバーで `hover_bg`、押下で `select_bg`。
#[test]
fn a_plain_button_takes_hover_and_select_from_the_theme() {
    let theme = AppTheme::default();
    let mut h = start(Kind::Plain);
    assert_eq!(fill(&h), None, "ホバーしていなければ地は透明のまま");

    hover(&mut h);
    let got = fill(&h).expect("ホバーで塗りが出る");
    assert!(close(got, theme.hover_bg), "{got:?} != hover_bg {:?}", theme.hover_bg);

    let c = h.rect_of("b").unwrap().center();
    h.press_at(c.x, c.y);
    h.frame();
    h.settle();
    let got = fill(&h).expect("押下で塗りが出る");
    assert!(close(got, theme.select_bg), "{got:?} != select_bg {:?}", theme.select_bg);
}

/// 色のあるボタンは、自分の色を文字色の方へ寄せる。暗いテーマでは明るくなる。
#[test]
fn an_accent_button_shifts_its_own_fill_toward_the_text_color() {
    let mut h = start(Kind::Accent);
    assert!(close(fill(&h).unwrap(), ACCENT), "ホバー前は accent そのまま");

    hover(&mut h);
    let got = fill(&h).unwrap();
    let lum = |c: Color| c.r + c.g + c.b;
    assert!(lum(got) > lum(ACCENT), "暗いテーマでは明るくなる: {got:?}");
    assert!(!close(got, AppTheme::default().hover_bg), "accent を捨てて hover_bg にしない");
}

/// 明るいテーマ (文字が暗い) なら、同じ既定で暗くなる。
#[test]
fn on_a_light_theme_the_tint_darkens() {
    let mut app = App::new(Kind::Accent);
    app.theme.text_primary = Color::new(0.05, 0.05, 0.05, 1.0);
    let mut h = Harness::new(app, 400.0, 200.0);
    h.frame();

    hover(&mut h);
    let got = fill(&h).unwrap();
    let lum = |c: Color| c.r + c.g + c.b;
    assert!(lum(got) < lum(ACCENT), "文字が暗いテーマでは暗くなる: {got:?}");
}

/// 回帰: `.accent()` 付きでも、書いたホバー色が出る。
#[test]
fn an_explicit_hover_color_wins_on_an_accent_button() {
    let mut h = start(Kind::AccentCustomHover);
    hover(&mut h);
    let got = fill(&h).unwrap();
    assert!(close(got, CUSTOM), "{got:?} != {CUSTOM:?}");
}

/// `.hover()` を書いたら既定は丸ごと置き換わる — 色を書いていなければ色は変えない。
#[test]
fn a_custom_hover_without_a_color_opts_out_of_the_tint() {
    let mut h = start(Kind::ScaleOnlyHover);
    hover(&mut h);
    let got = fill(&h).unwrap();
    assert!(close(got, ACCENT), "{got:?} != {ACCENT:?}");
}
