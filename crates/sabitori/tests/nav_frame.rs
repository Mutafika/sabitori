//! **左にナビ + 右に本文の枠組みが、窓の幅で形を変える**
//! ([#98](https://github.com/Mutafika/sabitori/issues/98))。
//!
//! sabitori-renta (業務 46 画面) はサイドバー 210px 固定で、820px の窓では本文が
//! 600px を切っていた。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_core::element::{div, text, Px};
use sabitori_core::render_list::RenderCommand;
use sabitori_widgets::{
    nav_frame, nav_frame_with, nav_item_id, nav_menu_button_id, NavFrameState, NavFrameStyle, NavGroup, NavItem,
    NavSlots,
};

struct Renta {
    page: String,
    nav: NavFrameState,
}

impl Renta {
    fn new() -> Self {
        Self { page: "dispatch".into(), nav: NavFrameState::new() }
    }
}

fn groups() -> Vec<NavGroup> {
    vec![
        NavGroup::new("業務")
            .item(NavItem::new("dispatch", "配車表").icon("▦").short("配車"))
            .item(NavItem::new("vehicles", "車両一覧").icon("◎").short("車両"))
            .item(NavItem::new("customers", "顧客").icon("☺")),
        NavGroup::new("請求").item(NavItem::new("invoices", "請求書").icon("¥")),
    ]
}

impl DeclarativeApp for Renta {
    fn view(&self, ctx: &ViewContext) -> Element {
        let page = div()
            .id("page")
            .w_full()
            .h_full()
            .p_px(16.0)
            .child(text(format!("page:{}", self.page)).id("page-title"));
        nav_frame(
            ctx,
            "nav",
            &self.nav,
            &NavFrameStyle::from_theme(&ctx.theme),
            &groups(),
            &self.page,
            |app: &mut Renta, id| app.page = id.to_string(),
            page,
        )
    }
}

fn at(width: f32) -> Harness<Renta> {
    let mut h = Harness::new(Renta::new(), width, 700.0);
    h.settle();
    h
}

#[test]
fn a_wide_window_gets_the_fixed_sidebar() {
    let h = at(1320.0);
    assert_eq!(h.rect_of("nav::list").unwrap().size.width, 210.0);
    let content = h.rect_of("nav::content").unwrap();
    assert_eq!(content.origin.x, 211.0, "サイドバー 210 + 線 1");
    assert_eq!(content.size.width, 1320.0 - 211.0);
    assert!(h.rect_of(&nav_menu_button_id("nav")).is_none());
    assert!(h.overflows().is_empty(), "{:?}", h.overflows());
}

#[test]
fn a_medium_window_gets_the_narrow_rail_and_gives_the_rest_to_the_content() {
    let h = at(820.0);
    assert!(h.rect_of("nav::list").is_none());
    assert_eq!(h.rect_of("nav::rail").unwrap().size.width, 76.0);
    let content = h.rect_of("nav::content").unwrap();
    assert_eq!(content.size.width, 820.0 - 77.0, "本文は 600 を切らない");
    assert!(h.overflows().is_empty(), "{:?}", h.overflows());
}

#[test]
fn a_compact_window_gets_a_bar_and_a_closed_drawer() {
    let h = at(500.0);
    assert!(h.rect_of("nav::rail").is_none());
    assert!(h.rect_of("nav::drawer").is_none(), "閉じている");
    let bar = h.rect_of("nav::bar").unwrap();
    assert_eq!(bar.size.height, 48.0);
    let content = h.rect_of("nav::content").unwrap();
    assert_eq!((content.origin.x, content.size.width), (0.0, 500.0), "本文は全幅");
    assert_eq!(content.origin.y, 49.0, "バー 48 + 線 1 の下");
    assert!(h.text_rect("配車表").is_some(), "バーに今の画面の名前");
    assert!(h.overflows().is_empty(), "{:?}", h.overflows());
}

