//! 折り返す複数行のテキスト欄が、 **ランタイム越しに**実際に動くこと。
//!
//! 0.4.0 のテキスト欄は 1 行専用だった — Enter は素通り、 貼り付けは改行を
//! 空白に潰し、 キャレットは `y = 0` に固定されていた。 「キャレットはあるか」
//! への答えが「あるが 1 行だけ」だったので、 折り返す欄を足した回のテスト。
//!
//! ここは `Harness` (ヘッドレス) なので、 テキスト計測は等幅のスタブ。
//! **スタブは折り返しを模さない**ので、 ここで見るのは `\n` を含む複数行の
//! 挙動と配線。 実際の折り返し (ソフト改行) の座標は `sabitori-text` 側で
//! 本物の shaper に対して固定してある。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_widgets::{text_area, text_input, TextInputState, TextInputStyle};

struct Memo {
    body: TextInputState,
    subject: TextInputState,
    submitted: bool,
}

impl Memo {
    fn new() -> Self {
        Self {
            body: TextInputState::new("本文"),
            subject: TextInputState::new("件名"),
            submitted: false,
        }
    }
}

impl DeclarativeApp for Memo {
    fn view(&self, ctx: &ViewContext) -> Element {
        let style = TextInputStyle::default_dark();
        div().w_full().h_full().flex_col().children([
            text_input(ctx, "subject", &self.subject, &style),
            text_area(ctx, "body", &self.body, &style, 6),
        ])
    }

    fn on_input(&mut self, event: &InputEvent) -> bool {
        // 単一行の欄では Enter がここまで上がってくる (フォーム送信)。
        if let InputEvent::KeyInput { key: Key::Enter, pressed: true, .. } = event {
            self.submitted = true;
        }
        false
    }
}

fn app() -> Harness<Memo> {
    let mut h = Harness::new(Memo::new(), 600.0, 400.0);
    h.frame();
    h
}

/// **Enter が改行を入れること。** 単一行の欄では素通りしていた。
#[test]
fn enter_inserts_a_newline_in_a_text_area() {
    let mut h = app();
    h.click("body");
    h.text("ab");
    h.key(Key::Enter, Modifiers::default());
    h.text("cd");
    h.frame();

    assert_eq!(h.app().body.text(), "ab\ncd");
}

/// 単一行の欄では Enter が**アプリまで上がる**こと。 ここを改行にしてしまうと、
/// 検索欄の「決定」やフォーム送信が死ぬ。
#[test]
fn enter_still_bubbles_out_of_a_single_line_field() {
    let mut h = app();
    h.click("subject");
    h.text("hello");
    h.key(Key::Enter, Modifiers::default());
    h.frame();

    assert_eq!(h.app().subject.text(), "hello", "改行が混ざっていない");
    assert!(h.app().submitted, "Enter がアプリまで届いていない");
}

/// 貼り付けが改行を保つこと。 単一行では空白に潰れる。
#[test]
fn pasting_keeps_newlines_in_a_text_area_but_flattens_them_in_a_field() {
    let mut h = app();

    h.click("body");
    h.paste("one\ntwo");
    h.frame();
    assert_eq!(h.app().body.text(), "one\ntwo");

    h.click("subject");
    h.paste("one\ntwo");
    h.frame();
    assert_eq!(h.app().subject.text(), "one two", "単一行では空白に潰れる");
}

/// `\r\n` が `\n` に均されること。 `\r` が残ると、 行末に見えない文字が
/// ぶら下がってキャレット計算が 1 バイトずれる。
#[test]
fn crlf_is_normalised_on_paste() {
    let mut h = app();
    h.click("body");
    h.paste("a\r\nb\rc");
    h.frame();

    assert_eq!(h.app().body.text(), "a\nb\nc");
}

/// **キャレットが行ごとに下がること。** 1 行目に貼り付いたままなら、
/// 折り返しても複数行に見えない。
#[test]
fn the_caret_moves_down_a_line() {
    let mut h = app();
    h.click("body");
    h.paste("first\nsecond");
    h.frame();

    let at_end = h.app().body.caret();
    assert_eq!(at_end.line, 1, "末尾は 2 行目のはず");
    assert!(at_end.y > 0.0, "y が 0 のまま — 1 行目に貼り付いている");
}

/// ↑ が視覚行を 1 つ戻ること。 単一行の欄では ↑ はアプリへ素通りする。
#[test]
fn the_up_arrow_moves_one_visual_line() {
    let mut h = app();
    h.click("body");
    h.paste("first\nsecond");
    h.frame();
    assert_eq!(h.app().body.caret().line, 1);

    h.key(Key::Up, Modifiers::default());
    h.frame();

    assert_eq!(h.app().body.caret().line, 0, "↑ で 1 行目に戻っていない");
}

