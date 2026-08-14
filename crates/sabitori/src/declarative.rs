//! Declarative app runner.
//! Run a GUI app by just returning an Element tree from `view()`.

use crate::input_router::{pinch_metrics, PinchGesture, PrimaryInput, TouchDrag, TOUCH_SLOP};

use std::sync::Arc;
use web_time::Instant;

use sabitori_core::build::{build_tree_measured, BuildResult};
use sabitori_core::element::Element;
use sabitori_core::ViewContext;
use sabitori_gpu::{GpuContext, GpuRenderer, RingRenderer, SceneRenderContext};
use sabitori_input::{Delivery, InputEvent, InputEventKind, Key, Modifiers, MouseButton as InputMouseButton, PointerKind, MOUSE_POINTER_ID};
use sabitori_text::TextRenderer;
use sabitori_widgets::TextInputState;
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::bridge::{draw_ui_layer, UiDrawLists, UiRenderers};
use sabitori_gpu::RenderPhase;

#[cfg(not(target_arch = "wasm32"))]
fn apply_window_icon(window: &winit::window::Window, png: &[u8]) {
    let decoded = match image::load_from_memory(png) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            log::warn!("window_icon: PNG decode failed: {e}");
            return;
        }
    };
    let (w, h) = (decoded.width(), decoded.height());
    let rgba = decoded.into_raw();
    match winit::window::Icon::from_rgba(rgba, w, h) {
        Ok(icon) => window.set_window_icon(Some(icon)),
        Err(e) => log::warn!("window_icon: Icon::from_rgba failed: {e}"),
    }
    // macOS dock icon は winit set_window_icon が no-op だが、 配布形態 (.app) の
    // .icns が常に勝つので framework 側では触らない。 dev (cargo run) のドック表示が
    // 欲しいアプリは `macos_configure_window` フックで NSApp.setApplicationIconImage を
    // 自前で叩く想定。
}

/// Simplified app trait using the declarative Element API.
/// Just implement `view()` to describe your UI.
/// Declarative descriptor for a secondary window the app wants open in
/// addition to the primary `view()` window. v1 extras are render-only —
/// they get their own GPU surface and their own `view_for(key, ctx)`
/// build pass, but receive no input (intended for click-through
/// overlays, HUDs, per-display panels). Phase 2 will route events.
#[derive(Clone, Debug)]
pub struct ExtraWindow {
    /// Stable identifier passed back to `view_for` / `set_extra_window` /
    /// `macos_configure_extra_window` so the app can tell its windows
    /// apart. Must be unique within an `extra_windows()` Vec.
    pub key: String,
    pub title: String,
    pub size: (f32, f32),
    /// Logical-pixel position of the top-left corner, or `None` to let
    /// the OS pick.
    pub position: Option<(f32, f32)>,
    pub min_size: (f32, f32),
    pub transparent: bool,
    pub decorations: bool,
    pub backdrop_blur: Option<BackdropBlur>,
    pub backdrop_blur_top_strip_height: Option<f32>,
    /// Opt this extra into 3D scene rendering (SceneApp-style). When
    /// `true`, the runtime allocates a depth texture and routes the
    /// extra's redraw through `render_scene_then_ui` — the app draws
    /// 3D via `render_extra_scene(key, …)` first, then `view_for(key, …)`
    /// composites a 2D overlay on top with `LoadOp::Load`. Default
    /// `false` keeps the 2D-only fast path.
    pub scene_3d: bool,
}

impl Default for ExtraWindow {
    fn default() -> Self {
        Self {
            key: String::new(),
            title: "Sabitori".into(),
            size: (1000.0, 700.0),
            position: None,
            min_size: (1.0, 1.0),
            transparent: false,
            decorations: true,
            backdrop_blur: None,
            backdrop_blur_top_strip_height: None,
            scene_3d: false,
        }
    }
}

/// Backdrop-blur material for translucent panel windows. macOS-specific in
/// effect — on other platforms the trait method returning `Some(_)` is a
/// no-op. Variants mirror the AppKit `NSVisualEffectMaterial` enum.
#[derive(Clone, Copy, Debug)]
pub enum BackdropBlur {
    Hud,
    Menu,
    Sidebar,
    UnderWindow,
    HeaderView,
    Popover,
    Titlebar,
}

/// UI input-capture snapshot, pushed by the runtime to
/// [`DeclarativeApp::on_ui_capture`] before pointer / wheel events are
/// forwarded to the app. The egui
/// `wants_pointer_input()` / `wants_keyboard_input()` pair for hosts that
/// drive their own scene (3D camera, canvas) underneath the sabitori UI:
/// store the latest snapshot and consult it before reacting to raw input.
///
/// ```ignore
/// fn on_ui_capture(&mut self, capture: UiCapture) { self.ui_capture = capture; }
/// fn on_input(&mut self, event: &InputEvent) -> bool {
///     if let InputEvent::PointerPressed { .. } = event {
///         if self.ui_capture.wants_pointer { return false; } // UIが消費 → カメラ操作しない
///         self.camera_drag_start();
///     }
///     false
/// }
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiCapture {
    /// The pointer is currently over an interactive UI region
    /// (id-bearing / clickable / hoverable / focusable / drop zone).
    /// Background-only divs don't register hit regions — give panels that
    /// should block the pointer an `.id()`.
    pub wants_pointer: bool,
    /// A focusable element (text input, etc.) currently holds focus, so
    /// keyboard input is being routed to `on_focused_input`. Suppress
    /// app-global shortcuts while this is true.
    pub wants_keyboard: bool,
}

/// `'static` を要求するのは、 `Element::click` で登録されたハンドラが
/// `&mut dyn Any` 経由でアプリ本体に降りるため。 アプリはランタイムが所有して
/// プロセス寿命まで持つので、 実質的な制約にはならない。
pub trait DeclarativeApp: 'static {
    /// Build the UI tree. Called every frame.
    /// Use `ctx.hovered` to check which element the mouse is over.
    fn view(&self, ctx: &ViewContext) -> Element;

    /// Optional overlay element (context menu, modal, tooltip).
    /// Rendered on a separate layer — no text bleed, no layout interference.
    fn overlay_view(&self, _ctx: &ViewContext) -> Option<Element> { None }

    /// Called when an element with an `.id()` is clicked (left button).
    fn on_click(&mut self, _id: &str) {}

    /// Called when an element with an `.id()` is right-clicked.
    /// `x`/`y` are logical coordinates for positioning a context menu.
    fn on_right_click(&mut self, _id: &str, _x: f32, _y: f32) {}

    /// Called when the mouse moves (logical coordinates).
    fn on_pointer_move(&mut self, _x: f32, _y: f32) {}

    /// Called whenever the hovered element changes (including to `None`).
    /// Complements `ctx.hovered` (a per-frame read in `view`) with a push
    /// notification usable from `&mut self` — e.g. a menu bar switching its
    /// open dropdown when the pointer slides to a neighboring label.
    fn on_hover_change(&mut self, _id: Option<&str>) {}

    /// Pushed by the runtime before pointer / wheel input is forwarded,
    /// whenever the capture state changes. See [`UiCapture`]. Hosts with
    /// their own scene input (3D camera) store the snapshot and gate raw
    /// input on it. Default no-op.
    fn on_ui_capture(&mut self, _capture: UiCapture) {}

    /// Called when the left mouse button is released.
    fn on_pointer_up(&mut self) {}

    /// Pushed by the runtime each frame after the UI tree is built, with the
    /// frame's [`BuildResult`](sabitori_core::build::BuildResult). Hosts that
    /// drive their own input (e.g. a slider in a floating panel) can cache the
    /// hit-region rects by id here to map cursor → value without hardcoding
    /// track geometry. Default no-op.
    fn on_build(&mut self, _build: &sabitori_core::build::BuildResult) {}

    /// Optional text-selection colors `(background, foreground)`. When set, the
    /// selection highlight is drawn in `background`, and the selected glyphs are
    /// recolored to `foreground` so they stay legible over it (the macOS
    /// white-on-blue model — a translucent background alone can't guarantee
    /// contrast against arbitrary text colors). `None` (default) keeps the
    /// built-in translucent system-blue highlight and leaves glyph colors
    /// untouched, so every existing app is unchanged. Themed apps return their
    /// per-theme selection pair here.
    fn selection_style(&self) -> Option<(sabitori_core::Color, sabitori_core::Color)> {
        None
    }

    /// Whether pointer drags may select text at all. `true` (default) keeps the
    /// normal behavior; returning `false` kills text selection app-wide — no
    /// anchor is ever set, so nothing is highlighted, recolored, or copied.
    ///
    /// This is the blunt switch for apps that are all chrome and no prose
    /// (dashboards, viewers, editors that draw their own caret): there, a
    /// selection can only ever be an accident of dragging a panel around.
    /// For the common mixed case — prose selectable, chrome not — leave this
    /// alone and mark the chrome with `Element::no_select` instead, which
    /// inherits over a whole subtree.
    fn text_selection_enabled(&self) -> bool {
        true
    }

    /// Called when files are dropped onto the window from another app/window.
    fn on_file_drop(&mut self, _paths: Vec<std::path::PathBuf>) {}

    /// Called when files are hovering over the window (drag from outside).
    fn on_file_hover(&mut self, _path: std::path::PathBuf) {}

    /// Called when external file hover is cancelled.
    fn on_file_hover_cancelled(&mut self) {}

    /// Called on scroll (trackpad/mouse wheel). `delta_y` is in logical pixels.
    fn on_scroll(&mut self, _delta_y: f32) {}

    /// Called on scroll (trackpad/mouse wheel) with both axes, in logical
    /// pixels. Fires alongside [`Self::on_scroll`] whenever no managed scroll
    /// container consumes the event. Implement this instead of `on_scroll`
    /// when horizontal deltas matter (e.g. 2D canvas panning).
    fn on_scroll_xy(&mut self, _delta_x: f32, _delta_y: f32) {}

    /// Called when a two-finger pinch gesture begins. `center_*` is the
    /// midpoint between the two fingers in logical coordinates.
    fn on_pinch_start(&mut self, _center_x: f32, _center_y: f32) {}

    /// Called while a pinch is active. `scale` is the absolute ratio of the
    /// current finger distance to the starting distance (1.0 at start).
    /// `center_*` is the current midpoint between the two fingers.
    fn on_pinch(&mut self, _scale: f32, _center_x: f32, _center_y: f32) {}

    /// Called when the pinch gesture ends (one of the two fingers lifted).
    fn on_pinch_end(&mut self) {}

    /// Called when mouse back button is pressed.
    fn on_navigate_back(&mut self) {}

    /// Called when mouse forward button is pressed.
    fn on_navigate_forward(&mut self) {}

    /// Called for input events. Return `true` if the event was consumed.
    ///
    /// # `true` を返すと何が止まるか
    ///
    /// ランタイムはこのイベントに対する**既定動作を行わない**。 キー押下の場合、
    /// 具体的には次が止まる:
    ///
    /// - Tab / Shift+Tab のフォーカス移動
    /// - Escape のフォーカス解除
    /// - Cmd/Ctrl+C による選択テキストのコピー
    /// - 「コピー以外のキーで選択を解除する」挙動
    ///
    /// 独自のキーバインドを持つアプリ (Tab を補完に使う、 Escape を自前の
    /// モーダル閉じに使う、 など) はこれで既定動作を抑止する。
    ///
    /// 配信はフォーカス中の要素が先で、 [`Self::on_focused_input`] が `true` を
    /// 返した場合はここには来ない (その場合も既定動作は止まる)。 ただし
    /// Tab / Escape はフォーカス操作そのものなので `on_focused_input` を経由せず、
    /// 直接ここに来る。
    ///
    /// 0.4.0 より前はこの戻り値をどこも読んでいなかった。 doc は "Return true if
    /// handled" と言っているのに `true` を返しても既定動作が走る、 という状態
    /// だった (issue #18)。
    ///
    /// ポインタ系イベントの既定動作 (クリック判定・ホバー・管理スクロール) は
    /// 現状これでは止められない。 どのランタイムがどの種別を配るかは
    /// [`input_delivery`] を参照。
    fn on_input(&mut self, _event: &InputEvent) -> bool { false }

    /// Called when a focused text input receives a character/key/IME event.
    /// `id` is the element's `.id()`. Return true if handled.
    /// Default: does nothing. Override to route to your TextInputState.
    fn on_focused_input(&mut self, _id: &str, _event: &InputEvent) -> bool { false }

    /// Element id the app wants to hold focus, if any. Polled once
    /// per frame; the runtime force-sets its `focused_id` to match,
    /// overriding click-driven focus changes. Useful for popups that
    /// open with a known input field and should keep focus until
    /// they're dismissed (Spotlight / command-palette pattern).
    /// Default `None` means "let click-to-focus drive it."
    fn desired_focus(&self) -> Option<String> { None }

    /// The caret rectangle to hand the platform IME, in window-logical pixels
    /// `(x, y, width, height)` — where the conversion / candidate window should
    /// anchor (e.g. Japanese 変換候補). Polled once per frame; the runtime calls
    /// `Window::set_ime_cursor_area` with it (deduped). Without this, winit
    /// leaves the area at the window origin, so the candidate window sits in the
    /// top-left instead of by the text. Default `None` keeps that old behavior.
    /// Return the focused input's caret rect (terminal cursor cell, text field
    /// caret, …); `None` when nothing is being typed into.
    fn ime_cursor_area(&self) -> Option<(f32, f32, f32, f32)> { None }

    /// Whether the platform IME should be active at all. Polled once per
    /// frame (deduped). Return `false` while the app has no text-entry
    /// target — winit then disables IME on the window, which also cancels an
    /// in-flight composition (macOS), so a dialog that closes mid-変換
    /// discards the composition instead of leaving an orphaned candidate
    /// window floating over the app. Default `true` keeps the old always-on
    /// behavior (right for apps like terminals that accept IME input without
    /// a focused field).
    fn ime_allowed(&self) -> bool { true }

    /// Called every frame with delta time. Use for animation ticking.
    fn tick(&mut self, _dt: f32) {}

    /// Whether the app currently has its own animations running.
    /// Return `true` to force the runtime into continuous-redraw mode.
    /// Default `false` lets the runtime drop to its idle 1Hz heartbeat
    /// when no built-in animator (scroll, tooltip, presence, style, drag)
    /// is active. Override if your `tick(dt)` is advancing custom state.
    fn is_animating(&self) -> bool { false }

    /// Element ids whose layout position should be reported back in
    /// [`BuildResult::probe_positions`](sabitori_core::build::BuildResult::probe_positions),
    /// **even while they are scrolled out of view**.
    ///
    /// `hit_regions` only carries visible elements — anything fully outside its
    /// parent clip is dropped — so an app cannot ask "where is element X" for
    /// off-screen content. That is precisely what scroll-to-element needs: to
    /// bring row 400 of a long list to the top, you must know where row 400 is
    /// while it is nowhere near the screen. Layout knows; this opts an id into
    /// having that position reported.
    ///
    /// Called once per frame before the tree is built. Return empty (the
    /// default) for zero cost — probing is skipped entirely when the set is empty.
    fn build_probes(&self) -> Vec<String> { Vec::new() }

    /// Return pending programmatic scroll requests for `.scroll(id)` containers.
    /// Drained once per frame after layout. `(id, y)` — pass `f32::MAX` for "bottom".
    /// Return empty to leave scroll untouched (user controls it via wheel).
    fn scroll_intents(&mut self) -> Vec<(String, f32)> { Vec::new() }

    /// Called when a drag completes over a drop zone.
    /// `data` is from `.draggable()`, `target_id` is the drop zone's `.id()`.
    fn on_drop(&mut self, _data: &str, _target_id: &str) {}

    /// Called when a drag exits the window (for OS-level drag).
    fn on_drag_out(&mut self, _data: &str) {}

    /// Return a ghost element to show while dragging.
    fn drag_ghost(&self, _ctx: &ViewContext) -> Option<Element> { None }

    /// Called when the cursor leaves the window. Useful for drag-out detection.
    fn on_cursor_left(&mut self) {}

    /// Access the window for platform-specific operations (e.g., OS drag).
    fn set_window(&mut self, _window: std::sync::Arc<winit::window::Window>) {}

    /// PNG bytes for the OS window / dock icon. Decoded once at window
    /// creation. Return `None` (default) to let the OS pick. Useful so
    /// `cargo run` shows the app's brand icon instead of the terminal /
    /// generic binary glyph. On macOS this also sets the running dock
    /// icon via `NSApp.setApplicationIconImage` — for shipped `.app`
    /// bundles the bundle's `.icns` still wins, this is a dev nicety.
    fn window_icon(&self) -> Option<Vec<u8>> { None }

    /// Window title.
    fn title(&self) -> &str { "Sabitori" }

    /// Initial window size (logical pixels).
    fn size(&self) -> (f32, f32) { (1000.0, 700.0) }

    /// Optional initial window position in logical pixels (top-left).
    /// `None` lets the OS pick. Useful for panel-style apps that want to
    /// pin themselves to a screen edge.
    fn position(&self) -> Option<(f32, f32)> { None }

    /// Minimum window size (logical pixels). Default 400x300.
    fn min_size(&self) -> (f32, f32) { (400.0, 300.0) }

    /// Whether the window background should be transparent.
    fn transparent(&self) -> bool { false }

    /// Optional backdrop-blur material rendered behind the wgpu surface.
    /// Requires `transparent()` returning true to actually be visible.
    /// macOS-only effect — non-macOS targets ignore the value.
    fn backdrop_blur(&self) -> Option<BackdropBlur> { None }

    /// If `Some(h)`, the backdrop blur covers only the top `h` logical pixels
    /// of the window, anchored to the top edge. Useful for panel apps with a
    /// fullscreen window where only the bar zone should be blurred — the
    /// rest stays truly transparent so apps beneath remain visible. `None`
    /// (default) covers the whole window. Ignored when `backdrop_blur`
    /// returns `None`.
    fn backdrop_blur_top_strip_height(&self) -> Option<f32> { None }

    /// Whether to use OS window decorations (title bar, buttons).
    /// Return false for a fully custom title bar.
    fn decorations(&self) -> bool { true }

    /// macOS-only hook, called once with the underlying winit `Window`
    /// right after creation. Use this to set platform-specific properties
    /// winit doesn't expose: `NSWindow.level`, `collectionBehavior`,
    /// `ignoresMouseEvents`, etc. Default is a no-op.
    #[cfg(target_os = "macos")]
    fn macos_configure_window(&self, _window: &Window) {}

    /// Override the generic sans-serif family with a specific face name.
    /// Use e.g. `"Hiragino Sans"` on macOS to keep kanji from routing
    /// through Chinese-styled system fonts. `None` (default) keeps
    /// cosmic-text's generic resolution.
    fn preferred_font_family(&self) -> Option<String> { None }

    /// Override the generic monospace family for *monospace* text (anything
    /// drawn with `.mono()`). Return a face name (e.g. `"Hack"`) to render
    /// fixed-width text in that font instead of the OS default monospace.
    /// Read every frame, so an app can drive it from a font picker and have
    /// the change apply live. `None` (default) keeps the generic resolution.
    fn preferred_monospace_family(&self) -> Option<String> { None }

    /// Fonts to load at startup. Return raw TTF/OTF data.
    /// These are registered before the first frame, so they're available
    /// to cosmic-text for text shaping and measurement.
    ///
    /// ```ignore
    /// fn fonts(&self) -> Vec<Vec<u8>> {
    ///     vec![
    ///         include_bytes!("../assets/fonts/Hack-Regular.ttf").to_vec(),
    ///         include_bytes!("../assets/fonts/Hack-Bold.ttf").to_vec(),
    ///     ]
    /// }
    /// ```
    fn fonts(&self) -> Vec<Vec<u8>> { Vec::new() }

    /// Return the app theme. Override to customize colors.
    /// The theme is available in `view()` via `ctx.theme`.
    fn theme(&self) -> sabitori_core::AppTheme { sabitori_core::AppTheme::default() }

    /// Opt into lazy rendering: skip redraws when nothing is animating
    /// and no input has occurred. Default `false` keeps the original
    /// 60fps redraw loop for compatibility. When `true`, the runtime
    /// only requests a redraw if input arrived, an animation is running,
    /// or the app reports state changes via `poll_dirty`. Idle CPU drops
    /// from "view+layout+GPU every 16ms" to ~0.
    fn lazy_render(&self) -> bool { false }

    /// Polled at the end of each tick (lazy mode only). Return `true` if
    /// `tick` mutated visual state and the next frame should be redrawn.
    /// Default `false` — input-driven UIs need not implement this since
    /// input events already invalidate the frame; override only when your
    /// `tick` drains async results that change what's on screen (e.g.
    /// channels from worker threads, completion signals).
    fn poll_dirty(&mut self) -> bool { false }

    /// Target tick + redraw cadence. Default 8ms (≈120Hz) so ProMotion
    /// displays run native and 60Hz displays still align cleanly to vsync
    /// (render finishes with headroom before the next 16.67ms slot, no
    /// drift jitter). Override to `Duration::from_millis(16)` if a 60Hz
    /// cap is preferred (lower CPU at the cost of vsync-drift jitter on
    /// 60Hz displays when render runs long).
    fn target_frame_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(8)
    }

    /// Secondary windows the app wants open alongside the primary `view()`
    /// window. Called once at startup. Each entry's `key` identifies the
    /// window for `view_for` / `set_extra_window` / `macos_configure_extra_window`
    /// routing. Default: empty (single-window app, fully backward compatible).
    ///
    /// v1 extras are render-only — they get their own GPU surface and view
    /// build, but no input events. Intended for click-through overlays
    /// (`setIgnoresMouseEvents:YES`), HUDs, per-display panels.
    fn extra_windows(&self) -> Vec<ExtraWindow> { Vec::new() }

    /// Build the UI for the given extra-window `key`. Default returns an
    /// empty container so apps that declare extras without overriding this
    /// see something obviously wrong (blank window) instead of mysteriously
    /// inheriting `view()`.
    fn view_for(&self, _key: &str, _ctx: &ViewContext) -> Element {
        sabitori_core::div()
    }

    /// macOS post-creation hook for an extra window. Use to set NSWindow
    /// level, `ignoresMouseEvents`, collection behavior — anything winit
    /// doesn't expose. Default no-op.
    #[cfg(target_os = "macos")]
    fn macos_configure_extra_window(&self, _key: &str, _window: &Window) {}

    /// Hand the app the underlying winit `Window` for an extra, mirroring
    /// `set_window` for the primary. Use for platform APIs that need a
    /// window handle (e.g. global event monitor wake-up). Default no-op.
    fn set_extra_window(&mut self, _key: &str, _window: std::sync::Arc<Window>) {}

    /// One-time 3D scene setup for an extra window with `scene_3d = true`.
    /// Mirrors `SceneApp::setup`. Use to create custom pipelines /
    /// buffers / bind groups against the extra's GPU device. Default
    /// no-op (extras with `scene_3d = false` never see this).
    #[cfg(not(target_arch = "wasm32"))]
    fn setup_extra_scene(&mut self, _key: &str, _ctx: &GpuContext) {}

    /// Called when an extra's window resizes. Mirrors
    /// `SceneApp::on_resize` — the depth texture has already been
    /// recreated by the runtime by the time this fires. Default no-op.
    #[cfg(not(target_arch = "wasm32"))]
    fn on_resize_extra_scene(&mut self, _key: &str, _ctx: &GpuContext) {}

    /// Render the 3D scene into an extra window. Same contract as
    /// `SceneApp::render_scene` — the app creates its own render pass
    /// on `ctx.encoder`. Called once per frame, *before* the 2D
    /// `view_for(key)` overlay, and only for extras with
    /// `scene_3d = true`. Default no-op.
    #[cfg(not(target_arch = "wasm32"))]
    fn render_extra_scene(&mut self, _key: &str, _ctx: &mut SceneRenderContext) {}
}

