//! SceneApp: custom GPU rendering + declarative UI overlay.
//!
//! Frame order:
//!   1. app.render_scene(ctx)  — custom 3D/2D scene with depth
//!   2. Sabitori UI overlay    — view() Element tree, no depth, LoadOp::Load
//!   3. Present

use std::sync::Arc;
use web_time::Instant;

use sabitori_core::build::build_tree_measured;
use sabitori_core::ViewContext;
use sabitori_gpu::{GpuContext, GpuRenderer, RenderPhase, SceneRenderContext};
use sabitori_input::{
    Delivery, InputEvent, InputEventKind, Key, Modifiers, MouseButton as SabiMouseButton,
    PointerKind, MOUSE_POINTER_ID,
};
use sabitori_text::TextRenderer;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, WindowEvent};
use winit::event_loop::{ActiveEventLoop, DeviceEvents, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

use crate::bridge::{
    draw_ui_layer, MeasureCache, TextRendererMeasurer, UiDrawLists, UiRenderers,
};
use crate::declarative::{DeclarativeApp, UiCapture};
use crate::input_router::{pinch_metrics, PinchGesture, PrimaryInput, TouchDrag, TOUCH_SLOP};

/// Extension trait for apps that combine custom GPU rendering with declarative UI.
///
/// Implement both `DeclarativeApp` (for 2D UI overlay) and `SceneApp` (for custom scene).
pub trait SceneApp: DeclarativeApp {
    /// Called once after GPU initialization.
    /// Create your custom pipelines, shaders, buffers, and bind groups here.
    fn setup(&mut self, ctx: &GpuContext);

    /// Called when the window is resized.
    /// The depth texture is already recreated by GpuRenderer before this is called.
    fn on_resize(&mut self, _ctx: &GpuContext) {}

    /// Render the custom scene. Called every frame before the UI overlay pass.
    ///
    /// The app is responsible for creating its own render pass on `ctx.encoder`,
    /// including clearing color/depth attachments and drawing custom geometry.
    fn render_scene(&mut self, ctx: &mut SceneRenderContext);

    /// Raw (unaccelerated) relative mouse motion from `DeviceEvent::MouseMotion`.
    /// Fires on every physical mouse move regardless of button state — gate on your own
    /// drag flags. Needed because macOS does NOT deliver `WindowEvent::CursorMoved` position
    /// updates while a non-primary button (middle/right) is held, so camera pan/orbit driven
    /// off `on_pointer_move` freezes mid-drag. Use this for those drags (CAD/3D-app style).
    fn on_raw_motion(&mut self, _dx: f64, _dy: f64) {}
}

/// このランタイムが [`InputEvent`] の各種別をアプリへどう届けるかの宣言。
///
/// [`crate::declarative::input_delivery`] と見比べること。 IME の届き方が
/// declarative と違っていた (`ImeEnabled` を組み立てない / preedit・commit が
/// フォーカス限定) が、 issue #22 で揃えた。 現在の差は `PointerLeft` の扱いだけで、
/// これは両ランタイムとも同じ (`on_cursor_left` で伝える)。
pub fn input_delivery(kind: InputEventKind) -> Delivery {
    match kind {
        InputEventKind::PointerMoved
        | InputEventKind::PointerPressed
        | InputEventKind::PointerReleased
        | InputEventKind::PointerCancelled => Delivery::ToApp,

        InputEventKind::PointerLeft => {
            Delivery::NotProduced("カーソルの離脱は DeclarativeApp::on_cursor_left で伝える")
        }

        InputEventKind::ImeEnabled
        | InputEventKind::ImePreedit
        | InputEventKind::ImeCommit
        | InputEventKind::KeyInput
        | InputEventKind::CharInput
        | InputEventKind::ModifiersChanged
        | InputEventKind::Paste => Delivery::ToApp,
    }
}

struct SceneAppState<A: SceneApp> {
    app: A,
    window: Option<Arc<Window>>,
    renderer: Option<GpuRenderer>,
    text_renderer: Option<TextRenderer>,
    /// Draws `image(key, data)` elements. `run_scene` previously had no image
    /// pipeline at all, so image elements were silently dropped; this runs the
    /// same one the declarative path uses.
    image_renderer: Option<sabitori_gpu::ImageRenderer>,
    /// Draws `ring(...)` elements — same story as `image_renderer`: absent from
    /// `run_scene` until now, so rings vanished without a warning.
    ring_renderer: Option<sabitori_gpu::RingRenderer>,
    /// Draws `polyline(...)` elements. See `ring_renderer`.
    line_renderer: Option<sabitori_gpu::LineRenderer>,
    measure_cache: std::cell::RefCell<MeasureCache>,
    last_frame: Instant,
    last_build: Option<sabitori_core::build::BuildResult>,
    mouse_x: f32,
    mouse_y: f32,
    hovered_id: Option<String>,
    /// 押下中の要素の id。`active_style` の畳み込みに使う。declarative の
    /// `AppState` と同じ意味・同じ寿命（押下で入り、解放・キャンセル・離脱で消える）。
    pressed_id: Option<String>,
    focused_id: Option<String>,
    /// Last cursor we asked winit to display, to dedup `set_cursor`. Mirrors
    /// the field of the same name in the declarative `AppState`.
    last_cursor: Option<sabitori_core::Cursor>,
    /// Last IME caret rect handed to winit (`set_ime_cursor_area`), to dedup —
    /// polled every frame from [`DeclarativeApp::ime_cursor_area`] but only
    /// re-sent when the caret rect changes.
    last_ime_area: Option<(f32, f32, f32, f32)>,
    /// Last IME-allowed state handed winit (`set_ime_allowed`), to dedup.
    /// See [`DeclarativeApp::ime_allowed`].
    last_ime_allowed: bool,
    modifiers: Modifiers,
    last_viewport_w: f32,
    last_viewport_h: f32,
    setup_done: bool,
    primary_input: PrimaryInput,
    active_touches: std::collections::HashMap<u64, (f32, f32)>,
    touch_drag: Option<TouchDrag>,
    pinch: Option<PinchGesture>,
    /// Last [`UiCapture`] snapshot pushed to the app (deduped).
    last_capture: UiCapture,
    /// 管理されたスクロールコンテナ(`.scroll(id)`)の状態。DeclarativeApp ランタイム
    /// と同様にホイールを該当領域へルーティングし、毎フレーム offset を patch する。
    scroll_states: std::collections::HashMap<String, sabitori_widgets::ScrollView>,
    /// Animated style transitions (hover/active spring). Mirrors `AppState`
    /// so `.hover()/.active()` with `.transition(...)` animate in run_scene.
    style_animator: sabitori_widgets::StyleAnimator,
    /// Presence (mount/unmount) animator backing `.animate_presence()`.
    presence_animator: sabitori_widgets::PresenceAnimator,
    /// Managed tooltip hover-delay state, driving `.tooltip()` auto-display.
    tooltip_state: sabitori_widgets::TooltipState,
    /// Drag & drop state for `.draggable()` / `.droppable()`. Mouse only for
    /// now; touch-drag in run_scene is a follow-up (see issue #25).
    drag_manager: sabitori_widgets::DragManager,
}

impl<A: SceneApp> SceneAppState<A> {
    /// ポインタ直下のホバー / tooltip / cursor を引き直し、変化をアプリへ通知する。
    /// 解決そのものは [`crate::runtime_shared::resolve_hover`] — declarative と
    /// 同じ関数を呼ぶ。
    fn update_hover(&mut self) {
        let hit = self
            .last_build
            .as_ref()
            .map(|b| crate::runtime_shared::resolve_hover(b, self.mouse_x, self.mouse_y))
            .unwrap_or_default();
        // 本文中リンクの上書きは declarative 側にしかない (こちらにテキスト選択層が
        // 無いため)。 それ以外は同じ経路を通る。
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

    /// 解決した cursor を OS へ送る。 実体は
    /// [`crate::runtime_shared::apply_cursor`] — declarative と同じ関数を呼ぶので、
    /// 2 ランタイムの cursor 解決は定義上一致する（かつては "ported verbatim" の
    /// 複製で、一致は口約束だった）。
    fn apply_cursor(&mut self, cursor: Option<sabitori_core::Cursor>) {
        crate::runtime_shared::apply_cursor(self.window.as_ref(), &mut self.last_cursor, cursor);
    }

    /// 押下対象の解決。 実体は [`crate::runtime_shared::hit_id_at`]。
    fn hit_id_at(&self, x: f32, y: f32) -> Option<String> {
        let build = self.last_build.as_ref()?;
        crate::runtime_shared::hit_id_at(build, x, y)
    }

    /// Recompute the [`UiCapture`] snapshot and push it to the app when it
    /// changed. SceneApp hosts gate their camera / scene input on this —
    /// the egui `wants_pointer_input()` / `wants_keyboard_input()` pattern.
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

    fn winit_to_sabi_button(button: winit::event::MouseButton) -> Option<SabiMouseButton> {
        match button {
            winit::event::MouseButton::Left => Some(SabiMouseButton::Left),
            winit::event::MouseButton::Right => Some(SabiMouseButton::Right),
            winit::event::MouseButton::Middle => Some(SabiMouseButton::Middle),
            _ => None,
        }
    }
}

impl<A: SceneApp> ApplicationHandler for SceneAppState<A> {
    #[cfg(not(target_arch = "wasm32"))]
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() { return; }
        let (w, h) = self.app.size();
        let mut attrs = WindowAttributes::default()
            .with_title(self.app.title())
            .with_inner_size(winit::dpi::LogicalSize::new(w, h))
            .with_min_inner_size(winit::dpi::LogicalSize::new(400.0, 300.0));
        if self.app.transparent() {
            attrs = attrs.with_transparent(true);
        }
        if !self.app.decorations() {
            attrs = attrs.with_decorations(false);
        }
        // 生の相対マウスモーションを受け取る（中/右ボタンドラッグ中も止まらないカメラ操作用）。
        event_loop.listen_device_events(DeviceEvents::Always);
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        // Enable IME so Japanese (and other) input methods deliver
        // preedit/commit events to the `WindowEvent::Ime` handler. Without
        // this winit never emits them and IME is silently dead in run_scene.
        window.set_ime_allowed(true);
        let mut gpu = GpuRenderer::new_with_alpha(window.clone(), self.app.transparent());
        let mut text = TextRenderer::new(&gpu.device, gpu.surface_config.format, &gpu.globals_bind_group_layout);
        let user_fonts = self.app.fonts();
        if !user_fonts.is_empty() {
            text.prefer_user_fonts(&user_fonts);
        }

        // Create depth texture for 3D scene rendering
        gpu.create_depth_texture();

        // Let the app create its custom pipelines/buffers
        self.app.setup(&gpu.gpu_context());
        self.setup_done = true;

        self.app.set_window(window.clone());
        self.window = Some(window);
        self.image_renderer = Some(sabitori_gpu::ImageRenderer::new(
            &gpu.device,
            gpu.surface_config.format,
            &gpu.globals_bind_group_layout,
        ));
        self.ring_renderer = Some(sabitori_gpu::RingRenderer::new(
            &gpu.device,
            gpu.surface_config.format,
            &gpu.globals_bind_group_layout,
        ));
        self.line_renderer = Some(sabitori_gpu::LineRenderer::new(
            &gpu.device,
            gpu.surface_config.format,
            &gpu.globals_bind_group_layout,
        ));
        self.renderer = Some(gpu);
        self.text_renderer = Some(text);
    }

    #[cfg(target_arch = "wasm32")]
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // On WASM, window + renderer init is handled externally (async).
    }

    /// 生の相対マウスモーション。ボタン状態に関係なく届くので、middle/right ドラッグ中も
    /// カメラ操作が止まらない（macOS は非主ボタンドラッグ中 CursorMoved 位置を更新しない）。
    /// アプリ側は自前のドラッグフラグで取捨する。
    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _device_id: DeviceId, event: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.app.on_raw_motion(delta.0, delta.1);
            if let Some(w) = self.window.as_ref() {
                w.request_redraw();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                if let (Some(w), Some(r)) = (self.window.as_ref(), self.renderer.as_mut()) {
                    r.resize(size.width, size.height, w.scale_factor());
                    self.app.on_resize(&r.gpu_context());
                    w.request_redraw();
                }
            }

            WindowEvent::CursorEntered { .. } => {
                // Invalidate the deduped cursor: the pointer may arrive
                // carrying a foreign cursor set by another window (winit
                // doesn't use macOS cursor rects, so the OS won't reset it at
                // the boundary). The next CursorMoved re-applies ours.
                self.last_cursor = None;
            }

            WindowEvent::CursorMoved { position, .. } => {
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
                // マウスの移動も `PointerMoved` として配る (declarative と同じ)。
                // enum の doc が約束している mouse 分を、このランタイムも出す。
                self.app.on_input(&InputEvent::PointerMoved {
                    id: MOUSE_POINTER_ID,
                    kind: PointerKind::Mouse,
                    position: sabitori_core::Point::new(self.mouse_x, self.mouse_y),
                    modifiers: self.modifiers,
                });
                // Promote a pending drag past the slop / update the active drag.
                self.drag_manager.on_move(self.mouse_x, self.mouse_y);
            }

            WindowEvent::CursorLeft { .. } => {
                if self.primary_input == PrimaryInput::Touch {
                    return;
                }
                self.hovered_id = None;
                self.pressed_id = None;
                // Pointer left the window mid-drag → notify + cancel.
                if let Some((data, _source_id)) = self.drag_manager.drag_info() {
                    self.app.on_drag_out(&data);
                    self.drag_manager.cancel();
                }
                self.app.on_cursor_left();
            }

            WindowEvent::MouseInput { state, button, .. } => {
                if self.primary_input == PrimaryInput::Touch {
                    return;
                }
                let pos = sabitori_core::Point::new(self.mouse_x, self.mouse_y);

                // Claim / release modality ownership on the primary button.
                if button == winit::event::MouseButton::Left {
                    match state {
                        winit::event::ElementState::Pressed => {
                            if self.primary_input == PrimaryInput::None {
                                self.primary_input = PrimaryInput::Mouse;
                            }
                        }
                        winit::event::ElementState::Released => {
                            if self.primary_input == PrimaryInput::Mouse {
                                self.primary_input = PrimaryInput::None;
                            }
                        }
                    }
                }

                // Forward mouse button events to on_input (for camera drag, etc.)
                if let Some(sabi_btn) = Self::winit_to_sabi_button(button) {
                    let event = match state {
                        winit::event::ElementState::Pressed => InputEvent::PointerPressed {
                            id: MOUSE_POINTER_ID,
                            kind: PointerKind::Mouse,
                            position: pos,
                            button: Some(sabi_btn),
                            modifiers: self.modifiers,
                        },
                        winit::event::ElementState::Released => InputEvent::PointerReleased {
                            id: MOUSE_POINTER_ID,
                            kind: PointerKind::Mouse,
                            position: pos,
                            button: Some(sabi_btn),
                            modifiers: self.modifiers,
                        },
                    };
                    self.app.on_input(&event);
                }

                // 押下中の要素を覚える → 次フレームで active_style が畳まれる (#3)。
                if button == winit::event::MouseButton::Left {
                    self.pressed_id = match state {
                        winit::event::ElementState::Pressed => self.hit_id_at(pos.x, pos.y),
                        winit::event::ElementState::Released => None,
                    };
                }

                // Also handle DeclarativeApp click/focus semantics
                if state == winit::event::ElementState::Pressed
                    && button == winit::event::MouseButton::Left
                {
                    let mut pending_drag: Option<(String, Option<String>)> = None;
                    if let Some(ref build) = self.last_build {
                        let mut focus_set = false;
                        for region in &build.hit_regions {
                            // 意味だけの領域 (role/label のみ) は透過する。 declarative
                            // 側と同じ扱い (`HitRegion::is_interactive` の doc を参照)。
                            if region.is_interactive() && region.rect.contains(pos) {
                                if region.focusable {
                                    self.focused_id = region.id.clone();
                                    focus_set = true;
                                }
                                if region.clickable {
                                    if let Some(ref id) = region.id {
                                        self.app.on_click(id);
                                    }
                                }
                                // A draggable element starts a pending drag that
                                // promotes to active once the pointer moves past
                                // the slop (handled by DragManager::on_move).
                                if let Some(ref drag_data) = region.drag_data {
                                    pending_drag = Some((drag_data.clone(), region.id.clone()));
                                }
                                break;
                            }
                        }
                        if !focus_set {
                            self.focused_id = None;
                        }
                    }
                    if let Some((data, source_id)) = pending_drag {
                        self.drag_manager.start_pending(data, source_id, self.mouse_x, self.mouse_y);
                    }
                    // Focus / pending-drag changes flip the capture snapshot.
                    self.push_ui_capture();
                }

                if state == winit::event::ElementState::Released
                    && button == winit::event::MouseButton::Left
                {
                    // Complete an active drag: drop onto the zone under the
                    // pointer, if any, before the app's pointer-up hook.
                    if let Some((data, _source_id)) = self.drag_manager.on_release() {
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
                    self.push_ui_capture();
                }

                if state == winit::event::ElementState::Pressed
                    && button == winit::event::MouseButton::Right
                {
                    if let Some(ref build) = self.last_build {
                        let mut found = false;
                        for region in &build.hit_regions {
                            if region.clickable && region.rect.contains(pos) {
                                if let Some(ref id) = region.id {
                                    self.app.on_right_click(id, self.mouse_x, self.mouse_y);
                                    found = true;
                                }
                                break;
                            }
                        }
                        if !found {
                            self.app.on_right_click("", self.mouse_x, self.mouse_y);
                        }
                    }
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                if self.primary_input == PrimaryInput::Touch {
                    return;
                }
                let (delta_x, delta_y) = match delta {
                    winit::event::MouseScrollDelta::LineDelta(x, y) => (x * 20.0, y * 20.0),
                    winit::event::MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                };
                // カーソル下の管理スクロールコンテナ(`.scroll(id)`)へルーティング。
                // 該当が無ければアプリの on_scroll へフォールバック（DeclarativeApp と同じ）。
                let mut handled = false;
                if let Some(ref build) = self.last_build {
                    let pt = sabitori_core::Point::new(self.mouse_x, self.mouse_y);
                    for region in &build.hit_regions {
                        if region.rect.contains(pt) {
                            if let Some(ref id) = region.id {
                                if let Some(sv) = self.scroll_states.get_mut(id) {
                                    sv.on_scroll_xy(delta_x, delta_y);
                                    handled = true;
                                    break;
                                }
                            }
                        }
                    }
                }
                if !handled {
                    self.app.on_scroll(delta_y);
                    self.app.on_scroll_xy(delta_x, delta_y);
                }
            }

            // Touch events — first finger drives click/focus (deferred to
            // release with TOUCH_SLOP), second finger promotes to pinch.
            // Gated by the first-come mouse/touch mutex.
            WindowEvent::Touch(touch) => {
                let Some(r) = self.renderer.as_ref() else { return };
                let scale = r.scale_factor;
                let x = touch.location.x as f32 / scale;
                let y = touch.location.y as f32 / scale;
                let pos = sabitori_core::Point::new(x, y);
                let id = touch.id.saturating_add(1);

                // Always update tracking + forward raw events.
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
                    }
                    winit::event::TouchPhase::Cancelled => {
                        self.active_touches.remove(&touch.id);
                        self.app.on_input(&InputEvent::PointerCancelled {
                            id,
                            kind: PointerKind::Touch,
                        });
                    }
                }

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
                            self.mouse_x = x;
                            self.mouse_y = y;
                            // Topmost clickable region under initial touch.
                            let mut click_target: Option<String> = None;
                            if let Some(ref build) = self.last_build {
                                let mut focus_set = false;
                                for region in &build.hit_regions {
                                    // タッチも同様に、 意味だけの領域は透過する。
                                    if region.is_interactive() && region.rect.contains(pos) {
                                        if region.focusable {
                                            self.focused_id = region.id.clone();
                                            focus_set = true;
                                        }
                                        if region.clickable {
                                            click_target = region.id.clone();
                                        }
                                        break;
                                    }
                                }
                                if !focus_set {
                                    self.focused_id = None;
                                }
                            }
                            self.touch_drag = Some(TouchDrag {
                                id: touch.id,
                                start: (x, y),
                                last: (x, y),
                                last_move_time: None,
                                click_target,
                                scroll_target: None,
                                moved_beyond_slop: false,
                            });
                        } else if count == 2 && self.pinch.is_none() {
                            if let Some(ref mut td) = self.touch_drag {
                                td.moved_beyond_slop = true;
                            }
                            let ids: Vec<u64> = self.active_touches.keys().copied().collect();
                            let (id_a, id_b) = (ids[0], ids[1]);
                            if let Some((dist, center)) =
                                pinch_metrics(&self.active_touches, id_a, id_b)
                            {
                                self.pinch = Some(PinchGesture {
                                    id_a,
                                    id_b,
                                    start_distance: dist.max(1.0),
                                });
                                self.app.on_pinch_start(center.0, center.1);
                            }
                        }
                    }
                    winit::event::TouchPhase::Moved => {
                        if let Some(ref pinch) = self.pinch {
                            if touch.id == pinch.id_a || touch.id == pinch.id_b {
                                if let Some((dist, center)) =
                                    pinch_metrics(&self.active_touches, pinch.id_a, pinch.id_b)
                                {
                                    let scale_f = dist / pinch.start_distance;
                                    self.app.on_pinch(scale_f, center.0, center.1);
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
                            self.app.on_pointer_move(x, y);
                            if let Some(ref mut td) = self.touch_drag {
                                td.last = (x, y);
                                let tdx = x - td.start.0;
                                let tdy = y - td.start.1;
                                if !td.moved_beyond_slop
                                    && (tdx * tdx + tdy * tdy).sqrt() > TOUCH_SLOP
                                {
                                    td.moved_beyond_slop = true;
                                }
                            }
                        }
                    }
                    winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => {
                        let is_cancel = matches!(touch.phase, winit::event::TouchPhase::Cancelled);
                        if let Some(pinch) = self.pinch.as_ref() {
                            if touch.id == pinch.id_a || touch.id == pinch.id_b {
                                self.pinch = None;
                                self.app.on_pinch_end();
                                self.touch_drag = None;
                            }
                        } else if self
                            .touch_drag
                            .as_ref()
                            .map(|t| t.id == touch.id)
                            .unwrap_or(false)
                        {
                            let td = self.touch_drag.take();
                            if let Some(td) = td {
                                if !is_cancel && !td.moved_beyond_slop {
                                    if let Some(cid) = td.click_target {
                                        self.app.on_click(&cid);
                                    }
                                }
                            }
                            self.app.on_pointer_up();
                        }
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
                let was_shift = self.modifiers.shift;
                self.modifiers = Modifiers {
                    shift: state.shift_key(),
                    ctrl: state.control_key(),
                    alt: state.alt_key(),
                    meta: state.super_key(),
                };
                // 修飾キー単独（特に macOS の Shift）は KeyboardInput が来ないことがあるため、
                // Shift の立ち上がり（false→true）を押下イベントとして app へ届ける。
                if !was_shift && self.modifiers.shift {
                    self.app.on_input(&InputEvent::KeyInput {
                        key: Key::Shift,
                        pressed: true,
                        modifiers: self.modifiers,
                    });
                }
                // 変化後の値をアプリへ配る (3 ランタイム共通)。 上の Shift 立ち上がり
                // 合成と違い、 全修飾キーの押下・解放をどちらも観測できる。
                self.app.on_input(&InputEvent::ModifiersChanged(self.modifiers));
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == winit::event::ElementState::Pressed {
                    // winit → Key の変換は sabitori_window::keymap に集約している
                    // （3 ランタイム共通）。対応が無い名前付きキーは Other として届ける。
                    let key = sabitori_window::keymap::key_from_winit(&event.logical_key)
                        .unwrap_or(Key::Other);
                    let key_event = InputEvent::KeyInput {
                        key,
                        pressed: true,
                        modifiers: self.modifiers,
                    };
                    // 配る順は declarative と同じ: **アプリが先、既定動作があと**。
                    // 消費されたら既定動作 (Escape のフォーカス解除) を行わない
                    // (issue #18)。 Escape はフォーカス操作そのものなので
                    // `on_focused_input` は経由しない。
                    let fid = self.focused_id.clone();
                    let handled_by_focus = if key != Key::Escape {
                        match fid.as_ref() {
                            Some(f) => self.app.on_focused_input(f, &key_event),
                            None => false,
                        }
                    } else {
                        false
                    };
                    let handled = handled_by_focus || self.app.on_input(&key_event);

                    if !handled && key == Key::Escape {
                        self.focused_id = None;
                        self.push_ui_capture();
                    }

                    // Cmd/Ctrl+V: クリップボードを読んで 1 イベントで配る (issue #20)。
                    // declarative と同じ扱い。
                    if !handled && crate::clipboard::is_paste_shortcut(key, self.modifiers) {
                        if let Some(text) = crate::clipboard::read_text() {
                            let ev = InputEvent::Paste { text };
                            crate::runtime_shared::dispatch(
                                &mut self.app,
                                self.focused_id.as_deref(),
                                &ev,
                            );
                        }
                    }

                    // 以前はここが素通しで、Backspace の "\x7f" がテキストとして
                    // 挿入され、Cmd+C の "c" も漏れていた。判定は keymap に集約。
                    for ch in sabitori_window::keymap::char_inputs(&event, self.modifiers) {
                        let char_event = InputEvent::CharInput(ch);
                        let consumed = match fid.as_ref() {
                            Some(f) => self.app.on_focused_input(f, &char_event),
                            None => false,
                        };
                        if !consumed {
                            self.app.on_input(&char_event);
                        }
                    }
                }
            }

            WindowEvent::Ime(ime_event) => {
                // declarative と同じ形。 以前はここが
                //   `if let Some(fid) = focused_id { on_focused_input(..) }`
                // だけで、 (a) `Ime::Enabled` を組み立てず、 (b) `on_input` への
                // フォールバックも無かった。 その結果フォーカス中の要素が無いと
                // 変換中の文字がどこにも届かず、 ターミナルのような
                // 「フォーカス要素は無いが IME 入力は受ける」 アプリが
                // SceneApp では書けなかった (issue #22)。
                let event = match &ime_event {
                    winit::event::Ime::Preedit(text, cursor) => InputEvent::ImePreedit {
                        text: text.clone(),
                        cursor: cursor.map(|(s, e)| (s, e)),
                    },
                    winit::event::Ime::Commit(text) => InputEvent::ImeCommit {
                        text: text.clone(),
                    },
                    winit::event::Ime::Enabled => InputEvent::ImeEnabled,
                    // `Ime::Disabled` に対応する InputEvent がまだ無い。
                    // 受け手 (FocusManager::on_ime_disabled) はいるので、
                    // variant を足すのは別 issue で。
                    winit::event::Ime::Disabled => return,
                };
                crate::runtime_shared::dispatch(
                    &mut self.app,
                    self.focused_id.as_deref(),
                    &event,
                );
            }

            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.05);
                self.last_frame = now;

                self.app.tick(dt);
                // After tick: let the app reassert its desired focus (e.g. a
                // modal that opens with a known input grabs focus its first
                // frame, without needing a click). 判断は declarative /
                // Harness と同じ 1 実装を通す (#28)。
                if crate::runtime_shared::apply_desired_focus(&self.app, &mut self.focused_id) {
                    self.push_ui_capture();
                }
                // Anchor the platform IME (conversion / candidate window) at
                // the app's caret. Polled every frame but deduped — only
                // re-sent when the caret rect changes. Without this, winit
                // leaves the area at the window origin and the candidate
                // window sits in the top-left.
                let ime_area = self.app.ime_cursor_area();
                if ime_area != self.last_ime_area {
                    self.last_ime_area = ime_area;
                    if let (Some(w), Some((x, y, cw, ch))) =
                        (self.window.as_ref(), ime_area)
                    {
                        w.set_ime_cursor_area(
                            winit::dpi::LogicalPosition::new(x, y),
                            winit::dpi::LogicalSize::new(cw, ch),
                        );
                    }
                }
                // IME on/off per app policy (deduped); disabling cancels an
                // in-flight composition. See DeclarativeApp::ime_allowed.
                let ime_allowed = self.app.ime_allowed();
                if ime_allowed != self.last_ime_allowed {
                    self.last_ime_allowed = ime_allowed;
                    if let Some(w) = self.window.as_ref() {
                        w.set_ime_allowed(ime_allowed);
                    }
                }
                // スクロールの慣性/スプリングを進める。
                for sv in self.scroll_states.values_mut() {
                    sv.tick(dt);
                }
                // Advance style/presence springs. run_scene redraws every
                // frame (about_to_wait always requests one), so no is_animating
                // gating is needed — they simply settle over successive frames.
                self.style_animator.tick(dt);
                self.presence_animator.tick(dt);
                self.tooltip_state.tick(dt);
                self.drag_manager.tick(dt);

                let Some(renderer) = self.renderer.as_mut() else { return };
                let mut tr = match self.text_renderer.take() {
                    Some(t) => t,
                    None => return,
                };

                let scale = renderer.scale_factor;
                let w = renderer.surface_config.width as f32 / scale;
                let h = renderer.surface_config.height as f32 / scale;

                if (w - self.last_viewport_w).abs() > 0.5
                    || (h - self.last_viewport_h).abs() > 0.5
                {
                    self.measure_cache.borrow_mut().clear();
                    self.last_viewport_w = w;
                    self.last_viewport_h = h;
                }

                // Build UI element tree
                let mono_advance =
                    tr.measure_text("0000000000", 100.0, false, true, None, None, None, sabitori_core::Typography::default())
                        .size
                        .width
                        / 1000.0;
                // 計測器は `ViewContext` に差すので、 view を呼ぶ前に作る。
                let measurer = TextRendererMeasurer::new(&mut tr, &self.measure_cache);
                // 管理スクロール状態を ViewContext へ（アプリが scroll_states を読めるように）。
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
                    tooltip: self.tooltip_state.info().map(|(text, x, y)| {
                        sabitori_core::TooltipInfo { text, x, y }
                    }),
                    drag: self.drag_manager.drag_info().map(|(data, source_id)| {
                        let over = self.last_build.as_ref().and_then(|build| {
                            let pt = sabitori_core::Point::new(self.mouse_x, self.mouse_y);
                            build.hit_regions.iter()
                                .find(|r| r.drop_zone && r.rect.contains(pt))
                                .and_then(|r| r.id.clone())
                        });
                        sabitori_core::DragInfo { data, source_id, over_drop_zone: over }
                    }),
                    theme: self.app.theme(),
                    presence: self.presence_animator.all_progress(),
                    // 抱えているテクスチャの量 (#47)。
                    textures: self
                        .image_renderer
                        .as_ref()
                        .map(|r| r.texture_stats())
                        .unwrap_or_default(),
                    // SceneApp doesn't wire up an image runtime yet; callers
                    // can still use `image(key, data)` with their own cache.
                    images: None,
                    mono_advance,
                    // 実フォント計測をアプリに渡す (issue #15)。 計測器は下の
                    // `build_tree_measured` でも使い回す。
                    measurer: Some(&measurer),
                    managed: Default::default(),
            actions: Default::default(),
                };

                let mut root = self.app.view(&ctx);

                // Presence (mount/unmount) animations, then hover/active spring
                // transitions + instant hover styles — same order and calls as
                // the declarative `AppState` redraw path.
                self.presence_animator.update_presence(&root);
                self.presence_animator.apply(&mut root);
                self.style_animator.update(&root, &self.hovered_id, &self.pressed_id);
                self.style_animator.apply(&mut root);
                sabitori_core::element::apply_state_styles(
                    &mut root,
                    &self.hovered_id,
                    &self.pressed_id,
                );

                // `.scroll(id)` のコンテナを登録し、現在の offset を要素へ patch する。
                // declarative と同じ関数を呼ぶ — 以前はこのファイルに逐語コピーが
                // あり、片方だけ直す事故の温床になっていた (issue #14 / #17)。
                crate::scroll_sync::patch_scroll_offsets(&mut root, &mut self.scroll_states);
                let mut build_result = build_tree_measured(&root, w, h, &measurer);
                // 測定したスクロール範囲(viewport/content)を管理状態へ反映 → 次フレームの
                // ホイールが正しい上限でクランプされる（コンテンツ高がここで確定）。
                crate::scroll_sync::apply_scroll_measures(&build_result, &mut self.scroll_states);
                // Apply programmatic scroll requests now that content extents
                // are known, so `smooth_scroll_to` clamps to the real range.
                for (id, y) in self.app.scroll_intents() {
                    if let Some(sv) = self.scroll_states.get_mut(&id) {
                        sv.smooth_scroll_to(y);
                    }
                }
                // Build the overlay tree: external `overlay_view()` + tooltip
                // popup + drag ghost, merged with any internal `.overlay()`
                // subtrees already captured in `build_result.overlay_list`
                // (which the old run_scene path silently dropped).
                let app_overlay = self.app.overlay_view(&ctx);
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
                let mut overlay_parts: Vec<sabitori_core::Element> = Vec::new();
                if let Some(el) = app_overlay { overlay_parts.push(el); }
                if let Some(el) = tooltip_element { overlay_parts.push(el); }
                if let Some(el) = drag_ghost_element { overlay_parts.push(el); }
                let overlay_element = match overlay_parts.len() {
                    0 => None,
                    1 => Some(overlay_parts.into_iter().next().unwrap()),
                    _ => Some(
                        sabitori_core::div()
                            .w(sabitori_core::Dimension::Px(w))
                            .h(sabitori_core::Dimension::Px(h))
                            .children(overlay_parts),
                    ),
                };
                let overlay_build = overlay_element.map(|el| {
                    let measurer = TextRendererMeasurer::new(&mut tr, &self.measure_cache);
                    build_tree_measured(&el, w, h, &measurer)
                });

                // Layered path when there's either an external overlay or an
                // internal `.overlay()` command stream — otherwise the flat
                // single-pass path (cheaper, no extra encoder).
                let has_external = overlay_build.is_some();
                let has_internal = !build_result.overlay_list.commands.is_empty();
                let has_overlay = has_external || has_internal;

                let device = renderer.device.clone();
                let queue = renderer.queue.clone();

                if has_overlay {
                    // Merge the external overlay's draws into the overlay
                    // command stream; keep its hit regions to splice in front.
                    let external_hits = if let Some(ob) = overlay_build {
                        build_result.overlay_list.commands.extend(ob.render_list.commands);
                        ob.hit_regions
                    } else {
                        Vec::new()
                    };
                    let (base_rects, base_lists) =
                        UiDrawLists::extract(&build_result.render_list, &mut tr);
                    let (overlay_rects, overlay_lists) =
                        UiDrawLists::extract(&build_result.overlay_list, &mut tr);
                    // External overlay hits go in front of everything else.
                    let mut merged = build_result;
                    merged.hit_regions.splice(0..0, external_hits);
                    self.last_build = Some(merged);
                    let mut ir = self.image_renderer.take();
                    let mut rr = self.ring_renderer.take();
                    let mut lr = self.line_renderer.take();
                    let _ = renderer.render_scene_then_ui_layered(
                        |scene_ctx| {
                            self.app.render_scene(scene_ctx);
                        },
                        &base_rects,
                        &overlay_rects,
                        // 矩形の数だけでは overlay の有無を判定できない (#44)。
                        !overlay_lists.is_empty(),
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
                    // フレーム境界。テクスチャ LRU の世代をここで進める —
                    // パスごとではなく、全パスを描き終えてから (#43)。
                    if let Some(ir) = ir.as_mut() {
                        ir.end_frame();
                    }
                    self.image_renderer = ir;
                    self.ring_renderer = rr;
                    self.line_renderer = lr;
                } else {
                    let (rects, lists) =
                        UiDrawLists::extract(&build_result.render_list, &mut tr);
                    self.last_build = Some(build_result);
                    let mut ir = self.image_renderer.take();
                    let mut rr = self.ring_renderer.take();
                    let mut lr = self.line_renderer.take();
                    let _ = renderer.render_scene_then_ui(
                        |scene_ctx| {
                            self.app.render_scene(scene_ctx);
                        },
                        &rects,
                        |pass, globals_bg| {
                            let mut r = UiRenderers {
                                images: ir.as_mut(),
                                rings: rr.as_mut(),
                                lines: lr.as_mut(),
                                text: &mut tr,
                            };
                            draw_ui_layer(&mut r, &lists, &device, &queue, pass, globals_bg);
                        },
                    );
                    // フレーム境界。テクスチャ LRU の世代をここで進める —
                    // パスごとではなく、全パスを描き終えてから (#43)。
                    if let Some(ir) = ir.as_mut() {
                        ir.end_frame();
                    }
                    self.image_renderer = ir;
                    self.ring_renderer = rr;
                    self.line_renderer = lr;
                }

                // Notify the app of this frame's build (hit-region rects by
                // id) so floating panels etc. can look themselves up. Set in
                // both branches above before render. (DeclarativeApp::on_build,
                // added in v0.2.5; default no-op.)
                if let Some(ref b) = self.last_build {
                    self.app.on_build(b);
                }

                self.text_renderer = Some(tr);
                // Fresh layout may move UI under the (stationary) pointer.
                self.push_ui_capture();
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = self.window.as_ref() { w.request_redraw(); }
    }
}