/// ↑↓ を往復しても**桁が痩せない**こと。
///
/// 長い行 → 短い行 → 長い行 と動いたとき、 短い行で切り詰めた桁を覚えたまま
/// にすると元の位置に戻れない。 エディタなら全部持っている挙動。
#[test]
fn the_column_survives_a_trip_through_a_short_line() {
    let mut h = app();
    h.click("body");
    h.paste("aaaaaaaaaa\nbb\ncccccccccc");
    h.frame();

    // 3 行目の末尾 (10 文字目) から出発。
    let start = h.app().body.cursor_pos();
    h.key(Key::Up, Modifiers::default()); // 短い 2 行目へ
    h.frame();
    h.key(Key::Down, Modifiers::default()); // 3 行目へ戻る
    h.frame();

    assert_eq!(
        h.app().body.cursor_pos(),
        start,
        "短い行を経由したら桁が戻らなくなっている"
    );
}

/// Home / End が**視覚行**の端に行くこと。 文字列全体の端ではない。
#[test]
fn home_and_end_work_on_the_visual_line() {
    let mut h = app();
    h.click("body");
    h.paste("first\nsecond");
    h.frame();

    h.key(Key::Home, Modifiers::default());
    h.frame();
    assert_eq!(h.app().body.cursor_pos(), 6, "2 行目の先頭 (= 'second' の s)");

    h.key(Key::End, Modifiers::default());
    h.frame();
    assert_eq!(h.app().body.cursor_pos(), 12, "2 行目の末尾");
}

/// 単一行の欄では Home / End が文字列全体の端に行くこと (従来どおり)。
#[test]
fn home_and_end_still_span_the_whole_string_in_a_field() {
    let mut h = app();
    h.click("subject");
    h.text("hello");
    h.key(Key::Home, Modifiers::default());
    h.frame();
    assert_eq!(h.app().subject.cursor_pos(), 0);
}

/// Shift+↑ が選択を伸ばすこと。
#[test]
fn shift_up_extends_the_selection() {
    let mut h = app();
    h.click("body");
    h.paste("first\nsecond");
    h.frame();

    h.key(Key::Up, Modifiers { shift: true, ..Default::default() });
    h.frame();

    let sel = h.app().body.selection_range();
    assert!(sel.is_some(), "Shift+↑ で選択が始まっていない");
    let (lo, hi) = sel.unwrap();
    assert!(lo < hi);
    assert_eq!(hi, 12, "出発点 (末尾) が選択の端として残る");
}

/// **選択範囲が実際に描かれること。**
///
/// 0.4.0 まで選択は state に持っているだけで一度も塗られていなかった —
/// Shift+→ で範囲は伸びるのに画面は何も変わらなかった。
#[test]
fn the_selection_is_actually_painted() {
    let mut h = app();
    h.click("subject");
    h.text("hello");
    h.frame();

    let before = h.build().render_list.rects().count();

    h.key(Key::Home, Modifiers { shift: true, ..Default::default() });
    h.frame();
    let after = h.build().render_list.rects().count();

    assert!(
        after > before,
        "選択しても矩形が増えていない ({before} → {after}) — 塗られていない"
    );
}

/// 選択の描画は**行ごとに割れる**こと。 1 個の矩形で返ると行間まで塗って
/// 隣の行に食い込む。
#[test]
fn a_multi_line_selection_paints_one_rect_per_line() {
    let mut h = app();
    h.click("body");
    h.paste("first\nsecond\nthird");
    h.frame();
    let before = h.build().render_list.rects().count();

    // 全選択 (3 行)。
    h.key(Key::A, Modifiers { meta: true, ctrl: true, ..Default::default() });
    h.frame();
    let after = h.build().render_list.rects().count();

    assert_eq!(after - before, 3, "3 行なら矩形も 3 個のはず");
}

/// **押した場所にキャレットが行くこと。** 配線が無ければ、 クリックしても
/// フォーカスが移るだけでカーソルは動かない。
#[test]
fn clicking_moves_the_caret_to_the_click() {
    let mut h = app();
    h.click("body");
    h.paste("first\nsecond");
    h.frame();
    assert_eq!(h.app().body.caret().line, 1, "貼った直後は末尾 = 2 行目");

    // 欄の左上あたり = 1 行目の先頭を押す。
    let rect = h.build().region_rect("body").expect("body の矩形が無い");
    h.click_at(rect.origin.x + 1.0, rect.origin.y + 1.0);
    h.frame();

    assert_eq!(h.app().body.caret().line, 0, "クリックでキャレットが動いていない");
    assert_eq!(h.app().body.cursor_pos(), 0);
}
