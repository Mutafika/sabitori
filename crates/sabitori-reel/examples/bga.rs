//! sabitori-reel — a **rhythm-game BGA**: an abstract radial-kaleidoscope loop
//! that plays behind the notes (this is the *background animation*, not the
//! gameplay). Rotating spokes, concentric rings emitted on each kick, a stack
//! of counter-rotating arc rings (the mandala), wireframe polygons, and a
//! pulsing core — all in cycling neon, on near-black.
//!
//! Every element is a **periodic function of one master loop period `P`**
//! (integer turns per loop, colors cycle once per loop, rings ride the beat),
//! so the clip is **seamlessly loopable** — set it to repeat behind anything.
//! Pure function of the frame, no external API: squarely reel's home turf.
//!
//!   cargo run -p sabitori-reel --example bga --release
//!
//! Output: `$TMPDIR/sabitori-bga.mp4`

use std::f32::consts::TAU;

use sabitori_reel::{
    div, interpolate, interpolate_color, interpolate_eased, polyline, text, Color, Easing, Element,
    FrameCtx, Px, Reel,
};

const W: f32 = 1280.0;
const H: f32 = 720.0;
const CX: f32 = 640.0;
const CY: f32 = 360.0;
const FPS: u32 = 60;

const BPM: f32 = 150.0;
const LOOP_BEATS: f32 = 16.0; // master loop = 16 beats
const LOOPS: f32 = 2.0; // render two loops

// Cycling neon palette: teal → cyan → indigo → magenta → (wrap).
const PALETTE: [&str; 4] = ["#5eead4", "#22d3ee", "#7c8cf0", "#e05ce0"];

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}

/// Sample the palette ring at `phase` (0..1 = one full trip around the wheel).
fn cycle(phase: f32) -> Color {
    let n = PALETTE.len();
    let x = phase.rem_euclid(1.0) * n as f32;
    let i = x.floor() as usize % n;
    interpolate_color(x - x.floor(), [0.0, 1.0], hex(PALETTE[i]), hex(PALETTE[(i + 1) % n]))
}

// ── geometry (points in canvas space; every polyline box is the full canvas) ──

fn circle_pts(r: f32, n: usize) -> Vec<(f32, f32)> {
    (0..=n)
        .map(|i| {
            let a = i as f32 / n as f32 * TAU;
            (CX + r * a.cos(), CY + r * a.sin())
        })
        .collect()
}

fn poly_pts(r: f32, k: usize, rot: f32) -> Vec<(f32, f32)> {
    (0..=k)
        .map(|i| {
            let a = rot + i as f32 / k as f32 * TAU;
            (CX + r * a.cos(), CY + r * a.sin())
        })
        .collect()
}

/// A neon stroke: a polyline in a full-canvas box, so its points are canvas px.
fn stroke(pts: Vec<(f32, f32)>, color: Color, w: f32, o: f32) -> Element {
    polyline()
        .points(pts)
        .stroke_width(w)
        .stroke_color(color)
        .w(Px(W))
        .h(Px(H))
        .absolute()
        .pos(0.0, 0.0)
        .opacity(o.clamp(0.0, 1.0))
}

/// A soft filled disc centered on the core (for the radial bloom).
fn glow(size: f32, color: Color, o: f32) -> Element {
    div()
        .absolute()
        .pos(CX - size * 0.5, CY - size * 0.5)
        .w(Px(size))
        .h(Px(size))
        .rounded_px(size * 0.5)
        .bg(color)
        .opacity(o)
}

// ── the scene ──────────────────────────────────────────────────────────────────

