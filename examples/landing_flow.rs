//! LP pattern — **Flow field**. A living particle stream, impossible in CSS.
//!
//! ~130 particles advect along a time-animated vector field (layered sines
//! standing in for curl noise). Each particle is drawn as a short velocity
//! streak, so the whole page reads as a flowing current. The simulation lives
//! in the app struct and integrates every `tick()`; `view()` just renders the
//! current state. The DOM would need a `<canvas>` + WebGL to do any of this —
//! here it's plain `div()`s, one per streak sample, rebuilt every frame.
//!
//! `cargo run --example landing_flow`

use sabitori::*;

const N: usize = 130; // particles
const TRAIL: usize = 6; // streak samples per particle

fn hx(s: &str) -> Color {
    Color::from_hex(s)
}

/// Divergence-ish flow direction at a normalized point (x,y in 0..1) and time.
fn field(x: f32, y: f32, t: f32) -> (f32, f32) {
    let a = (x * 6.0 + t * 0.30).sin()
        + (y * 5.0 - t * 0.23).cos()
        + (x * 2.4 + y * 3.1 + t * 0.17).sin();
    let angle = a * 1.4;
    (angle.cos(), angle.sin())
}

fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    x = ((x >> ((x >> 28).wrapping_add(4))) ^ x).wrapping_mul(277_803_737);
    x = (x >> 22) ^ x;
    (x as f32) / (u32::MAX as f32)
}

struct Flow {
    t: f32,
    /// Particle positions in normalized [0,1]² space — decoupled from window
    /// size so `tick()` (which has no viewport) can integrate freely.
    parts: Vec<[f32; 2]>,
}

impl Flow {
    fn new() -> Self {
        let parts = (0..N as u32).map(|i| [hash01(i * 2 + 1), hash01(i * 3 + 7)]).collect();
        Flow { t: 0.0, parts }
    }
}

impl DeclarativeApp for Flow {
    fn title(&self) -> &str {
        "Sabitori — Flow"
    }
    fn size(&self) -> (f32, f32) {
        (1100.0, 760.0)
    }
    fn is_animating(&self) -> bool {
        true
    }

    fn tick(&mut self, dt: f32) {
        let dt = dt.min(0.05); // clamp hitches so particles never teleport
        let t = self.t;
        for p in &mut self.parts {
            let (vx, vy) = field(p[0], p[1], t);
            p[0] = (p[0] + vx * 0.05 * dt).rem_euclid(1.0);
            p[1] = (p[1] + vy * 0.05 * dt).rem_euclid(1.0);
        }
        self.t += dt;
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        let (w, h, t) = (ctx.width, ctx.height, self.t);
        let blue = hx("#6ea8ff");
        let cyan = hx("#7de3ff");
        let purple = hx("#b18cff");

        // ── Flowing particle field ──
        let mut dots: Vec<Element> = Vec::with_capacity(N * TRAIL);
        for p in &self.parts {
            let (vx, vy) = field(p[0], p[1], t);
            let col = blue.lerp(cyan, 0.5 + 0.5 * vy).lerp(purple, (0.5 + 0.5 * vx) * 0.4);
            for k in 0..TRAIL {
                let kf = k as f32;
                let bx = p[0] - vx * 0.009 * kf; // streak trails behind velocity
                let by = p[1] - vy * 0.009 * kf;
                let fade = 1.0 - kf / TRAIL as f32;
                let size = 0.8 + 2.4 * fade;
                dots.push(
                    div()
                        .absolute()
                        .pos(bx * w - size / 2.0, by * h - size / 2.0)
                        .w(Px(size))
                        .h(Px(size))
                        .rounded_px(size)
                        .bg(col)
                        .opacity(0.55 * fade),
                );
            }
        }
        let flow = div().absolute().pos(0.0, 0.0).w(Px(w)).h(Px(h)).children(dots);

        // Soft dark halo so the wordmark stays legible over the current.
        let halo = div()
            .absolute()
            .pos(w / 2.0 - 300.0, h / 2.0 - 140.0)
            .w(Px(600.0))
            .h(Px(280.0))
            .rounded_px(140.0)
            .bg(hx("#06070f"))
            .opacity(0.5)
            .glow(hx("#06070f"), 100.0);

        // ── Hero ──
        let hero = div().flex_col().items_center().gap(22.0).children([
            text("A PARTICLE FIELD, REBUILT EVERY FRAME ON THE GPU")
                .font_size(11.0)
                .font_weight(600)
                .letter_spacing(2.0)
                .color(hx("#6b74a0")),
            text("sabitori")
                .font_size(80.0)
                .font_weight(300)
                .letter_spacing(-1.0)
                .color(hx("#f2f5ff")),
            text("UI that flows. Not a single line of CSS.")
                .font_size(17.0)
                .line_height(1.5)
                .color(hx("#a7afd6")),
            div()
                .id("go")
                .cursor(Cursor::Pointer)
                .flex_row()
                .items_center()
                .justify_center()
                .px_pad(Px(24.0))
                .py(Px(13.0))
                .rounded_px(999.0)
                .gradient(blue, purple, 0.0)
                .glow_sm(blue)
                .hover(|s| s.translate_y(-2.0).glow(purple, 24.0))
                .spring_transition(260.0, 22.0)
                .children([text("Enter the current  →")
                    .font_size(14.0)
                    .font_weight(600)
                    .letter_spacing(0.3)
                    .color(hx("#0a0c16"))]),
        ]);

        div()
            .w(Px(w))
            .h(Px(h))
            .bg(hx("#06070f"))
            .flex_col()
            .items_center()
            .justify_center()
            .children([flow, halo, hero])
    }
}

fn main() {
    sabitori::run_declarative(Flow::new());
}