/// このランタイムが [`InputEvent`] の各種別をアプリへどう届けるかの宣言。
///
/// [`Delivery`] の doc にある通り、 sabitori はイベント処理を共有しない 3 つの
/// ランタイムを持つ。 この関数は [`InputEventKind`] に対する**網羅マッチ**なので、
/// 種別が増えるとここがコンパイルエラーになり、 配線の判断を必ず通ることになる。
///
/// 消費側にとっては「このランタイムで何が来るか」の一覧でもある。
/// `sabitori::scene_app::input_delivery` / `sabitori_window::input_delivery` と
/// 見比べると、 ランタイム間の差が分かる。
pub fn input_delivery(kind: InputEventKind) -> Delivery {
    match kind {
        // ポインタ系は内部の hit-test / hover / 押下追跡にも使うが、 生のイベントも
        // そのまま `on_input` に流している。
        InputEventKind::PointerMoved
        | InputEventKind::PointerPressed
        | InputEventKind::PointerReleased
        | InputEventKind::PointerCancelled => Delivery::ToApp,

        // winit の `CursorLeft` は専用コールバックに変換していて、
        // `InputEvent::PointerLeft` は組み立てていない。
        InputEventKind::PointerLeft => {
            Delivery::NotProduced("カーソルの離脱は DeclarativeApp::on_cursor_left で伝える")
        }

        // IME / キーボードはフォーカス中の要素へ先に渡し、 消費されなければ
        // `on_input` に落とす。
        InputEventKind::ImeEnabled
        | InputEventKind::ImePreedit
        | InputEventKind::ImeCommit
        | InputEventKind::KeyInput
        | InputEventKind::CharInput => Delivery::ToApp,

        InputEventKind::ModifiersChanged => Delivery::ToApp,

        // Cmd/Ctrl+V を捕まえてクリップボードを読み、 1 イベントとして配る。
        InputEventKind::Paste => Delivery::ToApp,
    }
}

/// Per-extra-window resources. Each extra has its own GPU surface and
/// renderer set; v1 keeps them passive (no input, no managed scroll /
/// hover / drag) so the data they need is just the render pipeline plus
/// last-frame layout for any future hit-testing extension.
struct ExtraWindowState {
    key: String,
    window: Arc<Window>,
    renderer: GpuRenderer,
    text_renderer: TextRenderer,
    image_renderer: sabitori_gpu::ImageRenderer,
    ring_renderer: sabitori_gpu::RingRenderer,
    line_renderer: sabitori_gpu::LineRenderer,
    measure_cache: std::cell::RefCell<crate::bridge::MeasureCache>,
    pub(crate) last_build: Option<BuildResult>,
    /// Mirrors `ExtraWindow::scene_3d` so `redraw_extra` and the
    /// resize handler can branch without re-querying the app's
    /// `extra_windows()` list every frame.
    scene_3d: bool,
}

pub(crate) struct AppState<A: DeclarativeApp> {
    pub(crate) app: A,
    /// True when the next frame would visually differ from the last drawn
    /// one. Set on input events / animation activity / app-reported state
    /// changes; cleared after each render. Only consulted when the app
    /// opts into `DeclarativeApp::lazy_render` — otherwise the runtime
    /// redraws unconditionally at 60fps.
    dirty: bool,
    window: Option<Arc<Window>>,
    renderer: Option<GpuRenderer>,
    text_renderer: Option<TextRenderer>,
    image_renderer: Option<sabitori_gpu::ImageRenderer>,
    ring_renderer: Option<sabitori_gpu::RingRenderer>,
    line_renderer: Option<sabitori_gpu::LineRenderer>,
    /// Shared so a frame's measurer can borrow it while `&mut self` is also
    /// live — `build_frame` takes the measurer as a parameter, and a plain
    /// `&self.measure_cache` inside it would collide with that `&mut self`.
    /// An `Rc` handle cloned by the caller sidesteps the conflict without
    /// copying the cache.
    measure_cache: std::rc::Rc<std::cell::RefCell<crate::bridge::MeasureCache>>,
    last_frame: Instant,
    pub(crate) last_build: Option<BuildResult>,
    pub(crate) mouse_x: f32,
    pub(crate) mouse_y: f32,
    hovered_id: Option<String>,
    /// 現在押されている要素の id。`active_style` (= `.active()` / `.pressable()`)
    /// を畳むのに使う。押下で入り、解放・キャンセル・ウィンドウ外への離脱で消える。
    /// hover と同じく「id を 1 つ持つ」だけの単純な状態で、押したまま外へ払っても
    /// 解放まで押下表示は続く（CSS の `:active` と同じ）。
    pressed_id: Option<String>,
    /// Last cursor we asked winit to display. Used to dedup
    /// `set_cursor` calls — flipping the cursor every frame is cheap
    /// but not free, and Apple's NSCursor swap can show up as a
    /// visual flicker when called every move event.
    last_cursor: Option<sabitori_core::Cursor>,
    /// Last IME caret rect we handed winit (`set_ime_cursor_area`), to dedup —
    /// it's polled every frame but only changes as the caret moves. See
    /// [`DeclarativeApp::ime_cursor_area`].
    last_ime_area: Option<(f32, f32, f32, f32)>,
    /// Last IME-allowed state handed winit (`set_ime_allowed`), to dedup.
    /// See [`DeclarativeApp::ime_allowed`].
    last_ime_allowed: bool,
    pub(crate) focused_id: Option<String>,
    /// 「打った文字がどこにも行かなかった」警告を出したテキスト欄の id。
    /// 毎フレーム鳴らすとログが埋まるので 1 度だけにする。
    /// [`AppState::warn_if_typing_went_nowhere`] を参照。
    warned_unrouted_input: std::collections::HashSet<String>,
    /// `view()` の中でウィジェットが登録した「面倒を見るもの」。 毎フレーム
    /// 差し替わる。 [`AppState::adopt_managed`] を参照。
    managed: Vec<(String, std::rc::Rc<dyn sabitori_core::Managed>)>,
    /// `Element::click` が登録したクリック処理。 毎フレーム差し替わる。
    actions: Vec<(String, sabitori_core::Action)>,
    modifiers: Modifiers,
    last_viewport_w: f32,
    last_viewport_h: f32,
    /// Which modality owns the primary-pointer flow. Blocks the other.
    primary_input: PrimaryInput,
    /// All active touch positions keyed by winit touch id. Used for gesture
    /// recognition (pinch) and active-count tracking.
    active_touches: std::collections::HashMap<u64, (f32, f32)>,
    /// Drag/scroll state of the primary (first) touch. Extra fingers still emit
    /// `InputEvent::Pointer*` but don't steer this flow.
    touch_drag: Option<TouchDrag>,
    /// Active 2-finger pinch, if any.
    pinch: Option<PinchGesture>,
    /// Managed scroll states, keyed by the id given to `.scroll(id)`.
    pub(crate) scroll_states: std::collections::HashMap<String, sabitori_widgets::ScrollView>,
    /// Managed tooltip hover-delay state.
    tooltip_state: sabitori_widgets::TooltipState,
    /// Managed drag & drop state.
    drag_manager: sabitori_widgets::DragManager,
    /// Animated style transitions (hover spring animations).
    style_animator: sabitori_widgets::StyleAnimator,
    /// Presence (mount/unmount) animator.
    presence_animator: sabitori_widgets::PresenceAnimator,
    /// Shared image cache backing `ViewContext::image_url`. Populated in
    /// the background by async fetch + decode tasks.
    image_cache: std::sync::Arc<std::sync::Mutex<sabitori_core::image_cache::ImageCache>>,
    /// Fetch results queued by background tasks, drained into the cache
    /// each frame so `view()` sees ready images synchronously.
    image_pending: std::sync::Arc<std::sync::Mutex<Vec<(String, sabitori_core::image_cache::CacheState)>>>,
    /// Pre-built `ImageCtx` handed to each frame's `ViewContext`.
    image_ctx: sabitori_core::ImageCtx,
    /// Owned tokio runtime that spawns image fetches (native only). Kept
    /// alive by holding it in `AppState`.
    #[cfg(not(target_arch = "wasm32"))]
    _image_runtime: std::sync::Arc<tokio::runtime::Runtime>,
    /// Set in `new_events` when winit wakes from `WaitCancelled`/`Init`
    /// (i.e. an OS event arrived). Consumed in `about_to_wait` to trigger
    /// exactly one redraw per wake. This is what makes the variable
    /// refresh rate scheme work — events drive frames, not the loop.
    pending_redraw: bool,
    /// Set after a render whose glyph atlas overflowed (dropped glyphs). Forces
    /// `must_draw` on the next tick so `maybe_recover_atlas` runs a flush +
    /// re-shape — without it, `lazy_render` parks on the broken frame and the
    /// missing glyphs persist until the user interacts. Cleared each render by
    /// re-reading the atlas state.
    atlas_recover_pending: bool,
    /// Secondary windows declared via `DeclarativeApp::extra_windows`.
    /// Keyed by winit `WindowId` so `window_event` can dispatch in O(1).
    /// Empty for single-window apps — zero overhead by default.
    extras: std::collections::HashMap<WindowId, ExtraWindowState>,
    /// 直近フレームの text 要素の hit-test layout。 `render_list_to_gpu_with_hits`
    /// が描画時に出力する。 mouse 座標 → (text_idx, byte) 解決 / selection 描画 /
    /// clipboard 抽出で使う。 frame 単位で全置換 (= 古い frame の layout は捨てる)。
    text_layouts: Vec<crate::bridge::TextHitLayout>,
    /// 現在の文字選択 state (None = 未選択)。 anchor は drag 開始位置、 head は
    /// 現在のドラッグ位置。 Cmd+C / 描画時に anchor〜head の範囲を解釈する。
    selection: Option<TextSelection>,
    /// drag-selection 中か。 mouse_down で text 要素にヒットしたら true、
    /// mouse_up で false。 true の間の mouse_move は selection.head を更新する。
    selecting: bool,
    /// Last [`UiCapture`] snapshot pushed to the app. Used to dedup
    /// `on_ui_capture` calls to actual state transitions.
    pub(crate) last_capture: UiCapture,
}

/// 文字選択 state。 `(text_idx, byte_offset)` のペアを anchor / head で持つ。
/// 「正規化されてない」 順序: head の方が anchor より前に来ることもある。
/// 選択範囲を扱う側 (描画 / clipboard) は `range_normalized()` で並べ替えてから
/// 使う。
#[derive(Clone, Debug)]
pub(crate) struct TextSelection {
    /// drag 開始時の (text_idx, byte_offset)。
    pub(crate) anchor: (usize, usize),
    /// 現在の drag head の (text_idx, byte_offset)。
    pub(crate) head: (usize, usize),
    /// anchor 確定時点での anchor.0 が指す text 要素の content snapshot。
    /// view 切替 (list → article 等) で同じ text_idx が違うテキストを指すように
    /// なった時に selection を invalidate するために使う。
    pub(crate) anchor_content: String,
    /// head が動いた時に最新化される head.0 の content snapshot。
    /// drag 中はリアルタイム更新、 mouse_up 後は最後の値で固定。
    pub(crate) head_content: String,
}

