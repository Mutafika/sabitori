//! モーダルと、浮かせるメニュー (#75 の 4 / 7 / 15 / 16)。
//!
//! 業務画面の「一覧 → 行を押して編集ダイアログ → 中にフォーム」が、
//! 回避を書かずに組めること。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_widgets::{modal, DropdownState, DropdownStyle, ModalState, ModalStyle};

// ---------------------------------------------------------------------------
// 7 / 16: モーダルの高さと、中のフォーム
// ---------------------------------------------------------------------------

struct Screen {
    edit: ModalState,
    saved: u32,
    rows: usize,
}

impl Screen {
    fn with_rows(rows: usize) -> Self {
        Self { edit: ModalState::new(), saved: 0, rows }
    }
}

impl DeclarativeApp for Screen {
    fn view(&self, ctx: &ViewContext) -> Element {
        let fields: Vec<Element> = (0..self.rows)
            .map(|i| div().id(format!("field-{i}")).w_full().h(Px(40.0)))
            .collect();

        let dialog = modal(
            ctx,
            "edit",
            &self.edit,
            &ModalStyle::default_dark(),
            "予約を編集",
            vec![
                div().flex_col().gap(8.0).children(fields),
                div()
                    .id("save")
                    .w(Px(80.0))
                    .h(Px(32.0))
                    .click(ctx, "save", |app: &mut Screen| {
                        app.saved += 1;
                        app.edit.close();
                    }),
            ],
        );

        div()
            .w_full()
            .h_full()
            .child(div().id("open").w(Px(100.0)).h(Px(32.0)).click(
                ctx,
                "open",
                |app: &mut Screen| app.edit.open(),
            ))
            .children(dialog)
    }
}

/// **アプリは `tick` を書いていない。** それでも開ききること。
#[test]
fn a_modal_opens_without_the_app_ticking_it() {
    let mut h = Harness::new(Screen::with_rows(3), 800.0, 600.0);
    h.frame();
    assert!(h.rect_of("edit::dialog").is_none(), "閉じているのに出ている");

    h.click("open");
    h.settle();

    assert!(h.rect_of("edit::dialog").is_some(), "開かない");
    assert!(
        (h.app().edit.progress() - 1.0).abs() < 0.02,
        "開ききっていない: {}",
        h.app().edit.progress()
    );
}

/// **高さが中身なり。** 3 行のフォームが `max_height` (400) のダイアログに
/// スカスカで入るのではなく、中身の高さで収まること。
#[test]
fn a_short_form_makes_a_short_dialog() {
    let mut h = Harness::new(Screen::with_rows(3), 800.0, 600.0);
    h.frame();
    h.click("open");
    h.settle();

    let d = h.rect_of("edit::dialog").expect("ダイアログが無い");
    assert!(d.size.height < 400.0, "高さが上限に貼り付いている: {}", d.size.height);
    assert!(d.size.height > 100.0, "潰れている: {}", d.size.height);
}

/// **長いフォームは上限で止まる。** 止まらないと画面からはみ出す。
#[test]
fn a_long_form_stops_at_the_cap_and_scrolls_inside() {
    let mut h = Harness::new(Screen::with_rows(40), 800.0, 600.0);
    h.frame();
    h.click("open");
    h.settle();

    let d = h.rect_of("edit::dialog").expect("ダイアログが無い");
    assert!(d.size.height <= 400.5, "上限を超えている: {}", d.size.height);

    // 下のほうの欄は最初は見えず、スクロールすれば届く。
    assert!(h.rect_of("field-39").is_none(), "40 行目が最初から見えている");
    assert!(h.scroll_into_view("edit::body", "field-39"), "中がスクロールしない");
    assert!(h.rect_of("field-39").is_some(), "スクロールしても出てこない");
}

/// ダイアログの中を押しても閉じない。背景を押すと閉じる。
#[test]
fn clicking_inside_keeps_it_open_and_the_backdrop_closes_it() {
    let mut h = Harness::new(Screen::with_rows(3), 800.0, 600.0);
    h.frame();
    h.click("open");
    h.settle();

    let d = h.rect_of("edit::dialog").unwrap();
    // ダイアログの上端 (見出し行のあたり) を押す。
    h.click_at(d.origin.x + d.size.width / 2.0, d.origin.y + 6.0);
    h.frame();
    assert!(h.app().edit.is_open(), "中を押したら閉じた");

    // 画面の隅 = 背景。
    h.click_at(5.0, 5.0);
    h.frame();
    assert!(!h.app().edit.is_open(), "背景を押しても閉じない");
}

/// 保存前に消えては困るフォームでは背景で閉じない。
#[test]
fn a_non_dismissable_modal_ignores_the_backdrop() {
    let mut h = Harness::new(Screen::with_rows(3), 800.0, 600.0);
    h.app().edit.set_dismissable(false);
    h.frame();
    h.click("open");
    h.settle();

    h.click_at(5.0, 5.0);
    h.frame();
    assert!(h.app().edit.is_open(), "閉じてはいけない");
}

