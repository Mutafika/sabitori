//! macOS: Finder の「このアプリで開く」、`open -a` / `open -b`、Dock のアイコンへの
//! ドロップなど、LaunchServices 経由で「このアプリで開いて」と渡されたファイル・
//! フォルダを受け取る。
//!
//! 渡し方は `application:openURLs:` (NSApplicationDelegate)。winit 0.30 の delegate
//! (`WinitApplicationDelegate`) はこれを実装していないので、event loop を作った直後
//! (= delegate が張られた後、起動処理より前) に実行時にメソッドを足す。起動のきっかけ
//! になったパスも `applicationDidFinishLaunching:` より前に届くので、この順なら
//! 取りこぼさない。
//!
//! 届いたパスは main スレッドの列に積み、`wake` で event loop を起こす。
//! ランタイムが `take()` で汲んで [`DeclarativeApp::on_open_paths`] に渡す。
//!
//! `.app` として起動されたときだけ届く (`cargo run` の素の実行ファイルには
//! LaunchServices から何も来ない)。
//!
//! 他のアプリの「Finder に表示」(`NSFileViewer` を自分の bundle id にしたとき) は
//! ここには来ない — macOS 26 で試した限り、openURLs にも Apple Event `misc/mvis` にも
//! 届かなかった。
//!
//! 動いている最中に Dock のアイコン・Finder・`open -a` で開き直されたこと
//! (`applicationShouldHandleReopen:hasVisibleWindows:`) も同じ道で数えて
//! [`DeclarativeApp::on_reopen`] に渡す。
//!
//! [`DeclarativeApp::on_open_paths`]: crate::DeclarativeApp::on_open_paths
//! [`DeclarativeApp::on_reopen`]: crate::DeclarativeApp::on_reopen

#![cfg(target_os = "macos")]

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{class, msg_send, sel};
use objc2_foundation::{NSArray, NSURL};

thread_local! {
    static QUEUE: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
    static REOPENS: Cell<u32> = const { Cell::new(0) };
    static WAKE: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

/// NSApp の delegate に `application:openURLs:` と
/// `applicationShouldHandleReopen:hasVisibleWindows:` を足す。event loop を作った後、
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
            unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject, *mut NSArray<NSURL>),
            Imp,
        >(open_urls);
        // 戻り NO = 既に実装がある (winit が将来実装した等)。その時は向こうに任せる。
        objc2::ffi::class_addMethod(cls.cast_mut(), sel!(application:openURLs:), imp, c"v@:@@".as_ptr());
        let imp: Imp = std::mem::transmute::<
            unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject, Bool) -> Bool,
            Imp,
        >(reopen);
        // BOOL は arm64 では bool (B)、x86_64 では signed char (c)。
        #[cfg(target_arch = "aarch64")]
        let types = c"B@:@B";
        #[cfg(not(target_arch = "aarch64"))]
        let types = c"c@:@c";
        objc2::ffi::class_addMethod(
            cls.cast_mut(),
            sel!(applicationShouldHandleReopen:hasVisibleWindows:),
            imp,
            types.as_ptr(),
        );
    }
}

/// 届いたパスを全部取り出す (届いた順)。
pub(crate) fn take() -> Vec<PathBuf> {
    QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()))
}

/// 開き直された回数を取り出して 0 に戻す。
pub(crate) fn take_reopens() -> u32 {
    REOPENS.with(|n| n.replace(0))
}

fn wake() {
    WAKE.with(|w| {
        if let Some(wake) = &*w.borrow() {
            wake();
        }
    });
}

/// 開き直された。数えて起こすだけで、見た目は AppKit の決まりのまま (YES) —
/// 窓が全部しまわれていれば 1 枚戻し、あれば前に出すだけ。窓を足すかはアプリが決める。
unsafe extern "C-unwind" fn reopen(_this: *mut AnyObject, _cmd: Sel, _app: *mut AnyObject, _visible: Bool) -> Bool {
    REOPENS.with(|n| n.set(n.get() + 1));
    wake();
    Bool::YES
}

unsafe extern "C-unwind" fn open_urls(
    _this: *mut AnyObject,
    _cmd: Sel,
    _app: *mut AnyObject,
    urls: *mut NSArray<NSURL>,
) {
    let Some(urls) = (unsafe { urls.as_ref() }) else { return };
    let paths: Vec<PathBuf> = urls
        .iter()
        .filter_map(|u| u.path().map(|p| PathBuf::from(p.to_string())))
        .collect();
    if paths.is_empty() {
        return;
    }
    QUEUE.with(|q| q.borrow_mut().extend(paths));
    wake();
}
