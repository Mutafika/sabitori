use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use sabitori_core::Point;
use sabitori_gpu::{GpuRenderer, RectInstance};
use sabitori_input::{
    button_bit, ActivePointer, BUTTON_PRIMARY, InputEvent, MouseButton,
    PointerKind, PointerState, MOUSE_POINTER_ID,
};
use sabitori_scene::{NodeId, NodeTree};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Ime, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::ModifiersState;
use winit::window::{CursorIcon, Window, WindowAttributes, WindowId};

pub mod keymap;

/// Trait for building interactive UIs with Sabitori.
pub trait SabitoriApp {
    /// Build the node tree. Called once at startup and can be called again to rebuild.
    fn build(&self, tree: &mut NodeTree, width: f32, height: f32);

    /// Convert the node tree to render instances.
    fn render(&self, tree: &NodeTree) -> Vec<RectInstance>;

    /// Called when a node is clicked.
    fn on_click(&mut self, _id: NodeId) {}

    /// Called for keyboard, IME, and character input events.
    /// Return `true` if the event was handled (consumed).
    fn on_input(&mut self, _event: &InputEvent) -> bool {
        false
    }
}

/// Which input modality owns the primary-pointer flow. First-come wins.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PrimaryInput {
    None,
    Mouse,
    Touch,
}

struct AppState<A: SabitoriApp> {
    app: A,
    window: Option<Arc<Window>>,
    renderer: Option<GpuRenderer>,
    tree: NodeTree,
    pointer: PointerState,
    pressed_node: Option<NodeId>,
    #[cfg(not(target_arch = "wasm32"))]
    last_frame: Instant,
    #[cfg(target_arch = "wasm32")]
    last_frame_ms: f64,
    needs_rebuild: bool,
    cursor_icon: CursorIcon,
    /// Current modifier key state, updated on every key event.
    winit_modifiers: ModifiersState,
    /// Mouse/touch mutex. When `Mouse`, incoming touch events skip the primary
    /// flow (raw events still forward). When `Touch`, mouse press skips.
    primary_input: PrimaryInput,
}

