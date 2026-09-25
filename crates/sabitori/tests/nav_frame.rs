//! **左にナビ + 右に本文の枠組みが、窓の幅で形を変える**
//! ([#98](https://github.com/Mutafika/sabitori/issues/98))。
//!
//! sabitori-renta (業務 46 画面) はサイドバー 210px 固定で、820px の窓では本文が
//! 600px を切っていた。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_core::element::{div, text, Px};
use sabitori_widgets::{
    nav_frame, nav_item_id, nav_menu_button_id, NavFrameStyle, NavGroup, NavItem, NavState,
};

struct Renta {
    page: String,
    nav: NavState,
}

impl Renta {
    fn new() -> Self {
        Self { page: "dispatch".into(), nav: NavState::new() }
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
