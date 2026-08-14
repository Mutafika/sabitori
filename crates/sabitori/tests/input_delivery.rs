//! 3 ランタイムの入力配信表を、 ファサード越しに突き合わせる。
//!
//! sabitori はイベント処理を共有しない 3 つのランタイムを持ち、 配線は 1 つずつ手で
//! 書かれている。 その結果「core は持っているのにランタイムが配らない」 事故が
//! 繰り返し起きた (issue #1 / #3 / #12)。 `input_delivery` は各ランタイムが全種別に
//! ついて意思表示する網羅マッチで、 種別が増えると 3 つとも**コンパイルが壊れる**。
//!
//! このファイルの役目は 2 つ:
//!
//! 1. 表が「全種別ぶん引ける」ことの確認 (網羅マッチなので当然だが、 将来
//!    誰かが `_` 腕を足したらここが意味を持つ)
//! 2. **ランタイム間の一致を固定する** — declarative と scene_app は同じ
//!    `DeclarativeApp` を実装させる以上、 届き方も揃っていなければならない。
//!    差が「いつの間にか増えた」 のを検出する口。
//!
//! ⚠️ **限界**: ここが比べているのは宣言どうしで、 宣言と実装のズレは見ていない。
//! declarative については `frame_tests::declared_delivery_matches_reality` が
//! ヘッドレスで駆動して実挙動と突き合わせているが、 scene_app と
//! sabitori-window は窓が要るので同じことができていない。 ヘッドレス駆動を
//! 公開 API にする issue #19 が入ったら、 残りもそこで見ること。

use sabitori::{Delivery, InputEventKind as K};

/// 全種別について 3 ランタイムとも宣言を引けること。
#[test]
fn every_runtime_declares_every_kind() {
    for kind in K::ALL {
        // 引けること自体が確認事項。 網羅マッチが `_` に置き換わると、
        // 「引けるが中身は空」 という状態になり得るのでここで舐めておく。
        let _ = sabitori::declarative::input_delivery(*kind);
        let _ = sabitori::scene_app::input_delivery(*kind);
        let _ = sabitori_window::input_delivery(*kind);
    }
    assert_eq!(
        K::ALL.len(),
        12,
        "種別を増減したら、 3 ランタイムの input_delivery と CHANGELOG を確認すること"
    );
}

/// キーボード系はどのランタイムでもアプリに届く。 ここが崩れると、 消費側の
/// キーバインドがランタイム依存で動かなくなる。
#[test]
fn keyboard_reaches_the_app_on_every_runtime() {
    for kind in [K::KeyInput, K::CharInput, K::ModifiersChanged] {
        assert_eq!(
            sabitori::declarative::input_delivery(kind),
            Delivery::ToApp,
            "declarative: {kind:?}"
        );
        assert_eq!(
            sabitori::scene_app::input_delivery(kind),
            Delivery::ToApp,
            "scene_app: {kind:?}"
        );
        assert_eq!(
            sabitori_window::input_delivery(kind),
            Delivery::ToApp,
            "sabitori-window: {kind:?}"
        );
    }
}

/// `sabitori-window` はポインタを 1 つもアプリに渡さない。 retained な `NodeTree` が
/// hit-test と押下追跡を持ち、 結果だけ `on_click` で伝える設計。
#[test]
fn sabitori_window_keeps_all_pointer_events_internal() {
    for kind in [
        K::PointerMoved,
        K::PointerPressed,
        K::PointerReleased,
        K::PointerCancelled,
        K::PointerLeft,
    ] {
        assert!(
            matches!(sabitori_window::input_delivery(kind), Delivery::Internal(_)),
            "{kind:?} が Internal でなくなっている。 SabitoriApp に生のポインタを\
             渡す設計に変えたなら、 このテストと doc を更新すること"
        );
    }
}

/// declarative と scene_app は、 同じ `DeclarativeApp` を実装させる以上、
/// **同じ種別が同じ届き方をしなければならない**。
///
/// かつては IME だけ食い違っていた (issue #22): scene_app は `ImeEnabled` を
/// 組み立てず、 preedit / commit も `on_focused_input` にしか渡さなかったので、
/// フォーカス中の要素が無いと変換中の文字がどこにも届かなかった。 ターミナルの
/// ような「フォーカス要素は無いが IME 入力は受ける」 アプリが SceneApp では
/// 書けない、 という形で表に出る。
///
/// 揃えたので、 ここでは全種別の一致を固定する。 意図的に差を付けるなら、
/// その理由をこのテストに書いてから外すこと。
#[test]
fn declarative_and_scene_app_deliver_alike() {
    for kind in K::ALL {
        assert_eq!(
            sabitori::declarative::input_delivery(*kind),
            sabitori::scene_app::input_delivery(*kind),
            "{kind:?} の届き方が 2 ランタイムで食い違っている"
        );
    }
}

/// `PointerLeft` は 2 つの winit ランタイムのどちらも組み立てない。 カーソルの離脱は
/// `DeclarativeApp::on_cursor_left` で伝えている。 これは意図的な設計。
#[test]
fn pointer_left_is_reported_through_on_cursor_left() {
    for delivery in [
        sabitori::declarative::input_delivery(K::PointerLeft),
        sabitori::scene_app::input_delivery(K::PointerLeft),
    ] {
        assert!(
            matches!(delivery, Delivery::NotProduced(_)),
            "PointerLeft を発行するようになったなら on_cursor_left との二重配信に\
             なっていないか確認すること"
        );
    }
}
