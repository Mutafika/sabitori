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
//! 2. **既知の乖離を固定する** — declarative と scene_app で届き方が違う種別を
//!    明示的に assert しておき、 issue #22 で揃えたときに必ずここが落ちるようにする。
//!    差が「いつの間にか消えた / 増えた」 のを検出する口。

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
        11,
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

/// **既知の乖離。** declarative と scene_app は同じ `DeclarativeApp` を実装させる
/// のに、 IME の届き方が違う。 設計ではなく配線漏れなので issue #22 で揃える予定。
/// 揃えたらこのテストは落ちる — それが正しい。
#[test]
fn known_ime_divergence_between_declarative_and_scene_app() {
    // IME 有効化: declarative は届く / scene_app は組み立てていない。
    assert_eq!(
        sabitori::declarative::input_delivery(K::ImeEnabled),
        Delivery::ToApp,
    );
    assert!(
        matches!(
            sabitori::scene_app::input_delivery(K::ImeEnabled),
            Delivery::NotProduced(_)
        ),
        "scene_app が ImeEnabled を配るようになったなら #22 が解消済み。 \
         このテストを消して CHANGELOG に書くこと"
    );

    // preedit / commit: declarative は on_focused_input が消費しなければ on_input へ
    // 落とすが、 scene_app はフォーカス中の要素が無いとどこにも届かない。
    for kind in [K::ImePreedit, K::ImeCommit] {
        assert_eq!(
            sabitori::declarative::input_delivery(kind),
            Delivery::ToApp,
            "declarative: {kind:?}"
        );
        assert_eq!(
            sabitori::scene_app::input_delivery(kind),
            Delivery::FocusedOnly,
            "scene_app: {kind:?}"
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
