//! LP pattern — **Gravity well**. Interactive orbital physics, not keyframes.
//!
//! Ten labeled feature-chips orbit the wordmark, each a little body with
//! position + velocity. Every `tick()` they spring toward a slowly-rotating
//! anchor on their ring AND get pulled toward the cursor with an inverse-square
//! force — so moving the mouse gathers them into a swarm, and they spring back
//! to their orbits when you leave. The cursor is read via `on_pointer_move`
//! (which `tick` has no access to otherwise). CSS transitions can't express a
//! live n-body force field driven by the pointer.
//!
//! `cargo run --example landing_gravity`

use sabitori::*;
use std::cell::Cell;

const W: f32 = 1100.0;
const H: f32 = 760.0;
const CX: f32 = W / 2.0;
const CY: f32 = H / 2.0 - 10.0;

// label, hex, ring radius, angular speed, phase
const CHIPS: [(&str, &str, f32, f32, f32); 10] = [
    ("flexbox", "#7aa2f7", 205.0, 0.30, 0.00),
    ("springs", "#bb9af7", 300.0, -0.22, 0.63),
    ("SDF glow", "#7dcfff", 205.0, 0.30, 1.26),
    ("wasm", "#9ece6a", 300.0, -0.22, 1.90),
    ("markdown", "#e0af68", 205.0, 0.30, 2.51),
    ("60 fps", "#f7768e", 300.0, -0.22, 3.14),
    ("taffy", "#7aa2f7", 205.0, 0.30, 3.77),
    ("gradients", "#bb9af7", 300.0, -0.22, 4.40),
    ("shadows", "#7dcfff", 205.0, 0.30, 5.03),
    ("cosmic-text", "#9ece6a", 300.0, -0.22, 5.65),
];

fn hx(s: &str) -> Color {
    Color::from_hex(s)
}

fn anchor(i: usize, t: f32) -> (f32, f32) {
    let (_, _, r, sp, ph) = CHIPS[i];
    (CX + (sp * t + ph).cos() * r, CY + (sp * t + ph).sin() * r)
}

struct Gravity {
    t: f32,
    mx: f32,
    my: f32,
    /// One body per chip: [x, y, vx, vy] in pixel space.
    bodies: Vec<[f32; 4]>,
    /// Offset of the fixed-size design canvas within the (resizable) window,
    /// stashed each frame in `view()` so `on_pointer_move` maps back correctly.
    ox: Cell<f32>,
    oy: Cell<f32>,
}

impl Gravity {
    fn new() -> Self {
        let bodies = (0..CHIPS.len())
            .map(|i| {
                let (x, y) = anchor(i, 0.0);
                [x, y, 0.0, 0.0]
            })
            .collect();
        // Park the cursor far off-screen so the resting state is clean orbits;
        // the swarm only reacts once the pointer actually enters the window.
        Gravity { t: 0.0, mx: -3000.0, my: -3000.0, bodies, ox: Cell::new(0.0), oy: Cell::new(0.0) }
    }
}

impl DeclarativeApp for Gravity {
    fn title(&self) -> &str {
        "Sabitori — Gravity"
    }
    fn size(&self) -> (f32, f32) {
        (W, H)
    }
    fn is_animating(&self) -> bool {
        true
    }

    fn on_pointer_move(&mut self, x: f32, y: f32) {
        // Map the window cursor into the centered design canvas.
        self.mx = x - self.ox.get();
        self.my = y - self.oy.get();
    }

