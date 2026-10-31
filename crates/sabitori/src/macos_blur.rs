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
//!
//! 要素 1 つの箱だけをぼかす ([`sabitori_core::Element::backdrop_blur`]) のも同じ
//! 仕掛けで、箱ごとに 1 枚を [`BackdropViews`] が毎フレーム箱に合わせて動かす。

#![cfg(target_os = "macos")]

use objc2::msg_send;
use objc2::runtime::AnyObject;
use objc2::{Encoding, RefEncode};
use objc2_app_kit::NSView;
use objc2_foundation::{NSPoint, NSRect, NSSize};
use sabitori_core::build::BackdropRegion;
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

/// The winit contentView and its superview (the frame view the blur views
/// go into), or `None` if the window isn't an AppKit one.
fn content_and_frame_views(window: &Window) -> Option<(*mut AnyObject, *mut AnyObject)> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };
    let content_view = appkit.ns_view.as_ptr() as *mut AnyObject;
    let frame_view: *mut AnyObject = unsafe { msg_send![content_view, superview] };
    (!frame_view.is_null()).then_some((content_view, frame_view))
}

/// Make the wgpu CAMetalLayer non-opaque so transparent fragments
/// composite onto the blur underneath. The CAMetalLayer is the
/// contentView's own backing layer when winit creates the view
/// layer-backed (which it does for transparent windows).
unsafe fn clear_backing_layer(content_view: *mut AnyObject) {
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
}

/// A new NSVisualEffectView (+1 retained) with `frame`, sampling what is
/// behind the window, inserted just below `content_view` in `frame_view`.
unsafe fn new_effect_view(
    frame_view: *mut AnyObject,
    content_view: *mut AnyObject,
    frame: NSRect,
    blur: BackdropBlur,
) -> Option<*mut AnyObject> {
    // Allocate NSVisualEffectView. Use raw class lookup so we don't
    // require the NSVisualEffectView feature on objc2-app-kit.
    let cls = objc2::runtime::AnyClass::get(c"NSVisualEffectView")?;
    let fx_alloc: *mut AnyObject = msg_send![cls, alloc];
    let fx: *mut AnyObject = msg_send![fx_alloc, initWithFrame: frame];
    if fx.is_null() {
        return None;
    }
    let _: () = msg_send![fx, setMaterial: material_value(blur)];
    // BlendingMode 0 = BehindWindow (samples wallpaper / windows behind).
    let _: () = msg_send![fx, setBlendingMode: 0_i64];
    // State 1 = Active (always on; doesn't fade with window key state).
    let _: () = msg_send![fx, setState: 1_i64];

    // Pin the effect view's appearance to dark Aqua. Without this,
    // when the host app loses focus AppKit propagates the active
    // app's effective appearance into our view's appearance chain,
    // and the vibrancy material (which always tracks effective
    // appearance, regardless of `state`) shifts toward the light
    // variant — visible as a pale "wash-out" the moment the user
    // clicks into another app. The window-level appearance lock
    // alone doesn't reach this view because subviews look up
    // their appearance via the responder chain at draw time.
    if let Some(appearance_cls) = objc2::runtime::AnyClass::get(c"NSAppearance") {
        let dark_name = objc2_foundation::NSString::from_str("NSAppearanceNameDarkAqua");
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
        relativeTo: content_view
    ];
    Some(fx)
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
    let Some((content_view, frame_view)) = content_and_frame_views(window) else {
        return;
    };

    unsafe {
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

        clear_backing_layer(content_view);
        let Some(fx) = new_effect_view(frame_view, content_view, fx_frame, blur) else {
            return;
        };
        let _: () = msg_send![fx, setAutoresizingMask: fx_autoresize];
        // The frame view keeps it alive from here on.
        let _: () = msg_send![fx, release];
    }
}

/// What one region view was last set to, so a frame that changes nothing
/// sends AppKit nothing.
#[derive(Clone, Copy, PartialEq)]
struct Applied {
    frame: [f64; 4],
    radius: f64,
    alpha: f64,
    material: BackdropBlur,
}

struct RegionView {
    /// NSVisualEffectView, +1 retained by us (the frame view holds another).
    view: *mut AnyObject,
    material: BackdropBlur,
    /// `None` = hidden.
    applied: Option<Applied>,
}

