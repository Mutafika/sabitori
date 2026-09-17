//! DOM 起点の出来事でランタイムを起こす。
//!
//! # なぜ要るか
//!
//! [`DeclarativeApp::lazy_render`] は既定で `true` — つまり**何も起きていない
//! フレームは描かない**。起こすのは winit が配るイベント (クリック・キー・
//! リサイズ) で、そこで `dirty` が立つ。
//!
//! ところが web の橋渡し ([`crate::web_ime`] / [`crate::web_history`]) が拾う
//! のは **winit を通らない DOM のイベント**で、キューに積んだだけでは誰も
//! 汲みに来ない。汲むコードは描画フレームの中にあるので、
//!
//! > 描かない → 汲まない → `dirty` が立たない → 描かない
//!
//! で止まる。実際に踏んだ: **戻るボタンで URL は変わるのに画面が変わらない**
//! ([#74](https://github.com/Mutafika/sabitori/issues/74))。ソフトキーボードで
//! 打った文字も同じ形で止まる — canvas を触っていないので winit のイベントが
//! 1 つも来ず、次に画面を触るまで反映されない。
//!
//! なので DOM のイベントで何か積んだら、必ずここを呼んで 1 フレーム起こす。
//!
//! [`DeclarativeApp::lazy_render`]: crate::DeclarativeApp::lazy_render

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::sync::Arc;

thread_local! {
    static WINDOW: RefCell<Option<Arc<winit::window::Window>>> = const { RefCell::new(None) };
}

/// 起こす相手を覚える。ランタイムが窓を作った後に 1 回呼ぶ。
pub fn set_window(window: Arc<winit::window::Window>) {
    WINDOW.with(|w| *w.borrow_mut() = Some(window));
}

/// 1 フレーム描かせる。窓がまだ無ければ何もしない。
pub fn wake() {
    WINDOW.with(|w| {
        if let Some(window) = w.borrow().as_ref() {
            window.request_redraw();
        }
    });
}