impl<A: SabitoriApp> ApplicationHandler for AppState<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title("Sabitori")
            .with_inner_size(winit::dpi::LogicalSize::new(1200.0, 800.0));

        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("Failed to create window"),
        );

        // On native, create the renderer synchronously via pollster::block_on.
        // On WASM, this `resumed()` impl is NOT used directly — the
        // WasmAppHandler wrapper handles `resumed()` and spawns an async
        // renderer init instead. See the `#[cfg(target_arch = "wasm32")] pub fn run()`
        // function below.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let renderer = GpuRenderer::new(window.clone());
            self.renderer = Some(renderer);
        }

        self.window = Some(window);
        self.needs_rebuild = true;
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let (Some(window), Some(renderer)) =
                    (self.window.as_ref(), self.renderer.as_mut())
                {
                    renderer.resize(size.width, size.height, window.scale_factor());
                    self.needs_rebuild = true;
                }
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                if let (Some(window), Some(renderer)) =
                    (self.window.as_ref(), self.renderer.as_mut())
                {
                    let size = window.inner_size();
                    renderer.resize(size.width, size.height, window.scale_factor());
                    self.needs_rebuild = true;
                }
            }

            // Mouse events
            WindowEvent::CursorMoved { position, .. } => {
                if self.primary_input == PrimaryInput::Touch {
                    return;
                }
                if let Some(renderer) = self.renderer.as_ref() {
                    let scale = renderer.scale_factor;
                    let pos = Point::new(position.x as f32 / scale, position.y as f32 / scale);
                    self.pointer.mouse_position = pos;
                    self.pointer.inside_window = true;
                    // If a mouse button is held, update the active pointer entry too.
                    if let Some(existing) = self
                        .pointer
                        .active
                        .iter()
                        .find(|p| p.id == MOUSE_POINTER_ID)
                        .copied()
                    {
                        self.pointer.upsert(ActivePointer {
                            position: pos,
                            ..existing
                        });
                    }
                    self.process_event(InputEvent::PointerMoved {
                        id: MOUSE_POINTER_ID,
                        kind: PointerKind::Mouse,
                        position: pos,
                        modifiers: keymap::modifiers_from_winit(self.winit_modifiers),
                    });
                }
            }
            WindowEvent::CursorLeft { .. } => {
                if self.primary_input == PrimaryInput::Touch {
                    return;
                }
                self.pointer.inside_window = false;
                self.process_event(InputEvent::PointerLeft);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if self.primary_input == PrimaryInput::Touch {
                    return;
                }
                let btn = match button {
                    winit::event::MouseButton::Left => MouseButton::Left,
                    winit::event::MouseButton::Right => MouseButton::Right,
                    winit::event::MouseButton::Middle => MouseButton::Middle,
                    _ => return,
                };
                let pos = self.pointer.mouse_position;
                let bit = button_bit(btn);
                match state {
                    ElementState::Pressed => {
                        if btn == MouseButton::Left
                            && self.primary_input == PrimaryInput::None
                        {
                            self.primary_input = PrimaryInput::Mouse;
                        }
                        let prev = self
                            .pointer
                            .find(MOUSE_POINTER_ID)
                            .map(|p| p.buttons)
                            .unwrap_or(0);
                        self.pointer.upsert(ActivePointer {
                            id: MOUSE_POINTER_ID,
                            kind: PointerKind::Mouse,
                            position: pos,
                            buttons: prev | bit,
                        });
                        self.process_event(InputEvent::PointerPressed {
                            id: MOUSE_POINTER_ID,
                            kind: PointerKind::Mouse,
                            position: pos,
                            button: Some(btn),
                            modifiers: keymap::modifiers_from_winit(self.winit_modifiers),
                        });
                    }
                    ElementState::Released => {
                        let remaining = self
                            .pointer
                            .find(MOUSE_POINTER_ID)
                            .map(|p| p.buttons & !bit)
                            .unwrap_or(0);
                        if remaining == 0 {
                            self.pointer.remove(MOUSE_POINTER_ID);
                        } else {
                            self.pointer.upsert(ActivePointer {
                                id: MOUSE_POINTER_ID,
                                kind: PointerKind::Mouse,
                                position: pos,
                                buttons: remaining,
                            });
                        }
                        self.process_event(InputEvent::PointerReleased {
                            id: MOUSE_POINTER_ID,
                            kind: PointerKind::Mouse,
                            position: pos,
                            button: Some(btn),
                            modifiers: keymap::modifiers_from_winit(self.winit_modifiers),
                        });
                        if btn == MouseButton::Left
                            && self.primary_input == PrimaryInput::Mouse
                        {
                            self.primary_input = PrimaryInput::None;
                        }
                    }
                }
            }

            // Touch events (mobile / touchscreen).
            WindowEvent::Touch(touch) => {
                if self.primary_input == PrimaryInput::Mouse {
                    return;
                }
                let Some(renderer) = self.renderer.as_ref() else { return };
                let scale = renderer.scale_factor;
                let pos = Point::new(
                    touch.location.x as f32 / scale,
                    touch.location.y as f32 / scale,
                );
                // Shift winit's u64 touch id above MOUSE_POINTER_ID to avoid collision.
                let id = touch.id.saturating_add(1);
                match touch.phase {
                    TouchPhase::Started => {
                        if self.primary_input == PrimaryInput::None {
                            self.primary_input = PrimaryInput::Touch;
                        }
                        self.pointer.upsert(ActivePointer {
                            id,
                            kind: PointerKind::Touch,
                            position: pos,
                            buttons: sabitori_input::BUTTON_PRIMARY,
                        });
                        self.process_event(InputEvent::PointerPressed {
                            id,
                            kind: PointerKind::Touch,
                            position: pos,
                            button: None,
                            modifiers: keymap::modifiers_from_winit(self.winit_modifiers),
                        });
                    }
                    TouchPhase::Moved => {
                        if let Some(existing) = self.pointer.find(id).copied() {
                            self.pointer.upsert(ActivePointer {
                                position: pos,
                                ..existing
                            });
                        }
                        self.process_event(InputEvent::PointerMoved {
                            id,
                            kind: PointerKind::Touch,
                            position: pos,
                            modifiers: keymap::modifiers_from_winit(self.winit_modifiers),
                        });
                    }
                    TouchPhase::Ended => {
                        self.pointer.remove(id);
                        self.process_event(InputEvent::PointerReleased {
                            id,
                            kind: PointerKind::Touch,
                            position: pos,
                            button: None,
                            modifiers: keymap::modifiers_from_winit(self.winit_modifiers),
                        });
                        // Release ownership when the last finger lifts.
                        if self.primary_input == PrimaryInput::Touch
                            && !self
                                .pointer
                                .active
                                .iter()
                                .any(|p| p.kind == PointerKind::Touch)
                        {
                            self.primary_input = PrimaryInput::None;
                        }
                    }
                    TouchPhase::Cancelled => {
                        self.pointer.remove(id);
                        self.process_event(InputEvent::PointerCancelled {
                            id,
                            kind: PointerKind::Touch,
                        });
                        if self.primary_input == PrimaryInput::Touch
                            && !self
                                .pointer
                                .active
                                .iter()
                                .any(|p| p.kind == PointerKind::Touch)
                        {
                            self.primary_input = PrimaryInput::None;
                        }
                    }
                }
            }

            // Track modifier keys
            WindowEvent::ModifiersChanged(mods) => {
                self.winit_modifiers = mods.state();
                // 変化後の値をアプリへ配る (3 ランタイム共通)。
                self.process_event(InputEvent::ModifiersChanged(
                    keymap::modifiers_from_winit(self.winit_modifiers),
                ));
            }

            // IME events
            WindowEvent::Ime(ime) => {
                let input_event = match ime {
                    Ime::Enabled => InputEvent::ImeEnabled,
                    Ime::Preedit(text, cursor) => InputEvent::ImePreedit { text, cursor },
                    Ime::Commit(text) => InputEvent::ImeCommit { text },
                    Ime::Disabled => {
                        // Send an empty preedit to clear any composing state
                        InputEvent::ImePreedit {
                            text: String::new(),
                            cursor: None,
                        }
                    }
                };
                self.process_event(input_event);
            }

            // Keyboard input
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                let modifiers = keymap::modifiers_from_winit(self.winit_modifiers);

                // 対応する Key を持たない名前付きキーはイベントを出さない。
                if let Some(key) = keymap::key_from_winit(&event.logical_key) {
                    self.process_event(InputEvent::KeyInput {
                        key,
                        pressed,
                        modifiers,
                    });
                }

                for ch in keymap::char_inputs(&event, modifiers) {
                    self.process_event(InputEvent::CharInput(ch));
                }
            }

            WindowEvent::RedrawRequested => {
                #[cfg(not(target_arch = "wasm32"))]
                let dt = {
                    let now = Instant::now();
                    let dt = (now - self.last_frame).as_secs_f32().min(0.05);
                    self.last_frame = now;
                    dt
                };

                #[cfg(target_arch = "wasm32")]
                let dt = {
                    let now = web_sys::window()
                        .and_then(|w| w.performance())
                        .map(|p| p.now())
                        .unwrap_or(0.0);
                    let dt = if self.last_frame_ms > 0.0 {
                        ((now - self.last_frame_ms) / 1000.0) as f32
                    } else {
                        0.016 // ~60fps for first frame
                    };
                    self.last_frame_ms = now;
                    dt.min(0.05)
                };

                if let Some(renderer) = self.renderer.as_mut() {
                    // Rebuild tree if needed
                    if self.needs_rebuild {
                        let logical_w =
                            renderer.surface_config.width as f32 / renderer.scale_factor;
                        let logical_h =
                            renderer.surface_config.height as f32 / renderer.scale_factor;
                        self.tree = NodeTree::new();
                        self.app.build(&mut self.tree, logical_w, logical_h);
                        self.needs_rebuild = false;
                    }

                    // Animate
                    self.tree.animate(dt);

                    // Render
                    let rects = self.app.render(&self.tree);
                    match renderer.render(&rects) {
                        Ok(()) => {}
                        Err(wgpu::SurfaceError::Lost) => {
                            let config = renderer.surface_config.clone();
                            renderer
                                .resize(config.width, config.height, renderer.scale_factor as f64);
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            event_loop.exit();
                        }
                        Err(e) => {
                            tracing::warn!("Surface error: {e:?}");
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

impl<A: SabitoriApp> AppState<A> {
    /// Set the renderer after async initialization (used on WASM).
    #[cfg(target_arch = "wasm32")]
    fn set_renderer(&mut self, renderer: GpuRenderer) {
        self.renderer = Some(renderer);
        self.needs_rebuild = true;
    }

    fn process_event(&mut self, event: InputEvent) {
        // Let the app handle keyboard / IME events first.
        match &event {
            InputEvent::ImeEnabled
            | InputEvent::ImePreedit { .. }
            | InputEvent::ImeCommit { .. }
            | InputEvent::KeyInput { .. }
            | InputEvent::CharInput(_)
            // 修飾キーの変化もキーボード系。 ここに入れないと下の pointer 用 match の
            // `_ => {}` に落ちて、 app へ一度も届かない。
            | InputEvent::ModifiersChanged(_) => {
                self.app.on_input(&event);
                self.needs_rebuild = true;
                return;
            }
            _ => {}
        }

        // Treat a press as "primary" when it's either a mouse left-button or any touch/pen contact.
        let is_primary_press = |button: Option<MouseButton>, kind: PointerKind| -> bool {
            match kind {
                PointerKind::Mouse => matches!(button, Some(MouseButton::Left)),
                PointerKind::Touch | PointerKind::Pen => true,
            }
        };

        match event {
            InputEvent::PointerMoved { kind, position, .. } => {
                let hit = self.tree.hit_test(position);
                // Hover only applies to mouse (touch has no hover concept).
                if matches!(kind, PointerKind::Mouse) {
                    self.tree.update_hover(hit);
                    let new_cursor = if hit.is_some() {
                        CursorIcon::Pointer
                    } else {
                        CursorIcon::Default
                    };
                    if new_cursor != self.cursor_icon {
                        self.cursor_icon = new_cursor;
                        if let Some(window) = self.window.as_ref() {
                            window.set_cursor(new_cursor);
                        }
                    }
                }

                // Update pressed-node drag tracking while the primary contact is held.
                if self.pointer.primary_pressed() {
                    if let Some(pressed_id) = self.pressed_node {
                        let still_over = self
                            .tree
                            .nodes
                            .get(pressed_id)
                            .is_some_and(|n| n.hit_test(position));
                        self.tree.set_pressed(
                            if still_over {
                                Some(pressed_id)
                            } else {
                                None
                            },
                            true,
                        );
                    }
                }
            }
            InputEvent::PointerPressed {
                position,
                button,
                kind,
                ..
            } if is_primary_press(button, kind) => {
                let hit = self.tree.hit_test(position);
                self.pressed_node = hit;
                if let Some(id) = hit {
                    self.tree.set_pressed(Some(id), true);
                }
            }
            InputEvent::PointerReleased {
                position,
                button,
                kind,
                ..
            } if is_primary_press(button, kind) => {
                if let Some(pressed_id) = self.pressed_node.take() {
                    let still_over = self
                        .tree
                        .nodes
                        .get(pressed_id)
                        .is_some_and(|n| n.hit_test(position));
                    if still_over {
                        self.tree.fire_click(pressed_id);
                        self.app.on_click(pressed_id);
                    }
                }
                self.tree.set_pressed(None, false);
            }
            InputEvent::PointerCancelled { .. } => {
                // Treat cancellation like a release that missed the target.
                self.pressed_node = None;
                self.tree.set_pressed(None, false);
            }
            InputEvent::PointerLeft => {
                self.tree.update_hover(None);
                self.cursor_icon = CursorIcon::Default;
                if let Some(window) = self.window.as_ref() {
                    window.set_cursor(CursorIcon::Default);
                }
            }
            _ => {}
        }
    }
}

/// Run the application (native/desktop).
#[cfg(not(target_arch = "wasm32"))]
pub fn run<A: SabitoriApp + 'static>(app: A) {
    // ホスト側が既に subscriber を張っていることがある (ファイルログ等)。`init()`
    // はその場合 panic して起動不能になるので、譲る。
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut state = AppState {
        app,
        window: None,
        renderer: None,
        tree: NodeTree::new(),
        pointer: PointerState::default(),
        pressed_node: None,
        last_frame: Instant::now(),
        needs_rebuild: true,
        cursor_icon: CursorIcon::Default,
        winit_modifiers: ModifiersState::empty(),
        primary_input: PrimaryInput::None,
    };
    event_loop.run_app(&mut state).expect("Event loop failed");
}

/// Run the application (WASM).
///
/// On WASM, the event loop uses requestAnimationFrame internally.
/// The GPU renderer is initialized asynchronously inside `resumed()`
/// via `wasm_bindgen_futures::spawn_local`. Until the renderer is ready,
/// the window will be visible but frames will be skipped.
#[cfg(target_arch = "wasm32")]
pub fn run<A: SabitoriApp + 'static>(app: A) {
    use std::cell::RefCell;
    use std::rc::Rc;

    console_error_panic_hook::set_once();
    // ホスト側が既に logger を張っていることがある。`init_with_level` はその場合 Err を
    // 返すので、`expect` せず譲る (native 側の `try_init` と同じ方針)。
    let _ = console_log::init_with_level(log::Level::Info);

    // We need to handle async renderer init. The strategy:
    // 1. Create a wrapper that owns the AppState in Rc<RefCell<>>
    // 2. In resumed(), create the window & canvas, then spawn async renderer init
    // 3. The async init sets the renderer on the shared state
    let state = Rc::new(RefCell::new(AppState {
        app,
        window: None,
        renderer: None,
        tree: NodeTree::new(),
        pointer: PointerState::default(),
        pressed_node: None,
        last_frame_ms: 0.0,
        needs_rebuild: true,
        cursor_icon: CursorIcon::Default,
        winit_modifiers: ModifiersState::empty(),
        primary_input: PrimaryInput::None,
    }));

    // Wrapper that delegates to the inner AppState and handles async renderer init
    struct WasmAppHandler<A: SabitoriApp> {
        inner: Rc<RefCell<AppState<A>>>,
        renderer_init_started: bool,
    }

    impl<A: SabitoriApp + 'static> ApplicationHandler for WasmAppHandler<A> {
        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            {
                let state = self.inner.borrow();
                if state.window.is_some() {
                    return;
                }
            }

            let attrs = WindowAttributes::default()
                .with_title("Sabitori")
                .with_inner_size(winit::dpi::LogicalSize::new(1200.0, 800.0));

            let window = Arc::new(
                event_loop
                    .create_window(attrs)
                    .expect("Failed to create window"),
            );

            // Attach canvas to DOM
            {
                use winit::platform::web::WindowExtWebSys;
                let canvas = window.canvas().expect("Failed to get canvas from window");
                canvas.set_id("sabitori-canvas");
                canvas
                    .style()
                    .set_css_text("width: 100%; height: 100%; display: block;");

                let web_window = web_sys::window().expect("No global window");
                let document = web_window.document().expect("No document");
                let body = document.body().expect("No body");
                body.append_child(&canvas).expect("Failed to append canvas");
            }

            {
                let mut state = self.inner.borrow_mut();
                state.window = Some(window.clone());
                state.needs_rebuild = true;
            }

            // Spawn async renderer initialization
            if !self.renderer_init_started {
                self.renderer_init_started = true;
                let inner = Rc::clone(&self.inner);
                wasm_bindgen_futures::spawn_local(async move {
                    let renderer = GpuRenderer::new_async(window).await;
                    let mut state = inner.borrow_mut();
                    state.set_renderer(renderer);
                });
            }
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            id: WindowId,
            event: WindowEvent,
        ) {
            self.inner.borrow_mut().window_event(event_loop, id, event);
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            self.inner.borrow_mut().about_to_wait(event_loop);
        }
    }

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut handler = WasmAppHandler {
        inner: state,
        renderer_init_started: false,
    };
    event_loop
        .run_app(&mut handler)
        .expect("Event loop failed");
}

