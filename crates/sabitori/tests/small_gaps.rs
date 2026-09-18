//! 業務アプリを書いて出てきた小粒の穴 (#75) のうち、今回埋めたぶん。
//!
//! どれも「1 つずつ issue にするほどではないが、無いと毎画面で回避を書く」もの。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_widgets::TextInputState;

// ---------------------------------------------------------------------------
// 5: テキスト欄の変更通知
// ---------------------------------------------------------------------------

/// **打った / 貼った / 消した / 差し替えた、どの経路でも拾えること。**
///
/// 変更の回数を数える形にすると、経路を 1 つ足し忘れたときに
/// 「たまに検索が走らない」になる。中身を突き合わせて答える。
#[test]
fn a_text_field_reports_that_it_changed_however_it_changed() {
    let f = TextInputState::new("検索");
    assert!(!f.take_changed(), "触っていないのに変わったと言っている");

    f.set_text("R-00");
    assert!(f.take_changed(), "set_text を拾えていない");
    assert!(!f.take_changed(), "見たら下りること");

    // 打鍵。
    f.handle_input(&InputEvent::CharInput('4'));
    assert!(f.take_changed(), "打鍵を拾えていない");

    // 貼り付け。
    f.handle_input(&InputEvent::Paste { text: "2".into() });
    assert!(f.take_changed(), "貼り付けを拾えていない");

    // 削除。
    f.handle_input(&InputEvent::KeyInput {
        key: Key::Backspace,
        pressed: true,
        modifiers: Modifiers::default(),
    });
    assert!(f.take_changed(), "削除を拾えていない");

    // IME の確定。
    f.handle_input(&InputEvent::ImeCommit { text: "予約".into() });
    assert!(f.take_changed(), "IME の確定を拾えていない");
}

/// 同じ内容に戻ったら「変わっていない」。検索を投げ直さないため。
#[test]
fn coming_back_to_the_same_text_is_not_a_change() {
    let f = TextInputState::new("検索");
    f.set_text("abc");
    assert!(f.take_changed());

    f.set_text("abcd");
    f.set_text("abc");
    assert!(!f.take_changed(), "行って戻ったのに変更扱いになっている");
}

// ---------------------------------------------------------------------------
// 14: 1 回だけ焦点を当てる
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Form {
    focus_next: Option<String>,
    hold: Option<String>,
}

impl DeclarativeApp for Form {
    fn view(&self, _ctx: &ViewContext) -> Element {
        div().w_full().h_full().flex_col().children([
            div().id("name").w(Px(100.0)).h(Px(30.0)).focusable(),
            div().id("memo").w(Px(100.0)).h(Px(30.0)).focusable(),
        ])
    }

    fn take_focus_once(&mut self) -> Option<String> {
        self.focus_next.take()
    }

    fn desired_focus(&self) -> Option<String> {
        self.hold.clone()
    }
}

/// **1 回当てたら、以後は引き戻さない。**
///
/// `desired_focus` を返し続けると焦点を握りっぱなしになるので、アプリ側は
/// `Cell` に入れて最初の 1 回だけ返す、という回避を書いていた。
#[test]
fn focus_once_lets_the_user_move_away_afterwards() {
    let mut h = Harness::new(Form::default(), 400.0, 300.0);
    h.frame();

    h.app_mut().focus_next = Some("name".into());
    h.frame();
    assert_eq!(h.focused_id(), Some("name"), "1 回目で入ること");

    // ユーザーが別の欄を押す。
    h.click("memo");
    h.frame();
    assert_eq!(h.focused_id(), Some("memo"), "押した先に移らず引き戻された");
}

/// `desired_focus` の方は今までどおり主張し続ける (モーダルの閉じ込め)。
#[test]
fn desired_focus_still_holds() {
    let mut h = Harness::new(Form::default(), 400.0, 300.0);
    h.frame();

    h.app_mut().hold = Some("name".into());
    h.frame();
    h.click("memo");
    h.frame();

    assert_eq!(h.focused_id(), Some("name"), "主張しているのに出られている");
}

// ---------------------------------------------------------------------------
// 8: 画面外の要素まで送る
// ---------------------------------------------------------------------------

struct LongForm;