#[test]
fn items_select_in_every_mode() {
    for width in [1320.0, 820.0] {
        let mut h = at(width);
        h.click(&nav_item_id("nav", "invoices"));
        h.settle();
        assert_eq!(h.app().page, "invoices", "幅 {width}");
    }
}

#[test]
fn the_drawer_opens_from_the_menu_and_closes_when_an_item_is_picked() {
    let mut h = at(500.0);
    h.click(&nav_menu_button_id("nav"));
    h.settle();
    let panel = h.rect_of("nav::drawer").expect("開いていない");
    assert_eq!((panel.origin.x, panel.size.width), (0.0, 280.0));
    assert!(h.overflows().is_empty(), "{:?}", h.overflows());

    h.click(&nav_item_id("nav", "vehicles"));
    h.settle();
    assert_eq!(h.app().page, "vehicles");
    assert!(h.rect_of("nav::drawer").is_none(), "選んだら閉じる");
    assert!(h.text_rect("車両一覧").is_some(), "バーの名前も変わる");
}

#[test]
fn the_scrim_closes_the_drawer_without_touching_the_page_behind() {
    let mut h = at(500.0);
    h.click(&nav_menu_button_id("nav"));
    h.settle();
    // 引き出しの右 (幕の上) を押す。下の本文には届かない。
    h.click_at(450.0, 400.0);
    h.settle();
    assert!(h.rect_of("nav::drawer").is_none());
    assert_eq!(h.app().page, "dispatch");
}

/// 本文の中のリンクや「戻る」で画面が移ったときも閉じる。
#[test]
fn the_drawer_closes_when_the_page_changes_from_elsewhere() {
    let mut h = at(500.0);
    h.click(&nav_menu_button_id("nav"));
    h.settle();
    assert!(h.app().nav.is_drawer_open());
    h.app_mut().page = "customers".into();
    h.settle();
    assert!(!h.app().nav.is_drawer_open());
    assert!(h.rect_of("nav::drawer").is_none());
}

/// 開いたまま窓を広げたら閉じる。また狭めたときに急に被さらない。
#[test]
fn widening_the_window_closes_the_drawer() {
    let mut h = at(500.0);
    h.click(&nav_menu_button_id("nav"));
    h.settle();
    h.resize(1320.0, 700.0);
    h.settle();
    h.resize(500.0, 700.0);
    h.settle();
    assert!(h.rect_of("nav::drawer").is_none());
}

/// ナビの側はセーフエリアの内側に置く (iOS)。
#[test]
fn the_bar_sits_below_the_status_bar() {
    let mut h = Harness::new(Renta::new(), 440.0, 956.0);
    h.set_safe_area(62.0, 0.0, 34.0, 0.0);
    h.settle();
    let menu = h.rect_of(&nav_menu_button_id("nav")).unwrap();
    assert_eq!(menu.origin.y, 62.0);
    assert_eq!(h.rect_of("nav::content").unwrap().origin.y, 62.0 + 48.0 + 1.0);
}

/// 見出しの無いまとまり 1 つだけのナビ。
struct Plain(NavFrameState);

impl DeclarativeApp for Plain {
    fn view(&self, ctx: &ViewContext) -> Element {
        let groups = [NavGroup::default().item(NavItem::new("a", "A"))];
        nav_frame(ctx, "nav", &self.0, &NavFrameStyle::default_dark(), &groups, "a", |_: &mut Plain, _| {}, div())
    }
}

/// 一覧の上の余白 8px は、セーフエリアに足される (上書きで消えない)。
#[test]
fn the_list_keeps_its_top_padding_with_and_without_a_safe_area() {
    for (top, rail) in [(0.0, false), (0.0, true), (47.0, false)] {
        let width = if rail { 820.0 } else { 1320.0 };
        let mut h = Harness::new(Plain(NavFrameState::new()), width, 700.0);
        h.set_safe_area(top, 0.0, 0.0, 0.0);
        h.settle();
        assert_eq!(h.rect_of(&nav_item_id("nav", "a")).unwrap().origin.y, top + 8.0, "幅 {width} 上 {top}");
    }
}