impl TextSelection {
    /// `(start, end)` を (text_idx, byte) の lexicographic 順で返す。
    pub(crate) fn range_normalized(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// 1 文字も選択されていない (anchor == head) ならば true。
    pub(crate) fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

impl<A: DeclarativeApp> ApplicationHandler for AppState<A> {
    #[cfg(not(target_arch = "wasm32"))]
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() { return; }
        let (w, h) = self.app.size();
        let mut attrs = WindowAttributes::default()
            .with_title(self.app.title())
            .with_inner_size(winit::dpi::LogicalSize::new(w, h))
            .with_min_inner_size({
                let (mw, mh) = self.app.min_size();
                winit::dpi::LogicalSize::new(mw, mh)
            });
        if let Some((x, y)) = self.app.position() {
            attrs = attrs.with_position(winit::dpi::LogicalPosition::new(x, y));
        }
        if self.app.transparent() {
            attrs = attrs.with_transparent(true);
        }
        if !self.app.decorations() {
            attrs = attrs.with_decorations(false);
        }
        // macOS: 非アクティブ窓の**初回クリックを content に渡す**。 winit 既定は
        // false で、 他窓（Finder / ブラウザ等）から戻った 1 クリック目が「窓を前面に
        // 出すだけ」で吸われる。 ダッシュボード系は他アプリと行き来しながら操作するので、
        // これが無いと「ボタンを押しても効かない（実は毎回 1 クリック目が死ぬ）」に見える。
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs = attrs.with_accepts_first_mouse(true);
        }
        let window = Arc::new(event_loop.create_window(attrs).unwrap());

        // Platform-specific post-creation configuration. winit doesn't
        // expose macOS panel levels, collection behavior, mouse passthrough
        // etc., so we hand the consumer the raw window for those.
        #[cfg(target_os = "macos")]
        {
            // Attach NSVisualEffectView before the consumer hook runs, so
            // anything they set on the window (level, collection behavior)
            // isn't disturbed by our subview surgery.
            if let Some(blur) = self.app.backdrop_blur() {
                crate::macos_blur::attach_backdrop_blur(
                    &window,
                    blur,
                    self.app.backdrop_blur_top_strip_height(),
                );
            }
            self.app.macos_configure_window(&window);
        }

        // App icon. winit's set_window_icon covers Linux/Windows; macOS
        // gets its dock icon via NSApp.setApplicationIconImage (winit
        // is a no-op for the dock there).
        if let Some(png) = self.app.window_icon() {
            apply_window_icon(&window, &png);
        }

        // Enable IME so Japanese (and other) input methods receive preedit/commit events.
        // On iOS `set_ime_allowed` shows/hides the software keyboard, so there it is
        // toggled per focus in the redraw loop instead of forced on at startup.
        #[cfg(not(target_os = "ios"))]
        window.set_ime_allowed(true);
        let gpu = GpuRenderer::new_with_alpha(window.clone(), self.app.transparent());
        let mut text = TextRenderer::new(&gpu.device, gpu.surface_config.format, &gpu.globals_bind_group_layout);
        let user_fonts = self.app.fonts();
        if !user_fonts.is_empty() {
            text.prefer_user_fonts(&user_fonts);
        }
        text.set_preferred_family(self.app.preferred_font_family());
        text.set_preferred_monospace_family(self.app.preferred_monospace_family());
        let img = sabitori_gpu::ImageRenderer::new(&gpu.device, gpu.surface_config.format, &gpu.globals_bind_group_layout);
        let rings = sabitori_gpu::RingRenderer::new(&gpu.device, gpu.surface_config.format, &gpu.globals_bind_group_layout);
        let lines = sabitori_gpu::LineRenderer::new(&gpu.device, gpu.surface_config.format, &gpu.globals_bind_group_layout);
        self.app.set_window(window.clone());
        self.window = Some(window);
        self.renderer = Some(gpu);
        self.text_renderer = Some(text);
        self.image_renderer = Some(img);
        self.ring_renderer = Some(rings);
        self.line_renderer = Some(lines);

        // After the primary is up, create any declared extras. Done
        // here (rather than in a separate event-loop callback) so the
        // first frame of every window is rendered together.
        let extras = self.app.extra_windows();
        for spec in extras {
            let mut attrs = WindowAttributes::default()
                .with_title(&spec.title)
                .with_inner_size(winit::dpi::LogicalSize::new(spec.size.0, spec.size.1))
                .with_min_inner_size(winit::dpi::LogicalSize::new(spec.min_size.0, spec.min_size.1));
            if let Some((x, y)) = spec.position {
                attrs = attrs.with_position(winit::dpi::LogicalPosition::new(x, y));
            }
            if spec.transparent {
                attrs = attrs.with_transparent(true);
            }
            if !spec.decorations {
                attrs = attrs.with_decorations(false);
            }
            let extra_window = Arc::new(event_loop.create_window(attrs).unwrap());

            #[cfg(target_os = "macos")]
            {
                if let Some(blur) = spec.backdrop_blur {
                    crate::macos_blur::attach_backdrop_blur(
                        &extra_window,
                        blur,
                        spec.backdrop_blur_top_strip_height,
                    );
                }
                self.app.macos_configure_extra_window(&spec.key, &extra_window);
            }

            let mut extra_gpu = GpuRenderer::new_with_alpha(extra_window.clone(), spec.transparent);
            // Allocate the depth texture *before* setup_extra_scene so
            // the app's pipeline init can sample render targets that
            // include depth (matches SceneApp's setup ordering).
            if spec.scene_3d {
                extra_gpu.create_depth_texture();
            }
            let extra_text = TextRenderer::new(
                &extra_gpu.device,
                extra_gpu.surface_config.format,
                &extra_gpu.globals_bind_group_layout,
            );
            let extra_img = sabitori_gpu::ImageRenderer::new(
                &extra_gpu.device,
                extra_gpu.surface_config.format,
                &extra_gpu.globals_bind_group_layout,
            );
            let extra_rings = sabitori_gpu::RingRenderer::new(
                &extra_gpu.device,
                extra_gpu.surface_config.format,
                &extra_gpu.globals_bind_group_layout,
            );
            let extra_lines = sabitori_gpu::LineRenderer::new(
                &extra_gpu.device,
                extra_gpu.surface_config.format,
                &extra_gpu.globals_bind_group_layout,
            );
            if spec.scene_3d {
                self.app.setup_extra_scene(&spec.key, &extra_gpu.gpu_context());
            }
            let id = extra_window.id();
            self.app.set_extra_window(&spec.key, extra_window.clone());
            self.extras.insert(id, ExtraWindowState {
                key: spec.key,
                window: extra_window,
                renderer: extra_gpu,
                text_renderer: extra_text,
                image_renderer: extra_img,
                ring_renderer: extra_rings,
                line_renderer: extra_lines,
                measure_cache: std::cell::RefCell::new(crate::bridge::MeasureCache::new()),
                last_build: None,
                scene_3d: spec.scene_3d,
            });
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // On WASM, window + renderer init is handled externally (async).
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // Dispatch events for extra windows first. v1 extras are
        // render-only — only resize / redraw / close are meaningful;
        // input events are dropped (the window is expected to be
        // click-through via macos_configure_extra_window). When the
        // event belongs to an extra we always return early so the
        // primary-window path below never sees foreign WindowIds.
        if self.extras.contains_key(&id) {
            self.handle_extra_event(event_loop, id, event);
            return;
        }
        // Any event other than the redraw itself counts as a frame
        // invalidation: input, focus, resize, IME, drop, etc. all change
        // either layout or hit-testing in ways that should re-render.
        // We deliberately set this even in non-lazy mode — the flag is
        // free when nobody reads it, and keeping the bookkeeping uniform
        // means the lazy path doesn't drift over time.
        if !matches!(event, WindowEvent::RedrawRequested) {
            self.dirty = true;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let (Some(w), Some(r)) = (self.window.as_ref(), self.renderer.as_mut()) {
                    r.resize(size.width, size.height, w.scale_factor());
                    w.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Touch has exclusive ownership → ignore stray cursor moves.
                if self.primary_input == PrimaryInput::Touch {
                    return;
                }
                if let Some(r) = self.renderer.as_ref() {
                    let s = r.scale_factor;
                    self.mouse_x = position.x as f32 / s;
                    self.mouse_y = position.y as f32 / s;
                }
                self.update_hover();
                self.app.on_pointer_move(self.mouse_x, self.mouse_y);
                // マウスの移動も `PointerMoved` として配る。 `InputEvent::PointerMoved`
                // の doc は "For mouse, fires for both hover and drag" と言っているのに、
                // このランタイムは touch 分しか出していなかった (`SabitoriApp` は出す)。
                // `on_pointer_move` は座標しか渡さないので、 修飾キーを見るには
                // こちらが要る — ⇧ドラッグの直交スナップのような「押している間だけ」の
                // 操作は、 動いている最中の状態が取れないと書けない。
                self.app.on_input(&InputEvent::PointerMoved {
                    id: MOUSE_POINTER_ID,
                    kind: PointerKind::Mouse,
                    position: sabitori_core::Point::new(self.mouse_x, self.mouse_y),
                    modifiers: self.modifiers,
                });
                self.drag_manager.on_move(self.mouse_x, self.mouse_y);
                // text selection drag: button held + selecting=true なら head を更新。
                // hit_test_text が None でも head は前の値を保持 (= 端の text 上で
                // 止まる、 巻き戻りで戻れる)。
                if self.selecting {
                    // drag 中は最近傍 snap (strict=false)。 anchor は既に実テキスト上に
                    // 立っているので、 段落の外へ払っても選択が伸び続けてよい。
                    if let Some(head) = self.hit_test_text(self.mouse_x, self.mouse_y, false) {
                        let snap = self
                            .text_layouts
                            .iter()
                            .find(|l| l.text_idx == head.0)
                            .map(|l| l.content.clone())
                            .unwrap_or_default();
                        if let Some(ref mut sel) = self.selection {
                            sel.head = head;
                            sel.head_content = snap;
                        }
                    }
                }
            }
            WindowEvent::CursorEntered { .. } => {
                // The pointer enters carrying whatever OS cursor the previous
                // window set (winit windows don't use macOS cursor rects, so the
                // OS doesn't reset it at the boundary). `apply_cursor` dedups
                // against `last_cursor` and skips `set_cursor` when the resolved
                // cursor matches it — so a stale `last_cursor` from our last time
                // on this window would suppress the re-set and leave the foreign
                // cursor (e.g. an I-beam) showing. Invalidate it; the next
                // CursorMoved (→ update_hover → apply_cursor) re-applies our own.
                // CursorEntered carries no coords, so we only reset here.
                self.last_cursor = None;
            }
            WindowEvent::CursorLeft { .. } => {
                if self.primary_input == PrimaryInput::Touch {
                    return;
                }
                self.hovered_id = None;
                // ウィンドウの外へ出たら押下も解除する。 解放イベントが別ウィンドウで
                // 起きると戻ってこないので、ここで消さないと押しっぱなしの見た目が残る。
                self.pressed_id = None;
                // If a drag is active, notify the app it left the window
                if let Some((data, _source_id)) = self.drag_manager.drag_info() {
                    self.app.on_drag_out(&data);
                    self.drag_manager.cancel();
                }
                self.app.on_cursor_left();
            }
            WindowEvent::MouseInput {
                state: winit::event::ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                if self.primary_input == PrimaryInput::Touch {
                    return;
                }
                if self.primary_input == PrimaryInput::None {
                    self.primary_input = PrimaryInput::Mouse;
                }
                self.press_primary();
            }
            WindowEvent::MouseInput {
                state: winit::event::ElementState::Released,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                if self.primary_input == PrimaryInput::Touch {
                    return;
                }
                // Primary mouse button released — release ownership.
                if self.primary_input == PrimaryInput::Mouse {
                    self.primary_input = PrimaryInput::None;
                }
                self.release_primary();
            }
            WindowEvent::MouseInput {
                state: winit::event::ElementState::Pressed,
                button: winit::event::MouseButton::Right,
                ..
            } => {
                if let Some(ref build) = self.last_build {
                    let pt = sabitori_core::Point::new(self.mouse_x, self.mouse_y);
                    let mut found = false;
                    for region in &build.hit_regions {
                        if region.clickable && region.rect.contains(pt) {
                            if let Some(ref id) = region.id {
                                self.app.on_right_click(id, self.mouse_x, self.mouse_y);
                                found = true;
                            }
                            break;
                        }
                    }
                    if !found {
                        // Right-click on empty area
                        self.app.on_right_click("", self.mouse_x, self.mouse_y);
                    }
                }
            }
            WindowEvent::MouseInput {
                state: winit::event::ElementState::Pressed,
                button: winit::event::MouseButton::Back,
                ..
            } => {
                self.app.on_navigate_back();
            }
            WindowEvent::MouseInput {
                state: winit::event::ElementState::Pressed,
                button: winit::event::MouseButton::Forward,
                ..
            } => {
                self.app.on_navigate_forward();
            }
            // 中ボタンの押下/解放もアプリへ転送（CAD系のドラッグパン用途）。#62
            WindowEvent::MouseInput { state, button: winit::event::MouseButton::Middle, .. } => {
                if self.primary_input == PrimaryInput::Touch {
                    return;
                }
                let position = sabitori_core::Point::new(self.mouse_x, self.mouse_y);
                let ev = match state {
                    winit::event::ElementState::Pressed => InputEvent::PointerPressed {
                        id: MOUSE_POINTER_ID,
                        kind: PointerKind::Mouse,
                        position,
                        button: Some(InputMouseButton::Middle),
                        modifiers: self.modifiers,
                    },
                    winit::event::ElementState::Released => InputEvent::PointerReleased {
                        id: MOUSE_POINTER_ID,
                        kind: PointerKind::Mouse,
                        position,
                        button: Some(InputMouseButton::Middle),
                        modifiers: self.modifiers,
                    },
                };
                self.app.on_input(&ev);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.primary_input == PrimaryInput::Touch {
                    return;
                }
                let (delta_x, delta_y) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (x * 20.0, y * 20.0),
                    winit::event::MouseScrollDelta::PixelDelta(pos) => {
                        (pos.x as f32, pos.y as f32)
                    }
                };
                // Route scroll to managed scroll container under cursor
                let handled = self
                    .last_build
                    .as_ref()
                    .map(|build| {
                        crate::scroll_sync::route_wheel(
                            build,
                            &mut self.scroll_states,
                            self.mouse_x,
                            self.mouse_y,
                            delta_x,
                            delta_y,
                        )
                    })
                    .unwrap_or(false);
                if !handled {
                    self.app.on_scroll(delta_y);
                    self.app.on_scroll_xy(delta_x, delta_y);
                }
            }
            // Touch events — first finger drives the primary flow (tap, scroll,
            // drag). A second finger promotes to a pinch gesture. Extra fingers
            // still surface as `InputEvent::Pointer*` for apps that want raw
            // multi-touch.
            //
            // Taps fire on release (not press) so we can distinguish tap-vs-scroll.
            WindowEvent::Touch(touch) => {
                let Some(r) = self.renderer.as_ref() else { return };
                let s = r.scale_factor;
                let x = touch.location.x as f32 / s;
                let y = touch.location.y as f32 / s;
                let pos = sabitori_core::Point::new(x, y);
                let id = touch.id.saturating_add(1);

                // Always update active_touches tracking and forward raw events.
                match touch.phase {
                    winit::event::TouchPhase::Started => {
                        self.active_touches.insert(touch.id, (x, y));
                        self.app.on_input(&InputEvent::PointerPressed {
                            id,
                            kind: PointerKind::Touch,
                            position: pos,
                            button: None,
                            modifiers: self.modifiers,
                        });
                        self.pressed_id = self.hit_id_at(x, y);
                    }
                    winit::event::TouchPhase::Moved => {
                        self.active_touches.insert(touch.id, (x, y));
                        self.app.on_input(&InputEvent::PointerMoved {
                            id,
                            kind: PointerKind::Touch,
                            position: pos,
                            modifiers: self.modifiers,
                        });
                    }
                    winit::event::TouchPhase::Ended => {
                        self.active_touches.remove(&touch.id);
                        self.app.on_input(&InputEvent::PointerReleased {
                            id,
                            kind: PointerKind::Touch,
                            position: pos,
                            button: None,
                            modifiers: self.modifiers,
                        });
                        self.pressed_id = None;
                    }
                    winit::event::TouchPhase::Cancelled => {
                        self.active_touches.remove(&touch.id);
                        self.app.on_input(&InputEvent::PointerCancelled {
                            id,
                            kind: PointerKind::Touch,
                        });
                        self.pressed_id = None;
                    }
                }

                // Mouse owns the primary flow → skip touch-driven logic.
                if self.primary_input == PrimaryInput::Mouse {
                    return;
                }
                if self.primary_input == PrimaryInput::None
                    && matches!(touch.phase, winit::event::TouchPhase::Started)
                {
                    self.primary_input = PrimaryInput::Touch;
                }

                match touch.phase {
                    winit::event::TouchPhase::Started => {
                        let count = self.active_touches.len();
                        if count == 1 {
                            // First finger — set up single-touch drag.
                            self.mouse_x = x;
                            self.mouse_y = y;

                            let mut click_target: Option<String> = None;
                            let mut pending_drag: Option<(String, Option<String>)> = None;
                            if let Some(ref build) = self.last_build {
                                let mut focus_set = false;
                                for region in &build.hit_regions {
                                    // マウス押下と同じく、 意味だけの領域は透過する。
                                    if region.is_interactive() && region.rect.contains(pos) {
                                        if region.focusable {
                                            self.focused_id = region.id.clone();
                                            focus_set = true;
                                        }
                                        if region.clickable {
                                            click_target = region.id.clone();
                                        }
                                        if let Some(ref drag_data) = region.drag_data {
                                            pending_drag =
                                                Some((drag_data.clone(), region.id.clone()));
                                        }
                                        break;
                                    }
                                }
                                if !focus_set {
                                    self.focused_id = None;
                                }
                            }
                            if let Some((data, source_id)) = pending_drag {
                                self.drag_manager.start_pending(data, source_id, x, y);
                            }

                            let mut scroll_target: Option<String> = None;
                            if let Some(ref build) = self.last_build {
                                for region in &build.hit_regions {
                                    if region.rect.contains(pos) {
                                        if let Some(ref rid) = region.id {
                                            if self.scroll_states.contains_key(rid) {
                                                scroll_target = Some(rid.clone());
                                                break;
                                            }
                                        }
                                    }
                                }
                            }

                            // If the touch landed on a managed scroll container,
                            // begin a drag so any in-flight fling stops here.
                            if let Some(ref sid) = scroll_target {
                                if let Some(sv) = self.scroll_states.get_mut(sid) {
                                    sv.begin_drag();
                                }
                            }
                            self.touch_drag = Some(TouchDrag {
                                id: touch.id,
                                start: (x, y),
                                last: (x, y),
                                last_move_time: None,
                                click_target,
                                scroll_target,
                                moved_beyond_slop: false,
                            });
                        } else if count == 2 && self.pinch.is_none() {
                            // Second finger — promote to pinch. Cancel the
                            // single-touch drag so no tap fires and no scroll
                            // keeps running under the gesture.
                            if let Some(ref mut td) = self.touch_drag {
                                td.moved_beyond_slop = true;
                            }
                            self.drag_manager.cancel();

                            let ids: Vec<u64> = self.active_touches.keys().copied().collect();
                            // Pick the current touch + whichever other finger is active.
                            let id_a = ids[0];
                            let id_b = if ids[1] == id_a { ids[0] } else { ids[1] };
                            let (other_a, other_b) =
                                if id_a == touch.id { (id_b, id_a) } else { (id_a, id_b) };
                            if let Some((dist, center)) =
                                pinch_metrics(&self.active_touches, other_a, other_b)
                            {
                                self.pinch = Some(PinchGesture {
                                    id_a: other_a,
                                    id_b: other_b,
                                    start_distance: dist.max(1.0),
                                });
                                self.app.on_pinch_start(center.0, center.1);
                            }
                        }
                        // count >= 3: ignore, pinch already active (or would be with 2).
                    }
                    winit::event::TouchPhase::Moved => {
                        // Pinch takes precedence over single-touch drag.
                        if let Some(ref pinch) = self.pinch {
                            if touch.id == pinch.id_a || touch.id == pinch.id_b {
                                if let Some((dist, center)) =
                                    pinch_metrics(&self.active_touches, pinch.id_a, pinch.id_b)
                                {
                                    let scale = dist / pinch.start_distance;
                                    self.app.on_pinch(scale, center.0, center.1);
                                }
                            }
                        } else if self
                            .touch_drag
                            .as_ref()
                            .map(|t| t.id == touch.id)
                            .unwrap_or(false)
                        {
                            self.mouse_x = x;
                            self.mouse_y = y;
                            self.update_hover();
                            self.app.on_pointer_move(x, y);

                            if let Some(ref mut td) = self.touch_drag {
                                let dx = x - td.last.0;
                                let dy = y - td.last.1;
                                td.last = (x, y);
                                let now = Instant::now();
                                let dt = td
                                    .last_move_time
                                    .map(|t| (now - t).as_secs_f32())
                                    .unwrap_or(0.0);
                                td.last_move_time = Some(now);
                                let total_dx = x - td.start.0;
                                let total_dy = y - td.start.1;
                                if !td.moved_beyond_slop
                                    && (total_dx * total_dx + total_dy * total_dy).sqrt()
                                        > TOUCH_SLOP
                                {
                                    td.moved_beyond_slop = true;
                                }
                                self.drag_manager.on_move(x, y);
                                if !self.drag_manager.is_active() && td.moved_beyond_slop {
                                    if let Some(ref sid) = td.scroll_target {
                                        if let Some(sv) = self.scroll_states.get_mut(sid) {
                                            sv.drag_by(dx, dy, dt);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => {
                        let is_cancel = matches!(touch.phase, winit::event::TouchPhase::Cancelled);

                        // End pinch if one of its fingers lifted.
                        if let Some(pinch) = self.pinch.as_ref() {
                            if touch.id == pinch.id_a || touch.id == pinch.id_b {
                                self.pinch = None;
                                self.app.on_pinch_end();
                                // Don't resume single-drag; require full lift.
                                // Any in-flight fling on the original target is killed.
                                if let Some(td) = self.touch_drag.take() {
                                    if let Some(sid) = td.scroll_target {
                                        if let Some(sv) = self.scroll_states.get_mut(&sid) {
                                            sv.cancel_fling();
                                        }
                                    }
                                }
                                self.drag_manager.cancel();
                            }
                        } else if self
                            .touch_drag
                            .as_ref()
                            .map(|t| t.id == touch.id)
                            .unwrap_or(false)
                        {
                            let td = self.touch_drag.take();
                            let drop_completed = self.drag_manager.on_release();
                            if let Some((data, _source_id)) = drop_completed {
                                if !is_cancel {
                                    if let Some(ref build) = self.last_build {
                                        if let Some(target_id) = build
                                            .hit_regions
                                            .iter()
                                            .find(|r| r.drop_zone && r.rect.contains(pos))
                                            .and_then(|r| r.id.as_ref())
                                        {
                                            self.app.on_drop(&data, target_id);
                                        }
                                    }
                                }
                                // Drag-and-drop path: no scroll fling, just reset state.
                                if let Some(td) = td {
                                    if let Some(sid) = td.scroll_target {
                                        if let Some(sv) = self.scroll_states.get_mut(&sid) {
                                            sv.cancel_fling();
                                        }
                                    }
                                }
                            } else if let Some(td) = td {
                                // Hand off to fling on release, or kill on cancel.
                                if let Some(ref sid) = td.scroll_target {
                                    if let Some(sv) = self.scroll_states.get_mut(sid) {
                                        if is_cancel {
                                            sv.cancel_fling();
                                        } else {
                                            sv.end_drag();
                                        }
                                    }
                                }
                                if !is_cancel && !td.moved_beyond_slop {
                                    if let Some(cid) = td.click_target {
                                        self.app.on_click(&cid);
                                    }
                                }
                            }
                            if is_cancel {
                                self.drag_manager.cancel();
                            }
                            self.app.on_pointer_up();
                        }

                        // Release primary-input ownership when the last finger lifts.
                        if self.active_touches.is_empty()
                            && self.primary_input == PrimaryInput::Touch
                        {
                            self.primary_input = PrimaryInput::None;
                        }
                    }
                }
            }
            WindowEvent::DroppedFile(path) => {
                self.app.on_file_drop(vec![path]);
            }
            WindowEvent::HoveredFile(path) => {
                self.app.on_file_hover(path);
            }
            WindowEvent::HoveredFileCancelled => {
                self.app.on_file_hover_cancelled();
            }
            WindowEvent::ModifiersChanged(mods) => {
                let state = mods.state();
                self.set_modifiers(Modifiers {
                    shift: state.shift_key(),
                    ctrl: state.control_key(),
                    alt: state.alt_key(),
                    meta: state.super_key(),
                });
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // winit → Key の変換は sabitori_window::keymap に集約している
                // （3 ランタイム共通）。対応が無い名前付きキーは Other として
                // 届ける — 修飾キー単独押下を「何か押された」として観測する
                // 既存の挙動（選択解除ロジックが Other に依存）を保つため。
                let key = sabitori_window::keymap::key_from_winit(&event.logical_key)
                    .unwrap_or(Key::Other);
                let pressed = event.state == winit::event::ElementState::Pressed;
                // テキスト入力として送るべき文字の判定（制御文字の除去、Cmd 押下時の
                // 抑制、Alt を通すか等）は keymap::char_inputs に集約している。
                // 解放時は空を返すので、本体側の分岐と二重には効かない。
                let chars = sabitori_window::keymap::char_inputs(&event, self.modifiers);
                self.handle_key_input(key, pressed, chars);
            }
            WindowEvent::Ime(ime_event) => {
                let event = match &ime_event {
                    winit::event::Ime::Preedit(text_str, cursor) => {
                        InputEvent::ImePreedit { text: text_str.clone(), cursor: cursor.map(|(s, e)| (s, e)) }
                    }
                    winit::event::Ime::Commit(text_str) => {
                        InputEvent::ImeCommit { text: text_str.clone() }
                    }
                    winit::event::Ime::Enabled => InputEvent::ImeEnabled,
                    winit::event::Ime::Disabled => { return; }
                };
                let handled = self.route_to_managed(&event)
                    || match self.focused_id {
                        Some(ref id) => self.app.on_focused_input(id, &event),
                        None => false,
                    };
                if !handled {
                    self.app.on_input(&event);
                }
            }
            WindowEvent::RedrawRequested => {
                // Ticks moved to `about_to_wait` so they run on a fixed
                // 16ms cadence independent of redraw decisions. By the time
                // this handler fires, animator/app state for the current
                // frame is already up to date; we just lay out + draw.

                // iOS: winit's own `WinitUIView` conforms to `UIKeyInput`, so
                // `set_ime_allowed(true)` makes it first responder and raises the
                // software keyboard; typed text arrives as `WindowEvent::Keyboard-
                // Input` (routed above) with no extra shim. Gate the keyboard on
                // focus so it only shows while a text field is focused.
                #[cfg(target_os = "ios")]
                {
                    // Hidden UITextField owns the keyboard (full UITextInput → Japanese
                    // marked-text composition works). Its editingChanged deltas arrive as
                    // Text/Backspace, routed here like physical keys.
                    if let Some(w) = self.window.as_ref() {
                        crate::ios_keyboard::ensure_attached(w);
                    }
                    crate::ios_keyboard::set_active(self.focused_id.is_some());
                    let mut events: Vec<InputEvent> = Vec::new();
                    for ev in crate::ios_keyboard::drain() {
                        match ev {
                            crate::ios_keyboard::KbEvent::Text(s) => {
                                for ch in s.chars() {
                                    if !ch.is_control() {
                                        events.push(InputEvent::CharInput(ch));
                                    }
                                }
                            }
                            crate::ios_keyboard::KbEvent::Backspace => {
                                events.push(InputEvent::KeyInput {
                                    key: Key::Backspace,
                                    pressed: true,
                                    modifiers: self.modifiers,
                                });
                            }
                        }
                    }
                    for e in events {
                        let handled = self.route_to_managed(&e)
                            || match self.focused_id {
                                Some(ref fid) => self.app.on_focused_input(fid, &e),
                                None => false,
                            };
                        if !handled {
                            self.app.on_input(&e);
                        }
                    }
                }

                // Read the surface geometry through a shared borrow and let it
                // end here: `build_frame` below needs `&mut self`, so the
                // renderer can't stay borrowed across it. It is re-acquired
                // mutably once the trees are built.
                let Some((scale, w, h)) = self.renderer.as_ref().map(|r| {
                    let scale = r.scale_factor;
                    // Quantize to 2px grid to prevent layout jitter from sub-pixel viewport changes
                    let w = ((r.surface_config.width as f32 / scale) * 0.5).floor() * 2.0;
                    let h = ((r.surface_config.height as f32 / scale) * 0.5).floor() * 2.0;
                    (scale, w, h)
                }) else { return; };
                let mut tr = match self.text_renderer.take() {
                    Some(t) => t,
                    None => return,
                };

                // Pick up a live monospace-family change (font picker) before
                // measuring; the measure cache isn't keyed on the face, so bust
                // it when the font actually changes.
                if tr.set_preferred_monospace_family(self.app.preferred_monospace_family()) {
                    self.measure_cache.borrow_mut().clear();
                }

                // Bake glyphs at the display's scale factor so text is crisp on
                // HiDPI (logical layout is unaffected — see TextRenderer::scale_factor).
                // The setter flushes the atlas when the scale changes (display
                // move between different-DPR screens) so old-scale bitmaps
                // don't accumulate until the atlas fills.
                tr.set_scale_factor(scale);

                // Build this frame's trees. The measurer borrows `tr` and a
                // cloned handle to the measure cache — both locals — so
                // `build_frame` can take `&mut self` at the same time.
                let cache = std::rc::Rc::clone(&self.measure_cache);
                let FrameBuild { mut build_result, overlay_build } = {
                    let measurer = crate::bridge::TextRendererMeasurer::new(&mut tr, &cache);
                    self.build_frame(w, h, &measurer)
                };

                // `tr` was taken out of `self`; every path from here on must put
                // it back, or the next frame bails at the `take()` above and the
                // window goes permanently blank.
                let Some(renderer) = self.renderer.as_mut() else {
                    self.text_renderer = Some(tr);
                    return;
                };

                // Layered path applies when EITHER:
                // * external overlay_view returned content
                // * the main tree contained `.overlay()` subtrees (build_result.overlay_list)
                // The previous version only used the layered path for external
                // overlays, so context menus placed in `view()` with `.overlay()`
                // had their commands emitted to overlay_list and then silently
                // dropped (never drawn). This fixes auto-hoist rendering.
                let has_external_overlay = overlay_build.is_some();
                let has_internal_overlay = !build_result.overlay_list.commands.is_empty();
                let has_overlay = has_external_overlay || has_internal_overlay;

                // Both branches yield the build the frame was actually drawn
                // from, so there is exactly one place that stores it — see the
                // `commit_build` call below.
                let drawn_build = if has_overlay {
                    // Merge external overlay draws into build_result.overlay_list
                    // so the renderer has one overlay command stream.
                    let external_hits = if let Some(ext) = overlay_build {
                        build_result.overlay_list.commands.extend(ext.render_list.commands);
                        ext.hit_regions
                    } else {
                        Vec::new()
                    };
                    let (mut base_rects, mut base_lists, text_layouts) =
                        UiDrawLists::extract_with_hits(&build_result.render_list, &mut tr);
                    let (overlay_rects, overlay_lists) =
                        UiDrawLists::extract(&build_result.overlay_list, &mut tr);
                    // text_layouts は次フレームの mouse 入力 / Cmd+C で使うので保存。
                    self.text_layouts = text_layouts;
                    // View 切替で selection が無効になる場合に clear。
                    Self::invalidate_stale_selection(
                        &mut self.selection,
                        &mut self.selecting,
                        &self.text_layouts,
                    );
                    // Selection highlight rects を base_rects の末尾に append (=
                    // 他の base rects の上、 glyph の下に painter order で挟まる)。
                    // renderer の mutable borrow 中なので self.selection_rects() が
                    // borrow checker に怒られる。 selection + text_layouts は self が
                    // 持ってる field なので、 直接 field アクセスで rect 列を組む。
                    let (sel_bg, sel_fg) = match self.app.selection_style() {
                        Some((bg, fg)) => (bg, Some(fg.to_array())),
                        None => (sabitori_core::Color::new(0.31, 0.55, 0.95, 0.35), None),
                    };
                    // find-in-page ハイライトは selection の下に敷く (append 順 =
                    // painter order。 selection が上、 highlight が下、 両方 glyph の下)。
                    base_rects.extend(Self::compute_highlight_rects(&self.text_layouts));
                    base_rects.extend(Self::compute_link_rects(&self.text_layouts));
                    let sel_rects = Self::compute_selection_rects(
                        self.selection.as_ref(),
                        &self.text_layouts,
                        sel_bg,
                    );
                    if let Some(fg) = sel_fg {
                        Self::recolor_selected_glyphs(&mut base_lists.glyphs, &sel_rects, fg);
                    }
                    base_rects.extend(sel_rects);

                    // External overlay hit regions (if any) go in front of
                    // everything else — same precedence the old overlay_view
                    // path had.
                    let mut merged = build_result;
                    merged.hit_regions.splice(0..0, external_hits);

                    let device = renderer.device.clone();
                    let queue = renderer.queue.clone();
                    let mut ir = self.image_renderer.take();
                    let mut rr = self.ring_renderer.take();
                    let mut lr = self.line_renderer.take();
                    let _ = renderer.render_layered(
                        &base_rects,
                        &overlay_rects,
                        |phase, pass, globals_bg| {
                            let lists = match phase {
                                RenderPhase::BaseText => &base_lists,
                                RenderPhase::OverlayText => &overlay_lists,
                            };
                            let mut r = UiRenderers {
                                images: ir.as_mut(),
                                rings: rr.as_mut(),
                                lines: lr.as_mut(),
                                text: &mut tr,
                            };
                            draw_ui_layer(&mut r, lists, &device, &queue, pass, globals_bg);
                        },
                    );
                    self.image_renderer = ir;
                    self.ring_renderer = rr;
                    self.line_renderer = lr;
                    merged
                } else {
                    let (mut rects, mut lists, text_layouts) =
                        UiDrawLists::extract_with_hits(&build_result.render_list, &mut tr);
                    self.text_layouts = text_layouts;
                    Self::invalidate_stale_selection(
                        &mut self.selection,
                        &mut self.selecting,
                        &self.text_layouts,
                    );
                    let (sel_bg, sel_fg) = match self.app.selection_style() {
                        Some((bg, fg)) => (bg, Some(fg.to_array())),
                        None => (sabitori_core::Color::new(0.31, 0.55, 0.95, 0.35), None),
                    };
                    // find-in-page ハイライトは selection の下に敷く (glyph の下)。
                    rects.extend(Self::compute_highlight_rects(&self.text_layouts));
                    rects.extend(Self::compute_link_rects(&self.text_layouts));
                    let sel_rects = Self::compute_selection_rects(
                        self.selection.as_ref(),
                        &self.text_layouts,
                        sel_bg,
                    );
                    if let Some(fg) = sel_fg {
                        Self::recolor_selected_glyphs(&mut lists.glyphs, &sel_rects, fg);
                    }
                    rects.extend(sel_rects);

                    let device = renderer.device.clone();
                    let queue = renderer.queue.clone();
                    let mut ir = self.image_renderer.take();
                    let mut rr = self.ring_renderer.take();
                    let mut lr = self.line_renderer.take();
                    let _ = renderer.render_with(&rects, |pass, globals_bg| {
                        let mut r = UiRenderers {
                            images: ir.as_mut(),
                            rings: rr.as_mut(),
                            lines: lr.as_mut(),
                            text: &mut tr,
                        };
                        draw_ui_layer(&mut r, &lists, &device, &queue, pass, globals_bg);
                    });
                    self.image_renderer = ir;
                    self.ring_renderer = rr;
                    self.line_renderer = lr;
                    build_result
                };

                self.commit_build(drawn_build);

                // If the atlas overflowed this frame (glyphs dropped → blank
                // text), force one more frame so maybe_recover_atlas can flush +
                // re-shape. Read before moving `tr` back; survives the
                // `dirty = false` reset below via its own flag. Guard against a
                // spin: if this WAS the forced recovery frame and it STILL
                // overflowed, the frame's glyph set genuinely exceeds the atlas
                // — stop forcing (one glitch frame beats a busy loop).
                let overflowed = tr.atlas_overflowed();
                self.atlas_recover_pending = overflowed && !self.atlas_recover_pending;
                self.text_renderer = Some(tr);
                // Frame finished — invalidate the dirty flag so lazy mode
                // can park the next about_to_wait until something changes.
                self.dirty = false;
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let elapsed = self.last_frame.elapsed();
        // Default 8ms (~120Hz). App can override via `target_frame_interval`.
        let target = self.app.target_frame_interval();

        // Park the loop until the next 16ms boundary if we're early.
        if elapsed < target {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                Instant::now() + (target - elapsed),
            ));
            return;
        }

        let dt = elapsed.as_secs_f32().min(0.05);
        self.last_frame = Instant::now();

        // Run all ticks on the fixed cadence. They drive animations
        // (style spring, scroll spring, presence) and app-side async
        // drains (e.g. mpsc channel polls). Cheap enough to run every
        // frame even when we end up not redrawing.
        self.advance(dt);
        // After tick: let the app reassert its desired focus. Popups
        // that open with a known input (e.g. command palette) use
        // this to grab focus the first frame they're rendered,
        // without needing the user to click the input first.
        if let Some(desired) = self.app.desired_focus() {
            if self.focused_id.as_deref() != Some(&desired) {
                self.focused_id = Some(desired);
            }
        }
        // Anchor the platform IME (conversion / candidate window) at the app's
        // caret. Polled every frame but deduped — only re-sent when the caret
        // rect changes — so the Cocoa call doesn't fire 125×/sec. Without this,
        // winit leaves the area at the window origin and the candidate window
        // sits in the top-left.
        // アプリが明示した矩形が最優先。 無ければ、 フォーカス中の登録済みテキスト欄
        // から自動で算出する — その欄の画面矩形はレイアウト済みで `hit_regions` に
        // あり、 キャレットの x は表示文字列を実フォントで測れば出る。 つまり
        // ランタイムは必要な材料を全部持っている。 アプリが `ime_cursor_area` を
        // 書かなくても、 変換候補は正しい位置に出る。
        let ime_area = self.app.ime_cursor_area().or_else(|| self.managed_ime_area());
        if ime_area != self.last_ime_area {
            self.last_ime_area = ime_area;
            if let (Some(w), Some((x, y, cw, ch))) = (self.window.as_ref(), ime_area) {
                w.set_ime_cursor_area(
                    winit::dpi::LogicalPosition::new(x, y),
                    winit::dpi::LogicalSize::new(cw, ch),
                );
            }
        }
        // Enable/disable the platform IME per app policy (deduped). Disabling
        // cancels an in-flight composition, so a dialog closing mid-composition
        // doesn't leave an orphaned candidate window.
        let ime_allowed = self.app.ime_allowed();
        if ime_allowed != self.last_ime_allowed {
            self.last_ime_allowed = ime_allowed;
            if let Some(w) = self.window.as_ref() {
                w.set_ime_allowed(ime_allowed);
            }
        }
        // Layout / focus may have changed since the last pointer event —
        // keep the capture snapshot current once per tick.
        self.push_ui_capture();
        let app_dirty = self.app.poll_dirty();
        // Scroll spring / fling keeps animating after the user lets go of the
        // wheel. Without this check, lazy_render parks the loop and the
        // momentum scroll stutters or freezes mid-flight.
        let scroll_animating = self.scroll_states.values().any(|sv| sv.is_animating());
        let any_anim = self.style_animator.is_animating()
            || self.drag_manager.is_active()
            || scroll_animating
            // tooltip の hover-delay / fade 中は tick を回し続ける。無いと lazy_render が
            // loop を park して delay タイマが止まり、マウスを動かすまで tooltip が出ない。
            || self.tooltip_state.is_pending();
        let must_draw = !self.app.lazy_render()
            || self.dirty
            || app_dirty
            || any_anim
            || self.atlas_recover_pending;

        if must_draw {
            if let Some(w) = self.window.as_ref() {
                w.request_redraw();
            }
            // Extras share the same redraw cadence as the primary —
            // particle bursts, animated overlays etc. need every-frame
            // updates while is_animating() is true. They don't have
            // their own dirty flag (no input → no per-window dirty
            // signal), so piggybacking on the primary's decision keeps
            // the bookkeeping minimal.
            for extra in self.extras.values() {
                extra.window.request_redraw();
            }
        }

        // Schedule the next wakeup at the 16ms boundary. Without this the
        // loop would either spin (Poll) or sleep forever (Wait).
        //
        // iOS: when idle (nothing to draw), park fully with `Wait` instead of a
        // 16ms timer. The constant timer wakeups re-enter winit's iOS redraw
        // phase every frame and starve UIKit's text-input run-loop sources, so
        // the software keyboard shows but `insertText:` is never delivered.
        // Parking when idle lets the keyboard session be serviced; real input
        // (a keystroke, an animation) wakes the loop and restores the timer.
        #[cfg(target_os = "ios")]
        if must_draw {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                self.last_frame + target,
            ));
        } else {
            event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);
        }
        #[cfg(not(target_os = "ios"))]
        event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
            self.last_frame + target,
        ));
    }
}

/// One frame's built element trees, before any GPU work touches them.
/// Produced by [`AppState::build_frame`].
pub(crate) struct FrameBuild {
    /// The main UI tree.
    pub(crate) build_result: BuildResult,
    /// The overlay tree — app `overlay_view` plus the tooltip and drag ghost —
    /// when any of those produced content this frame.
    pub(crate) overlay_build: Option<BuildResult>,
}

impl<A: DeclarativeApp> AppState<A> {
    /// Runtime state for `app` with no window or GPU resources attached yet.
    ///
    /// Everything that isn't a GPU resource is initialized here, so the state is
    /// already usable for what happens independently of rendering: input
    /// routing, animators, managed scroll, and [`Self::build_frame`].
    /// `resumed` fills in the window and the renderers afterwards.
    pub(crate) fn new(app: A) -> Self {
        let image_cache = std::sync::Arc::new(std::sync::Mutex::new(
            sabitori_core::image_cache::ImageCache::new(),
        ));
        let image_pending = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        #[cfg(not(target_arch = "wasm32"))]
        let image_runtime = std::sync::Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .thread_name("sabitori-image")
                .build()
                .expect("image runtime"),
        );
        #[cfg(not(target_arch = "wasm32"))]
        let image_ctx = crate::image_runtime::make_image_ctx(
            image_cache.clone(),
            image_pending.clone(),
            image_runtime.handle().clone(),
        );
        #[cfg(target_arch = "wasm32")]
        let image_ctx = crate::image_runtime::make_image_ctx(
            image_cache.clone(),
            image_pending.clone(),
        );
        Self {
            app,
            dirty: true,
            window: None,
            renderer: None,
            text_renderer: None,
            image_renderer: None,
            ring_renderer: None,
            line_renderer: None,
            measure_cache: std::rc::Rc::new(std::cell::RefCell::new(
                crate::bridge::MeasureCache::new(),
            )),
            last_frame: Instant::now(),
            last_build: None,
            mouse_x: 0.0,
            mouse_y: 0.0,
            hovered_id: None,
            pressed_id: None,
            last_cursor: None,
            last_ime_area: None,
            last_ime_allowed: true,
            focused_id: None,
            warned_unrouted_input: std::collections::HashSet::new(),
            managed: Vec::new(),
            actions: Vec::new(),
            last_viewport_w: 0.0,
            last_viewport_h: 0.0,
            primary_input: PrimaryInput::None,
            active_touches: std::collections::HashMap::new(),
            touch_drag: None,
            pinch: None,
            scroll_states: std::collections::HashMap::new(),
            modifiers: Modifiers::default(),
            tooltip_state: sabitori_widgets::TooltipState::new(),
            drag_manager: sabitori_widgets::DragManager::new(),
            style_animator: sabitori_widgets::StyleAnimator::new(),
            presence_animator: sabitori_widgets::PresenceAnimator::new(),
            image_cache,
            image_pending,
            image_ctx,
            #[cfg(not(target_arch = "wasm32"))]
            _image_runtime: image_runtime,
            pending_redraw: true,
            atlas_recover_pending: false,
            extras: std::collections::HashMap::new(),
            text_layouts: Vec::new(),
            selection: None,
            selecting: false,
            last_capture: UiCapture::default(),
        }
    }