/// **`settle` が閉じアニメーションを終わらせる** (#75 の 15)。
///
/// 終わらないと、閉じかけの背景が残って**次のクリックを吸う**。テストの中で
/// だけ「押しても何も起きない」が起きるので、原因にたどり着くのが遅れる。
#[test]
fn settle_finishes_the_closing_animation() {
    let mut h = Harness::new(Screen::with_rows(3), 800.0, 600.0);
    h.frame();
    h.click("open");
    h.settle();

    h.click("save");
    assert_eq!(h.app().saved, 1);
    let frames = h.settle();
    assert!(frames < 120, "打ち切りまで回った (収束していない)");
    assert!(h.app().edit.is_fully_closed(), "閉じきっていない");
    assert!(h.rect_of("edit::backdrop").is_none(), "背景が残っている");

    // 次のクリックが下の画面に届く。
    h.click("open");
    h.frame();
    assert!(h.app().edit.is_open(), "次のクリックが吸われた");
}

// ---------------------------------------------------------------------------
// 4: モーダルの中のドロップダウンが、下を押し下げずに浮く
// ---------------------------------------------------------------------------

struct Plans {
    plan: DropdownState,
}

impl DeclarativeApp for Plans {
    fn view(&self, ctx: &ViewContext) -> Element {
        let style = DropdownStyle::default_dark();
        div()
            .w_full()
            .h_full()
            .flex_col()
            .child(self.plan.trigger(&style, ctx.hovered.as_deref()))
            .child(div().id("below").w(Px(200.0)).h(Px(40.0)))
            .children(self.plan.menu(ctx.hovered.as_deref(), &style))
    }

    fn on_click(&mut self, id: &str) {
        self.plan.handle_click(id);
    }
}

/// **開いても下の行が動かない。** `menu_inline` は押し下げるので、
/// フォームの中で使うと行が飛び跳ねる。
#[test]
fn an_anchored_menu_floats_instead_of_pushing_the_form_down() {
    let mut h = Harness::new(
        Plans {
            plan: DropdownState::new(
                "plan",
                vec!["ベーシック (¥1,100/日)".into(), "スタンダード".into(), "プレミアム".into()],
            ),
        },
        800.0,
        600.0,
    );
    h.frame();
    let before = h.rect_of("below").expect("下の行が無い");

    h.click("plan");
    h.frame();

    let after = h.rect_of("below").expect("下の行が消えた");
    assert_eq!(before.origin.y, after.origin.y, "メニューが下を押し下げている");

    // メニューはトリガーの真下に、同じ幅で出る。
    let trigger = h.rect_of("plan").unwrap();
    let item = h.rect_of("plan::item:0").expect("項目が出ていない");
    assert!(
        item.origin.y > trigger.origin.y + trigger.size.height,
        "トリガーの下に出ていない"
    );
    assert!(
        (item.size.width - trigger.size.width).abs() < 1.0,
        "幅が揃っていない: 項目 {} / トリガー {}",
        item.size.width,
        trigger.size.width
    );
}

/// 項目を選べること (浮かせても当たり判定が前に来る)。
#[test]
fn an_item_in_the_floating_menu_can_be_picked() {
    let mut h = Harness::new(
        Plans {
            plan: DropdownState::new("plan", vec!["木造".into(), "鉄骨造".into(), "RC造".into()]),
        },
        800.0,
        600.0,
    );
    h.frame();
    h.click("plan");
    h.frame();

    h.click("plan::item:2");
    h.frame();
    assert_eq!(h.app().plan.selected, 2, "選べていない");
    assert!(!h.app().plan.open, "選んだのに閉じていない");
}

/// **長いフォームのダイアログにも掴める帯が出る** ([#90](https://github.com/Mutafika/sabitori/issues/90))。
/// 中身の `.scroll` はダイアログの内側にあり、アプリからは付けられない。
#[test]
fn a_long_form_in_a_modal_has_a_grabbable_bar() {
    let mut h = Harness::new(Screen::with_rows(40), 800.0, 600.0);
    h.frame();
    h.click("open");
    h.settle();

    let bars = h.frame().scroll_bars();
    assert!(
        bars.iter().any(|b| b.id == "edit::body"),
        "ダイアログの中身に帯が無い: {:?}",
        bars
    );
}

/// **帯を付けても、欄の右端は欄のもの** (#90 のレビューで見つけた)。
///
/// 掴める帯は右端 14px の押しを食う。中身の右端にそのまま帯を置くと、
/// `w_full` の欄の右端を押した時に欄ではなく帯が掴まれる。
#[test]
fn the_bar_does_not_steal_presses_from_the_right_edge_of_a_field() {
    let mut h = Harness::new(Screen::with_rows(40), 800.0, 600.0);
    h.frame();
    h.click("open");
    h.settle();
    h.frame();

    let f = h.rect_of("field-0").expect("欄が無い");
    let bar = h.frame().scroll_bars().into_iter().find(|b| b.id == "edit::body").expect("帯が無い");
    let bar_left = bar.rect.origin.x + bar.rect.size.width - bar.lane;
    assert!(
        bar_left >= f.origin.x + f.size.width,
        "帯の掴める所 (x >= {bar_left}) が欄 (右端 {}) に被っている",
        f.origin.x + f.size.width
    );

    // 中身の幅は変わらない: 欄の右端はダイアログの余白 (24px + 枠線) の内側のまま。
    let d = h.rect_of("edit::dialog").unwrap();
    let inner_right = d.origin.x + d.size.width - 24.0;
    assert!(
        (f.origin.x + f.size.width - inner_right).abs() <= 1.5,
        "欄の幅が変わった: 右端 {} / 余白の内側 {inner_right}",
        f.origin.x + f.size.width
    );
}
