//! macOS native drag & drop using objc2.

#![cfg(target_os = "macos")]

use std::path::Path;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, AllocAnyThread, DefinedClass, MainThreadOnly, msg_send};
use objc2_app_kit::*;
use objc2_foundation::*;

// ---------------------------------------------------------------------------
// Minimal NSDraggingSource implementation
// ---------------------------------------------------------------------------

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SabitoriDragSource"]
    // 受け側に許す操作 (写す / 移す…)。
    #[ivars = NSDragOperation]
    struct DragSource;

    unsafe impl NSObjectProtocol for DragSource {}

    unsafe impl NSDraggingSource for DragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn _source_operation_mask(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            self.operations()
        }
    }
);

impl DragSource {
    fn new(mtm: objc2::MainThreadMarker, operations: NSDragOperation) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(operations);
        unsafe { msg_send![super(this), init] }
    }

    fn operations(&self) -> NSDragOperation {
        *self.ivars()
    }
}

/// 写すことだけを許す (受け側が Finder でも写す)。
fn copy_only() -> NSDragOperation {
    NSDragOperation::Copy
}

/// 写すか移すかを受け側に任せる (Finder は自分の窓どうしと同じ決め方をする)。
fn copy_or_move() -> NSDragOperation {
    NSDragOperation::Copy | NSDragOperation::Move | NSDragOperation::Generic
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Get the current mouse position in window-local logical coordinates.
/// Works even during OS drag operations when winit doesn't send CursorMoved.
pub fn get_mouse_position(window: &winit::window::Window) -> Option<(f32, f32)> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = window.window_handle().ok()?;
    let ns_view_ptr = match handle.as_raw() {
        RawWindowHandle::AppKit(h) => h.ns_view.as_ptr(),
        _ => return None,
    };

    unsafe {
        let ns_view: &NSView = &*(ns_view_ptr as *const NSView);
        let ns_window = ns_view.window()?;

        // Get mouse location in screen coordinates
        let screen_loc = NSEvent::mouseLocation();
        // Convert to window coordinates
        let win_rect = NSRect::new(screen_loc, NSSize::new(0.0, 0.0));
        let win_loc = ns_window.convertPointFromScreen(screen_loc);

        // Flip Y (AppKit is bottom-up, we need top-down)
        let frame = ns_view.frame();
        let x = win_loc.x as f32;
        let y = (frame.size.height - win_loc.y) as f32;

        Some((x, y))
    }
}

/// Copy file paths to the macOS system clipboard.
pub fn copy_paths_to_clipboard(paths: &[&Path]) {
    if paths.is_empty() { return; }
    let text: Vec<String> = paths.iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    let joined = text.join("\n");
    use std::process::{Command, Stdio};
    use std::io::Write;
    if let Ok(mut child) = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        if let Some(ref mut stdin) = child.stdin {
            let _ = stdin.write_all(joined.as_bytes());
        }
        let _ = child.wait();
    }
}

/// Start an OS-level file drag session from a winit window.
/// The files can be dropped onto any app that accepts file drops.
///
/// 受け側には**写すことだけ**を許す — Finder に落としても、同じボリュームでも写す
/// (元を残したいクリップボードの履歴などに向く)。ファイラのように元の場所から
/// 持ち出すなら [`start_file_drag_movable`]。
pub fn start_file_drag(window: &winit::window::Window, paths: &[&Path]) -> bool {
    start_file_drag_with_preview(window, paths, None)
}

/// 写すか移すかを受け側に任せる OS のファイルドラッグ (ファイラ向け)。
///
/// [`start_file_drag`] と違い移すことも許すので、Finder は自分の窓どうしと同じく
/// 決める: 同じボリュームなら移し、違えば写す (⌥ で写す、⌘ で移す)。移したときは
/// 受け側が動かすので、こちらで消す物は無い。
pub fn start_file_drag_movable(window: &winit::window::Window, paths: &[&Path]) -> bool {
    start_drag(window, paths, None, copy_or_move())
}

/// Like [`start_file_drag`], but lets the caller supply the drag
/// preview image. `preview` is raw image bytes (PNG / TIFF / JPEG —
/// anything `NSImage initWithData:` recognises). When `None`, the
/// drag uses a 1×1 transparent placeholder so AppKit falls back to
/// whatever default icon the target accepts.
///
/// Yoink-style apps use this to show a thumbnail of the dragged
/// content under the cursor — e.g. matcha-shell renders the
/// clipboard image itself as the preview for an image entry, and
/// the file icon for a file entry.
pub fn start_file_drag_with_preview(
    window: &winit::window::Window,
    paths: &[&Path],
    preview: Option<&[u8]>,
) -> bool {
    start_drag(window, paths, preview, copy_only())
}