    /// Build this frame's element trees for a `w` × `h` logical viewport,
    /// measuring text through `measurer`.
    ///
    /// This is the whole per-frame pipeline **except** the GPU: assemble the
    /// [`ViewContext`] from the current input / scroll / drag state, call the
    /// app's `view`, run the presence and style animators over the result,
    /// patch in managed scroll offsets, lay it out, feed the measured scroll
    /// extents back, apply the app's scroll intents, and build the overlay
    /// tree the same way.
    ///
    /// Splitting it out keeps the frame's *logic* reachable without a window or
    /// a `wgpu` device — the render path calls it with a `TextRenderer`-backed
    /// measurer, and tests call it with a stub one.
    pub(crate) fn build_frame(
        &mut self,
        w: f32,
        h: f32,
        measurer: &dyn sabitori_core::build::TextMeasure,
    ) -> FrameBuild {
        // Build scroll info for ViewContext
        let scroll_info: std::collections::HashMap<String, sabitori_core::ScrollInfo> =
            self.scroll_states
                .iter()
                .map(|(id, sv)| {
                    (
                        id.clone(),
                        sabitori_core::ScrollInfo {
                            scroll_x: sv.scroll_x.value(),
                            scroll_y: sv.scroll_y.value(),
                            viewport_width: sv.viewport_width,
                            viewport_height: sv.viewport_height,
                            content_width: sv.content_width,
                            content_height: sv.content_height,
                        },
                    )
                })
                .collect();

        let tooltip_info = self.tooltip_state.info().map(|(text, x, y)| {
            sabitori_core::TooltipInfo { text, x, y }
        });

        let drag_info = if let Some((data, source_id)) = self.drag_manager.drag_info() {
            let over = self.last_build.as_ref().and_then(|build| {
                let pt = sabitori_core::Point::new(self.mouse_x, self.mouse_y);
                build.hit_regions.iter()
                    .find(|r| r.drop_zone && r.rect.contains(pt))
                    .and_then(|r| r.id.clone())
            });
            Some(sabitori_core::DragInfo { data, source_id, over_drop_zone: over })
        } else {
            None
        };

        // Drop all completed fetches into the cache before the
        // view reads from it, so ready images are visible the same
        // frame they finish.
        crate::image_runtime::drain_pending(
            &self.image_cache,
            &self.image_pending,
        );

        // Per-px monospace advance for the active `.mono()` face, so
        // terminal/code grids can tile exactly (see ViewContext::mono_advance).
        let mono_advance = measurer
            .measure("0000000000", 100.0, false, true, None, None, None, sabitori_core::Typography::default())
            .size
            .width
            / 1000.0;
        let ctx = ViewContext {
            width: w,
            height: h,
            hovered: self.hovered_id.clone(),
            focused: self.focused_id.clone(),
            mouse_x: self.mouse_x,
            mouse_y: self.mouse_y,
            shift_held: self.modifiers.shift,
            cmd_held: self.modifiers.meta,
            scroll_states: scroll_info,
            tooltip: tooltip_info,
            drag: drag_info,
            theme: self.app.theme(),
            presence: self.presence_animator.all_progress(),
            images: Some(self.image_ctx.clone()),
            mono_advance,
            // 実フォント計測をアプリに渡す。 これが無いとキャレット位置を
            // 計算する手段が無く、 等幅以外のテキスト欄にカーソルを置けない
            // (issue #15)。
            measurer: Some(measurer),
            managed: Default::default(),
            actions: Default::default(),
        };
        // Build main UI tree
        let mut root = self.app.view(&ctx);

        // Apply presence (mount/unmount) animations
        self.presence_animator.update_presence(&root);
        self.presence_animator.apply(&mut root);

        // Apply hover/active styles and spring transitions:
        // 1. Elements WITH transitions → use StyleAnimator (smooth spring)
        // 2. それ以外、および animator が扱わないフィールド → 即時に畳む
        self.style_animator.update(&root, &self.hovered_id, &self.pressed_id);
        self.style_animator.apply(&mut root);
        sabitori_core::element::apply_state_styles(&mut root, &self.hovered_id, &self.pressed_id);

        // Patch scroll offsets from managed state + register new scroll containers
        crate::scroll_sync::patch_scroll_offsets(&mut root, &mut self.scroll_states);
        // Off-screen positions the app asked for (scroll-to-element). Empty set =
        // the probing branch is skipped entirely, so apps that never ask pay nothing.
        let probes: std::collections::HashSet<String> = self.app.build_probes().into_iter().collect();
        let build_result = if probes.is_empty() {
            build_tree_measured(&root, w, h, measurer)
        } else {
            sabitori_core::build::build_tree_measured_probed(&root, w, h, measurer, &probes)
        };

        // Feed back measured scroll extents to managed state (both axes).
        crate::scroll_sync::apply_scroll_measures(&build_result, &mut self.scroll_states);

        // Apply programmatic scroll requests after content_height is known
        for (id, y) in self.app.scroll_intents() {
            if let Some(sv) = self.scroll_states.get_mut(&id) {
                sv.smooth_scroll_to(y);
            }
        }

        // Build overlay tree separately (if any)
        // Merge tooltip and drag ghost into the overlay if active
        let app_overlay = self.app.overlay_view(&ctx);

        // `view()` / `overlay_view()` の中でウィジェットが登録したものを引き取る。
        // 以後の入力配信・tick・フォーカス反映はランタイムが持つので、 アプリ側に
        // 書くことは何も無い (`sabitori_core::Managed` の doc を参照)。
        self.adopt_managed(ctx.take_managed());
        self.actions = ctx.take_actions();
        let tooltip_element = self.tooltip_state.info().map(|(text, tx, ty)| {
            sabitori_core::tooltip_popup(
                &text, tx, ty, w, h,
                sabitori_core::Color::new(0.15, 0.15, 0.18, 0.95),
                sabitori_core::Color::new(0.9, 0.9, 0.9, 1.0),
                sabitori_core::Color::new(0.3, 0.3, 0.35, 0.8),
            )
        });
        let drag_ghost_element = if self.drag_manager.is_active() {
            self.app.drag_ghost(&ctx)
        } else {
            None
        };
        // Collect all overlay parts
        let mut overlay_parts: Vec<Element> = Vec::new();
        if let Some(el) = app_overlay { overlay_parts.push(el); }
        if let Some(el) = tooltip_element { overlay_parts.push(el); }
        if let Some(el) = drag_ghost_element { overlay_parts.push(el); }
        // 常に full-viewport コンテナで包む。単一 overlay をそのまま root にすると
        // `.absolute()`(tooltip/drag ghost)の mt/ml が解決する親を失い左上に潰れる
        // （複数時だけ包んでいたので tooltip 単独表示で位置がバグっていた）。
        let overlay_element = if overlay_parts.is_empty() {
            None
        } else {
            Some(sabitori_core::div()
                .w(sabitori_core::Dimension::Px(w))
                .h(sabitori_core::Dimension::Px(h))
                .children(overlay_parts))
        };
        let overlay_build = overlay_element.map(|mut el| {
            // Overlay trees also participate in managed scroll: register their
            // `.scroll(id)` containers so wheel/touch routing (which consults
            // the merged build with overlay hits prepended) can scroll modal
            // lists AND let a full-screen scrim absorb scroll (background lock).
            // Without this, overlay scroll containers are never in scroll_states,
            // so route_wheel falls through to the base tree behind the modal.
            crate::scroll_sync::patch_scroll_offsets(&mut el, &mut self.scroll_states);
            let built = build_tree_measured(&el, w, h, measurer);
            crate::scroll_sync::apply_scroll_measures(&built, &mut self.scroll_states);
            built
        });

        FrameBuild { build_result, overlay_build }
    }

    /// Record the build this frame was drawn from and hand it to the app in the
    /// same step.
    ///
    /// The app gets it so it can look elements up by id (hit-region rects)
    /// without re-deriving layout — scroll-to-element, floating panels pinned to
    /// a widget, and the like.
    ///
    /// Storing and notifying are deliberately one operation. They used to be
    /// two, and `run_declarative` did the first without ever doing the second
    /// ([#57](https://github.com/Mutafika/sabitori/issues/57)) — so every
    /// declarative app was rendered from a build it was never told about, and
    /// `hit_regions` was unreachable. Keeping this the only writer of
    /// `last_build` makes that pairing impossible to get wrong again.
    pub(crate) fn commit_build(&mut self, build: BuildResult) {
        self.last_build = Some(build);
        // `last_build` was just assigned, so the app always sees this frame.
        if let Some(ref build) = self.last_build {
            self.app.on_build(build);
        }
    }

    /// `WindowEvent::KeyboardInput` の本体を winit から剥がしたもの。 `chars` は
    /// `keymap::char_inputs` が解決済みの文字（解放時は空）。
    ///
    /// 押下・解放の**両方**で呼ばれ、`KeyInput` もその両方を発行する。押下だけを
    /// 配っていた頃は、⇧の押下は届くのに解放が来ないので、アプリ側で「押しっぱなし」
    /// を保持すると二度と落ちなかった（⇧+ドラッグ = 選択に足す、のような修飾つき
    /// 操作が書けない）。副作用は `if pressed` の中に閉じてある。
    /// 主ボタン押下の処理本体。 winit の match から切り出してある。
    ///
    /// 切り出しは [`crate::testing`] のためでもある — ヘッドレスでクリックを
    /// 流せないと、 消費側は自分のアプリに回帰テストを書けない (issue #19)。
    /// 座標は呼び出し前に `mouse_x` / `mouse_y` へ入れておくこと。
    /// **テキスト欄にフォーカスがあるのに、 打った文字を誰も受け取らなかった**
    /// ときに一度だけ警告する。
    ///
    /// これは設定ミスであって、 正常な状態ではない。 `text_input(..)` を `view()`
    /// に置いてクリックすると、 フォーカスは入るし枠も光る。 でも
    /// [`DeclarativeApp::on_focused_input`] を実装していないと、 **打った文字は
    /// どこにも行かない**。 既定実装が `false` を返すだけなので、 コンパイルは
    /// 通り、 パニックもせず、 ただ何も起きない。 0.4.0 が潰してきたのと同じ
    /// 形の失敗が、 いちばんよく使うウィジェットに残っていた。
    ///
    /// 構造で防げれば一番いいが、 文字の行き先はアプリの状態なので、 ランタイム
    /// からは代わりに書き込めない。 せめて**黙って落ちるのをやめる**。
    ///
    /// 判定は `Role::TextInput` / `Role::TextArea` を名乗る要素にフォーカスが
    /// ある場合だけ。 フォーカスできるボタンが打鍵を無視するのは正常なので、
    /// そこでは鳴らさない。 id ごとに 1 回。
    fn warn_if_typing_went_nowhere(&mut self) {
        let Some(id) = self.focused_id.clone() else { return };
        let Some(build) = self.last_build.as_ref() else { return };
        let is_text_field = build.hit_regions.iter().any(|r| {
            r.id.as_deref() == Some(id.as_str())
                && matches!(
                    r.role,
                    Some(sabitori_core::element::Role::TextInput)
                        | Some(sabitori_core::element::Role::TextArea)
                )
        });
        if !is_text_field || !self.warned_unrouted_input.insert(id.clone()) {
            return;
        }
        log::warn!(
            "sabitori: テキスト欄 `{id}` にフォーカスがあるのに、 打った文字を \
             誰も受け取っていない。 `DeclarativeApp::on_focused_input` を実装して \
             `{id}` を状態へ繋ぐこと:\n\
             \n\
             \x20   fn on_focused_input(&mut self, id: &str, e: &InputEvent) -> bool {{\n\
             \x20       match id {{ \"{id}\" => self.field.on_focused_input(e), _ => false }}\n\
             \x20   }}\n"
        );
    }

