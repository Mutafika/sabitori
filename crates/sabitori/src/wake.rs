//! よそのスレッドからランタイムを起こす ([`wake`])。
//!
//! # なぜ要るか
//!
//! アプリの状態が UI のスレッドの外で変わることがある — 端末なら、子プロセスの
//! 出力を読むスレッドが画面の中身を書き換える。ランタイムはそれを
//! [`DeclarativeApp::poll_dirty`] で毎周回たずねて知る。デスクトップは周回を
//! 16ms ごとに回し続けるので、たずねるだけで間に合う (待機中の負荷は小さい)。
//!
//! 電池で動く携帯 (iOS / Android) はそうはいかない。iPad の mearie は、描くものが
//! 何も無いのに 1 秒 60 回起きて CPU を 6% 使い続け、電池を減らしていた。携帯の
//! ランタイムは描くものが無ければ寝る (たずねるのは 0.25 秒に 1 回) ので、
//! 外で何か変わったら、変えた側がここを呼んで起こす。
//!
//! 呼ぶのは何度でもよい: 起こしたのにまだ汲まれていない間は、何もしない。
//! ランタイムが立っていない (テスト・ヘッドレス) なら、何もしない。
//!
//! [`DeclarativeApp::poll_dirty`]: crate::DeclarativeApp::poll_dirty

use std::sync::atomic::{AtomicBool, Ordering};

/// 起こしたのに、まだ周回が汲んでいない。
static PENDING: AtomicBool = AtomicBool::new(false);

#[cfg(not(target_arch = "wasm32"))]
static PROXY: std::sync::OnceLock<std::sync::Mutex<winit::event_loop::EventLoopProxy<()>>> =
    std::sync::OnceLock::new();

/// ランタイムを起こして、次の周回で [`DeclarativeApp::poll_dirty`] をたずね
/// させる。どのスレッドから呼んでもよい。
///
/// [`DeclarativeApp::poll_dirty`]: crate::DeclarativeApp::poll_dirty
pub fn wake() {
    if PENDING.swap(true, Ordering::AcqRel) {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(proxy) = PROXY.get()
        && let Ok(proxy) = proxy.lock()
    {
        let _ = proxy.send_event(());
    }
}

/// ランタイムが起こされる先を受け取る (`run_declarative` が 1 度だけ)。
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn install(proxy: winit::event_loop::EventLoopProxy<()>) {
    let _ = PROXY.set(std::sync::Mutex::new(proxy));
}

/// 起こされて、まだ汲んでいない。
#[cfg_attr(not(any(target_os = "ios", target_os = "android")), allow(dead_code))]
pub(crate) fn pending() -> bool {
    PENDING.load(Ordering::Acquire)
}

/// 周回が汲んだ。これより後の [`wake`] はまた起こす。
pub(crate) fn taken() {
    PENDING.store(false, Ordering::Release);
}
