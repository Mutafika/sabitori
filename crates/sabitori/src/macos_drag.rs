//! macOS native drag & drop using objc2.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
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

/// ファイルのアイコンの一辺 (Finder のアイコン表示の既定と同じ)。
const ICON: f64 = 64.0;
/// アイコンを付けて重ねて見せる数。それより後ろの物は隠れる。
const STACKED: usize = 3;
/// 重ねたアイコンのずらし幅。
const STACK_STEP: f64 = 6.0;

/// `i` 番目のファイルのアイコンの左上。カーソル `(x, y)` が 1 つ目の真ん中に来て、2 つ目から
/// 右下へずらして重ねる (winit の view は上下が逆で、下が +y)。隠れる分は最後に見せる物と同じ所。
fn stack_origin(i: usize, x: f64, y: f64) -> (f64, f64) {
    let k = i.min(STACKED - 1) as f64 * STACK_STEP;
    (x - ICON / 2.0 + k, y - ICON / 2.0 + k)
}

// ---------------------------------------------------------------------------
// 窓の中で描いていた掴んだ絵 (ゴースト) を、そのまま OS のドラッグの絵にする
// ---------------------------------------------------------------------------

/// 窓の外へ持ち出すドラッグの絵。窓の中で描いていたゴースト
/// ([`DeclarativeApp::drag_ghost`](crate::DeclarativeApp::drag_ghost)) を写した物。
pub(crate) struct DragOutImage {
    /// PNG (色は乗算していない)。
    png: Vec<u8>,
    /// 論理 px の大きさ。
    width: f64,
    height: f64,
    /// 絵の左上から見たカーソルの位置 (論理 px、下が +y)。窓の中と同じ所を掴んだまま運ぶ。
    cursor: (f64, f64),
}

thread_local! {
    /// ランタイムが `on_drag_out` の間だけ置く。そこでアプリが OS のドラッグを始めれば使う。
    static DRAG_OUT_IMAGE: RefCell<Option<DragOutImage>> = const { RefCell::new(None) };
}

/// 窓の外へ持ち出すドラッグの絵を置く (`None` で片付ける)。
pub(crate) fn set_drag_out_image(image: Option<DragOutImage>) {
    DRAG_OUT_IMAGE.with(|slot| *slot.borrow_mut() = image);
}