    /// `view()` が登録したものを引き取り、 **フォーカス状態をその場で反映する**。
    ///
    /// 反映をここでやるのは、 「フォーカスが変わった」 と 「ツリーが組み直された」
    /// のどちらが先でも、 描画に使うフレームでは必ず一致させたいから。
    /// アプリが `state.focused = true` を書く必要は無い。
    pub(crate) fn adopt_managed(
        &mut self,
        managed: Vec<(String, std::rc::Rc<dyn sabitori_core::Managed>)>,
    ) {
        for (id, target) in &managed {
            if let Some(field) = target.as_any().downcast_ref::<TextInputState>() {
                field.set_focused(self.focused_id.as_deref() == Some(id.as_str()));
                // 折り返し幅は「欄が実際に何 px だったか」で決まるが、 それは
                // レイアウトが終わるまで分からない。 前フレームの実測を渡して
                // おき、 次の `view()` がそれで折り返す。 幅が変わった最初の
                // 1 フレームだけ古い幅で折り返し、 次で追いつく。
                if let Some(rect) = self.last_build.as_ref().and_then(|b| b.region_rect(id)) {
                    field.set_measured_width(rect.size.width);
                }
            }
        }
        self.managed = managed;
    }

    /// フォーカス中の登録済みテキスト欄から、 IME 変換候補を出す矩形を作る。
    ///
    /// これを返さないと winit は候補位置をウィンドウ原点のままにするので、
    /// **日本語の変換候補が画面の左上に出る**。 0.4.0 より前はアプリが
    /// `ime_cursor_area` を実装して自分で計算する必要があり、 書かなければ
    /// 黙って左上に出ていた。
    ///
    /// 欄の矩形は `hit_regions` にあり、 キャレットの x は表示文字列を実フォントで
    /// 測れば出る。 材料はランタイムが全部持っているので、 アプリに書かせる理由が無い。
    fn managed_ime_area(&self) -> Option<(f32, f32, f32, f32)> {
        let id = self.focused_id.as_deref()?;
        let field = self.managed_text_field(id)?;
        let build = self.last_build.as_ref()?;
        let rect = build.region_rect(id)?;
        // キャレットの欄内オフセットは `text_input` が描くときに実フォントで
        // 測って書き込んである。 ここは画面座標を足すだけ。
        let (dx, caret_h) = field.caret_offset();
        Some((rect.origin.x + dx, rect.origin.y, 1.0, caret_h.max(1.0)))
    }

    /// `id` に結びついたクリック処理を走らせる。 あれば `true`。
    ///
    /// `Element::click(ctx, id, f)` で登録されたもの。 id とハンドラが同じ
    /// 呼び出しで書かれているので、 **文字列の食い違いが起こらない**。
    pub(crate) fn run_action(&mut self, id: &str) -> bool {
        // `self.app` を可変で貸すあいだ `self.actions` を借りたままにできないので、
        // Rc を 1 つ複製して外に出す。
        let action = self
            .actions
            .iter()
            .find(|(aid, _)| aid == id)
            .map(|(_, a)| a.clone());
        match action {
            Some(action) => {
                action(&mut self.app as &mut dyn std::any::Any);
                true
            }
            None => false,
        }
    }

    /// 登録済みのテキスト欄をフォーカス id で引く。
    fn managed_text_field(&self, id: &str) -> Option<&TextInputState> {
        self.managed
            .iter()
            .find(|(mid, _)| mid == id)
            .and_then(|(_, t)| t.as_any().downcast_ref::<TextInputState>())
    }

    /// フォーカス中の要素が登録済みなら、 そこへイベントを流す。 消費したら `true`。
    ///
    /// **アプリの `on_focused_input` より先**に見る。 登録されている欄は
    /// ランタイムの持ち物で、 アプリが二重に処理する余地を作らないため。
    pub(crate) fn route_to_managed(&mut self, event: &InputEvent) -> bool {
        let Some(id) = self.focused_id.clone() else { return false };
        match self.managed_text_field(&id) {
            Some(field) => field.handle_input(event),
            None => false,
        }
    }

    /// 打鍵が行き場を失ったテキスト欄の id。 テストから配線漏れを assert する
    /// ための口 ([`crate::testing::Harness::unrouted_text_inputs`])。
    pub(crate) fn unrouted_text_inputs(&self) -> &std::collections::HashSet<String> {
        &self.warned_unrouted_input
    }

    /// 時間を `dt` 秒ぶん進める。 アプリの `tick` と、 ランタイムが持つ
    /// アニメーション状態 (スクロールのばね・慣性、 tooltip の遅延、 ドラッグ、
    /// style / presence) を **1 箇所で**まとめて回す。
    ///
    /// ## なぜ関数に括り出すか
    ///
    /// 以前はこの並びが `about_to_wait` にベタ書きされていた。 そのため
    /// [`testing::Harness`](crate::testing::Harness) には**時間が無く**、
    /// ばねが 1mm も動かなかった。 `scroll_intents()` は `smooth_scroll_to`
    /// (= ばねの目標を置くだけ) なので、 プログラム的スクロールを使うアプリは
    /// **テストすると必ず「動かない」ように見える**。 実機では動くのに。
    ///
    /// tick する対象が増えたときに、 ランタイムだけ更新されて Harness が
    /// 置き去りになるのも防ぐ。
    pub(crate) fn advance(&mut self, dt: f32) {
        self.app.tick(dt);
        // 登録済みのテキスト欄はランタイムが進める (キャレット点滅)。
        // アプリが `state.tick(dt)` を書く必要は無い。
        for (_, target) in &self.managed {
            if let Some(field) = target.as_any().downcast_ref::<TextInputState>() {
                field.advance(dt);
            }
        }
        for sv in self.scroll_states.values_mut() {
            sv.tick(dt);
        }
        self.tooltip_state.tick(dt);
        self.drag_manager.tick(dt);
        self.style_animator.tick(dt);
        self.presence_animator.tick(dt);
    }

    /// ランタイムかアプリのどこかがまだ動いているか。 [`Self::advance`] を
    /// 回し続けるべきかの判定で、 テストの「落ち着くまで待つ」もこれを見る。
    pub(crate) fn is_animating(&self) -> bool {
        self.app.is_animating()
            || self.scroll_states.values().any(|sv| sv.is_animating())
            || self.style_animator.is_animating()
            || self.drag_manager.is_active()
            || self.tooltip_state.is_pending()
    }

    pub(crate) fn press_primary(&mut self) {
        // マウス押下もタッチ同様 InputEvent::Pointer* としてアプリへ転送する
        // （キャンバスのドラッグパン等が押下状態を観測できるように）。#62
        self.app.on_input(&InputEvent::PointerPressed {
            id: MOUSE_POINTER_ID,
            kind: PointerKind::Mouse,
            position: sabitori_core::Point::new(self.mouse_x, self.mouse_y),
            button: Some(InputMouseButton::Left),
            modifiers: self.modifiers,
        });
        // 押下中の要素を覚える。 次のフレームの `apply_state_styles` が
        // ここから `active_style` を畳む (#3)。 hover と同じ引き方だが、
        // 押下対象は `clickable`（= id 付き）で見る — `.active()` は
        // hover_style を持たない要素にも書けるため。
        self.pressed_id = self.hit_id_at(self.mouse_x, self.mouse_y);
        if let Some(ref build) = self.last_build {
            let pt = sabitori_core::Point::new(self.mouse_x, self.mouse_y);
            let mut focus_set = false;
            let mut pending_drag: Option<(String, Option<String>)> = None;
            let mut hit_clickable_or_drag = false;
            // クリック対象はここでは決めるだけ。 実行はループを抜けてから —
            // ハンドラは `&mut self` を要り、 `build` を借りたままでは呼べない。
            let mut click_target: Option<String> = None;
            for region in &build.hit_regions {
                // 意味だけの領域 (role/label のみ) は透過する。 これを止めると
                // 表のセルに `Role::Cell` を書いた瞬間、 行のクリックが死ぬ
                // (`HitRegion::is_interactive` の doc を参照)。
                if region.is_interactive() && region.rect.contains(pt) {
                    // Handle focus
                    if region.focusable {
                        self.focused_id = region.id.clone();
                        focus_set = true;
                        // 登録済みテキスト欄なら、 **押した場所にキャレットを置く**。
                        // 欄の内側が原点の座標に直して渡し、 解決は次の `view()`
                        // でやる (実フォントで測れるのがそこだけなので)。
                        if let Some(id) = region.id.as_deref() {
                            if let Some(field) = self.managed_text_field(id) {
                                field.request_point(
                                    pt.x - region.rect.origin.x,
                                    pt.y - region.rect.origin.y,
                                    self.modifiers.shift,
                                );
                            }
                        }
                    }
                    // Handle click (still fires for draggable elements)
                    if region.clickable {
                        click_target = region.id.clone();
                        hit_clickable_or_drag = true;
                    }
                    // Check for drag data
                    if let Some(ref drag_data) = region.drag_data {
                        pending_drag = Some((drag_data.clone(), region.id.clone()));
                        hit_clickable_or_drag = true;
                    }
                    break;
                }
            }
            // `Element::click` で登録された処理が先。 その後で従来の
            // `on_click(id)` も呼ぶ (併用しても壊れない)。
            if let Some(id) = click_target {
                self.run_action(&id);
                self.app.on_click(&id);
            }
            // Blur if clicked on a non-focusable region (or empty area)
            if !focus_set {
                self.focused_id = None;
            }
            // Start pending drag if a draggable element was hit
            let had_pending_drag = pending_drag.is_some();
            if let Some((data, source_id)) = pending_drag {
                self.drag_manager.start_pending(data, source_id, self.mouse_x, self.mouse_y);
            }

            // Text selection: テキスト上の mouse_down は常に selection 起点
            // として扱う。 sabitori の `clickable` flag は id 付き要素を全て
            // true にする仕様 (= sabitori-markdown の heading / image / scroll
            // container も clickable 扱い) なので、 clickable を gate に
            // すると本文上ですら selection できない。 on_click は別途
            // 上のループで既に発火済みなので click と selection が両立する。
            // 「ボタンを click」 = drag 無し → mouse_up で anchor==head 解除
            // 「テキストを drag」 = drag → selection 確定。
            if let Some((link_id, _)) = self.link_at(self.mouse_x, self.mouse_y) {
                // 本文中リンク click → その id を on_click 発火し遷移。
                // selection の起点にはしない（drag より優先）。
                self.app.on_click(&link_id);
                self.selection = None;
                self.selecting = false;
            } else if had_pending_drag || !self.app.text_selection_enabled() {
                // 明示的に draggable な要素は drag system 優先で selection 解除。
                // app が selection 自体を切っている場合も同じ扱い (#67)。
                self.selection = None;
                self.selecting = false;
            } else if let Some(hit) = self.hit_test_text(self.mouse_x, self.mouse_y, true) {
                let snap = self
                    .text_layouts
                    .iter()
                    .find(|l| l.text_idx == hit.0)
                    .map(|l| l.content.clone())
                    .unwrap_or_default();
                self.selection = Some(TextSelection {
                    anchor: hit,
                    head: hit,
                    anchor_content: snap.clone(),
                    head_content: snap,
                });
                self.selecting = true;
            } else {
                // 空白領域 click → 既存 selection を取り消す。
                self.selection = None;
                self.selecting = false;
            }
            // hit_clickable_or_drag は warning 抑止のため _ で受ける。
            let _ = hit_clickable_or_drag;
        }
        // Focus / pending-drag transitions feed the capture snapshot.
        self.push_ui_capture();
    
    }

    /// 主ボタン解放の処理本体。 [`Self::press_primary`] と同じ理由で切り出してある。
    pub(crate) fn release_primary(&mut self) {
        self.app.on_input(&InputEvent::PointerReleased {
            id: MOUSE_POINTER_ID,
            kind: PointerKind::Mouse,
            position: sabitori_core::Point::new(self.mouse_x, self.mouse_y),
            button: Some(InputMouseButton::Left),
            modifiers: self.modifiers,
        });
        self.pressed_id = None;
        // text selection drag 終了。 selection 自体は維持して Cmd+C を待つ。
        self.selecting = false;
        // 1 文字も範囲が無いなら (= 単なる click) 視覚 noise になるので消す。
        if let Some(ref sel) = self.selection {
            if sel.is_empty() {
                self.selection = None;
            }
        }
        // Complete drag if active
        if let Some((data, _source_id)) = self.drag_manager.on_release() {
            // Find drop zone under cursor
            if let Some(ref build) = self.last_build {
                let pt = sabitori_core::Point::new(self.mouse_x, self.mouse_y);
                if let Some(target_id) = build.hit_regions.iter()
                    .find(|r| r.drop_zone && r.rect.contains(pt))
                    .and_then(|r| r.id.as_ref())
                {
                    self.app.on_drop(&data, target_id);
                }
            }
        }
        self.app.on_pointer_up();
        // Drag end may flip `wants_pointer` back off.
        self.push_ui_capture();
    
    }

    pub(crate) fn handle_key_input(&mut self, key: Key, pressed: bool, chars: Vec<char>) {
        // 配る順が要点: **アプリが先、既定動作があと**。
        //
        // 以前は既定動作 (コピー・選択解除・フォーカス移動) を先に実行してから
        // イベントを配っていて、 `on_input` の戻り値もどこでも読んでいなかった。
        // doc は "Return true if handled" と言っているのに、 `true` を返しても
        // 既定動作はそのまま走る — 契約が嘘になっていた (issue #18)。
        // アプリが独自キーバインドを持てるよう、 消費されたら既定動作を止める。
        let key_event = InputEvent::KeyInput {
            key,
            pressed,
            modifiers: self.modifiers,
        };

        // フォーカス中の要素 → アプリ の順。 Tab / Escape は「フォーカス操作
        // そのもの」なので、 フィールドには渡さない (テキスト欄が Tab を食べて
        // しまうと移動できなくなる)。
        let handled_by_focus = if key != Key::Tab && key != Key::Escape {
            // 登録済みの欄が先。 id の借用と `&mut self` が重なるので clone で外す。
            self.route_to_managed(&key_event)
                || match self.focused_id.clone() {
                    Some(id) => self.app.on_focused_input(&id, &key_event),
                    None => false,
                }
        } else { false };
        // 押下・解放の**両方**を配る。押下だけを配っていた頃は、⇧の押下は届くのに
        // 解放が来ないので、アプリ側で「押しっぱなし」を保持すると二度と落ちなかった。
        let handled = handled_by_focus || self.app.on_input(&key_event);

        // 既定動作は押下のみ、 かつ誰も消費しなかったときだけ。 解放でも走らせると
        // ⇧を離しただけで選択が消える、といった挙動になる。
        if pressed && !handled {
            // Cmd+C (macOS) / Ctrl+C (other): 選択テキストをクリップボードへ。
            // 0.4.0 より前は macOS 専用 (pbcopy サブプロセス) で、 他は
            // `let _ = text;` と捨てていた (issue #20)。
            let is_copy = crate::clipboard::is_copy_shortcut(key, self.modifiers);
            if is_copy {
                if let Some(text) = self.selected_text() {
                    crate::clipboard::write_text(&text);
                }
            }
            // Cmd+V (macOS) / Ctrl+V (other): クリップボードを読んで 1 イベントで配る。
            //
            // 0.4.0 より前は**ペーストの実装がどこにも無かった**。 widgets 側に
            // 受け口のコメントだけがあり、 実際には何も届かなかった (issue #20)。
            // `CharInput` の連打にしないのは、 消費側が undo の単位や IME の状態と
            // 噛み合わせられなくなるため。
            if crate::clipboard::is_paste_shortcut(key, self.modifiers) {
                if let Some(text) = crate::clipboard::read_text() {
                    let ev = InputEvent::Paste { text };
                    // 登録済みの欄が先。 他のイベントと同じ順序。
                    if !self.route_to_managed(&ev) {
                        crate::runtime_shared::dispatch(
                            &mut self.app,
                            self.focused_id.as_deref(),
                            &ev,
                        );
                    }
                }
            }
            // Any key other than the copy shortcut dismisses the
            // selection — typing, pasting (Cmd+V), Enter, navigation all
            // end it, like a terminal/editor. Without this the highlight
            // persists after a paste and re-paints over the new text (it
            // looks like you're stuck in "selection mode"). Bare modifier
            // presses map to `Key::Other`, which must NOT clear: holding
            // Cmd to then press C would otherwise wipe the selection
            // before the copy above can read it.
            if !is_copy && key != Key::Other {
                self.selection = None;
            }
            // Escape clears focus
            if key == Key::Escape {
                self.focused_id = None;
            }
            // Tab / Shift+Tab moves focus between focusable elements
            if key == Key::Tab {
                if let Some(ref build) = self.last_build {
                    let focusable_ids: Vec<String> = build.hit_regions.iter()
                        .rev() // hit_regions are front-to-back; reverse for document order
                        .filter(|r| r.focusable && r.id.is_some())
                        .map(|r| r.id.clone().unwrap())
                        .collect();
                    if !focusable_ids.is_empty() {
                        let current_idx = self.focused_id.as_ref()
                            .and_then(|id| focusable_ids.iter().position(|f| f == id));
                        let next = if self.modifiers.shift {
                            // Shift+Tab: go backwards
                            match current_idx {
                                Some(0) | None => focusable_ids.len() - 1,
                                Some(i) => i - 1,
                            }
                        } else {
                            // Tab: go forwards
                            match current_idx {
                                Some(i) if i + 1 < focusable_ids.len() => i + 1,
                                _ => 0,
                            }
                        };
                        self.focused_id = Some(focusable_ids[next].clone());
                    }
                }
            }
            // Escape / Tab がフォーカスを動かしたら capture を撮り直す。
            // 配信より後になったので、 アプリはこの frame の `on_input` では
            // 移動前の状態を見る。 移動後は直後の `on_ui_capture` で届く。
            if key == Key::Escape || key == Key::Tab {
                self.push_ui_capture();
            }
        }
        if pressed {
            // テキスト入力として送るべき文字の判定（制御文字の除去、Cmd 押下時の
            // 抑制、Alt を通すか等）は keymap::char_inputs に集約している。
            // ここはルーティングだけ。
            for ch in chars {
                let char_event = InputEvent::CharInput(ch);
                // 登録済みの欄が先。 アプリが二重に処理する余地を作らない。
                let handled = self.route_to_managed(&char_event)
                    || match self.focused_id {
                        Some(ref id) => self.app.on_focused_input(id, &char_event),
                        None => false,
                    };
                if !handled {
                    let handled_by_app = self.app.on_input(&char_event);
                    if !handled_by_app {
                        self.warn_if_typing_went_nowhere();
                    }
                }
            }
        }
    }
    /// `self.text_layouts` を走査して mouse 座標に最も近い (text_idx, byte_offset)
    /// を返す。 hit_regions の hit 判定で取れなかった「テキスト本文上のクリック」
    /// で selection を始めるのに使う。 戻り値 None = どの text 要素にも近くない
    /// (= 領域外)。
    ///
    /// `strict` は「近い」の許容量を決める。
    ///
    /// - `true` (mouse_down): 文字か、その行ボックス＋わずかな許容に**実際に当たった
    ///   時だけ** Some。 これが無いと、 文字の無いキャンバスを押しただけで画面の
    ///   どこかにある label に anchor が立ち、 そのままドラッグすると anchor〜head
    ///   の間の text が全部選択されて画面が青く染まる (#68)。 選択の**開始**は
    ///   厳密でなければならない。
    /// - `false` (drag 中の head 更新): 従来どおり最近傍に snap する。 anchor が既に
    ///   実テキスト上に立っている以上、 そこから外へ払っても選択が伸び続けるのが
    ///   ブラウザを含む一般的な挙動で、 段落の下や左右の余白を通るドラッグで選択が
    ///   途切れないためにも要る。
    fn hit_test_text(&self, x: f32, y: f32, strict: bool) -> Option<(usize, usize)> {
        Self::hit_test_text_in(&self.text_layouts, x, y, strict)
    }