// ═══════════════════════════════════════════════════════════════
//  EmbeddedRunner — winit なしで sabitori UI を駆動する
// ═══════════════════════════════════════════════════════════════

/// プラグインウィンドウ等に sabitori を埋め込むためのランナー。
/// winit のイベントループを使わず、外部から入力イベント注入 + フレーム描画を行う。
pub struct EmbeddedRunner<A: SabitoriApp> {
    pub app: A,
    renderer: GpuRenderer,
    tree: NodeTree,
    pointer: PointerState,
    pressed_node: Option<NodeId>,
    needs_rebuild: bool,
    #[cfg(not(target_arch = "wasm32"))]
    last_frame: Instant,
}

impl<A: SabitoriApp> EmbeddedRunner<A> {
    /// raw ウィンドウハンドルから EmbeddedRunner を構築。
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(
        app: A,
        surface_target: wgpu::SurfaceTargetUnsafe,
        width: u32,
        height: u32,
        scale_factor: f32,
    ) -> Self {
        let renderer = GpuRenderer::new_from_raw(surface_target, width, height, scale_factor);
        Self {
            app,
            renderer,
            tree: NodeTree::new(),
            pointer: PointerState::default(),
            pressed_node: None,
            needs_rebuild: true,
            last_frame: Instant::now(),
        }
    }

