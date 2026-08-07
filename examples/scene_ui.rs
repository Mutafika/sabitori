//! SceneApp ⇄ DeclarativeApp UI-parity demo (issue #25).
//!
//! A custom GPU scene (here just a clear pass) with a declarative UI overlay
//! that exercises the `DeclarativeApp` features `run_scene` now honors — each
//! of which used to compile but silently do nothing under `SceneApp`:
//!
//!   Tier A  ·  `.cursor(Pointer)` OS cursor · `desired_focus()`
//!   Tier B  ·  animated `.hover()` via `.spring_transition(...)`
//!   Tier C  ·  `.tooltip(...)` popup · `overlay_view()` modal ·
//!              `.draggable()` / `.droppable()`
//!
//! Run: `cargo run -p sabitori --example scene_ui`
//!
//! What to try: hover a chip (cursor turns to a hand, background springs,
//! tooltip fades in), drag the 🍎 card into the drop zone (border turns green
//! while hovering it, status updates on drop), click "Open dialog" (a modal
//! renders in the overlay layer and its Close button auto-focuses).

use sabitori::element::*;
use sabitori::*;

#[derive(Default)]
struct SceneUiApp {
    clicks: u32,
    modal_open: bool,
    last_drop: Option<String>,
}

impl DeclarativeApp for SceneUiApp {
    fn title(&self) -> &str {
        "Sabitori — SceneApp UI parity (#25)"
    }
    fn size(&self) -> (f32, f32) {
        (960.0, 640.0)
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        let text_c = Color::from_hex("#e6e9ef");
        let sub = Color::from_hex("#9aa5ce");
        let surface = Color::from_hex("#24283b");
        let surface_hi = Color::from_hex("#2f3549");
        let border_c = Color::from_hex("#414868");
        let primary = Color::from_hex("#7aa2f7");
        let accent = Color::from_hex("#bb9af7");
        let ok = Color::from_hex("#9ece6a");

        // A pointer-cursor, tooltip-bearing, spring-animated-hover chip.
        let chip = |id: &str, label: &str| {
            div()
                .id(id)
                .cursor(Cursor::Pointer)
                .tooltip(format!("“{label}” — cursor + tooltip are both live now"))
                .bg(surface)
                .rounded_px(10.0)
                .border(1.0, primary.with_alpha(0.35))
                .p(Px(12.0))
                .hover(|s| s.bg(surface_hi).border_color(primary))
                .spring_transition(220.0, 22.0)
                .children([text(label).font_size(14.0).color(text_c)])
        };

        // Draggable card.
        let drag_card = div()
            .id("drag-apple")
            .draggable("🍎 apple")
            .cursor(Cursor::Pointer)
            .bg(accent.with_alpha(0.22))
            .rounded_px(10.0)
            .border(1.0, accent)
            .p(Px(14.0))
            .children([text("🍎 Drag me").font_size(15.0).color(text_c)]);

        // Drop zone — border turns green while a drag hovers it.
        let over_bin = ctx
            .drag
            .as_ref()
            .map(|d| d.over_drop_zone.as_deref() == Some("bin"))
            .unwrap_or(false);
        let bin = div()
            .id("bin")
            .droppable()
            .w(Px(220.0))
            .h(Px(110.0))
            .bg(surface)
            .rounded_px(12.0)
            .border(2.0, if over_bin { ok } else { border_c })
            .flex_col()
            .items_center()
            .justify_center()
            .gap(6.0)
            .children([
                text("Drop zone").font_size(13.0).color(sub),
                text(self.last_drop.as_deref().unwrap_or("— nothing dropped —"))
                    .font_size(13.0)
                    .color(if self.last_drop.is_some() { ok } else { sub }),
            ]);

        let open_btn = div()
            .id("open-dialog")
            .cursor(Cursor::Pointer)
            .bg(primary)
            .rounded_px(9.0)
            .p(Px(12.0))
            .hover(|s| s.bg(Color::from_hex("#8ab4f8")))
            .spring_transition(220.0, 22.0)
            .children([text("Open dialog").font_size(14.0).color(Color::from_hex("#11131c"))]);

        div()
            .w(Px(ctx.width))
            .h(Px(ctx.height))
            .flex_col()
            .items_center()
            .justify_center()
            .gap(22.0)
            .children([
                text("SceneApp UI parity")
                    .font_size(30.0)
                    .color(text_c)
                    .bold(),
                text("These declarative features now work over a custom GPU scene")
                    .font_size(14.0)
                    .color(sub),
                div().flex_row().gap(12.0).children([
                    chip("tool-build", "Build"),
                    chip("tool-test", "Test"),
                    chip("tool-ship", "Ship"),
                ]),
                text(format!("chips clicked: {}", self.clicks))
                    .font_size(13.0)
                    .color(sub),
                div()
                    .flex_row()
                    .gap(20.0)
                    .items_center()
                    .children([drag_card, bin]),
                open_btn,
            ])
    }

    // Tier C: renders in the overlay layer (was silently dropped in run_scene).
    fn overlay_view(&self, ctx: &ViewContext) -> Option<Element> {
        if !self.modal_open {
            return None;
        }
        let dialog = div()
            .bg(Color::from_hex("#1f2335"))
            .rounded_px(16.0)
            .border(1.0, Color::from_hex("#7aa2f7"))
            .p(Px(24.0))
            .gap(16.0)
            .flex_col()
            .children([
                text("Overlay dialog")
                    .font_size(20.0)
                    .color(Color::from_hex("#e6e9ef"))
                    .bold(),
                text("overlay_view() draws here; desired_focus() grabbed Close on open.")
                    .font_size(13.0)
                    .color(Color::from_hex("#9aa5ce")),
                div()
                    .id("close-dialog")
                    .cursor(Cursor::Pointer)
                    .bg(Color::from_hex("#7aa2f7"))
                    .rounded_px(8.0)
                    .p(Px(12.0))
                    .hover(|s| s.bg(Color::from_hex("#8ab4f8")))
                    .spring_transition(220.0, 22.0)
                    .children([text("Close").font_size(14.0).color(Color::from_hex("#11131c"))]),
            ]);
        // Full-viewport dimmed backdrop, dialog centered.
        Some(
            div()
                .w(Px(ctx.width))
                .h(Px(ctx.height))
                .bg(Color::new(0.0, 0.0, 0.0, 0.5))
                .flex_col()
                .items_center()
                .justify_center()
                .children([dialog]),
        )
    }

    // Tier A: the app asks the runtime to focus Close whenever the modal opens.
    fn desired_focus(&self) -> Option<String> {
        if self.modal_open {
            Some("close-dialog".to_string())
        } else {
            None
        }
    }

    fn on_click(&mut self, id: &str) {
        match id {
            "open-dialog" => self.modal_open = true,
            "close-dialog" => self.modal_open = false,
            _ if id.starts_with("tool-") => self.clicks += 1,
            _ => {}
        }
    }

    fn on_drop(&mut self, data: &str, target_id: &str) {
        self.last_drop = Some(format!("{data} → #{target_id}"));
    }
}

impl SceneApp for SceneUiApp {
    fn setup(&mut self, _ctx: &GpuContext) {}

    // The app owns its scene pass: here, just clear color + depth. The UI
    // overlay is drawn on top afterwards by the run_scene runtime.
    fn render_scene(&mut self, ctx: &mut SceneRenderContext) {
        let _pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("scene-clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.surface_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.02,
                        g: 0.03,
                        b: 0.06,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: ctx.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
}

fn main() {
    run_scene(SceneUiApp::default());
}