// ---------------------------------------------------------------------------
// 見出し・足元・アイコン (#105)
// ---------------------------------------------------------------------------

/// rentacar-node の React 版のサイドバー: アプリ名・項目・ログイン中の人とログアウト。
/// 項目はアイコンを指定しない (サイドバーでは名前だけで足りる)。
#[derive(Default)]
struct Admin {
    nav: NavFrameState,
    logged_out: bool,
    many: bool,
    rail_parts: bool,
    style: Option<NavFrameStyle>,
    icons: bool,
}

impl DeclarativeApp for Admin {
    fn view(&self, ctx: &ViewContext) -> Element {
        let n = if self.many { 40 } else { 3 };
        let mut items: Vec<NavItem> =
            (0..n).map(|i| NavItem::new(format!("p{i}"), format!("画面{i}"))).collect();
        if self.icons {
            items[0] = NavItem::new("p0", "画面0").icon_view(|color, size| {
                div().id("car-icon").w(Px(size)).h(Px(size)).bg(color)
            });
        }
        let groups = [NavGroup::new("業務").items(items)];
        let mut slots = NavSlots::new()
            .header(text("レンタカー管理").id("app-name").bold().px_pad(Px(16.0)).py(Px(12.0)))
            .footer(div().id("who").p_px(12.0).flex_col().gap(6.0).children([
                text("管理者"),
                div().click(ctx, "logout", |a: &mut Admin| a.logged_out = true).h(Px(28.0)).child(text("ログアウト")),
            ]));
        if self.rail_parts {
            slots = slots.rail_header(div().id("rail-logo").w(Px(32.0)).h(Px(32.0)));
        }
        let style = self.style.clone().unwrap_or_else(NavFrameStyle::default_dark);
        nav_frame_with(ctx, "nav", &self.nav, &style, &groups, "p0", |_: &mut Admin, _| {}, div(), slots)
    }
}

fn admin(app: Admin, width: f32) -> Harness<Admin> {
    let mut h = Harness::new(app, width, 700.0);
    h.settle();
    h
}

/// 描かれた文字のうち、中身がちょうど `s` のもの。
fn exact_texts(h: &Harness<Admin>, s: &str) -> usize {
    let b = h.build();
    b.all_commands()
        .filter(|c| matches!(c, RenderCommand::Text(t) if &*t.content == s))
        .count()
}

#[test]
fn the_sidebar_puts_the_header_on_top_and_the_footer_at_the_bottom() {
    let mut h = admin(Admin::default(), 1320.0);
    let name = h.rect_of("app-name").unwrap();
    let list = h.rect_of("nav::list").unwrap();
    let who = h.rect_of("who").unwrap();
    assert_eq!(name.origin.y, 0.0);
    assert!(list.origin.y >= name.origin.y + name.size.height, "一覧は見出しの下");
    assert_eq!(who.origin.y + who.size.height, 700.0, "足元は窓の下端");
    assert!(list.origin.y + list.size.height <= who.origin.y, "一覧は足元の上で終わる");
    assert!(h.overflows().is_empty(), "{:?}", h.overflows());
    h.click("logout");
    assert!(h.app().logged_out);
}

/// 項目が多くても、見出しと足元は動かない (一覧だけがスクロールする)。
#[test]
fn only_the_list_scrolls_between_the_header_and_the_footer() {
    let mut h = admin(Admin { many: true, ..Default::default() }, 1320.0);
    let who = h.rect_of("who").unwrap();
    assert_eq!(who.origin.y + who.size.height, 700.0, "足元が押し出されない");
    h.scroll("nav::list", 300.0);
    h.settle();
    assert_eq!(h.rect_of("app-name").unwrap().origin.y, 0.0);
    assert_eq!(h.rect_of("who").unwrap(), who);
    assert!(h.scroll_y("nav::list").unwrap() > 0.0);
}