    /// 入力イベントを注入する。
    pub fn inject_event(&mut self, event: InputEvent) {
        // キーボード / IME は直接 app に渡す
        match &event {
            InputEvent::ImeEnabled
            | InputEvent::ImePreedit { .. }
            | InputEvent::ImeCommit { .. }
            | InputEvent::KeyInput { .. }
            | InputEvent::CharInput(_)
            // 修飾キーの変化もキーボード系。 ここに入れないと下の pointer 用 match の
            // `_ => {}` に落ちて、 app へ一度も届かない。
            | InputEvent::ModifiersChanged(_) => {
                self.app.on_input(&event);
                self.needs_rebuild = true;
                return;
            }
            _ => {}
        }

        match event {
            InputEvent::PointerMoved { id, kind, position, .. } => {
                if kind == PointerKind::Mouse {
                    self.pointer.mouse_position = position;
                    self.pointer.inside_window = true;
                }
                if let Some(existing) = self.pointer.find(id).copied() {
                    self.pointer.upsert(ActivePointer { position, ..existing });
                }
                let hit = self.tree.hit_test(position);
                self.tree.update_hover(hit);

                if self.pointer.primary_pressed() {
                    if let Some(pressed_id) = self.pressed_node {
                        let still_over = self
                            .tree
                            .nodes
                            .get(pressed_id)
                            .is_some_and(|n| n.hit_test(position));
                        self.tree.set_pressed(
                            if still_over { Some(pressed_id) } else { None },
                            true,
                        );
                    }
                }
            }
            InputEvent::PointerPressed {
                id,
                kind,
                position,
                button,
                ..
            } => {
                let is_primary =
                    button == Some(MouseButton::Left) || kind != PointerKind::Mouse;
                if !is_primary {
                    return;
                }
                self.pointer.upsert(ActivePointer {
                    id,
                    kind,
                    position,
                    buttons: BUTTON_PRIMARY,
                });
                let hit = self.tree.hit_test(position);
                self.pressed_node = hit;
                if let Some(node_id) = hit {
                    self.tree.set_pressed(Some(node_id), true);
                }
            }
            InputEvent::PointerReleased {
                id,
                kind: _,
                position,
                button,
                ..
            } => {
                let is_primary = button == Some(MouseButton::Left) || button.is_none();
                if !is_primary {
                    return;
                }
                self.pointer.remove(id);
                if let Some(pressed_id) = self.pressed_node.take() {
                    let still_over = self
                        .tree
                        .nodes
                        .get(pressed_id)
                        .is_some_and(|n| n.hit_test(position));
                    if still_over {
                        self.tree.fire_click(pressed_id);
                        self.app.on_click(pressed_id);
                    }
                }
                self.tree.set_pressed(None, false);
            }
            InputEvent::PointerLeft => {
                self.pointer.inside_window = false;
                self.tree.update_hover(None);
            }
            _ => {}
        }
        self.needs_rebuild = true;
    }

