//! LP pattern — **Particle typography**. The wordmark *is* a physics swarm.
//!
//! "sabitori" is spelled by ~220 particles, each with a home target sampled
//! from a hand-built 5×7 bitmap font. Every `tick()` each particle springs
//! toward its home AND gets shoved away by an inverse-square repulsion from the
//! cursor — so dragging the pointer through the word scatters it like sand, and
//! it springs back into crisp letters the moment you leave. The cursor is read
//! via `on_pointer_move`. No `<canvas>`, no WebGL, no CSS keyframe could do a
//! per-glyph interactive particle field like this — here it's plain `div()`s.
//!
//! `cargo run --example landing_particles`

use sabitori::*;
use std::cell::Cell;

const W: f32 = 1100.0;
const H: f32 = 760.0;

// ── Bitmap font (5 wide × 7 tall, lowercase-ish) ──────────────────────────
const CELL: f32 = 13.0; // px per font cell
const PER: usize = 3; // particles per lit cell
const ADV: f32 = 78.0; // per-letter advance (5 cells + 1 gap) × CELL
const WORD_W: f32 = 8.0 * 65.0 + 7.0 * 13.0; // 8 letters, 5-cell bodies, 13px gaps
const START_X: f32 = W / 2.0 - WORD_W / 2.0;
const BASE_Y: f32 = H / 2.0 - 20.0 - 45.5; // word block is 7×CELL tall (91px)

fn glyph(c: char) -> [&'static str; 7] {
    match c {
        's' => [".....", ".....", ".####", "#....", ".###.", "....#", "####."],
        'a' => [".....", ".....", ".###.", "...#.", ".####", "#..#.", ".###."],
        'b' => ["#....", "#....", "#....", "###..", "#..#.", "#..#.", "###.."],
        'i' => ["..#..", ".....", "..#..", "..#..", "..#..", "..#..", "..#.."],
        't' => [".#...", ".#...", "###..", ".#...", ".#...", ".#..#", "..##."],
        'o' => [".....", ".....", ".###.", "#...#", "#...#", "#...#", ".###."],
        'r' => [".....", ".....", "#.##.", "##..#", "#....", "#....", "#...."],
        _ => [".....", ".....", ".....", ".....", ".....", ".....", "....."],
    }
}

// ── Physics tuning ────────────────────────────────────────────────────────
const SPRING: f32 = 70.0;
const DAMP: f32 = 10.0;
const REPEL: f32 = 240_000.0;
const RADIUS2: f32 = 150.0 * 150.0;

fn hx(s: &str) -> Color {
    Color::from_hex(s)
}
fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    x = ((x >> ((x >> 28).wrapping_add(4))) ^ x).wrapping_mul(277_803_737);
    x = (x >> 22) ^ x;
    (x as f32) / (u32::MAX as f32)
}

/// One particle: a home target it springs back to, plus live state + a colour
/// factor (0 = left edge of the word, 1 = right edge).
#[derive(Clone, Copy)]
struct P {
    hx: f32,
    hy: f32,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    cf: f32,
}

struct Particles {
    t: f32,
    mx: f32,
    my: f32,
    parts: Vec<P>,
    /// Offset of the fixed-size design canvas within the (resizable) window,
    /// stashed each frame in `view()` so `on_pointer_move` can map the window
    /// cursor back into design space. `Cell` because `view()` takes `&self`.
    ox: Cell<f32>,
    oy: Cell<f32>,
}

impl Particles {
    fn new() -> Self {
        let word = "sabitori";
        let mut parts: Vec<P> = Vec::new();
        let mut gi: u32 = 1;
        for (li, ch) in word.chars().enumerate() {
            let g = glyph(ch);
            let bx = START_X + li as f32 * ADV;
            for (r, row) in g.iter().enumerate() {
                for (c, cell) in row.chars().enumerate() {
                    if cell != '#' {
                        continue;
                    }
                    let cxp = bx + c as f32 * CELL + CELL / 2.0;
                    let cyp = BASE_Y + r as f32 * CELL + CELL / 2.0;
                    for _ in 0..PER {
                        let jx = (hash01(gi * 2 + 1) - 0.5) * CELL * 0.6;
                        let jy = (hash01(gi * 3 + 7) - 0.5) * CELL * 0.6;
                        let home_x = cxp + jx;
                        let home_y = cyp + jy;
                        // Start scattered across the window; the spring assembles
                        // the word over the first second.
                        let sx = hash01(gi * 5 + 11) * W;
                        let sy = hash01(gi * 7 + 3) * H;
                        let cf = ((home_x - START_X) / WORD_W).clamp(0.0, 1.0);
                        parts.push(P { hx: home_x, hy: home_y, x: sx, y: sy, vx: 0.0, vy: 0.0, cf });
                        gi += 1;
                    }
                }
            }
        }
        // Park the cursor off-screen so the resting state is the crisp wordmark.
        Particles { t: 0.0, mx: -3000.0, my: -3000.0, parts, ox: Cell::new(0.0), oy: Cell::new(0.0) }
    }
}

fn tri_color(cf: f32) -> Color {
    let blue = hx("#6ea8ff");
    let cyan = hx("#7de3ff");
    let purple = hx("#b18cff");
    if cf < 0.5 {
        blue.lerp(cyan, cf * 2.0)
    } else {
        cyan.lerp(purple, (cf - 0.5) * 2.0)
    }
}