fn scene(ctx: FrameCtx) -> Element {
    let t = ctx.t();
    let beat = 60.0 / BPM;
    let p = LOOP_BEATS * beat; // master loop period (s)
    let loop01 = (t / p).rem_euclid(1.0);

    // Beat + bar envelopes (sharp attack, eased decay).
    let bp = (t / beat).rem_euclid(1.0);
    let kick = interpolate_eased(bp, [0.0, 0.20], [1.0, 0.0], Easing::EaseOutCubic);
    let barp = (t / (beat * 4.0)).rem_euclid(1.0);
    let barkick = interpolate_eased(barp, [0.0, 0.14], [1.0, 0.0], Easing::EaseOutCubic);

    // Complementary color tracks.
    let acc_a = cycle(loop01);
    let acc_b = cycle(loop01 + 0.5);
    let acc_c = cycle(loop01 + 0.25);

    let base_rot = TAU * loop01; // one slow turn per loop, shared by the tunnel + guides

    let mut layers: Vec<Element> = Vec::new();

    // Bar flash — a faint full-frame brighten on each downbeat. (No big filled
    // blooms: neon reads best as bright strokes on near-black, so the only fill
    // is the tight core hotspot at the very end.)
    layers.push(
        div()
            .absolute()
            .pos(0.0, 0.0)
            .w(Px(W))
            .h(Px(H))
            .bg(hex("#cfe0ff"))
            .opacity(barkick * 0.035),
    );

    // ── kaleidoscope TUNNEL ──
    // N hexagonal "slices" stream out of the vanishing point (center) toward
    // the camera. Depth `u` (0 = far … 1 = near) drives an exponential
    // perspective scale, a progressive twist (the tunnel spirals as it recedes),
    // fog (thin + dim far → thick + bright near, then fade as it exits), and a
    // front-to-back color shift. Painter-sorted far→near so near occludes far.
    const N: usize = 18;
    const K: usize = 6; // hexagonal rings
    let r0 = 340.0;
    let (smin, smax) = (0.05_f32, 1.9_f32);
    let twist = 1.15;
    let t_tunnel = p / 4.0; // a slice crosses the tunnel this often (p is a multiple → seamless)

    let mut slices: Vec<(f32, usize)> = (0..N)
        .map(|j| (((t / t_tunnel) + j as f32 / N as f32).rem_euclid(1.0), j))
        .collect();
    slices.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    for (u, j) in slices {
        let s = smin * (smax / smin).powf(u); // exponential perspective
        let fade = interpolate(u, [0.0, 0.10], [0.0, 1.0]) * interpolate(u, [0.82, 1.0], [1.0, 0.0]);
        if fade <= 0.001 {
            continue;
        }
        let o = fade * (0.30 + 0.70 * u) + kick * 0.15 * fade; // near = brighter
        let w = interpolate(u, [0.0, 1.0], [1.0, 3.4]); // near = thicker
        let half = if j % 2 == 1 { TAU / (K as f32 * 2.0) } else { 0.0 }; // interlock alt slices
        let rot = base_rot + u * twist + half;
        let col = cycle(loop01 + u * 0.7); // hue shifts front→back
        layers.push(stroke(poly_pts(r0 * s, K, rot), col, w, o));
    }

    // Corridor guides: a few dim radial lines through the vanishing point.
    let guides = 6usize;
    for k in 0..guides {
        let th = base_rot * 0.5 + k as f32 / guides as f32 * TAU;
        let (dx, dy) = (th.cos(), th.sin());
        let pts = vec![(CX + dx * 26.0, CY + dy * 26.0), (CX + dx * 780.0, CY + dy * 780.0)];
        layers.push(stroke(pts, acc_c, 1.2, 0.06 + kick * 0.14));
    }

    // Pulsing core: a tight bloom (rects, drawn under the strokes) + a spinning
    // diamond + a bright ring right at the center.
    layers.push(glow(74.0 + kick * 22.0, acc_b, 0.32));
    layers.push(glow(38.0 + kick * 15.0, hex("#f4fbff"), 0.88));
    layers.push(stroke(circle_pts(46.0 + kick * 14.0, 40), hex("#eafcff"), 2.0, 0.9));
    layers.push(stroke(
        poly_pts(26.0 + kick * 12.0, 4, -TAU * loop01 * 2.0),
        acc_a,
        2.5,
        0.85,
    ));

    // Faint corner wordmark.
    layers.push(
        text("SABITORI · REEL — BGA")
            .font_size(15.0)
            .bold()
            .letter_spacing(3.0)
            .color(hex("#8b95ad"))
            .opacity(0.35)
            .absolute()
            .pos(40.0, H - 46.0),
    );

    div()
        .w(Px(W))
        .h(Px(H))
        .gradient(hex("#04040a"), hex("#08060f"), TAU * 0.25)
        .children(layers)
}

fn main() {
    let beat = 60.0 / BPM;
    let frames = (LOOPS * LOOP_BEATS * beat * FPS as f32).round() as u32;
    let reel = Reel::new(W as u32, H as u32, FPS, frames);
    let out = std::env::temp_dir().join("sabitori-bga.mp4");

    println!(
        "rendering + encoding {} frames ({}x{} @ {FPS}fps, {:.1}s seamless loop ×{}) …",
        reel.frames,
        reel.width,
        reel.height,
        LOOP_BEATS * beat,
        LOOPS as u32
    );
    match reel.render_mp4(&scene, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => {
            eprintln!("encode failed: {e}");
            let dir = std::env::temp_dir().join("sabitori-bga");
            let paths = reel.render_pngs(&scene, &dir).expect("render failed");
            eprintln!("wrote {} PNGs instead -> {}", paths.len(), dir.display());
        }
    }
}
