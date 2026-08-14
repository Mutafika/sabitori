//! macOS native drag & drop using objc2.

#![cfg(target_os = "macos")]

use std::path::Path;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, AllocAnyThread, MainThreadOnly, msg_send};
use objc2_app_kit::*;
use objc2_foundation::*;

// ---------------------------------------------------------------------------
// Minimal NSDraggingSource implementation
// ---------------------------------------------------------------------------

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "SabitoriDragSource"]
    struct DragSource;

    unsafe impl NSObjectProtocol for DragSource {}

    unsafe impl NSDraggingSource for DragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn _source_operation_mask(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            NSDragOperation::Copy
        }
    }
);

impl DragSource {
    fn new(mtm: objc2::MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
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
pub fn start_file_drag(window: &winit::window::Window, paths: &[&Path]) -> bool {
    start_file_drag_with_preview(window, paths, None)
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

        let source = DragSource::new(mtm);

        let _session = ns_view.beginDraggingSessionWithItems_event_source(
            &ns_array,
            &event,
            &ProtocolObject::from_ref(&*source),
        );

        true
    }
}
