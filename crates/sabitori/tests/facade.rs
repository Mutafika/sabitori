//! ファサード (`sabitori::*`) だけで下流が書けることを、 コンパイル時に保証する。
//!
//! 単体テストではなく **integration test** なのが要点。 crate の外から `use sabitori::…`
//! するので、 下流と全く同じ解決経路を通る。 crate 内の `#[cfg(test)] mod tests` では
//! `sabitori_input::` に直接触れてしまい、 re-export の漏れを検出できない。
//!
//! 背景: `PointerKind` が re-export から漏れていて、 `InputEvent::PointerPressed` が
//! **ファサード経由では構築できなかった**。 型自体は `InputEvent` として見えているのに
//! 必須フィールドの型が名前で書けない、 という形の漏れで、 sabitori 側は 13 クレート全部を
//! ワークスペース内から参照するためワークスペースのビルドでは永遠に気づけない。
//! 下流クレートのリンク時に初めて出た。
//!
//! アサーションが薄いのは意図的で、 **このファイルが通ること自体がテスト**。

use sabitori::{
    ActivePointer, InputEvent, InteractionState, Key, Modifiers, MouseButton, Point, PointerId,
    PointerKind, PointerState, BUTTON_PRIMARY, MOUSE_POINTER_ID,
};

/// ポインタ系イベントを 4 種とも組む。 `kind` の型が名前で書けないと、 ここが落ちる。
#[test]
fn pointer_events_are_constructible_through_the_facade() {
    let at = Point::new(1.0, 2.0);
    let events = [
        InputEvent::PointerMoved { id: MOUSE_POINTER_ID, kind: PointerKind::Mouse, position: at },
        InputEvent::PointerPressed {
            id: MOUSE_POINTER_ID,
            kind: PointerKind::Mouse,
            position: at,
            button: Some(MouseButton::Left),
        },
        InputEvent::PointerReleased {
            id: 7 as PointerId,
            kind: PointerKind::Touch,
            position: at,
            button: None,
        },
        InputEvent::PointerCancelled { id: 8, kind: PointerKind::Pen },
    ];
    assert_eq!(events.len(), 4);
}

/// `PointerState` は re-export されているが、 `find` の戻り値と `upsert` の引数は
/// `ActivePointer`、 id は `PointerId`、 `buttons` の判定には `BUTTON_*` が要る。
/// どれか 1 つでも欠けると `PointerState` はファサード越しには使い物にならない。
#[test]
fn pointer_state_round_trips_through_the_facade() {
    let mut st = PointerState::default();
    st.upsert(ActivePointer {
        id: MOUSE_POINTER_ID,
        kind: PointerKind::Mouse,
        position: Point::new(3.0, 4.0),
        buttons: BUTTON_PRIMARY,
    });

    let found: Option<&ActivePointer> = st.find(MOUSE_POINTER_ID);
    assert!(found.is_some(), "upsert したポインタが find で引けない");
    assert!(st.primary_pressed());

    let removed: Option<ActivePointer> = st.remove(MOUSE_POINTER_ID);
    assert!(removed.is_some());
    assert!(!st.primary_pressed());
}

/// キーボード / 修飾キー / ノード状態も同様にファサードから名前で書けること。
#[test]
fn keyboard_and_state_types_are_reachable() {
    let ev = InputEvent::KeyInput {
        key: Key::Escape,
        pressed: true,
        modifiers: Modifiers::default(),
    };
    assert!(matches!(ev, InputEvent::KeyInput { key: Key::Escape, pressed: true, .. }));

    let s = InteractionState { hovered: true, pressed: false, focused: true };
    assert!(s.hovered && s.focused);
}