impl DeclarativeApp for LongForm {
    fn view(&self, _ctx: &ViewContext) -> Element {
        let mut rows: Vec<Element> = (0..30)
            .map(|i| div().id(format!("row-{i}")).w_full().h(Px(40.0)))
            .collect();
        rows.push(div().id("save").w(Px(80.0)).h(Px(32.0)));
        div().w_full().h_full().child(
            div()
                .id("form")
                .scroll("form")
                .w_full()
                .h(Px(300.0))
                .flex_col()
                .children(rows),
        )
    }
}

/// 画面外の保存ボタンまでスクロールして、押せる状態にできること。
#[test]
fn scroll_into_view_reaches_a_button_below_the_fold() {
    let mut h = Harness::new(LongForm, 400.0, 300.0);
    h.frame();
    assert!(h.rect_of("save").is_none(), "最初は画面外のはず");

    assert!(h.scroll_into_view("form", "save"), "届かなかった");
    assert!(h.rect_of("save").is_some(), "見えていない");
}

/// 無いものは `false` で返る (探し続けて固まらない)。
#[test]
fn scroll_into_view_gives_up_on_something_that_is_not_there() {
    let mut h = Harness::new(LongForm, 400.0, 300.0);
    h.frame();

    assert!(!h.scroll_into_view("form", "居ない"), "無いのに見つけたと言っている");
}

// ---------------------------------------------------------------------------
// 3: 表のセルに Element
// ---------------------------------------------------------------------------

use sabitori_widgets::{table_with, Cell, TableColumn, TableState, TableStyle};

struct Vehicles {
    table: TableState,
    badge_clicks: u32,
}

impl Default for Vehicles {
    fn default() -> Self {
        let mut t = TableState::new(vec![
            TableColumn::flex("車両"),
            TableColumn::fixed("状態", 120.0),
        ]);
        t.set_rows(vec![
            vec![Cell::text("R-0042"), Cell::text("貸出可能")],
            vec![Cell::text("R-0043"), Cell::text("整備中")],
        ]);
        Self { table: t, badge_clicks: 0 }
    }
}

impl DeclarativeApp for Vehicles {
    fn view(&self, ctx: &ViewContext) -> Element {
        // 表は**自分の箱いっぱいに広がる**前提。大きさを書かないと、幅ゼロの
        // 伸縮列と高さゼロの本体になって中身ごと消える (`table` の doc)。
        table_with(
            ctx,
            "vehicles",
            &self.table,
            &TableStyle::default_dark(),
            |row, col, cell| {
                // 状態の列だけバッジにする。id は行ごとに変える
                // (同じ id が 2 つあると、片方の状態がもう片方へ漏れる)。
                (col == 1).then(|| {
                    div()
                        .id(format!("badge-{row}"))
                        .px_pad(Px(8.0))
                        .py(Px(2.0))
                        .rounded(Px(999.0))
                        .bg(Color::from_hex("#3ddc84"))
                        .click(ctx, format!("badge-{row}"), |app: &mut Vehicles| {
                            app.badge_clicks += 1
                        })
                        .child(text(cell.text.clone()).color(Color::BLACK))
                })
            },
        )
        .w_full()
        .h_full()
    }
}

/// セルに組んだ要素が実際に出て、押せること。
#[test]
fn a_table_cell_can_hold_an_element() {
    let mut h = Harness::new(Vehicles::default(), 600.0, 400.0);
    h.frame();

    assert!(h.rect_of("badge-0").is_some(), "バッジが出ていない");
    assert!(h.rect_of("badge-1").is_some(), "2 行目のバッジが出ていない");

    h.click("badge-0");
    assert_eq!(h.app().badge_clicks, 1, "行内のボタンが鳴らない");
}

/// 組まなかった列は今までどおり文字のまま (既存の表が変わらないこと)。
#[test]
fn cells_without_a_custom_element_still_render_text() {
    let h = Harness::new(Vehicles::default(), 600.0, 400.0);
    let mut h = h;
    h.frame();

    let texts: Vec<String> = h
        .build()
        .render_list
        .commands
        .iter()
        .filter_map(|c| match c {
            RenderCommand::Text(t) => Some(t.content.to_string()),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|t| t == "R-0042"), "車両番号が出ていない: {texts:?}");
}