/// Run a SceneApp (native).
#[cfg(not(target_arch = "wasm32"))]
pub fn run_scene<A: SceneApp + 'static>(app: A) {
    // ホスト側が既に subscriber を張っていることがある (ファイルログ等)。`init()`
    // はその場合 panic して起動不能になるので、譲る。
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let event_loop = EventLoop::new().unwrap();
    let mut state = SceneAppState {
        app,
        window: None,
        renderer: None,
        text_renderer: None,
        image_renderer: None,
        ring_renderer: None,
        line_renderer: None,
        measure_cache: std::cell::RefCell::new(MeasureCache::new()),
        last_frame: Instant::now(),
        last_build: None,
        mouse_x: 0.0,
        mouse_y: 0.0,
        hovered_id: None,
        pressed_id: None,
        focused_id: None,
        last_cursor: None,
        last_ime_area: None, last_ime_allowed: true,
        modifiers: Modifiers::default(),
        last_viewport_w: 0.0,
        last_viewport_h: 0.0,
        setup_done: false,
        primary_input: PrimaryInput::None,
        active_touches: std::collections::HashMap::new(),
        touch_drag: None,
        pinch: None,
        last_capture: UiCapture::default(),
        scroll_states: std::collections::HashMap::new(),
        style_animator: sabitori_widgets::StyleAnimator::new(),
        presence_animator: sabitori_widgets::PresenceAnimator::new(),
        tooltip_state: sabitori_widgets::TooltipState::new(),
        drag_manager: sabitori_widgets::DragManager::new(),
    };
    event_loop.run_app(&mut state).unwrap();
}

