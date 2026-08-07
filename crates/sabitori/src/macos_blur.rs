//! NSVisualEffectView backdrop-blur attachment for transparent windows.
//!
//! Apps that opt into [`DeclarativeApp::backdrop_blur`] get a translucent
//! backdrop behind their wgpu surface — wallpaper visible through gaps,
//! Caelestia/Material 3-style islands, etc.
//!
//! The implementation is the well-known "frame view" trick: NSVisualEffectView
//! is added as a sibling of the winit contentView, positioned below it in
//! the parent (frame) NSView. The wgpu CAMetalLayer is forced non-opaque so
//! transparent regions of the rendered surface composite onto the blur.
//!
//! `[contentView superview]` returning the frame view is technically a
//! private hierarchy detail, but it's stable across macOS releases and is
//! used by every menu-bar replacement / dock alternative I know of.

#![cfg(target_os = "macos")]

use objc2::msg_send;
use objc2::runtime::AnyObject;
use objc2::{Encoding, RefEncode};
use objc2_app_kit::NSView;
use objc2_foundation::{NSPoint, NSRect, NSSize};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

use crate::declarative::BackdropBlur;

/// Opaque CGColor marker. CALayer's `setBackgroundColor:` takes a
/// `CGColorRef` (encoded `^{CGColor=}`), NOT an Objective-C object —
/// passing a `*mut AnyObject` (encoded `^@`) trips objc2 0.6's runtime
/// signature validator. We never construct one of these; we only need
/// a `*mut CGColor` pointer with the right encoding to pass nil.
#[repr(C)]
struct CGColor {
    _private: [u8; 0],
}

unsafe impl RefEncode for CGColor {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("CGColor", &[]));
}

fn material_value(blur: BackdropBlur) -> i64 {
    // Values from <AppKit/NSVisualEffectView.h>.
    match blur {
        BackdropBlur::Titlebar => 3,
        BackdropBlur::Menu => 5,
        BackdropBlur::Popover => 6,
        BackdropBlur::Sidebar => 7,
        BackdropBlur::UnderWindow => 9,
        BackdropBlur::HeaderView => 10,
        BackdropBlur::Hud => 12,
    }
}

/// Install an NSVisualEffectView behind the window's content view. Idempotent
/// in spirit but should be called exactly once at window-creation time.
///
/// `top_strip_height`: if `Some(h)`, the blur view covers only the top `h`
/// logical pixels of the window, anchored to the top edge with width-sizable
/// autoresize. Useful for panel apps that have a fullscreen NSWindow but
/// only want blur in the bar zone — without this, the rest of the window's
/// transparent regions composite onto the blur instead of showing the
/// actual desktop / app windows beneath. `None` covers the entire frame.
///
/// Caller must have set the window to transparent — otherwise the wgpu
/// surface paints an opaque background and you'll see no blur.
pub fn attach_backdrop_blur(window: &Window, blur: BackdropBlur, top_strip_height: Option<f32>) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let appkit = match handle.as_raw() {
        RawWindowHandle::AppKit(h) => h,
        _ => return,
    };
    let ns_view_ptr = appkit.ns_view.as_ptr() as *mut AnyObject;

    unsafe {
        let content_view: &NSView = &*(ns_view_ptr as *const NSView);
        let frame_view: *mut AnyObject = msg_send![content_view, superview];
        if frame_view.is_null() {
            return;
        }
        let frame_view_ref: &NSView = &*(frame_view as *const NSView);
        let frame_bounds: NSRect = frame_view_ref.bounds();

        // Pick the blur view's frame + autoresize. AppKit window coords are
        // origin-bottom-left, so a top strip of height `h` lives at
        // y = frame_h - h. WidthSizable + MinYMargin keeps it pinned to the
        // top edge as the window resizes.
        let (fx_frame, fx_autoresize): (NSRect, u64) = match top_strip_height {
            None => (frame_bounds, /* width|height sizable */ 2 | 16),
            Some(h) => {
                let h = h as f64;
                let rect = NSRect {
                    origin: NSPoint::new(0.0, frame_bounds.size.height - h),
                    size: NSSize::new(frame_bounds.size.width, h),
                };
                (rect, /* width sizable | min-Y-margin */ 2 | 8)
            }
        };

        // Make the wgpu CAMetalLayer non-opaque so transparent fragments
        // composite onto the blur underneath. The CAMetalLayer is the
        // contentView's own backing layer when winit creates the view
        // layer-backed (which it does for transparent windows).
        let backing_layer: *mut AnyObject = msg_send![content_view, layer];
        if !backing_layer.is_null() {
            let _: () = msg_send![backing_layer, setOpaque: false];
            // Clear backgroundColor so the layer doesn't paint over blur in
            // sub-pixel gaps. Passing nil resets to the default (clear).
            // Pointer must be typed as `*mut CGColor` — the runtime
            // expects `^{CGColor=}`, not `^@`.
            let nil_color: *mut CGColor = std::ptr::null_mut();
            let _: () = msg_send![backing_layer, setBackgroundColor: nil_color];
        }

        // Allocate NSVisualEffectView. Use raw class lookup so we don't
        // require the NSVisualEffectView feature on objc2-app-kit.
        let cls = match objc2::runtime::AnyClass::get(c"NSVisualEffectView") {
            Some(c) => c,
            None => return,
        };
        let fx_alloc: *mut AnyObject = msg_send![cls, alloc];
        let fx: *mut AnyObject = msg_send![fx_alloc, initWithFrame: fx_frame];
        if fx.is_null() {
            return;
        }
        let _: () = msg_send![fx, setMaterial: material_value(blur)];
        // BlendingMode 0 = BehindWindow (samples wallpaper / windows behind).
        let _: () = msg_send![fx, setBlendingMode: 0_i64];
        // State 1 = Active (always on; doesn't fade with window key state).
        let _: () = msg_send![fx, setState: 1_i64];
        let _: () = msg_send![fx, setAutoresizingMask: fx_autoresize];

        // Pin the effect view's appearance to dark Aqua. Without this,
        // when the host app loses focus AppKit propagates the active
        // app's effective appearance into our view's appearance chain,
        // and the vibrancy material (which always tracks effective
        // appearance, regardless of `state`) shifts toward the light
        // variant — visible as a pale "wash-out" the moment the user
        // clicks into another app. The window-level appearance lock
        // alone doesn't reach this view because subviews look up
        // their appearance via the responder chain at draw time.
        if let Some(appearance_cls) =
            objc2::runtime::AnyClass::get(c"NSAppearance")
        {
            let dark_name =
                objc2_foundation::NSString::from_str("NSAppearanceNameDarkAqua");
            let appearance: *mut AnyObject = msg_send![
                appearance_cls,
                appearanceNamed: &*dark_name,
            ];
            if !appearance.is_null() {
                let _: () = msg_send![fx, setAppearance: appearance];
            }
        }

        // NSWindowBelow = -1. Inserts fx underneath contentView in the
        // frame-view's subview list. (NSWindowOut = 0, NSWindowAbove = 1 —
        // and addSubview:positioned: rejects anything other than ±1 with a
        // libsystem_c assertion abort, not a graceful failure.)
        let _: () = msg_send![
            frame_view,
            addSubview: fx,
            positioned: -1_i64,
            relativeTo: content_view as *const NSView as *mut AnyObject
        ];
    }
}