    fn tick(&mut self, dt: f32) {
        let dt = dt.min(0.05);
        self.t += dt;
        let (t, mx, my) = (self.t, self.mx, self.my);
        for (i, b) in self.bodies.iter_mut().enumerate() {
            let (ax, ay) = anchor(i, t);
            // Spring back to the rotating orbit anchor…
            let mut fx = (ax - b[0]) * 5.0;
            let mut fy = (ay - b[1]) * 5.0;
            // …plus inverse-square pull toward the cursor (the gravity well).
            let dx = mx - b[0];
            let dy = my - b[1];
            let d2 = dx * dx + dy * dy + 600.0;
            let g = 120_000.0 / d2;
            fx += dx * g;
            fy += dy * g;
            // Semi-implicit Euler with velocity damping.
            b[2] = (b[2] + fx * dt) * (1.0 - 4.0 * dt);
            b[3] = (b[3] + fy * dt) * (1.0 - 4.0 * dt);
            b[0] += b[2] * dt;
            b[1] += b[3] * dt;
        }
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        // Center the fixed 1100×760 design canvas in the current window, and
        // remember the offset so pointer input maps back correctly.
        self.ox.set((ctx.width - W) / 2.0);
        self.oy.set((ctx.height - H) / 2.0);

        let mut kids: Vec<Element> = Vec::new();

        // Faint orbit rings.
        for r in [205.0_f32, 300.0] {
            kids.push(
                div()
                    .absolute()
                    .pos(CX - r, CY - r)
                    .w(Px(r * 2.0))
                    .h(Px(r * 2.0))
                    .rounded_px(r)
                    .border(1.0, hx("#20263f")),
            );
        }

        // Central gravity glow.
        kids.push(
            div()
                .absolute()
                .pos(CX - 40.0, CY - 40.0)
                .w(Px(80.0))
                .h(Px(80.0))
                .rounded_px(40.0)
                .bg(hx("#7aa2f7"))
                .opacity(0.10)
                .glow(hx("#7aa2f7"), 120.0),
        );

        // Orbiting chips.
        for (i, b) in self.bodies.iter().enumerate() {
            let (label, hexc, _, _, _) = CHIPS[i];
            let col = hx(hexc);
            kids.push(
                div()
                    .absolute()
                    .pos(b[0] - 62.0, b[1] - 16.0)
                    .w(Px(124.0))
                    .h(Px(32.0))
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .gap(7.0)
                    .rounded_px(13.0)
                    .bg(hx("#141830"))
                    .border(1.0, col)
                    .glow_sm(col)
                    .children([
                        div().w(Px(7.0)).h(Px(7.0)).rounded_px(3.5).bg(col),
                        text(label)
                            .font_size(12.5)
                            .font_weight(600)
                            .letter_spacing(0.2)
                            .color(hx("#eef1ff")),
                    ]),
            );
        }

        // Hero — horizontally centered, sitting on the gravity center.
        kids.push(
            div()
                .absolute()
                .pos(0.0, CY - 66.0)
                .w(Px(W))
                .flex_col()
                .items_center()
                .gap(12.0)
                .children([
                    text("sabitori")
                        .font_size(60.0)
                        .font_weight(300)
                        .letter_spacing(-0.5)
                        .color(hx("#f2f5ff")),
                    text("Move your cursor — the field responds.")
                        .font_size(15.0)
                        .line_height(1.5)
                        .color(hx("#a7afd6")),
                ]),
        );

        // Footer hint.
        kids.push(
            div().absolute().pos(0.0, H - 46.0).w(Px(W)).flex_row().justify_center().children([
                text("REAL N-BODY PHYSICS · NO CSS · NO CANVAS")
                    .font_size(10.5)
                    .font_weight(600)
                    .letter_spacing(1.8)
                    .color(hx("#4a5178")),
            ]),
        );

        // Fixed-size design canvas; its absolute children are relative to it,
        // so centering it moves the whole composition as a unit.
        let poster = div().w(Px(W)).h(Px(H)).children(kids);

        // Full-window backdrop that fills any resize; poster floats centered.
        div()
            .w(Px(ctx.width))
            .h(Px(ctx.height))
            .gradient(hx("#080a14"), hx("#10132a"), 90.0)
            .flex_row()
            .items_center()
            .justify_center()
            .children([poster])
    }
}

fn main() {
    sabitori::run_declarative(Gravity::new());
}