    /// `hit_test_text` の本体。 `self` を取らないので単体テストから叩ける。
    fn hit_test_text_in(
        text_layouts: &[crate::bridge::TextHitLayout],
        x: f32,
        y: f32,
        strict: bool,
    ) -> Option<(usize, usize)> {
        // 行ボックスからの許容距離 (行高に対する倍率)。 縦は行間の中点まで、
        // 横は 1 行高ぶん (≒ 1em 強) — 行末の余白 click を caret 行末に snap
        // させるのに要る幅で、 それ以上離れたら「当たっていない」。
        const SLACK_V: f32 = 0.5;
        const SLACK_H: f32 = 1.0;

        let mut best: Option<(f32, usize, usize)> = None;
        for layout in text_layouts {
            // `.no_select()` / button label は selection の対象外 (#67)。
            if layout.hits.is_empty() || layout.no_select {
                continue;
            }
            // clip_rect 外は無視。 scroll 領域外の text に selection が漏れない。
            if let Some(c) = layout.clip_rect {
                if x < c.origin.x
                    || x > c.origin.x + c.size.width
                    || y < c.origin.y
                    || y > c.origin.y + c.size.height
                {
                    continue;
                }
            }

            // pass 1: 最も y が近い行 index を選ぶ。
            let mut min_line_dy: f32 = f32::MAX;
            let mut chosen_line: Option<u32> = None;
            for hit in &layout.hits {
                let top = hit.y;
                let bot = hit.y + hit.h;
                let dy = if y < top { top - y } else if y > bot { y - bot } else { 0.0 };
                if dy < min_line_dy {
                    min_line_dy = dy;
                    chosen_line = Some(hit.line_index);
                }
            }
            let Some(line) = chosen_line else { continue };

            // pass 2: その行で x が最も近い glyph を選ぶ。 left half hit → byte_start、
            // right half hit → byte_end (= 次文字の手前)。 行末の余白に落ちたら
            // 最後の glyph の byte_end (= 行末) に snap。
            // 併せて行の実寸 (高さ・左右端) を集める — strict の足切りに使う。
            let mut best_in_line: Option<(f32, &sabitori_text::GlyphHit, bool)> = None;
            let mut line_h: f32 = 0.0;
            let mut line_x0: f32 = f32::MAX;
            let mut line_x1: f32 = f32::MIN;
            for hit in layout.hits.iter().filter(|h| h.line_index == line) {
                line_h = line_h.max(hit.h);
                line_x0 = line_x0.min(hit.x);
                line_x1 = line_x1.max(hit.x + hit.w);
                let mid = hit.x + hit.w * 0.5;
                let dx = (mid - x).abs();
                let is_left = x < mid;
                if best_in_line.map_or(true, |(d, _, _)| dx < d) {
                    best_in_line = Some((dx, hit, is_left));
                }
            }
            let Some((dx, hit, is_left)) = best_in_line else { continue };

            // #68: 距離の足切り。 これが無いと全 text がグローバル最小スコアの
            // 競争に残り続け、 画面のどこを押しても必ずどれかの文字を掴む。
            if strict {
                let h = line_h.max(1.0);
                if min_line_dy > h * SLACK_V {
                    continue;
                }
                let slack = h * SLACK_H;
                if x < line_x0 - slack || x > line_x1 + slack {
                    continue;
                }
            }

            let byte = if is_left { hit.byte_start } else { hit.byte_end };
            let score = min_line_dy + dx;
            if best.map_or(true, |(s, _, _)| score < s) {
                best = Some((score, layout.text_idx, byte));
            }
        }
        best.map(|(_, idx, byte)| (idx, byte))
    }

    /// Selection 範囲を plain text として返す。 (text_idx, byte) の lexicographic
    /// 順に anchor〜head を解釈、 跨いだ text 間は改行で join。
    fn selected_text(&self) -> Option<String> {
        let sel = self.selection.as_ref()?;
        if sel.is_empty() {
            return None;
        }
        let (start, end) = sel.range_normalized();
        // Join selected pieces by *visual geometry*, not by element index. A
        // text element is one TextDraw, and a layout may emit one element per
        // glyph (e.g. a terminal grid draws an absolutely-positioned element
        // per cell). Blindly joining elements with "\n" would then stack every
        // character on its own line ("縦書き"). Instead:
        //   - newline only when the next piece sits on a lower visual row
        //     (its glyph `y` increased);
        //   - within a row, fill the horizontal gap between pieces with spaces
        //     inferred from glyph x-positions — blank cells are often not
        //     emitted as elements at all, so the space must be reconstructed.
        let mut out = String::new();
        // (row y, x of right edge of last piece, advance estimate) of the
        // previously appended piece — for line/space inference.
        let mut prev: Option<(f32, f32, f32)> = None;
        for layout in &self.text_layouts {
            let idx = layout.text_idx;
            // 塗らないものは copy もしない — 見た目と clipboard を一致させる (#67)。
            if layout.no_select || idx < start.0 || idx > end.0 {
                continue;
            }
            let (b0, b1) = if idx == start.0 && idx == end.0 {
                (start.1, end.1)
            } else if idx == start.0 {
                (start.1, layout.content.len())
            } else if idx == end.0 {
                (0, end.1)
            } else {
                (0, layout.content.len())
            };
            // safety: char boundary 修正 (cosmic-text の byte 位置は概ね合うが念のため)。
            let mut a = b0.min(layout.content.len());
            let mut b = b1.min(layout.content.len());
            while a > 0 && !layout.content.is_char_boundary(a) {
                a -= 1;
            }
            while b > 0 && b <= layout.content.len() && !layout.content.is_char_boundary(b) {
                b -= 1;
            }
            if a >= b {
                continue;
            }
            let piece = &layout.content[a..b];

            // Geometry of just the glyphs inside the selected byte range.
            let sel_hits = layout
                .hits
                .iter()
                .filter(|h| h.byte_start >= a && h.byte_start < b);
            let mut y = f32::INFINITY;
            let mut x0 = f32::INFINITY;
            let mut x1 = f32::NEG_INFINITY;
            let mut adv = 0.0_f32;
            let mut any = false;
            for h in sel_hits {
                any = true;
                y = y.min(h.y);
                x0 = x0.min(h.x);
                x1 = x1.max(h.x + h.w);
                adv = adv.max(h.w);
            }

            if any {
                if let Some((py, px_end, padv)) = prev {
                    if (y - py).abs() > 1.0 {
                        out.push('\n'); // dropped to a new visual row
                    } else {
                        // Same row: turn an x-gap into the right number of
                        // spaces (unit = the narrower cell, robust to wide chars).
                        let unit = padv.min(adv).max(1.0);
                        let gap = ((x0 - px_end) / unit).round() as i32;
                        for _ in 0..gap.max(0) {
                            out.push(' ');
                        }
                    }
                }
                prev = Some((y, x1, adv));
            }
            out.push_str(piece);
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    /// View 切替 (list → article 等) で selection が指してる text 要素の content
    /// が別物に変わっていたら selection を clear する。 text_idx が新しい text
    /// 要素を指してしまうと、 関係ないテキストの一部が highlight されるバグの
    /// 原因になる。 anchor / head の snapshot と現在の text_layouts を照合。
    fn invalidate_stale_selection(
        selection: &mut Option<TextSelection>,
        selecting: &mut bool,
        text_layouts: &[crate::bridge::TextHitLayout],
    ) {
        let Some(sel) = selection.as_ref() else { return };
        let lookup = |idx: usize| -> Option<&str> {
            text_layouts.iter().find(|l| l.text_idx == idx).map(|l| l.content.as_str())
        };
        let anchor_ok = lookup(sel.anchor.0).map_or(false, |c| c == sel.anchor_content);
        let head_ok = lookup(sel.head.0).map_or(false, |c| c == sel.head_content);
        if !anchor_ok || !head_ok {
            *selection = None;
            *selecting = false;
        }
    }

    /// Recolor glyph instances that sit inside any selection rect to `fg`, so
    /// selected text stays readable over the highlight. Geometric test (a glyph's
    /// center inside a rect) rather than index-matching, because glyph instances
    /// and hit boxes aren't 1:1 — a blank glyph (space) yields a hit but no
    /// instance, so `glyphs[i]` doesn't line up with `hits[i]`.
    fn recolor_selected_glyphs(
        glyphs: &mut [sabitori_text::GlyphInstance],
        sel_rects: &[sabitori_gpu::RectInstance],
        fg: [f32; 4],
    ) {
        if sel_rects.is_empty() {
            return;
        }
        for g in glyphs.iter_mut() {
            let cx = g.position[0] + g.size[0] * 0.5;
            let cy = g.position[1] + g.size[1] * 0.5;
            let inside = sel_rects.iter().any(|r| {
                let [x, y, w, h] = r.rect;
                cx >= x && cx <= x + w && cy >= y && cy <= y + h
            });
            if inside {
                g.color = fg;
            }
        }
    }

    /// Selection ハイライト用の RectInstance を返す。 render path で renderer
    /// が mutable borrow されている都合上 self を取らない静的 helper にしてある。
    /// `bg` は選択背景色 (app の `selection_style()` 由来、 未指定なら system blue)。
    fn compute_selection_rects(
        selection: Option<&TextSelection>,
        text_layouts: &[crate::bridge::TextHitLayout],
        bg: sabitori_core::Color,
    ) -> Vec<sabitori_gpu::RectInstance> {
        let Some(sel) = selection else { return Vec::new() };
        if sel.is_empty() {
            return Vec::new();
        }
        let (start, end) = sel.range_normalized();
        let color = bg;
        let mut out: Vec<sabitori_gpu::RectInstance> = Vec::new();
        for layout in text_layouts {
            let idx = layout.text_idx;
            // anchor/head には決してならないが、 選択範囲に**挟まれる**ことはある。
            // 塗りからも外さないと、 選択できない label が選択済みに見える (#67)。
            if layout.no_select || idx < start.0 || idx > end.0 {
                continue;
            }
            let (b0, b1) = if idx == start.0 && idx == end.0 {
                (start.1, end.1)
            } else if idx == start.0 {
                (start.1, usize::MAX)
            } else if idx == end.0 {
                (0, end.1)
            } else {
                (0, usize::MAX)
            };
            // 同じ line_index の hit をまとめて 1 rect にする。
            use std::collections::BTreeMap;
            let mut by_line: BTreeMap<u32, (f32, f32, f32, f32)> = BTreeMap::new();
            for hit in &layout.hits {
                if hit.byte_end <= b0 || hit.byte_start >= b1 {
                    continue;
                }
                let entry = by_line
                    .entry(hit.line_index)
                    .or_insert((hit.x, hit.y, hit.x + hit.w, hit.y + hit.h));
                entry.0 = entry.0.min(hit.x);
                entry.1 = entry.1.min(hit.y);
                entry.2 = entry.2.max(hit.x + hit.w);
                entry.3 = entry.3.max(hit.y + hit.h);
            }
            for (_, (x0, y0, x1, y1)) in by_line {
                out.push(sabitori_gpu::RectInstance {
                    rect: [x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0)],
                    corner_radii: [0.0; 4],
                    fill_color: color.to_array(),
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    gradient_angle: 0.0,
                    rotation: 0.0,
                    _pad0: 0.0,
                    shadow_color: [0.0; 4],
                    shadow_offset: [0.0; 2],
                    shadow_params: [0.0; 2],
                    gradient_end_color: [0.0; 4],
                    clip_rect: layout
                        .clip_rect
                        .map(|c| [c.origin.x, c.origin.y, c.size.width, c.size.height])
                        .unwrap_or([0.0; 4]),
                });
            }
        }
        out
    }

    /// Find-in-page ハイライト rect を全 text layout から集める。 各 `TextHitLayout`
    /// が持つ `HighlightSpec` の byte 範囲を、 selection と同じ per-line union で背景
    /// rect に変換する。 `current` の範囲だけ `current_color` で塗る。 呼び出し側で
    /// base_rects の末尾に append され、 glyph の下・要素背景の上に挟まる。
    fn compute_highlight_rects(
        text_layouts: &[crate::bridge::TextHitLayout],
    ) -> Vec<sabitori_gpu::RectInstance> {
        let mut out: Vec<sabitori_gpu::RectInstance> = Vec::new();
        for layout in text_layouts {
            // Specs paint in order, so a later one covers an earlier one where
            // their ranges overlap — the app decides precedence by call order.
            for hl in &layout.highlight {
                for (ri, &(b0, b1)) in hl.ranges.iter().enumerate() {
                    if b1 <= b0 {
                        continue;
                    }
                    let color = if hl.current == Some(ri) { hl.current_color } else { hl.color };
                    // 同じ line_index の hit をまとめて 1 rect にする (selection と同じ)。
                    use std::collections::BTreeMap;
                    let mut by_line: BTreeMap<u32, (f32, f32, f32, f32)> = BTreeMap::new();
                    for hit in &layout.hits {
                        if hit.byte_end <= b0 || hit.byte_start >= b1 {
                            continue;
                        }
                        let entry = by_line
                            .entry(hit.line_index)
                            .or_insert((hit.x, hit.y, hit.x + hit.w, hit.y + hit.h));
                        entry.0 = entry.0.min(hit.x);
                        entry.1 = entry.1.min(hit.y);
                        entry.2 = entry.2.max(hit.x + hit.w);
                        entry.3 = entry.3.max(hit.y + hit.h);
                    }
                    for (_, (x0, y0, x1, y1)) in by_line {
                        out.push(sabitori_gpu::RectInstance {
                            rect: [x0, y0, (x1 - x0).max(1.0), (y1 - y0).max(1.0)],
                            corner_radii: [2.0; 4],
                            fill_color: color.to_array(),
                            border_color: [0.0; 4],
                            border_width: 0.0,
                            gradient_angle: 0.0,
                            rotation: 0.0,
                            _pad0: 0.0,
                            shadow_color: [0.0; 4],
                            shadow_offset: [0.0; 2],
                            shadow_params: [0.0; 2],
                            gradient_end_color: [0.0; 4],
                            clip_rect: layout
                                .clip_rect
                                .map(|c| [c.origin.x, c.origin.y, c.size.width, c.size.height])
                                .unwrap_or([0.0; 4]),
                        });
                    }
                }
            }
        }
        out
    }

    /// in-body リンク範囲の下線 rect (compute_highlight_rects の下線版)。各範囲を
    /// line ごとにまとめ、行の下端に細い下線を link color で敷く。glyph の下に描く。
    fn compute_link_rects(
        text_layouts: &[crate::bridge::TextHitLayout],
    ) -> Vec<sabitori_gpu::RectInstance> {
        let mut out: Vec<sabitori_gpu::RectInstance> = Vec::new();
        for layout in text_layouts {
            let Some(ranges) = layout.link_ranges.as_ref() else { continue };
            for r in ranges {
                if r.end <= r.start {
                    continue;
                }
                use std::collections::BTreeMap;
                // line_index -> (x0, x1, bottom_y)
                let mut by_line: BTreeMap<u32, (f32, f32, f32)> = BTreeMap::new();
                for hit in &layout.hits {
                    if hit.byte_end <= r.start || hit.byte_start >= r.end {
                        continue;
                    }
                    let entry = by_line
                        .entry(hit.line_index)
                        .or_insert((hit.x, hit.x + hit.w, hit.y + hit.h));
                    entry.0 = entry.0.min(hit.x);
                    entry.1 = entry.1.max(hit.x + hit.w);
                    entry.2 = entry.2.max(hit.y + hit.h);
                }
                for (_, (x0, x1, bottom)) in by_line {
                    out.push(sabitori_gpu::RectInstance {
                        rect: [x0, bottom - 1.5, (x1 - x0).max(1.0), 1.5],
                        corner_radii: [0.0; 4],
                        fill_color: r.color.to_array(),
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        gradient_angle: 0.0,
                        rotation: 0.0,
                        _pad0: 0.0,
                        shadow_color: [0.0; 4],
                        shadow_offset: [0.0; 2],
                        shadow_params: [0.0; 2],
                        gradient_end_color: [0.0; 4],
                        clip_rect: layout
                            .clip_rect
                            .map(|c| [c.origin.x, c.origin.y, c.size.width, c.size.height])
                            .unwrap_or([0.0; 4]),
                    });
                }
            }
        }
        out
    }

    /// カーソル (x,y) が in-body リンク範囲上なら `(id, tooltip)` を返す。glyph の
    /// 実 hitbox への厳密内包判定 (hit_test_text の snap と違い、範囲外の近接では None)。
    fn link_at(&self, x: f32, y: f32) -> Option<(String, Option<String>)> {
        for layout in &self.text_layouts {
            let Some(ranges) = layout.link_ranges.as_ref() else { continue };
            if let Some(c) = layout.clip_rect {
                if x < c.origin.x
                    || x > c.origin.x + c.size.width
                    || y < c.origin.y
                    || y > c.origin.y + c.size.height
                {
                    continue;
                }
            }
            for hit in &layout.hits {
                if x >= hit.x && x < hit.x + hit.w && y >= hit.y && y < hit.y + hit.h {
                    for r in ranges {
                        if hit.byte_start >= r.start && hit.byte_end <= r.end {
                            return Some((r.id.clone(), r.tooltip.clone()));
                        }
                    }
                }
            }
        }
        None
    }

    /// 修飾キーの状態を更新し、**変化後**の値をアプリへ配る。
    ///
    /// 状態を先に書き換えてから配るのが要点。`KeyInput` に載る `self.modifiers` は
    /// 修飾キー自身のイベントでは変化前を指す — macOS の winit は `flagsChanged:` で
    /// `KeyboardInput` を先に、`ModifiersChanged` を後に積むためで、⇧の押下イベントは
    /// `shift: false` を、解放イベントは `shift: true` を載せて届く。
    /// [`InputEvent::ModifiersChanged`] だけが変化そのものを正しく伝える。
    pub(crate) fn set_modifiers(&mut self, modifiers: Modifiers) {
        self.modifiers = modifiers;
        self.app.on_input(&InputEvent::ModifiersChanged(modifiers));
    }

    /// 押下対象の解決。 実体は [`crate::runtime_shared::hit_id_at`]。
    fn hit_id_at(&self, x: f32, y: f32) -> Option<String> {
        let build = self.last_build.as_ref()?;
        crate::runtime_shared::hit_id_at(build, x, y)
    }

    /// ポインタ直下のホバー / tooltip / cursor を引き直し、変化をアプリへ通知する。
    /// 解決そのものは [`crate::runtime_shared::resolve_hover`]（scene ランタイムと
    /// 共通）で、ここが足すのは本文中リンクの上書きだけ。
    pub(crate) fn update_hover(&mut self) {
        let mut hit = self
            .last_build
            .as_ref()
            .map(|b| crate::runtime_shared::resolve_hover(b, self.mouse_x, self.mouse_y))
            .unwrap_or_default();
        // 本文中リンク上なら tooltip=リンクの preview、cursor=pointer に上書き
        // （hit_regions は本文段落を hoverable にしないので link_at で別途拾う）。
        // ここだけが scene ランタイムとの差 — あちらにテキスト選択層は無い。
        if let Some((link_id, tip)) = self.link_at(self.mouse_x, self.mouse_y) {
            hit.hovered_id = Some(link_id);
            hit.tooltip = tip.or(hit.tooltip);
            hit.cursor = Some(sabitori_core::Cursor::Pointer);
        }

        // Feed tooltip state
        self.tooltip_state.on_hover_change(
            hit.hovered_id.as_deref(),
            hit.tooltip.as_deref(),
            self.mouse_x,
            self.mouse_y,
        );
        if self.hovered_id != hit.hovered_id {
            self.app.on_hover_change(hit.hovered_id.as_deref());
        }
        self.hovered_id = hit.hovered_id;
        self.apply_cursor(hit.cursor);
        self.push_ui_capture();
    }

    /// Recompute the [`UiCapture`] snapshot and push it to the app when it
    /// changed. Called after hover / focus / drag state transitions so a
    /// scene-hosting app always has a current view of "does the UI want
    /// this input?" before the next raw event arrives.
    fn push_ui_capture(&mut self) {
        crate::runtime_shared::push_ui_capture(
            self.last_build.as_ref(),
            self.mouse_x,
            self.mouse_y,
            self.drag_manager.is_active(),
            self.focused_id.is_some(),
            &mut self.last_capture,
            &mut self.app,
        );
    }

    /// 解決した cursor を OS へ送る。 dedup も winit へのマッピングも
    /// [`crate::runtime_shared::apply_cursor`] に集約 — マッピング表が 2 つあると、
    /// `Cursor` に variant を足した時に片方だけ忘れられる。
    fn apply_cursor(&mut self, cursor: Option<sabitori_core::Cursor>) {
        crate::runtime_shared::apply_cursor(self.window.as_ref(), &mut self.last_cursor, cursor);
    }

    /// Dispatch winit events for an extra window. v1 only acts on the
    /// three events that affect render correctness — everything else
    /// (mouse, key, focus, IME, etc.) is silently dropped because the
    /// extra is expected to be click-through. Phase 2 will widen this
    /// when extras need real input.
    fn handle_extra_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(extra) = self.extras.get_mut(&id) {
                    let scale = extra.window.scale_factor();
                    extra.renderer.resize(size.width, size.height, scale);
                    #[cfg(not(target_arch = "wasm32"))]
                    if extra.scene_3d {
                        let ctx = extra.renderer.gpu_context();
                        let key = extra.key.clone();
                        self.app.on_resize_extra_scene(&key, &ctx);
                    }
                    if let Some(extra) = self.extras.get(&id) {
                        extra.window.request_redraw();
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(extra) = self.extras.get_mut(&id) {
                    let size = extra.window.inner_size();
                    let scale = extra.window.scale_factor();
                    extra.renderer.resize(size.width, size.height, scale);
                    #[cfg(not(target_arch = "wasm32"))]
                    if extra.scene_3d {
                        let ctx = extra.renderer.gpu_context();
                        let key = extra.key.clone();
                        self.app.on_resize_extra_scene(&key, &ctx);
                    }
                    if let Some(extra) = self.extras.get(&id) {
                        extra.window.request_redraw();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.redraw_extra(id);
            }
            _ => {}
        }
    }

    /// Build + draw a single extra window. Mirrors the non-overlay
    /// branch of the primary's RedrawRequested handler with a stripped
    /// `ViewContext` (no hover / focus / scroll / drag — extras are
    /// passive in v1).
    fn redraw_extra(&mut self, id: WindowId) {
        // We need both `&mut extra` and `&self.app` simultaneously.
        // Pull the extra out by detaching pieces we own (renderers
        // implement no Drop side-effects we care about) so the borrow
        // checker stays happy.
        let Some(extra) = self.extras.get_mut(&id) else { return; };

        let scale = extra.renderer.scale_factor;
        let w = ((extra.renderer.surface_config.width as f32 / scale) * 0.5).floor() * 2.0;
        let h = ((extra.renderer.surface_config.height as f32 / scale) * 0.5).floor() * 2.0;

        // Drain image fetch results so the extra's view sees ready
        // images this frame, identical to the primary path.
        crate::image_runtime::drain_pending(
            &self.image_cache,
            &self.image_pending,
        );

        let mono_advance = extra
            .text_renderer
            .measure_text("0000000000", 100.0, false, true, None, None, None, sabitori_core::Typography::default())
            .size
            .width
            / 1000.0;
        // 計測器を先に作り、 `ViewContext` に差してから view を呼ぶ。 以前は
        // view のあとで作っていたが、 それだとアプリが実フォント計測に触れない
        // (issue #15)。 `extra.text_renderer` の可変借用を後段の
        // `UiDrawLists::extract` に返すため、 ブロックで閉じる。
        let (root, build_result) = {
            let measurer = crate::bridge::TextRendererMeasurer::new(
                &mut extra.text_renderer,
                &extra.measure_cache,
            );
            let ctx = ViewContext {
                width: w,
                height: h,
                hovered: None,
                focused: None,
                mouse_x: 0.0,
                mouse_y: 0.0,
                shift_held: false,
                cmd_held: false,
                scroll_states: std::collections::HashMap::new(),
                tooltip: None,
                drag: None,
                theme: self.app.theme(),
                presence: std::collections::HashMap::new(),
                images: Some(self.image_ctx.clone()),
                mono_advance,
                measurer: Some(&measurer),
                managed: Default::default(),
            actions: Default::default(),
            };
            let root = self.app.view_for(&extra.key, &ctx);
            let built = build_tree_measured(&root, w, h, &measurer);
            (root, built)
        };
        let _ = &root;

        let (rects, lists) =
            UiDrawLists::extract(&build_result.render_list, &mut extra.text_renderer);
        extra.last_build = Some(build_result);

        let device = extra.renderer.device.clone();
        let queue = extra.renderer.queue.clone();

        // Split self so the scene closure can access `self.app`
        // mutably while `extra.renderer` is also borrowed mutably —
        // both are disjoint fields once we destructure. This is the
        // same disentangling SceneAppState does for the primary
        // window's render_scene_then_ui call site.
        #[cfg(not(target_arch = "wasm32"))]
        let scene_3d = extra.scene_3d;
        #[cfg(not(target_arch = "wasm32"))]
        let key = extra.key.clone();
        let img_r = &mut extra.image_renderer;
        let ring_r = &mut extra.ring_renderer;
        let line_r = &mut extra.line_renderer;
        let tr = &mut extra.text_renderer;
        let renderer = &mut extra.renderer;
        #[cfg(not(target_arch = "wasm32"))]
        let app = &mut self.app;

        // The 2D UI overlay closure body is duplicated between the
        // 3D and 2D branches because higher-rank lifetimes on
        // wgpu::RenderPass<'_> make the inferred closure type
        // monomorphic when factored out — the borrow checker rejects
        // it as "not general enough" for the FnOnce signature both
        // render_with and render_scene_then_ui expect.
        // 3D scene rendering is native-only (wgpu scene pipeline); on
        // wasm `scene_3d` extras can't be set up, so we always fall
        // through to the 2D path below.
        #[cfg(not(target_arch = "wasm32"))]
        if scene_3d {
            let _ = renderer.render_scene_then_ui(
                |scene_ctx| {
                    app.render_extra_scene(&key, scene_ctx);
                },
                &rects,
                |pass, globals_bg| {
                    let mut r = UiRenderers {
                        images: Some(&mut *img_r),
                        rings: Some(&mut *ring_r),
                        lines: Some(&mut *line_r),
                        text: tr,
                    };
                    draw_ui_layer(&mut r, &lists, &device, &queue, pass, globals_bg);
                },
            );
            return;
        }
        let _ = renderer.render_with(&rects, |pass, globals_bg| {
            let mut r = UiRenderers {
                images: Some(&mut *img_r),
                rings: Some(&mut *ring_r),
                lines: Some(&mut *line_r),
                text: tr,
            };
            draw_ui_layer(&mut r, &lists, &device, &queue, pass, globals_bg);
        });
    }
}

/// Run a declarative app.
/// Run a declarative app (native).
#[cfg(not(target_arch = "wasm32"))]
pub fn run_declarative<A: DeclarativeApp + 'static>(app: A) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let event_loop = EventLoop::new().unwrap();
    let mut state = AppState::new(app);
    event_loop.run_app(&mut state).unwrap();
}

/// Run a declarative app (WASM).
#[cfg(target_arch = "wasm32")]
pub fn run_declarative<A: DeclarativeApp + 'static>(app: A) {
    use std::cell::RefCell;
    use std::rc::Rc;

