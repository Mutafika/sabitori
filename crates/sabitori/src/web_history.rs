//! Web の URL と戻るボタン (History) の橋渡し。
//!
//! wasm のアプリは canvas 1 枚なので、何もしないと**ブラウザから見て 1 ページ**
//! でしかない。戻るボタンを押すとアプリごと前のサイトへ行き、再読み込みすると
//! 必ず最初の画面 (ログイン) に戻り、詳細画面の URL を人に渡せない
//! ([#74](https://github.com/Mutafika/sabitori/issues/74))。
//!
//! ここがやるのは 2 つだけ:
//!
//! - アプリが名乗る断片 ([`DeclarativeApp::url_fragment`]) が変わったら
//!   `history.pushState` する
//! - `popstate` と起動時の URL を [`DeclarativeApp::on_url_changed`] へ渡す
//!
//! **経路の意味には触らない。** `#/vehicle/42` と `Route::VehicleDetail(42)` の
//! 対応はアプリが持つ。ランタイムが Route を知ろうとすると、アプリごとに違う
//! 規約を型に押し込むことになる。
//!
//! [`DeclarativeApp::url_fragment`]: crate::DeclarativeApp::url_fragment
//! [`DeclarativeApp::on_url_changed`]: crate::DeclarativeApp::on_url_changed

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

thread_local! {
    /// `popstate` と起動時の URL。ランタイムが毎フレーム [`take`] する。
    static INCOMING: RefCell<Option<String>> = const { RefCell::new(None) };
    /// 直近にランタイムが押した / 読んだ断片。これと違う値をアプリが名乗った
    /// ときだけ `pushState` する。
    static CURRENT: RefCell<Option<String>> = const { RefCell::new(None) };
    /// `popstate` を張ったか。
    static HOOKED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// `popstate` を張り、起動時の URL を 1 回だけ流す。2 回目以降は何もしない。
pub fn ensure_attached() {
    HOOKED.with(|h| {
        if h.get() {
            return;
        }
        let Some(window) = web_sys::window() else { return };

        // 起動時の URL。**セッションが生きているなら、その画面へ直行できる。**
        // 再読み込みで必ずログインに戻る、を避けるのがこの 1 行。
        let initial = current_hash();
        if !initial.is_empty() {
            INCOMING.with(|i| *i.borrow_mut() = Some(initial.clone()));
        }
        CURRENT.with(|c| *c.borrow_mut() = Some(initial));

        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
            let hash = current_hash();
            CURRENT.with(|c| *c.borrow_mut() = Some(hash.clone()));
            INCOMING.with(|i| *i.borrow_mut() = Some(hash));
            // 戻るボタンは winit を通らない。起こさないと「URL は変わったのに
            // 画面が変わらない」で止まる (実際に踏んだ)。
            crate::web_wake::wake();
        });
        let ok = window
            .add_event_listener_with_callback("popstate", cb.as_ref().unchecked_ref())
            .is_ok();
        // listener は張りっぱなしにする (アプリと同じ寿命)。
        cb.forget();
        h.set(ok);
    });
}

/// 戻る / 進む / 起動時の URL があれば取り出す。汲むと下りる。
pub fn take() -> Option<String> {
    INCOMING.with(|i| i.borrow_mut().take())
}

/// アプリが名乗る断片を反映する。変わったときだけ `pushState` する。
///
/// 毎フレーム呼ばれるので、**同じ値なら何もしない**のが要点。素直に毎回
/// `pushState` すると履歴が 60 回/秒 積まれて、戻るボタンが効かなくなる。
pub fn sync(fragment: Option<&str>) {
    let Some(fragment) = fragment else { return };
    let changed = CURRENT.with(|c| c.borrow().as_deref() != Some(fragment));
    if !changed {
        return;
    }
    CURRENT.with(|c| *c.borrow_mut() = Some(fragment.to_string()));

    let Some(window) = web_sys::window() else { return };
    let Ok(history) = window.history() else { return };
    // `pushState` は popstate を出さないので、自分の遷移を自分で解釈し直す
    // 心配は無い (`on_url_changed` はアプリ起点の遷移では鳴らない)。
    let _ = history.push_state_with_url(&JsValue::NULL, "", Some(fragment));
}

fn current_hash() -> String {
    web_sys::window()
        .and_then(|w| w.location().hash().ok())
        .unwrap_or_default()
}
