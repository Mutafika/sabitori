//! **配線ゼロ**でテキスト入力が動くことの固定。
//!
//! 0.4.0 より前は、 `text_input(..)` を `view()` に置くだけでは動かず、
//! `on_focused_input` / `tick` / `ime_cursor_area` の 3 つを別途実装する必要が
//! あった。 忘れると **フォーカスは入って枠も光るのに打った文字がどこにも
//! 行かない** — コンパイルは通り、 パニックもせず、 ただ何も起きない。
//!
//! いまは `text_input` が `ViewContext` に自分を登録し、 ランタイムが配信・
//! tick・フォーカス反映を引き受ける。 **書き忘れる場所が存在しない。**
//!
//! ここのアプリは全部 `view()` しか実装していない。 それで日本語変換まで
//! 通ることを見る。

use sabitori::testing::Harness;
use sabitori::*;
use sabitori_widgets::{text_input, TextInputState, TextInputStyle};

fn style() -> TextInputStyle {
    TextInputStyle {
        bg: Color::from_hex("#202020"),
        border: Color::from_hex("#404040"),
        text: Color::WHITE,
        placeholder: Color::from_hex("#808080"),
        font_size: 14.0,
        radius: 4.0,
        padding: 8.0,
        focus_border: None,
        caret: None,
        preedit: None,
        selection: None,
    }
}

/// **`view()` だけ。** 他のトレイトメソッドは 1 つも実装していない。
struct BareMinimum {
    name: TextInputState,
}

impl DeclarativeApp for BareMinimum {
    fn view(&self, ctx: &ViewContext) -> Element {
        text_input(ctx, "name", &self.name, &style())
    }
}

fn app() -> Harness<BareMinimum> {
    let mut h = Harness::new(
        BareMinimum { name: TextInputState::new("名前") },
        400.0,
        200.0,
    );
    h.frame();
    h
}

/// 打った文字が入ること。 これが 0.4.0 の最大の穴だった。
#[test]
fn typing_works_with_no_wiring_at_all() {
    let mut h = app();

    h.click("name");
    h.text("kubo");

    assert_eq!(h.app().name.text(), "kubo");
}

/// 配線漏れとして報告されないこと (登録済みなので当然だが、 検出器の裏取り)。
#[test]
fn nothing_is_reported_as_unrouted() {
    let mut h = app();
    h.click("name");
    h.text("a");
    assert!(
        h.unrouted_text_inputs().is_empty(),
        "配線漏れ扱いされている: {:?}",
        h.unrouted_text_inputs()
    );
}

/// **日本語変換が通ること。** 変換中は確定テキストに入らず、 表示だけに出る。
#[test]
fn japanese_conversion_works_with_no_wiring() {
    let mut h = app();
    h.click("name");
    h.text("a");

    h.ime_preedit("にほん", None);
    assert_eq!(h.app().name.text(), "a", "変換中は確定していない");
    assert_eq!(
        h.app().name.display_text_with_preedit(),
        "aにほん",
        "変換中の文字列がその場に見えている"
    );
    assert!(h.app().name.is_composing());

    h.ime_commit("日本");
    assert_eq!(h.app().name.text(), "a日本", "確定で本文に入る");
    assert!(!h.app().name.is_composing());
}

/// フォーカス状態をランタイムが反映すること。 アプリは何も書いていない。
#[test]
fn focus_state_is_maintained_by_the_runtime() {
    let mut h = app();
    assert!(!h.app().name.is_focused(), "最初はフォーカス無し");

    h.click("name");
    h.frame();
    assert!(h.app().name.is_focused());

    // 欄の外を押すとフォーカスが外れる。
    h.click_at(5.0, 190.0);
    h.frame();
    assert!(!h.app().name.is_focused());
}

/// 編集キーが効くこと (Backspace / 矢印)。
#[test]
fn editing_keys_work_with_no_wiring() {
    let mut h = app();
    h.click("name");
    h.text("abc");

    h.key(Key::Backspace, Modifiers::default());
    assert_eq!(h.app().name.text(), "ab");

    h.key(Key::Left, Modifiers::default());
    h.text("X");
    assert_eq!(h.app().name.text(), "aXb", "カーソル位置に挿入される");
}

/// ペーストが届くこと。
#[test]
fn paste_works_with_no_wiring() {
    let mut h = app();
    h.click("name");
    h.paste("https://example.com");
    assert_eq!(h.app().name.text(), "https://example.com");
}

/// キャレットの点滅がランタイムの時間で進むこと (`tick` を実装していない)。
#[test]
fn the_caret_blinks_without_an_app_side_tick() {
    let mut h = app();
    h.click("name");
    h.frame();
    assert!(h.app().name.cursor_visible(), "フォーカス直後は見えている");

    // 半周期を越えるまで進めると消える。
    for _ in 0..40 {
        h.tick(0.016);
    }
    assert!(!h.app().name.cursor_visible(), "点滅している");
}

/// 欄が 2 つあっても、 打鍵はフォーカス中の方にだけ入ること。
#[test]
fn two_fields_stay_independent() {
    struct TwoFields {
        a: TextInputState,
        b: TextInputState,
    }
    impl DeclarativeApp for TwoFields {
        fn view(&self, ctx: &ViewContext) -> Element {
            div().flex_col().w_full().h_full().children([
                text_input(ctx, "a", &self.a, &style()),
                text_input(ctx, "b", &self.b, &style()),
            ])
        }
    }

    let mut h = Harness::new(
        TwoFields { a: TextInputState::new("A"), b: TextInputState::new("B") },
        400.0,
        300.0,
    );
    h.frame();

    h.click("a");
    h.text("first");
    h.frame();
    h.click("b");
    h.text("second");

    assert_eq!(h.app().a.text(), "first");
    assert_eq!(h.app().b.text(), "second");
}

/// アプリが `on_focused_input` を**書いた**場合でも、 二重に入らないこと。
///
/// 登録済みの欄はランタイムが先に処理する。 消費されたイベント (文字入力・
/// 編集キー) はアプリのハンドラまで来ないので、 移行中のコードが両方持っていても
/// 文字が 2 つ入ったりしない。 欄が消費しなかったイベントは今までどおり
/// アプリへ落ちる。
#[test]
fn an_app_side_handler_does_not_double_insert() {
    struct BothWays {
        name: TextInputState,
    }
    impl DeclarativeApp for BothWays {
        fn view(&self, ctx: &ViewContext) -> Element {
            text_input(ctx, "name", &self.name, &style())
        }
        fn on_focused_input(&mut self, id: &str, e: &InputEvent) -> bool {
            // 移行前のコードがそのまま残っている状況を再現する。
            match id {
                "name" => self.name.with_mut(|i| i.on_focused_input(e)),
                _ => false,
            }
        }
    }

    let mut h = Harness::new(
        BothWays { name: TextInputState::new("名前") },
        400.0,
        200.0,
    );
    h.frame();
    h.click("name");
    h.text("ab");

    assert_eq!(h.app().name.text(), "ab", "二重に入らない");
}