    /// 1フレーム描画する。ホスト側のタイマーから呼び出す。
    pub fn render_frame(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        let dt = {
            let now = Instant::now();
            let dt = (now - self.last_frame).as_secs_f32().min(0.05);
            self.last_frame = now;
            dt
        };
        #[cfg(target_arch = "wasm32")]
        let dt = 1.0 / 60.0_f32;

        if self.needs_rebuild {
            let logical_w =
                self.renderer.surface_config.width as f32 / self.renderer.scale_factor;
            let logical_h =
                self.renderer.surface_config.height as f32 / self.renderer.scale_factor;
            self.tree = NodeTree::new();
            self.app.build(&mut self.tree, logical_w, logical_h);
            self.needs_rebuild = false;
        }

        self.tree.animate(dt);

        let rects = self.app.render(&self.tree);
        match self.renderer.render(&rects) {
            Ok(()) => {}
            Err(wgpu::SurfaceError::Lost) => {
                let config = self.renderer.surface_config.clone();
                self.renderer
                    .resize(config.width, config.height, self.renderer.scale_factor as f64);
            }
            Err(e) => {
                tracing::warn!("Embedded surface error: {e:?}");
            }
        }
    }

    /// リサイズ通知。
    pub fn resize(&mut self, width: u32, height: u32, scale_factor: f64) {
        self.renderer.resize(width, height, scale_factor);
        self.needs_rebuild = true;
    }

    /// リビルドを強制する (パラメータ変更時など)。
    pub fn request_rebuild(&mut self) {
        self.needs_rebuild = true;
    }
}
