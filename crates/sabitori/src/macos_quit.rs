//! macOS: 「終了」(⌘Q・Dock の「終了」・ログアウト) を、終わる前にアプリへ聞く。
//!
//! AppKit は終わる前に delegate の `applicationShouldTerminate:` を呼ぶ。winit 0.30 の
//! delegate はこれを実装していない (= 聞かずに終わる) ので、[`crate::macos_open`] と
//! 同じく、event loop を作った直後に実行時にメソッドを足す。
//!
//! アプリはランタイムの中にいて、ここからは触れない。なのでその場では答えず
//! `NSTerminateLater` を返して待たせ、ループを起こす。ランタイムが窓ごとのアプリに
//! [`DeclarativeApp::on_close_requested`] で聞き、[`reply`] で答える。待っている間
//! AppKit は `NSModalPanelRunLoopMode` でループを回すが、winit の観測と起こし口は
//! common modes に載っているので、そのまま届く。
//!
//! [`DeclarativeApp::on_close_requested`]: crate::DeclarativeApp::on_close_requested

#![cfg(target_os = "macos")]

use std::cell::{Cell, RefCell};

use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{class, msg_send, sel};

thread_local! {
    static PENDING: Cell<bool> = const { Cell::new(false) };
    static WAKE: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

/// `NSApplicationTerminateReply::NSTerminateLater`
const TERMINATE_LATER: usize = 2;

/// NSApp の delegate に `applicationShouldTerminate:` を足す。event loop を作った後、
/// 回し始める前に main スレッドで 1 度だけ呼ぶ。
pub(crate) fn install(wake: impl Fn() + 'static) {
    WAKE.with(|w| *w.borrow_mut() = Some(Box::new(wake)));
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let delegate: *mut AnyObject = msg_send![app, delegate];
        let Some(delegate) = delegate.as_ref() else { return };
        let cls: *const AnyClass = delegate.class();
        let imp: Imp = std::mem::transmute::<
            unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize,
            Imp,
        >(should_terminate);
        // 戻りは NSApplicationTerminateReply (NSUInteger = Q)。戻り NO = 既に実装がある
        // (winit が将来実装した等)。その時は向こうに任せる。
        objc2::ffi::class_addMethod(
            cls.cast_mut(),
            sel!(applicationShouldTerminate:),
            imp,
            c"Q@:@".as_ptr(),
        );
    }
}

/// 終了を聞かれていたら `true` (1 回取り出すと下りる)。
pub(crate) fn take() -> bool {
    PENDING.with(|p| p.replace(false))
}

/// 待たせていた終了に答える。`true` なら終わる、`false` なら終わるのをやめる。
pub(crate) fn reply(ok: bool) {
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let _: () = msg_send![app, replyToApplicationShouldTerminate: Bool::new(ok)];
    }
}

unsafe extern "C-unwind" fn should_terminate(_this: *mut AnyObject, _cmd: Sel, _app: *mut AnyObject) -> usize {
    PENDING.with(|p| p.set(true));
    WAKE.with(|w| {
        if let Some(wake) = &*w.borrow() {
            wake();
        }
    });
    TERMINATE_LATER
}