/// 画面と同じ大きさに描いたゴースト (`rgba`、色は乗算済み、地は透明) から、描かれている所
/// だけを切り出す。`srgb` は描いた面が sRGB か、`scale` は 1 論理 px の画素数、`mouse` は
/// ゴーストを組んだときのカーソル (論理 px)。何も描かれていなければ `None`。
pub(crate) fn drag_out_image(
    rgba: &[u8],
    width: u32,
    height: u32,
    srgb: bool,
    scale: f32,
    mouse: (f32, f32),
) -> Option<DragOutImage> {
    use image::ImageEncoder;

    let alpha = |x: u32, y: u32| rgba[((y * width + x) * 4 + 3) as usize];
    let (mut x0, mut y0, mut x1, mut y1) = (width, height, 0, 0);
    for y in 0..height {
        for x in 0..width {
            if alpha(x, y) > 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x0 >= x1 {
        return None;
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let mut straight = Vec::with_capacity((w * h * 4) as usize);
    for y in y0..y1 {
        let row = ((y * width + x0) * 4) as usize;
        for px in rgba[row..row + (w * 4) as usize].as_chunks::<4>().0 {
            straight.extend_from_slice(&unpremultiply(px, srgb));
        }
    }
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(&straight, w, h, image::ExtendedColorType::Rgba8)
        .ok()?;
    let s = f64::from(scale.max(1.0));
    Some(DragOutImage {
        png,
        width: f64::from(w) / s,
        height: f64::from(h) / s,
        cursor: (f64::from(mouse.0) - f64::from(x0) / s, f64::from(mouse.1) - f64::from(y0) / s),
    })
}

/// 乗算済みの 1 画素を、乗算していない値に戻す。sRGB の面は混ぜるのも乗算も線形の値で
/// されているので、線形に直してから割る (そのまま割ると半透明の所の色がずれる)。
fn unpremultiply(px: &[u8; 4], srgb: bool) -> [u8; 4] {
    let a = px[3];
    match a {
        0 => [0, 0, 0, 0],
        255 => [px[0], px[1], px[2], 255],
        _ => {
            let af = f32::from(a) / 255.0;
            let un = |c: u8| {
                let c = f32::from(c) / 255.0;
                let v = if srgb {
                    linear_to_srgb((srgb_to_linear(c) / af).min(1.0))
                } else {
                    (c / af).min(1.0)
                };
                (v * 255.0).round() as u8
            };
            [un(px[0]), un(px[1]), un(px[2]), a]
        }
    }
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
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
/// anything `NSImage initWithData:` recognises). When `None`, the drag
/// shows the app's in-window drag ghost as it was drawn
/// ([`DeclarativeApp::on_drag_out`](crate::DeclarativeApp::on_drag_out) から始めたとき。
/// 窓の中と同じ絵のまま運べる)、無ければ each file's own icon
/// (Finder と同じ。いくつもなら後ろに重ねる)。
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

        // 窓の中で描いていたゴースト (ランタイムが `on_drag_out` の間だけ置く)。
        // 渡された絵があればそちら。
        let ghost = match preview {
            None => DRAG_OUT_IMAGE.with(|slot| slot.borrow_mut().take()),
            Some(_) => None,
        };
        let ghost = ghost.and_then(|g| {
            let img = NSImage::initWithData(NSImage::alloc(), &NSData::with_bytes(&g.png))?;
            // 画素は Retina の分あるので、論理 px の大きさで見せる (くっきり描かれる)
            img.setSize(NSSize::new(g.width, g.height));
            Some((g, img))
        });
        // winit の view は上下が逆 (左上が原点、下が +y)。窓の座標 (左下が原点) を直す
        let mouse_loc = ns_view.convertPoint_fromView(event.locationInWindow(), None);

        let mut items: Vec<Retained<NSDraggingItem>> = Vec::new();
        for (i, path) in paths.iter().enumerate() {
            let path_str = path.to_string_lossy();
            let ns_str = NSString::from_str(&path_str);
            let url = NSURL::fileURLWithPath(&ns_str);

            let item = NSDraggingItem::initWithPasteboardWriter(
                NSDraggingItem::alloc(),
                &ProtocolObject::from_ref(&*url),
            );
            let (img, frame): (Retained<NSImage>, NSRect) = match (&ghost, preview_img.as_ref()) {
                // 窓の中と同じ所を掴んだまま。ゴーストは掴んだ物みんなを描いているので、
                // 絵は 1 つ目にだけ付ける
                (Some((g, ghost_img)), _) => {
                    let frame = NSRect::new(
                        NSPoint::new(mouse_loc.x - g.cursor.0, mouse_loc.y - g.cursor.1),
                        NSSize::new(g.width, g.height),
                    );
                    let img = if i == 0 {
                        ghost_img.clone()
                    } else {
                        NSImage::initWithSize(NSImage::alloc(), NSSize::new(g.width, g.height))
                    };
                    (img, frame)
                }
                (None, Some(img)) => {
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
                    // Anchor the preview so its center lands on the cursor.
                    let frame = NSRect::new(
                        NSPoint::new(mouse_loc.x - w / 2.0, mouse_loc.y - h / 2.0),
                        NSSize::new(w, h),
                    );
                    (img.clone(), frame)
                }
                // 絵もゴーストも無ければ、ファイルそのもののアイコン。ここで絵を付けないと何も
                // 見えない (Finder の上だけは Finder が自分の絵に描き替えるので見え、窓へ
                // 戻すとまた消えていた)。
                (None, None) => {
                    let img = if i < STACKED {
                        let icon = NSWorkspace::sharedWorkspace().iconForFile(&ns_str);
                        icon.setSize(NSSize::new(ICON, ICON));
                        icon
                    } else {
                        // 重ねた後ろに隠れる分。何千個でもアイコンを引かずに済ませる
                        NSImage::initWithSize(NSImage::alloc(), NSSize::new(ICON, ICON))
                    };
                    let (x, y) = stack_origin(i, mouse_loc.x, mouse_loc.y);
                    (img, NSRect::new(NSPoint::new(x, y), NSSize::new(ICON, ICON)))
                }
            };
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

    #[test]
    fn file_icons_stack_down_and_to_the_right_of_the_cursor() {
        assert_eq!(stack_origin(0, 100.0, 100.0), (68.0, 68.0), "1 つ目の真ん中にカーソル");
        assert_eq!(stack_origin(1, 100.0, 100.0), (74.0, 74.0));
        assert_eq!(stack_origin(2, 100.0, 100.0), (80.0, 80.0));
        assert_eq!(stack_origin(9, 100.0, 100.0), (80.0, 80.0), "隠れる分は最後に見せる物の所");
    }

    /// 窓いっぱいに描いたゴーストから描かれた所だけを切り出し、カーソルは絵の中の同じ所を
    /// 指したまま。Retina (2 倍) の画素は論理 px に直す。
    #[test]
    fn the_ghost_is_cut_out_where_it_was_drawn() {
        let (w, h) = (40u32, 30u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        // (10, 6)〜(20, 12) の画素に、半分透けた白 (乗算済みなので 128)
        for y in 6..12 {
            for x in 10..20 {
                let i = ((y * w + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&[128, 128, 128, 128]);
            }
        }
        let g = drag_out_image(&rgba, w, h, false, 2.0, (8.0, 4.0)).expect("描かれている");
        assert_eq!((g.width, g.height), (5.0, 3.0), "10×6 画素 = 5×3 論理 px");
        assert_eq!(g.cursor, (3.0, 1.0), "カーソル (8, 4) は絵の左上 (5, 3) から (3, 1)");
        let png = image::load_from_memory(&g.png).unwrap().to_rgba8();
        assert_eq!(png.dimensions(), (10, 6));
        assert_eq!(png.get_pixel(0, 0).0, [255, 255, 255, 128], "乗算を戻す");
        assert!(drag_out_image(&vec![0; 16], 2, 2, false, 1.0, (0.0, 0.0)).is_none(), "何も無い");
    }

    #[test]
    fn unpremultiplying_an_srgb_pixel_divides_in_linear_light() {
        // 線形で 0.5 の灰を 50% で描くと、面には sRGB(0.25) が入る
        let stored = (linear_to_srgb(0.25) * 255.0).round() as u8;
        let back = unpremultiply(&[stored, stored, stored, 128], true);
        let want = (linear_to_srgb(0.25 / (128.0 / 255.0)) * 255.0).round() as u8;
        assert!((i16::from(back[0]) - i16::from(want)).abs() <= 1, "{back:?} vs {want}");
        assert_eq!(unpremultiply(&[1, 2, 3, 0], true), [0, 0, 0, 0]);
        assert_eq!(unpremultiply(&[1, 2, 3, 255], true), [1, 2, 3, 255]);
    }
}
