//! 焦点の無い別窓にも、指の動きを届ける (macOS)。
//!
//! AppKit は `mouseMoved:` を**キーの窓にだけ**送る。Quick Look のように焦点を
//! 奪わない別窓 (`ExtraWindow::active == false`) は、指を乗せても動きが来ず、
//! ホバーが効かない (押した時とホイールは、winit がその場の位置を添えて届ける)。
//! `NSTrackingActiveAlways` の tracking area を窓の view に貼ると、キーかどうかに
//! 関係なく view の `mouseMoved:` が呼ばれ、winit が `CursorMoved` にする。
//!
//! 窓がキーの間は、AppKit の配る分と tracking area の分で同じ動きが 2 回届くことが
//! ある。位置を置き直すだけなので害は無い (2 回目は何も変わらない)。

#![cfg(target_os = "macos")]

use objc2::runtime::AnyObject;
use objc2::AllocAnyThread;
use objc2_app_kit::{NSTrackingArea, NSTrackingAreaOptions, NSView};
use objc2_foundation::NSRect;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// 窓の view に、いつでも `mouseMoved:` を受ける tracking area を貼る。
/// 窓を作った時に 1 回。範囲は見えている所に追従する (`InVisibleRect`) ので、
/// 大きさが変わっても貼り直さなくてよい。
pub(crate) fn track_mouse_moved(window: &Window) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    // SAFETY: winit の AppKit ハンドルの `ns_view` は、窓が生きている間有効な NSView。
    let view: &NSView = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
    let owner: &AnyObject = view;
    let options = NSTrackingAreaOptions::MouseMoved
        | NSTrackingAreaOptions::ActiveAlways
        | NSTrackingAreaOptions::InVisibleRect;
    // SAFETY: owner は `mouseMoved:` を実装する winit の view。userInfo は無し。
    let area = unsafe {
        NSTrackingArea::initWithRect_options_owner_userInfo(
            NSTrackingArea::alloc(),
            NSRect::ZERO,
            options,
            Some(owner),
            None,
        )
    };
    view.addTrackingArea(&area);
}