#[test]
fn the_drawer_carries_the_header_and_footer_too() {
    let mut h = admin(Admin::default(), 500.0);
    assert!(h.rect_of("app-name").is_none(), "閉じている間は無い");
    h.click(&nav_menu_button_id("nav"));
    h.settle();
    let panel = h.rect_of("nav::drawer").unwrap();
    let who = h.rect_of("who").unwrap();
    assert_eq!(h.rect_of("app-name").unwrap().origin.y, panel.origin.y);
    assert_eq!(who.origin.y + who.size.height, panel.origin.y + panel.size.height);
    h.click("logout");
    assert!(h.app().logged_out);
}

/// 細い列は 76px しか無いので、細い列用を渡さなければ何も出さない。
#[test]
fn the_rail_shows_only_its_own_parts() {
    let h = admin(Admin::default(), 820.0);
    assert!(h.rect_of("app-name").is_none());
    assert!(h.rect_of("who").is_none());
    let h = admin(Admin { rail_parts: true, ..Default::default() }, 820.0);
    let logo = h.rect_of("rail-logo").unwrap();
    assert!(logo.origin.y < h.rect_of(&nav_item_id("nav", "p0")).unwrap().origin.y);
}

/// アイコンを指定しない項目は、サイドバーで名前の先頭の字を並べない
/// (「配 配車表」「顧 顧客」)。細い列では字が要るので出す。
#[test]
fn the_sidebar_does_not_repeat_the_first_letter_as_an_icon() {
    let h = admin(Admin::default(), 1320.0);
    assert_eq!(exact_texts(&h, "画"), 0, "サイドバーに頭の字が出ている");
    let label = h.text_rect("画面1").unwrap();
    let row = h.rect_of(&nav_item_id("nav", "p1")).unwrap();
    assert_eq!(label.origin.x, row.origin.x + 8.0, "名前が行の頭から始まる");
    let h = admin(Admin::default(), 820.0);
    assert_eq!(exact_texts(&h, "画"), 3, "細い列には出す");
}

#[test]
fn an_icon_view_is_drawn_in_the_accent_colour_when_selected() {
    let style = NavFrameStyle::default_dark();
    let h = admin(Admin { icons: true, ..Default::default() }, 1320.0);
    let icon = h.rect_of("car-icon").unwrap();
    assert_eq!((icon.size.width, icon.size.height), (14.0, 14.0));
    let fill = h.build().render_list.commands.iter().find_map(|c| match c {
        RenderCommand::Rect(r) if r.rect == icon => Some(r.fill_color),
        _ => None,
    });
    assert_eq!(fill, Some(style.accent), "選ばれている項目のアイコンは accent");
    // 細い列でも同じ関数で描く (18px)。
    let h = admin(Admin { icons: true, ..Default::default() }, 820.0);
    assert_eq!(h.rect_of("car-icon").unwrap().size.width, 18.0);
    // アイコンの無い項目も名前の頭が揃う (空の欄を取る)。
    let h = admin(Admin { icons: true, ..Default::default() }, 1320.0);
    let (a, b) = (h.text_rect("画面0").unwrap(), h.text_rect("画面1").unwrap());
    assert_eq!(a.origin.x, b.origin.x);
}

#[test]
fn sidebar_icons_off_hides_even_given_icons() {
    let style = NavFrameStyle { sidebar_icons: false, ..NavFrameStyle::default_dark() };
    let h = admin(Admin { icons: true, style: Some(style.clone()), ..Default::default() }, 1320.0);
    assert!(h.rect_of("car-icon").is_none());
    let h = admin(Admin { icons: true, style: Some(style), ..Default::default() }, 820.0);
    assert!(h.rect_of("car-icon").is_some(), "細い列では出す");
}