    console_error_panic_hook::set_once();
    // ホスト側が既に logger を張っていることがある。`init_with_level` はその場合 Err を
    // 返すので、`expect` せず譲る (native 側の `try_init` と同じ方針)。
    let _ = console_log::init_with_level(log::Level::Info);

    let state = Rc::new(RefCell::new(AppState::new(app)));

    struct WasmHandler<A: DeclarativeApp> {
        inner: Rc<RefCell<AppState<A>>>,
        renderer_init_started: bool,
    }

    impl<A: DeclarativeApp + 'static> ApplicationHandler for WasmHandler<A> {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            {
                let s = self.inner.borrow();
                if s.window.is_some() { return; }
            }

            let (w, h) = self.inner.borrow().app.size();
            let title = self.inner.borrow().app.title().to_string();
            let attrs = WindowAttributes::default()
                .with_title(title)
                .with_inner_size(winit::dpi::LogicalSize::new(w, h));
            let window = Arc::new(event_loop.create_window(attrs).unwrap());

            // Attach canvas to DOM
            {
                use winit::platform::web::WindowExtWebSys;
                let canvas = window.canvas().expect("Failed to get canvas");
                canvas.set_id("sabitori-canvas");
                canvas.style().set_css_text("width: 100%; height: 100%; display: block;");
                let doc = web_sys::window().unwrap().document().unwrap();
                doc.body().unwrap().append_child(&canvas).unwrap();
            }

            {
                let mut s = self.inner.borrow_mut();
                s.app.set_window(window.clone());
                s.window = Some(window.clone());
            }

            if !self.renderer_init_started {
                self.renderer_init_started = true;
                let inner = Rc::clone(&self.inner);
                wasm_bindgen_futures::spawn_local(async move {
                    let mut gpu = GpuRenderer::new_async(window.clone()).await;
                    // Fix initial 1x1 canvas size
                    let size = window.inner_size();
                    if size.width > 1 && size.height > 1 {
                        gpu.resize(size.width, size.height, window.scale_factor());
                    }
                    let mut s = inner.borrow_mut();
                    let mut text = TextRenderer::new(
                        &gpu.device,
                        gpu.surface_config.format,
                        &gpu.globals_bind_group_layout,
                    );
                    let user_fonts = s.app.fonts();
                    if !user_fonts.is_empty() {
                        text.prefer_user_fonts(&user_fonts);
                    }
                    let img = sabitori_gpu::ImageRenderer::new(&gpu.device, gpu.surface_config.format, &gpu.globals_bind_group_layout);
                    let rings = sabitori_gpu::RingRenderer::new(&gpu.device, gpu.surface_config.format, &gpu.globals_bind_group_layout);
                    s.renderer = Some(gpu);
                    s.text_renderer = Some(text);
                    s.image_renderer = Some(img);
                    s.ring_renderer = Some(rings);
                    log::info!("Declarative renderer ready");
                });
            }
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
            self.inner.borrow_mut().window_event(event_loop, id, event);
        }

        fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
            self.inner.borrow_mut().new_events(event_loop, cause);
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            self.inner.borrow_mut().about_to_wait(event_loop);
        }
    }

    let event_loop = EventLoop::new().unwrap();
    let mut handler = WasmHandler {
        inner: state,
        renderer_init_started: false,
    };
    event_loop.run_app(&mut handler).unwrap();
}

/// Headless frame tests.
///
/// [`AppState::build_frame`] is the whole per-frame pipeline minus the GPU, and
/// [`AppState::new`] builds a state with no window and no renderers attached —
/// together they let a frame run in a plain `#[test]`, with a stub measurer
/// standing in for the text renderer. That covers the runtime behaviour that
/// used to be reachable only by launching a window and looking at it:
/// layout of the app's own tree, managed scroll registration, scroll intents,
/// and the app callbacks driven off a build.
#[cfg(test)]
mod frame_tests {
    use super::*;
    use sabitori_core::build::TextMeasure;
    use sabitori_core::{Size, TextMetrics, Typography};

    /// Deterministic stand-in for the text renderer.
    ///
    /// Real shaping depends on the fonts installed on the machine, which would
    /// make every asserted rect platform-dependent. Here each glyph is exactly
    /// `0.5 × font_size` wide and a line is `font_size` tall, so expected
    /// geometry can be written out by hand. Wrapping is not modelled — no test
    /// below depends on it.
    struct StubMeasure;

    use sabitori_core::build::{CaretPos, TextShape};

    impl TextMeasure for StubMeasure {
        fn measure(
            &self,
            content: &str,
            font_size: f32,
            _bold: bool,
            _monospace: bool,
            _font_family: Option<&str>,
            _max_width: Option<f32>,
            _max_lines: Option<u32>,
            _typo: Typography,
        ) -> TextMetrics {
            TextMetrics {
                size: Size {
                    width: content.chars().count() as f32 * font_size * 0.5,
                    height: font_size,
                },
                baseline: font_size * 0.8,
            }
        }

        fn caret_pos(&self, content: &str, byte_offset: usize, shape: TextShape<'_>) -> CaretPos {
            sabitori_core::build::approx_caret::caret_pos(content, byte_offset, shape.font_size * 0.5, shape.font_size * 1.0)
        }

        fn offset_at(&self, content: &str, point: (f32, f32), shape: TextShape<'_>) -> usize {
            sabitori_core::build::approx_caret::offset_at(content, point, shape.font_size * 0.5, shape.font_size * 1.0)
        }

        fn range_rects(
            &self,
            content: &str,
            range: (usize, usize),
            shape: TextShape<'_>,
        ) -> Vec<sabitori_core::Rect> {
            sabitori_core::build::approx_caret::range_rects(content, range, shape.font_size * 0.5, shape.font_size * 1.0)
        }
    }

    /// Records what the runtime pushed to the app, so tests can assert on the
    /// callbacks rather than on pixels.
    #[derive(Default)]
    struct RecordingApp {
        /// The element ids present in each `on_build`, one entry per call.
        builds: Vec<Vec<String>>,
        /// Absolute rect of `id` as of the most recent `on_build`.
        rects: std::collections::HashMap<String, sabitori_core::Rect>,
        /// Builds what `view` returns this frame. A closure rather than an
        /// `Element` because `Element` is deliberately not `Clone` — the
        /// runtime calls `view` fresh every frame, and so does this.
        tree: Option<Box<dyn Fn() -> Element>>,
        /// Handed to the runtime by `scroll_intents`.
        intents: Vec<(String, f32)>,
        /// Handed to the runtime by `build_probes`.
        probes: Vec<String>,
        /// `probe_positions` as of the most recent `on_build`.
        probed: std::collections::HashMap<String, f32>,
        /// `on_input` で受けたキーイベントを (key, pressed, shift) で順に記録する。
        keys: Vec<(Key, bool, bool)>,
        /// `on_input` で受けた文字。
        chars: Vec<char>,
        /// `on_input` で受けた `ModifiersChanged` の値を順に記録する。
        modifier_changes: Vec<Modifiers>,
        /// `on_input` で受けた `PointerMoved` を (kind, x, shift) で記録する。
        moves: Vec<(PointerKind, f32, bool)>,
        /// `on_input` に届いた全イベントの種別。 `input_delivery` の宣言と
        /// 実挙動を突き合わせるのに使う。
        received: Vec<InputEventKind>,
        /// `view()` の中で `ctx.caret_x(..)` を呼んだ結果。 `view` は `&self` なので
        /// `Cell` 越しに書く。
        measured_caret: std::cell::Cell<f32>,
        /// `on_input` で常に `true` を返す (= 全イベントを消費する)。
        consume_all: bool,
        /// `view()` に渡された `ctx` が計測器を持っていたか。
        ///
        /// 値だけでは配線漏れを検出できない — StubMeasure (1 文字 = font_size*0.5) と
        /// `mono_advance` フォールバックは同じ答えを返すので、 `measurer: None` でも
        /// caret の値は一致してしまう。 有無そのものを見る必要がある。
        measurer_present: std::cell::Cell<bool>,
    }

    impl DeclarativeApp for RecordingApp {
        fn view(&self, _ctx: &ViewContext) -> Element {
            // 実フォント計測がアプリまで届いているかを毎フレーム記録する。
            self.measurer_present.set(_ctx.measurer.is_some());
            self.measured_caret
                .set(_ctx.caret_x("abcd", 2, 20.0, false));
            match &self.tree {
                Some(build) => build(),
                None => sabitori_core::div(),
            }
        }

        fn scroll_intents(&mut self) -> Vec<(String, f32)> {
            std::mem::take(&mut self.intents)
        }

        fn build_probes(&self) -> Vec<String> {
            self.probes.clone()
        }

        fn on_input(&mut self, event: &InputEvent) -> bool {
            // 種別だけを別に貯める。 `input_delivery` の宣言が実挙動と合っているかを
            // 突き合わせるのに使う (下の declared_delivery_matches_reality)。
            self.received.push(event.kind());
            match event {
                InputEvent::KeyInput { key, pressed, modifiers } => {
                    self.keys.push((*key, *pressed, modifiers.shift));
                }
                InputEvent::CharInput(ch) => self.chars.push(*ch),
                InputEvent::ModifiersChanged(m) => self.modifier_changes.push(*m),
                InputEvent::PointerMoved { kind, position, modifiers, .. } => {
                    self.moves.push((*kind, position.x, modifiers.shift));
                }
                _ => {}
            }
            self.consume_all
        }

        fn on_build(&mut self, build: &sabitori_core::build::BuildResult) {
            let mut ids = Vec::new();
            for region in &build.hit_regions {
                if let Some(id) = &region.id {
                    ids.push(id.clone());
                    self.rects.insert(id.clone(), region.rect);
                }
            }
            self.builds.push(ids);
            self.probed = build.probe_positions.clone();
        }
    }

    /// Run one frame end to end: build the trees, then commit the main one the
    /// way the render path does once it has drawn from it.
    fn run_frame(state: &mut AppState<RecordingApp>, w: f32, h: f32) {
        let frame = state.build_frame(w, h, &StubMeasure);
        state.commit_build(frame.build_result);
    }

    /// #57: a declarative app must be handed the build it was rendered from.
    /// Before the fix `on_build` existed but nothing ever called it, so
    /// `hit_regions` was unreachable and scroll-to-element could not be written.
    #[test]
    fn commit_build_hands_hit_regions_to_the_app() {
        let mut state = AppState::new(RecordingApp {
            tree: Some(Box::new(|| {
                sabitori_core::div()
                    .p_px(20.0)
                    .child(sabitori_core::text("hello").id("label"))
            })),
            ..Default::default()
        });

        run_frame(&mut state, 400.0, 300.0);

        assert_eq!(state.app.builds.len(), 1, "on_build should fire once per committed frame");
        assert_eq!(state.app.builds[0], vec!["label".to_string()]);

        // The rect is absolute, which is the whole point — an app derives a
        // scroll target from the difference between two of these.
        let rect = state.app.rects["label"];
        assert_eq!(rect.origin.x, 20.0, "padding should offset the label");
        assert_eq!(rect.origin.y, 20.0);
        // 5 chars × the 14px default font size × 0.5 (see StubMeasure).
        assert_eq!(rect.size.width, 35.0);
    }

    /// Each committed frame is pushed, so an app can track layout over time.
    #[test]
    fn every_frame_is_pushed() {
        let mut state = AppState::new(RecordingApp {
            tree: Some(Box::new(|| {
                sabitori_core::div().child(sabitori_core::text("x").id("a"))
            })),
            ..Default::default()
        });

        run_frame(&mut state, 400.0, 300.0);
        run_frame(&mut state, 400.0, 300.0);
        run_frame(&mut state, 400.0, 300.0);

        assert_eq!(state.app.builds.len(), 3);
        assert!(state.app.builds.iter().all(|ids| ids == &vec!["a".to_string()]));
    }

    /// `build_frame` also owns the managed-scroll bookkeeping: a container that
    /// appears in the tree gets registered with its measured extents, which is
    /// what makes `scroll_intents` able to target it at all.
    /// #57 の本丸。長い一覧の**画面外**の行でも、probe に申告すれば位置が返ること。
    /// `hit_regions` は可視要素しか持たない（clip 外はゼロ矩形で捨てられる）ので、
    /// 「400 行目を先頭に持ってくる」は probe 無しには書けない。
    #[test]
    fn probes_report_offscreen_positions_that_hit_regions_drop() {
        let rows = || -> Element {
            let rows: Vec<Element> = (0..50)
                .map(|i| {
                    sabitori_core::div()
                        .id(format!("row-{i}"))
                        .h(sabitori_core::Dimension::Px(40.0))
                        .child(sabitori_core::text(format!("row {i}")))
                })
                .collect();
            sabitori_core::div()
                .scroll("pane")
                .flex_col()
                .h(sabitori_core::Dimension::Px(200.0))
                .children(rows)
        };

        // probe 無し: 画面外の行はどこにも出てこない。
        let mut bare = AppState::new(RecordingApp { tree: Some(Box::new(rows)), ..Default::default() });
        run_frame(&mut bare, 400.0, 300.0);
        assert!(
            !bare.app.builds[0].iter().any(|id| id == "row-40"),
            "row-40 is 1600px down a 200px pane — it must not be in hit_regions",
        );
        assert!(bare.app.probed.is_empty(), "no probes requested → no positions reported");

        // probe あり: 同じ行の位置が返る。
        let mut probed = AppState::new(RecordingApp {
            tree: Some(Box::new(rows)),
            probes: vec!["row-40".to_string(), "pane".to_string()],
            ..Default::default()
        });
        run_frame(&mut probed, 400.0, 300.0);

        let pane_y = *probed.app.probed.get("pane").expect("pane position");
        let row_y = *probed.app.probed.get("row-40").expect("row-40 position despite being off-screen");
        // 40 行 × 40px 下。scroll は 0 なので content 座標 = 絶対 Y の差。
        assert!(
            (row_y - pane_y - 1600.0).abs() < 0.5,
            "row-40 should sit 1600px into the content, got {}",
            row_y - pane_y,
        );
        // hit_regions 側は probe を足しても変わらない（可視のものだけ）。
        assert!(!probed.app.builds[0].iter().any(|id| id == "row-40"));
    }

    #[test]
    fn scroll_containers_are_registered_with_measured_extents() {
        let mut state = AppState::new(RecordingApp {
            tree: Some(Box::new(|| {
                let rows: Vec<Element> = (0..50)
                    .map(|i| sabitori_core::text(format!("row {i}")).id(format!("row-{i}")))
                    .collect();
                sabitori_core::div()
                    .scroll("pane")
                    .flex_col()
                    .h(sabitori_core::Dimension::Px(200.0))
                    .children(rows)
            })),
            ..Default::default()
        });

        run_frame(&mut state, 400.0, 300.0);

        let pane = state.scroll_states.get("pane").expect("pane should be registered");
        assert_eq!(pane.viewport_height, 200.0);
        assert!(
            pane.content_height > 200.0,
            "50 rows should overflow a 200px pane, got {}",
            pane.content_height,
        );
    }

    /// A scroll intent returned by the app is applied after the content extent
    /// is known, so it can be clamped against the real scrollable range.
    #[test]
    fn scroll_intents_move_managed_state() {
        let mut state = AppState::new(RecordingApp {
            tree: Some(Box::new(|| {
                let rows: Vec<Element> = (0..50)
                    .map(|i| sabitori_core::text(format!("row {i}")).id(format!("row-{i}")))
                    .collect();
                sabitori_core::div()
                    .scroll("pane")
                    .flex_col()
                    .h(sabitori_core::Dimension::Px(200.0))
                    .children(rows)
            })),
            ..Default::default()
        });

        // First frame registers the pane and measures its content.
        run_frame(&mut state, 400.0, 300.0);
        assert_eq!(state.scroll_states["pane"].scroll_y.target(), 0.0);

        state.app.intents = vec![("pane".to_string(), 120.0)];
        run_frame(&mut state, 400.0, 300.0);

        assert_eq!(state.scroll_states["pane"].scroll_y.target(), 120.0);
    }

    /// An intent past the end is clamped to the scrollable range rather than
    /// scrolling the content off-screen.
    #[test]
    fn scroll_intents_are_clamped_to_the_content() {
        let mut state = AppState::new(RecordingApp {
            tree: Some(Box::new(|| {
                let rows: Vec<Element> = (0..10)
                    .map(|i| sabitori_core::text(format!("row {i}")).id(format!("row-{i}")))
                    .collect();
                sabitori_core::div()
                    .scroll("pane")
                    .flex_col()
                    .h(sabitori_core::Dimension::Px(200.0))
                    .children(rows)
            })),
            ..Default::default()
        });

        run_frame(&mut state, 400.0, 300.0);
        state.app.intents = vec![("pane".to_string(), 100_000.0)];
        run_frame(&mut state, 400.0, 300.0);

        let pane = &state.scroll_states["pane"];
        let max = (pane.content_height - pane.viewport_height).max(0.0);
        assert_eq!(pane.scroll_y.target(), max);
    }

    /// Overlay content (`overlay_view`, tooltip, drag ghost) is laid out as its
    /// own tree against the same viewport.
    #[test]
    fn overlay_tree_is_built_separately() {
        struct WithOverlay;
        impl DeclarativeApp for WithOverlay {
            fn view(&self, _ctx: &ViewContext) -> Element {
                sabitori_core::div().child(sabitori_core::text("base").id("base"))
            }
            fn overlay_view(&self, _ctx: &ViewContext) -> Option<Element> {
                Some(sabitori_core::div().child(sabitori_core::text("menu").id("menu")))
            }
        }

        let mut state = AppState::new(WithOverlay);
        let frame = state.build_frame(400.0, 300.0, &StubMeasure);

        let base_ids: Vec<_> = frame.build_result.hit_regions.iter()
            .filter_map(|r| r.id.clone()).collect();
        assert_eq!(base_ids, vec!["base".to_string()]);

        let overlay = frame.overlay_build.expect("overlay_view returned content");
        let overlay_ids: Vec<_> = overlay.hit_regions.iter()
            .filter_map(|r| r.id.clone()).collect();
        assert!(overlay_ids.contains(&"menu".to_string()), "got {overlay_ids:?}");
    }

    /// With nothing to overlay, no second tree is built.
    #[test]
    fn no_overlay_tree_when_nothing_overlays() {
        let mut state = AppState::new(RecordingApp {
            tree: Some(Box::new(|| {
                sabitori_core::div().child(sabitori_core::text("x").id("a"))
            })),
            ..Default::default()
        });
        let frame = state.build_frame(400.0, 300.0, &StubMeasure);
        assert!(frame.overlay_build.is_none());
    }

    // -----------------------------------------------------------------
    // 押下状態 (#3)
    // -----------------------------------------------------------------