fn start_drag(
    window: &winit::window::Window,
    paths: &[&Path],
    preview: Option<&[u8]>,
    operations: NSDragOperation,
) -> bool {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    if paths.is_empty() { return false; }

    let handle = match window.window_handle() {
        Ok(h) => h,
        Err(_) => return false,
    };

    let ns_view_ptr = match handle.as_raw() {
        RawWindowHandle::AppKit(h) => h.ns_view.as_ptr(),
        _ => return false,
    };

    let Some(mtm) = objc2::MainThreadMarker::new() else { return false; };

    unsafe {
        let ns_view: &NSView = &*(ns_view_ptr as *const NSView);

        // Get current event from the application
        let app = NSApplication::sharedApplication(mtm);
        let event = match app.currentEvent() {
            Some(e) => e,
            None => return false,
        };

        // Decode the preview image once and reuse across all
        // dragging items (typical use is a single-item drag so this
        // is moot, but the multi-file case still gets one shared
        // thumbnail rather than per-item icons).
        let preview_img: Option<Retained<NSImage>> = preview.and_then(|bytes| {
            let data = NSData::with_bytes(bytes);
            let img = NSImage::initWithData(NSImage::alloc(), &data);
            img.filter(|i| i.size().width > 0.0 && i.size().height > 0.0)
        });

        // Default size for the drag visual when the caller didn't
        // provide one. macOS auto-clamps display anyway.
        const FALLBACK_W: f64 = 96.0;
        const FALLBACK_H: f64 = 96.0;

        let mut items: Vec<Retained<NSDraggingItem>> = Vec::new();
        for path in paths {
            let path_str = path.to_string_lossy();
            let ns_str = NSString::from_str(&path_str);
            let url = NSURL::fileURLWithPath(&ns_str);

            let item = NSDraggingItem::initWithPasteboardWriter(
                NSDraggingItem::alloc(),
                &ProtocolObject::from_ref(&*url),
            );
            let mouse_loc = event.locationInWindow();
            let (img, w, h): (Retained<NSImage>, f64, f64) = match preview_img.as_ref() {
                Some(img) => {
                    let size = img.size();
                    // Cap the preview to a reasonable on-screen
                    // size so a huge screenshot doesn't render as
                    // a giant ghost following the cursor.
                    let max_dim = 128.0;
                    let scale = if size.width.max(size.height) > max_dim {
                        max_dim / size.width.max(size.height)
                    } else {
                        1.0
                    };
                    let w = size.width * scale;
                    let h = size.height * scale;
                    (img.clone(), w, h)
                }
                None => {
                    let img = NSImage::initWithSize(
                        NSImage::alloc(),
                        NSSize::new(FALLBACK_W, FALLBACK_H),
                    );
                    (img, 1.0, 1.0)
                }
            };
            // Anchor the preview so its center lands on the cursor.
            let frame = NSRect::new(
                NSPoint::new(mouse_loc.x - w / 2.0, mouse_loc.y - h / 2.0),
                NSSize::new(w, h),
            );
            item.setDraggingFrame_contents(frame, Some(&img));
            items.push(item);
        }

        let ns_items: Vec<&NSDraggingItem> = items.iter().map(|i| &**i).collect();
        let ns_array = NSArray::from_slice(&ns_items);

        let source = DragSource::new(mtm, operations);

        let _session = ns_view.beginDraggingSessionWithItems_event_source(
            &ns_array,
            &event,
            &ProtocolObject::from_ref(&*source),
        );

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drag_source_offers_the_operations_it_was_made_with() {
        // SAFETY: DragSource は AppKit の UI に触らない NSObject の子。作って ivar を
        // 読むだけなので、テストのスレッド (main ではない) で作っても害は無い
        let mtm = unsafe { objc2::MainThreadMarker::new_unchecked() };
        assert_eq!(DragSource::new(mtm, copy_only()).operations(), NSDragOperation::Copy);
        let movable = DragSource::new(mtm, copy_or_move()).operations();
        assert!(movable.contains(NSDragOperation::Copy));
        assert!(movable.contains(NSDragOperation::Move), "Finder が移せる");
        assert!(movable.contains(NSDragOperation::Generic), "Finder の既定 (同じボリュームなら移す) を選べる");
    }
}