/// One window's blur views for [`sabitori_core::Element::backdrop_blur`]:
/// a view per region, moved onto its element's box every frame. Views
/// left over when there are fewer regions are hidden, not destroyed, so a
/// popup opening and closing doesn't churn AppKit views.
#[derive(Default)]
pub(crate) struct BackdropViews {
    views: Vec<RegionView>,
    /// The backing layer was made non-opaque (once per window).
    cleared: bool,
}

impl BackdropViews {
    /// Match the blur views to this frame's `regions` (logical px, top-left
    /// origin — the window's content coordinates).
    pub(crate) fn sync(&mut self, window: &Window, regions: &[BackdropRegion]) {
        if regions.is_empty() && self.views.iter().all(|v| v.applied.is_none()) {
            return;
        }
        let Some((content_view, frame_view)) = content_and_frame_views(window) else {
            return;
        };
        unsafe {
            if !self.cleared {
                clear_backing_layer(content_view);
                self.cleared = true;
            }
            let content_ref: &NSView = &*(content_view as *const NSView);
            let content_h = content_ref.bounds().size.height;
            let flipped: bool = msg_send![content_view, isFlipped];
            for (i, r) in regions.iter().enumerate() {
                let Some(rect) = visible_rect(r) else {
                    self.hide(i);
                    continue;
                };
                let (x, y, w, h) = (
                    rect.origin.x as f64,
                    rect.origin.y as f64,
                    rect.size.width as f64,
                    rect.size.height as f64,
                );
                let in_content = NSRect {
                    origin: NSPoint::new(x, if flipped { y } else { content_h - y - h }),
                    size: NSSize::new(w, h),
                };
                let frame: NSRect = msg_send![content_view, convertRect: in_content, toView: frame_view];
                let want = Applied {
                    frame: [frame.origin.x, frame.origin.y, frame.size.width, frame.size.height],
                    radius: r.corner_radii.top_left
                        .max(r.corner_radii.top_right)
                        .max(r.corner_radii.bottom_right)
                        .max(r.corner_radii.bottom_left) as f64,
                    alpha: r.opacity.clamp(0.0, 1.0) as f64,
                    material: r.material,
                };
                if i == self.views.len() {
                    let Some(view) = new_effect_view(frame_view, content_view, frame, r.material) else {
                        return;
                    };
                    let _: () = msg_send![view, setWantsLayer: true];
                    self.views.push(RegionView { view, material: r.material, applied: None });
                }
                let rv = &mut self.views[i];
                if rv.applied == Some(want) {
                    continue;
                }
                let view = rv.view;
                if rv.material != want.material {
                    let _: () = msg_send![view, setMaterial: material_value(want.material)];
                    rv.material = want.material;
                }
                let _: () = msg_send![view, setFrame: frame];
                let _: () = msg_send![view, setAlphaValue: want.alpha];
                let layer: *mut AnyObject = msg_send![view, layer];
                if !layer.is_null() {
                    let _: () = msg_send![layer, setCornerRadius: want.radius];
                    let _: () = msg_send![layer, setMasksToBounds: true];
                }
                if rv.applied.is_none() {
                    let _: () = msg_send![view, setHidden: false];
                }
                rv.applied = Some(want);
            }
            for i in regions.len()..self.views.len() {
                self.hide(i);
            }
        }
    }

    fn hide(&mut self, i: usize) {
        let Some(rv) = self.views.get_mut(i) else { return };
        if rv.applied.take().is_some() {
            unsafe {
                let _: () = msg_send![rv.view, setHidden: true];
            }
        }
    }
}

impl Drop for BackdropViews {
    fn drop(&mut self) {
        for rv in self.views.drain(..) {
            unsafe {
                let _: () = msg_send![rv.view, removeFromSuperview];
                let _: () = msg_send![rv.view, release];
            }
        }
    }
}

/// The part of `r` its clip leaves showing; `None` when nothing shows.
fn visible_rect(r: &BackdropRegion) -> Option<sabitori_core::Rect> {
    if r.opacity <= 0.0 || r.rect.size.width <= 0.0 || r.rect.size.height <= 0.0 {
        return None;
    }
    let Some(c) = r.clip else { return Some(r.rect) };
    let x0 = r.rect.origin.x.max(c.origin.x);
    let y0 = r.rect.origin.y.max(c.origin.y);
    let x1 = (r.rect.origin.x + r.rect.size.width).min(c.origin.x + c.size.width);
    let y1 = (r.rect.origin.y + r.rect.size.height).min(c.origin.y + c.size.height);
    (x1 > x0 && y1 > y0).then(|| sabitori_core::Rect::new(x0, y0, x1 - x0, y1 - y0))
}