    /// 押下対象の解決 → `active_style` の畳み込み、までの配線。runtime に押下を
    /// 追う状態が無かった頃は `.active()` がどこからも読まれず、押しても何も
    /// 起きなかった。
    #[test]
    fn pressing_an_element_folds_its_active_style() {
        let tree = || {
            sabitori_core::div().child(
                sabitori_core::div()
                    .id("btn")
                    .w(sabitori_core::Dimension::Px(100.0))
                    .h(sabitori_core::Dimension::Px(40.0))
                    .active(|s| s.scale(0.5)),
            )
        };
        let mut state = AppState::new(RecordingApp {
            tree: Some(Box::new(tree)),
            ..Default::default()
        });
        run_frame(&mut state, 400.0, 300.0);

        let idle = state.app.rects["btn"];
        assert_eq!(idle.size.width, 100.0, "押していないので素の寸法");

        // 押下対象がカーソル位置から引けること (mouse_down が使う経路)。
        assert_eq!(state.hit_id_at(10.0, 10.0).as_deref(), Some("btn"));
        assert_eq!(state.hit_id_at(300.0, 200.0), None, "外は掴まない");

        // 押下中として組み直すと active_style が乗る。
        state.pressed_id = Some("btn".to_string());
        run_frame(&mut state, 400.0, 300.0);

        let pressed = state.app.rects["btn"];
        assert_eq!(pressed.size.width, 50.0, "押下で縮む");
        assert_eq!(pressed.origin.x, 25.0, "中心を軸に縮むので原点は内側へ");
    }

    /// 他の要素を押している間は畳まれない。 押下 id の取り違えは「隣のボタンが
    /// 凹む」形で出る。
    #[test]
    fn pressing_another_element_leaves_this_one_alone() {
        let mut state = AppState::new(RecordingApp {
            tree: Some(Box::new(|| {
                sabitori_core::div().child(
                    sabitori_core::div()
                        .id("btn")
                        .w(sabitori_core::Dimension::Px(100.0))
                        .h(sabitori_core::Dimension::Px(40.0))
                        .active(|s| s.scale(0.5)),
                )
            })),
            ..Default::default()
        });
        state.pressed_id = Some("someone-else".to_string());
        run_frame(&mut state, 400.0, 300.0);

        assert_eq!(state.app.rects["btn"].size.width, 100.0);
    }

    // -----------------------------------------------------------------
    // 修飾キーの観測 (#12)
    // -----------------------------------------------------------------

    /// 本題の回帰: 修飾キーの変化が**変化後**の値でアプリに届くこと。
    ///
    /// `KeyInput` に載る値は修飾キー自身のイベントでは変化前を指す（winit が
    /// macOS で `KeyboardInput` を先に、`ModifiersChanged` を後に積むため）。
    /// この口だけが「⇧が今どうなったか」を正しく伝える。
    #[test]
    fn modifier_changes_are_delivered_with_the_new_value() {
        let mut state = AppState::new(RecordingApp::default());

        state.set_modifiers(Modifiers { shift: true, ..Default::default() });
        state.set_modifiers(Modifiers::default());

        let seen: Vec<bool> = state.app.modifier_changes.iter().map(|m| m.shift).collect();
        assert_eq!(seen, vec![true, false], "押下→解放が変化後の値で届くこと");
    }

    /// 配る前に runtime 自身の状態が更新されていること。順が逆だと、アプリが
    /// `on_input` の中で `ctx.shift_held` 相当を読んだ時に食い違う。
    #[test]
    fn the_runtime_state_is_updated_before_dispatch() {
        let mut state = AppState::new(RecordingApp::default());
        state.set_modifiers(Modifiers { alt: true, ..Default::default() });

        assert!(state.modifiers.alt, "runtime の状態も新しい値になっていること");
        assert_eq!(state.app.modifier_changes.len(), 1);
        assert!(state.app.modifier_changes[0].alt);
    }

    // -----------------------------------------------------------------
    // キー入力のルーティング
    // -----------------------------------------------------------------

    /// 本題の回帰: 解放が届かないと、アプリは「⇧を押している間」を持てない。
    /// 押下だけを配っていた頃は、押しっぱなしフラグが二度と落ちなかった。
    #[test]
    fn key_release_reaches_the_app() {
        let mut state = AppState::new(RecordingApp::default());

        state.modifiers = Modifiers { shift: true, ..Default::default() };
        state.handle_key_input(Key::Shift, true, Vec::new());
        state.modifiers = Modifiers::default();
        state.handle_key_input(Key::Shift, false, Vec::new());

        assert_eq!(
            state.app.keys,
            vec![(Key::Shift, true, true), (Key::Shift, false, false)],
            "押下と解放が両方、その時点の修飾キー付きで届くこと"
        );
    }

    /// 解放は「アプリへ転送する」だけ。副作用（選択解除・文字入力）まで走らせると、
    /// ⇧を離しただけで選択が消えるといった別のバグになる。
    #[test]
    fn key_release_has_no_side_effects() {
        let mut state = AppState::new(RecordingApp::default());
        state.selection = Some(TextSelection {
            anchor: (0, 0),
            head: (0, 3),
            anchor_content: "abc".into(),
            head_content: "abc".into(),
        });

        // 解放は選択を消さない。chars を渡しても文字入力にはならない
        // （実際の呼び出し側でも char_inputs は解放時に空を返す）。
        state.handle_key_input(Key::A, false, vec!['a']);
        assert!(state.selection.is_some(), "解放で選択が消えてはいけない");
        assert!(state.app.chars.is_empty(), "解放で文字が入ってはいけない");

        // 押下は従来どおり選択を解除し、文字を流す。
        state.handle_key_input(Key::A, true, vec!['a']);
        assert!(state.selection.is_none(), "押下は選択を解除する");
        assert_eq!(state.app.chars, vec!['a']);
    }

    /// 修飾キー単独押下は `Key::Other` に落ちることがあり、それは選択を消さない
    /// （⌘を押してから C、が選択を先に消してしまうため）。解放でも同じ。
    #[test]
    fn bare_modifier_does_not_clear_the_selection() {
        let mut state = AppState::new(RecordingApp::default());
        state.selection = Some(TextSelection {
            anchor: (0, 0),
            head: (0, 3),
            anchor_content: "abc".into(),
            head_content: "abc".into(),
        });

        state.handle_key_input(Key::Other, true, Vec::new());
        assert!(state.selection.is_some());
        state.handle_key_input(Key::Other, false, Vec::new());
        assert!(state.selection.is_some());
    }

    /// フォーカス可能な 2 要素を持つツリー。 Tab 移動の観測用。
    fn two_focusables() -> Element {
        sabitori_core::div().flex_col().children([
            sabitori_core::div()
                .id("a")
                .w(sabitori_core::Dimension::Px(100.0))
                .h(sabitori_core::Dimension::Px(30.0))
                .focusable(),
            sabitori_core::div()
                .id("b")
                .w(sabitori_core::Dimension::Px(100.0))
                .h(sabitori_core::Dimension::Px(30.0))
                .focusable(),
        ])
    }

    /// 前提: 消費しなければ Tab はフォーカスを動かす。 下のテストの対照。
    #[test]
    fn tab_moves_focus_when_the_app_does_not_consume_it() {
        let mut state = AppState::new(RecordingApp {
            tree: Some(Box::new(two_focusables)),
            ..Default::default()
        });
        run_frame(&mut state, 400.0, 300.0);

        state.handle_key_input(Key::Tab, true, Vec::new());
        assert!(state.focused_id.is_some(), "Tab でフォーカスが入る");
    }

    /// **issue #18 の回帰テスト.** `on_input` が `true` を返したら、 ランタイムの
    /// 既定動作 (Tab のフォーカス移動) を行わないこと。
    ///
    /// doc は "Return true if handled" と言っていたのに、 0.4.0 より前は戻り値を
    /// どの呼び出し箇所でも読んでいなかった。 Tab を補完に使うアプリのように、
    /// 既定動作を奪いたいケースが書けなかった。
    #[test]
    fn consuming_tab_prevents_the_runtime_from_moving_focus() {
        let mut state = AppState::new(RecordingApp {
            tree: Some(Box::new(two_focusables)),
            consume_all: true,
            ..Default::default()
        });
        run_frame(&mut state, 400.0, 300.0);

        state.handle_key_input(Key::Tab, true, Vec::new());

        assert_eq!(
            state.focused_id, None,
            "アプリが消費したのにランタイムがフォーカスを動かした"
        );
        // イベント自体は届いていること (消費 = 届かない、ではない)。
        assert!(
            state.app.keys.iter().any(|(k, pressed, _)| *k == Key::Tab && *pressed),
            "消費するアプリにもイベントは届く"
        );
    }

    /// **issue #18 の回帰テスト.** Escape のフォーカス解除も抑止できること。
    /// 自前のモーダルを Escape で閉じたいアプリ向け。
    #[test]
    fn consuming_escape_keeps_the_current_focus() {
        let mut state = AppState::new(RecordingApp {
            tree: Some(Box::new(two_focusables)),
            consume_all: true,
            ..Default::default()
        });
        run_frame(&mut state, 400.0, 300.0);
        state.focused_id = Some("a".to_string());

        state.handle_key_input(Key::Escape, true, Vec::new());

        assert_eq!(
            state.focused_id.as_deref(),
            Some("a"),
            "アプリが消費したのにフォーカスが外れた"
        );
    }

    /// **issue #15 の回帰テスト.** `view()` の中で実フォント計測が使えること。
    ///
    /// これが無いとキャレットの x 位置を計算する手段が無く、 等幅以外のテキスト欄に
    /// カーソルを置けない。 `ViewContext::mono_advance` (等幅 1 セルぶんの送り) が
    /// 唯一の計測手段だった頃は、 プロポーショナル書体では原理的に書けなかった。
    ///
    /// ランタイムが `measurer` を差し忘れると `None` へのフォールバック
    /// (mono_advance からの概算) に落ちるので、 **StubMeasure の値と一致するか**で
    /// 「概算ではなく本物が来ている」ことまで見る。
    #[test]
    fn view_can_measure_text_with_the_real_measurer() {
        let mut state = AppState::new(RecordingApp::default());
        run_frame(&mut state, 800.0, 600.0);

        // 配線そのもの。 値の比較では検出できない (下の注記参照)。
        assert!(
            state.app.measurer_present.get(),
            "ランタイムが ViewContext に計測器を差していない"
        );
        // 計測の中身。 StubMeasure は 1 文字 = font_size * 0.5 なので "ab" @ 20px = 20.0。
        //
        // 注意: この値だけでは配線漏れを検出できない。 StubMeasure から導かれる
        // `mono_advance` は 0.5 で、 計測器なしのフォールバック
        // (chars * mono_advance * font_size) と答えが一致するため。 上の
        // `measurer_present` が本命で、 こちらは計算そのものの確認。
        assert_eq!(state.app.measured_caret.get(), 20.0);
    }

    /// `input_delivery` の宣言が、 実際にランタイムを回したときの挙動と一致すること。
    ///
    /// 表は手書きなので、 放っておけば実装とズレる — それでは issue #12 の
    /// 「宣言はあるが配線が無い」 を別の形で再生産するだけになる。 ここでは
    /// **ヘッドレスで叩ける入口を持つ種別**について、 `ToApp` と宣言したものが
    /// 本当に `on_input` へ届くことを確認する。
    ///
    /// ポインタ系と IME は winit の `WindowEvent` からしか駆動できず、 それには
    /// 窓が要るのでここでは検証できない。 ヘッドレス駆動を公開 API にする
    /// issue #19 が入ったら、 残りもここに足すこと。
    #[test]
    fn declared_delivery_matches_reality() {
        use sabitori_input::Delivery;

        let mut state = AppState::new(RecordingApp::default());

        // 叩ける入口: キー入力 (文字つき) と修飾キーの変化。
        state.handle_key_input(Key::A, true, vec!['a']);
        state.set_modifiers(Modifiers { shift: true, ..Default::default() });

        let driven = [
            InputEventKind::KeyInput,
            InputEventKind::CharInput,
            InputEventKind::ModifiersChanged,
        ];
        for kind in driven {
            assert_eq!(
                crate::declarative::input_delivery(kind),
                Delivery::ToApp,
                "{kind:?} を ToApp 以外に変えたなら、 このテストの driven からも外すこと"
            );
            assert!(
                state.app.received.contains(&kind),
                "{kind:?} は ToApp と宣言されているのに on_input へ届いていない"
            );
        }

        // 逆向きも見る: 宣言が ToApp でない種別が紛れ込んでいないこと。
        for kind in &state.app.received {
            assert_eq!(
                crate::declarative::input_delivery(*kind),
                Delivery::ToApp,
                "{kind:?} が on_input に届いているのに、 宣言が ToApp になっていない"
            );
        }
    }
}

/// Multiple [`HighlightSpec`]s on one text element.
///
/// One spec paints one color across its ranges (plus a second color for the
/// single `current` range), which covers find-in-page. Text carrying two
/// independent colorings at once — a new/old comparison with red deletions and
/// green insertions interleaved through one sentence — needs a spec per
/// coloring. See https://github.com/Mutafika/sabitori/issues/64.
#[cfg(test)]
mod highlight_tests {
    use super::*;
    use crate::bridge::TextHitLayout;
    use sabitori_core::{Color, HighlightSpec};
    use sabitori_text::GlyphHit;

    const DEL: Color = Color::new(0.9, 0.2, 0.2, 1.0);
    const ADD: Color = Color::new(0.2, 0.8, 0.3, 1.0);

    /// One glyph per byte on a single line, 10px wide each, so a byte range
    /// `(a, b)` lands at x = 10a with width 10(b - a).
    fn layout(len: usize, highlight: Vec<HighlightSpec>) -> TextHitLayout {
        TextHitLayout {
            text_idx: 0,
            content: "x".repeat(len),
            hits: (0..len)
                .map(|i| GlyphHit {
                    byte_start: i,
                    byte_end: i + 1,
                    x: i as f32 * 10.0,
                    y: 0.0,
                    w: 10.0,
                    h: 16.0,
                    line_index: 0,
                })
                .collect(),
            clip_rect: None,
            highlight,
            link_ranges: None,
            no_select: false,
        }
    }

    /// The regression: both colorings must survive. Before #64 the style field
    /// held a single spec, so the second `.highlight()` call replaced the first
    /// and one of the two colors vanished entirely.
    #[test]
    fn two_specs_both_paint() {
        let layouts = vec![layout(
            10,
            vec![
                HighlightSpec { ranges: vec![(0, 2)], color: DEL, ..Default::default() },
                HighlightSpec { ranges: vec![(5, 8)], color: ADD, ..Default::default() },
            ],
        )];
        let rects = AppState::<TestApp>::compute_highlight_rects(&layouts);

        assert_eq!(rects.len(), 2, "one rect per range, both specs");
        assert_eq!(rects[0].fill_color, DEL.to_array());
        assert_eq!(rects[0].rect[0], 0.0);
        assert_eq!(rects[0].rect[2], 20.0, "bytes 0..2 = two 10px glyphs");
        assert_eq!(rects[1].fill_color, ADD.to_array());
        assert_eq!(rects[1].rect[0], 50.0);
        assert_eq!(rects[1].rect[2], 30.0);
    }

    /// Specs paint in call order, so the later one lands on top where they
    /// overlap. That is what lets an app put find-in-page over a diff coloring.
    #[test]
    fn later_specs_paint_over_earlier_ones() {
        let layouts = vec![layout(
            10,
            vec![
                HighlightSpec { ranges: vec![(0, 10)], color: DEL, ..Default::default() },
                HighlightSpec { ranges: vec![(2, 4)], color: ADD, ..Default::default() },
            ],
        )];
        let rects = AppState::<TestApp>::compute_highlight_rects(&layouts);

        assert_eq!(rects.len(), 2);
        // Painter order is append order, so the narrow ADD rect is emitted last.
        assert_eq!(rects[0].fill_color, DEL.to_array());
        assert_eq!(rects[1].fill_color, ADD.to_array());
        assert_eq!(rects[1].rect[0], 20.0);
    }

    /// `current` still accents one range within its own spec, unchanged.
    #[test]
    fn current_still_overrides_within_a_spec() {
        let layouts = vec![layout(
            10,
            vec![HighlightSpec {
                ranges: vec![(0, 2), (4, 6)],
                color: DEL,
                current: Some(1),
                current_color: ADD,
            }],
        )];
        let rects = AppState::<TestApp>::compute_highlight_rects(&layouts);

        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0].fill_color, DEL.to_array());
        assert_eq!(rects[1].fill_color, ADD.to_array(), "the current range wins");
    }

    /// No specs, no rects — the default stays a no-op.
    #[test]
    fn no_specs_paints_nothing() {
        let rects = AppState::<TestApp>::compute_highlight_rects(&[layout(10, Vec::new())]);
        assert!(rects.is_empty());
    }

    // ---------------------------------------------------------------
    // #68 — hit_test_text の距離足切り
    // ---------------------------------------------------------------

    /// `layout()` と同じ 1 行 10px グリッドを、 任意の index / 位置 / no_select で。
    /// 行は y = `y`..`y + 16`、 x = `x0`..`x0 + 10 * len`。
    fn hit_layout(idx: usize, len: usize, x0: f32, y: f32, no_select: bool) -> TextHitLayout {
        TextHitLayout {
            text_idx: idx,
            content: "x".repeat(len),
            hits: (0..len)
                .map(|i| GlyphHit {
                    byte_start: i,
                    byte_end: i + 1,
                    x: x0 + i as f32 * 10.0,
                    y,
                    w: 10.0,
                    h: 16.0,
                    line_index: 0,
                })
                .collect(),
            clip_rect: None,
            highlight: Vec::new(),
            link_ranges: None,
            no_select,
        }
    }

    /// 本題の回帰: 文字の無い場所を押しても、 遠くの label を掴んではいけない。
    /// 足切りが無かった頃はここで anchor が立ち、 そのままドラッグすると画面中の
    /// text が端から端まで選択されて青く染まった。
    #[test]
    fn strict_press_far_from_any_text_hits_nothing() {
        let layouts = vec![hit_layout(0, 10, 0.0, 0.0, false)]; // 0..100 × 0..16
        assert_eq!(
            AppState::<TestApp>::hit_test_text_in(&layouts, 600.0, 400.0, true),
            None,
            "キャンバスの真ん中を押しただけで selection が始まってはいけない"
        );
    }

    /// 一方 drag 中 (strict=false) は最近傍 snap のまま。 anchor が実テキスト上に
    /// 立っている以上、 余白へ払っても選択が伸び続けるのが期待値。
    #[test]
    fn lenient_drag_still_snaps_from_far_away() {
        let layouts = vec![hit_layout(0, 10, 0.0, 0.0, false)];
        assert_eq!(
            AppState::<TestApp>::hit_test_text_in(&layouts, 600.0, 400.0, false),
            Some((0, 10)),
            "最後の glyph の byte_end = 行末に snap"
        );
    }

    /// 文字の上はもちろん当たる。 2 文字目 (x = 10..20) の左半分 → byte_start = 1。
    #[test]
    fn strict_press_on_a_glyph_hits_it() {
        let layouts = vec![hit_layout(0, 10, 0.0, 0.0, false)];
        assert_eq!(
            AppState::<TestApp>::hit_test_text_in(&layouts, 12.0, 8.0, true),
            Some((0, 1))
        );
    }

    /// 行末の少し先を押したら行末 caret に snap する (許容 = 行高 1 つぶん)。
    /// ここまで切ってしまうと「行の後ろをクリックして行末に caret」が死ぬ。
    #[test]
    fn strict_press_just_past_the_line_end_snaps_to_it() {
        let layouts = vec![hit_layout(0, 10, 0.0, 0.0, false)];
        assert_eq!(
            AppState::<TestApp>::hit_test_text_in(&layouts, 110.0, 8.0, true),
            Some((0, 10)),
            "行末 +10px は許容 (16px) の内側"
        );
        assert_eq!(
            AppState::<TestApp>::hit_test_text_in(&layouts, 130.0, 8.0, true),
            None,
            "行末 +30px は許容の外 = 当たっていない"
        );
    }

    /// 縦は行高の半分まで。 行間の中点を越えたら当たっていない。
    #[test]
    fn strict_press_below_the_line_misses() {
        let layouts = vec![hit_layout(0, 10, 0.0, 0.0, false)];
        assert_eq!(
            AppState::<TestApp>::hit_test_text_in(&layouts, 50.0, 20.0, true),
            Some((0, 5)),
            "行の下 4px は許容 (8px) の内側"
        );
        assert_eq!(
            AppState::<TestApp>::hit_test_text_in(&layouts, 50.0, 40.0, true),
            None,
            "行の下 24px は許容の外"
        );
    }

    // ---------------------------------------------------------------
    // #67 — no_select
    // ---------------------------------------------------------------

    /// `.no_select()` した text は、 その文字の**真上**を押しても anchor にならない。
    #[test]
    fn no_select_text_is_never_hit() {
        let layouts = vec![hit_layout(0, 10, 0.0, 0.0, true)];
        assert_eq!(AppState::<TestApp>::hit_test_text_in(&layouts, 12.0, 8.0, true), None);
        assert_eq!(AppState::<TestApp>::hit_test_text_in(&layouts, 12.0, 8.0, false), None);
    }

    /// no_select は anchor/head にならないだけでは足りない。 選択範囲に挟まれた時に
    /// 塗られてしまうと「選択できないはずの label が選択済みに見える」。
    #[test]
    fn no_select_text_caught_in_a_range_is_not_painted() {
        let layouts = vec![
            hit_layout(0, 4, 0.0, 0.0, false),
            hit_layout(1, 4, 0.0, 20.0, true), // 挟まれる chrome
            hit_layout(2, 4, 0.0, 40.0, false),
        ];
        let sel = TextSelection {
            anchor: (0, 0),
            head: (2, 4),
            anchor_content: "xxxx".into(),
            head_content: "xxxx".into(),
        };
        let rects = AppState::<TestApp>::compute_selection_rects(Some(&sel), &layouts, DEL);

        assert_eq!(rects.len(), 2, "no_select の 1 行を飛ばして 2 行ぶんだけ塗る");
        assert_eq!(rects[0].rect[1], 0.0);
        assert_eq!(rects[1].rect[1], 40.0, "y=20 の no_select 行は塗られない");
    }

    struct TestApp;
    impl DeclarativeApp for TestApp {
        fn view(&self, _ctx: &ViewContext) -> Element {
            sabitori_core::div()
        }
    }
}