/// Run a SceneApp (WASM).
#[cfg(target_arch = "wasm32")]
pub fn run_scene<A: SceneApp + 'static>(app: A) {
    use std::cell::RefCell;
    use std::rc::Rc;

    console_error_panic_hook::set_once();
    // ホスト側が既に logger を張っていることがある。`init_with_level` はその場合 Err を
    // 返すので、`expect` せず譲る (native 側の `try_init` と同じ方針)。
    let _ = console_log::init_with_level(log::Level::Info);

    let state = Rc::new(RefCell::new(SceneAppState {
        app,
        window: None,
        renderer: None,
        text_renderer: None,
        image_renderer: None,
        ring_renderer: None,
        line_renderer: None,
        measure_cache: std::cell::RefCell::new(MeasureCache::new()),
        last_frame: Instant::now(),
        last_build: None,
        mouse_x: 0.0,
        mouse_y: 0.0,
        hovered_id: None,
        pressed_id: None,
        focused_id: None,
        last_cursor: None,
        last_ime_area: None, last_ime_allowed: true,
        modifiers: Modifiers::default(),
        last_viewport_w: 0.0,
        last_viewport_h: 0.0,
        setup_done: false,
        primary_input: PrimaryInput::None,
        active_touches: std::collections::HashMap::new(),
        touch_drag: None,
        pinch: None,
        last_capture: UiCapture::default(),
        scroll_states: std::collections::HashMap::new(),
        style_animator: sabitori_widgets::StyleAnimator::new(),
        presence_animator: sabitori_widgets::PresenceAnimator::new(),
        tooltip_state: sabitori_widgets::TooltipState::new(),
        drag_manager: sabitori_widgets::DragManager::new(),
    }));

    struct WasmSceneHandler<A: SceneApp> {
        inner: Rc<RefCell<SceneAppState<A>>>,
        renderer_init_started: bool,
    }

    impl<A: SceneApp + 'static> ApplicationHandler for WasmSceneHandler<A> {
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
            // Enable IME so input methods deliver preedit/commit events.
            window.set_ime_allowed(true);

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
                    let size = window.inner_size();
                    if size.width > 1 && size.height > 1 {
                        gpu.resize(size.width, size.height, window.scale_factor());
                    }
                    gpu.create_depth_texture();

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
                    s.app.setup(&gpu.gpu_context());
                    s.setup_done = true;
                    s.image_renderer = Some(sabitori_gpu::ImageRenderer::new(
                        &gpu.device,
                        gpu.surface_config.format,
                        &gpu.globals_bind_group_layout,
                    ));
                    s.renderer = Some(gpu);
                    s.text_renderer = Some(text);
                    log::info!("Scene renderer ready");
                });
            }
        }

        fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
            if let Ok(mut s) = self.inner.try_borrow_mut() {
                s.window_event(event_loop, id, event);
            }
        }

        fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
            if let Ok(mut s) = self.inner.try_borrow_mut() {
                s.about_to_wait(_event_loop);
            }
        }
    }

    let event_loop = EventLoop::new().unwrap();
    let mut handler = WasmSceneHandler {
        inner: state,
        renderer_init_started: false,
    };
    event_loop.run_app(&mut handler).unwrap();
}