impl DeclarativeApp for Particles {
    fn title(&self) -> &str {
        "Sabitori — Particles"
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
        for p in &mut self.parts {
            // Idle shimmer keeps the word breathing even at rest.
            let hx_t = p.hx + (t * 1.3 + p.hx * 0.03).sin() * 1.3;
            let hy_t = p.hy + (t * 1.1 + p.hy * 0.03).cos() * 1.3;
            let mut fx = (hx_t - p.x) * SPRING;
            let mut fy = (hy_t - p.y) * SPRING;
            // Inverse-square shove away from the cursor.
            let dx = p.x - mx;
            let dy = p.y - my;
            let d2 = dx * dx + dy * dy;
            if d2 < RADIUS2 {
                let g = REPEL / (d2 + 500.0);
                fx += dx * g;
                fy += dy * g;
            }
            p.vx = (p.vx + fx * dt) * (1.0 - DAMP * dt);
            p.vy = (p.vy + fy * dt) * (1.0 - DAMP * dt);
            p.x += p.vx * dt;
            p.y += p.vy * dt;
        }
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        // Center the fixed 1100×760 design canvas in whatever the window is now,
        // and remember the offset so pointer input maps back correctly.
        self.ox.set((ctx.width - W) / 2.0);
        self.oy.set((ctx.height - H) / 2.0);

        let mut kids: Vec<Element> = Vec::with_capacity(self.parts.len() + 8);

        // The swarm — crisp, bright points against the dark gradient. Per-particle
        // SDF glow was too heavy here (~220 blur passes tanked the frame rate), so
        // the atmosphere comes from a couple of big soft lights *behind* the word,
        // in a neutral hue so they don't camouflage the coloured particles.
        for (cx, glow_r) in [(W * 0.42, 300.0), (W * 0.58, 300.0)] {
            kids.push(
                div()
                    .absolute()
                    .pos(cx - 24.0, H / 2.0 - 44.0)
                    .w(Px(48.0))
                    .h(Px(48.0))
                    .rounded_px(24.0)
                    .bg(hx("#223055"))
                    .opacity(0.16)
                    .glow(hx("#2b3a66"), glow_r),
            );
        }
        for p in &self.parts {
            let s = 3.0;
            kids.push(
                div()
                    .absolute()
                    .pos(p.x - s / 2.0, p.y - s / 2.0)
                    .w(Px(s))
                    .h(Px(s))
                    .rounded_px(s)
                    .bg(tri_color(p.cf))
                    .opacity(1.0),
            );
        }

        // Eyebrow above the word.
        kids.push(
            div().absolute().pos(0.0, H / 2.0 - 118.0).w(Px(W)).flex_row().justify_center().children([
                text("A WORDMARK MADE OF ~220 PARTICLES")
                    .font_size(11.0)
                    .font_weight(600)
                    .letter_spacing(2.0)
                    .color(hx("#6b74a0")),
            ]),
        );

        // Tagline + CTA below the word.
        kids.push(
            div().absolute().pos(0.0, H / 2.0 + 66.0).w(Px(W)).flex_row().justify_center().children([
                text("Drag your cursor through the letters.")
                    .font_size(16.0)
                    .line_height(1.5)
                    .color(hx("#a7afd6")),
            ]),
        );
        // CTA pill. NOTE: use a real corner radius (≈ half the height), NOT
        // rounded_px(999) — a corner radius larger than the box's half-extent is
        // a degenerate SDF here and paints nothing at all.
        kids.push(
            div()
                .id("go")
                .cursor(Cursor::Pointer)
                .absolute()
                .pos((W - 212.0) / 2.0, H / 2.0 + 104.0)
                .w(Px(212.0))
                .h(Px(48.0))
                .flex_row()
                .items_center()
                .justify_center()
                .rounded_px(24.0)
                .bg(hx("#7aa2f7"))
                .glow_sm(hx("#7aa2f7"))
                .hover(|s| s.translate_y(-2.0).glow(hx("#b18cff"), 24.0))
                .spring_transition(260.0, 22.0)
                .children([text("Disturb the field  →").font_size(14.0).font_weight(600).letter_spacing(0.3).color(hx("#0a0c16"))]),
        );

        // Footer.
        kids.push(
            div().absolute().pos(0.0, H - 44.0).w(Px(W)).flex_row().justify_center().children([
                text("SPRING + REPULSION · REBUILT EVERY FRAME · NO CANVAS")
                    .font_size(10.5)
                    .font_weight(600)
                    .letter_spacing(1.8)
                    .color(hx("#454c72")),
            ]),
        );

        // The design canvas (fixed size); its absolute children are positioned
        // relative to it, so centering it moves the whole composition as a unit.
        let poster = div().w(Px(W)).h(Px(H)).children(kids);

        // Full-window backdrop that fills any resize; poster floats centered.
        div()
            .w(Px(ctx.width))
            .h(Px(ctx.height))
            .gradient(hx("#07080f"), hx("#0d1020"), 90.0)
            .flex_row()
            .items_center()
            .justify_center()
            .children([poster])
    }
}

fn main() {
    sabitori::run_declarative(Particles::new());
}
