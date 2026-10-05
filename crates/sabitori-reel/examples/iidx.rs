//! sabitori-reel — an **IIDX-style autoplay** rhythm chart, rendered offscreen
//! to an mp4. Notes fall down 7+1 lanes to a judgment line; each lane flashes a
//! beam + key light as its note lands, a turntable spins on the left, and a
//! combo/score HUD ticks up on the right — closing on a FULL COMBO banner.
//!
//! The whole thing is a **pure function of the frame**: a hand-authored chart
//! (`Vec<Note>`) drives everything, so a note's position is just
//! `judge_y - (hit_time - t) * speed`. No input, no audio (mux a track later
//! with ffmpeg) — just the visual, which is squarely in reel's lane.
//!
//!   cargo run -p sabitori-reel --example iidx --release
//!
//! Output: `$TMPDIR/sabitori-iidx.mp4`

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use sabitori_reel::{
    div, interpolate, interpolate_color, interpolate_eased, polyline, spring_with, text, Color,
    Easing, Element, FrameCtx, Px, Reel, Spring,
};

const W: f32 = 1280.0;
const H: f32 = 720.0;
const FPS: u32 = 60;
const SECS: u32 = 13;

const BPM: f32 = 150.0;
const SPEED: f32 = 470.0; // note fall speed, px/s
const JUDGE_Y: f32 = 628.0; // the judgment line

// Playfield: scratch (0) + 7 keys. Odd key lanes are white, even are blue.
const FIELD_LEFT: f32 = 452.0;
const LANE_W: [f32; 8] = [58.0, 46.0, 40.0, 46.0, 40.0, 46.0, 40.0, 46.0];
const LANE_GAP: f32 = 2.0;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}

// ── background BGA (the kaleidoscope tunnel from `bga`, dimmed) ──────────────────

const BGA_PALETTE: [&str; 4] = ["#5eead4", "#22d3ee", "#7c8cf0", "#e05ce0"];

fn bga_cycle(phase: f32) -> Color {
    let n = BGA_PALETTE.len();
    let x = phase.rem_euclid(1.0) * n as f32;
    let i = x.floor() as usize % n;
    interpolate_color(x - x.floor(), [0.0, 1.0], hex(BGA_PALETTE[i]), hex(BGA_PALETTE[(i + 1) % n]))
}

fn ring_pts(cx: f32, cy: f32, r: f32, k: usize, rot: f32) -> Vec<(f32, f32)> {
    (0..=k)
        .map(|i| {
            let a = rot + i as f32 / k as f32 * TAU;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

fn bga_stroke(pts: Vec<(f32, f32)>, color: Color, w: f32, o: f32) -> Element {
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

/// The kaleidoscope tunnel (hexagon slices spiraling out of a vanishing point),
/// dimmed to sit behind the gameplay. Same BPM as the chart, so it pulses on the
/// beat together with the notes. (Engine draws all lines above all rects, so the
/// tunnel reads as a dim holographic veil over the field rather than strictly
/// behind it — kept faint so the notes stay the focus.)
fn bga_layers(t: f32) -> Vec<Element> {
    const CXB: f32 = W * 0.5;
    const CYB: f32 = H * 0.5;
    const DIM: f32 = 0.5;
    const N: usize = 18;
    const K: usize = 6;
    let beat = 60.0 / BPM;
    let p = 16.0 * beat;
    let loop01 = (t / p).rem_euclid(1.0);
    let kick =
        interpolate_eased((t / beat).rem_euclid(1.0), [0.0, 0.20], [1.0, 0.0], Easing::EaseOutCubic);
    let base_rot = TAU * loop01;
    let t_tunnel = p / 4.0;
    let (smin, smax) = (0.05_f32, 1.9_f32);
    let (twist, r0) = (1.15_f32, 340.0_f32);

    let mut slices: Vec<(f32, usize)> = (0..N)
        .map(|j| (((t / t_tunnel) + j as f32 / N as f32).rem_euclid(1.0), j))
        .collect();
    slices.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

    let mut out: Vec<Element> = Vec::new();
    for (u, j) in slices {
        let s = smin * (smax / smin).powf(u);
        let fade = interpolate(u, [0.0, 0.10], [0.0, 1.0]) * interpolate(u, [0.82, 1.0], [1.0, 0.0]);
        if fade <= 0.001 {
            continue;
        }
        let o = (fade * (0.28 + 0.72 * u) + kick * 0.12 * fade) * DIM;
        let w = interpolate(u, [0.0, 1.0], [1.0, 3.0]);
        let half = if j % 2 == 1 { TAU / (K as f32 * 2.0) } else { 0.0 };
        let col = bga_cycle(loop01 + u * 0.7);
        out.push(bga_stroke(ring_pts(CXB, CYB, r0 * s, K, base_rot + u * twist + half), col, w, o));
    }
    out
}

// ── chart ────────────────────────────────────────────────────────────────────

struct Note {
    t: f32,
    lane: usize,
}

/// A hand-authored groove: a driving scratch on every beat, a staircase stream
/// zig-zagging up and down the keys on 8th notes, plus chord + off-beat accents.
/// Deterministic (index math only), so the render stays a pure function of `t`.
fn chart(secs: f32) -> Vec<Note> {
    let beat = 60.0 / BPM;
    let six = beat / 4.0; // 16th note
    let lead = 1.0; // let the field breathe for a beat before notes rain in
    let steps = (secs / six) as usize;
    let mut v = Vec::new();
    for s in 0..steps {
        let t = lead + s as f32 * six;
        // Scratch on each downbeat.
        if s % 4 == 0 {
            v.push(Note { t, lane: 0 });
        }
        // Main stream on 8th notes: a staircase 1→7 then 7→2, over and over.
        if s % 2 == 0 {
            let p = (s / 2) % 12;
            let lane = if p < 7 { 1 + p } else { 13 - p }; // 1..7 then 6..2
            v.push(Note { t, lane });
        }
        // Chord accent every 2 beats: a second key a third away.
        if s % 8 == 0 {
            let base = 1 + (s / 8) % 7;
            v.push(Note { t, lane: 1 + (base + 1) % 7 });
        }
        // Off-beat blue sparkle on a black key (2/4/6).
        if s % 8 == 6 {
            v.push(Note { t, lane: 2 + (s / 8 % 3) * 2 });
        }
    }
    v
}

// ── geometry / palette ─────────────────────────────────────────────────────────

fn lane_left(l: usize) -> f32 {
    FIELD_LEFT + LANE_W[..l].iter().sum::<f32>() + LANE_GAP * l as f32
}
fn field_right() -> f32 {
    lane_left(7) + LANE_W[7]
}
fn field_center() -> f32 {
    (FIELD_LEFT + field_right()) * 0.5
}

/// (top highlight, base) note colors by lane.
fn note_colors(l: usize) -> (&'static str, &'static str) {
    if l == 0 {
        ("#ff9d9d", "#e5352f") // scratch: red
    } else if l % 2 == 1 {
        ("#ffffff", "#c9d6f7") // white keys
    } else {
        ("#a6c6ff", "#3f74e6") // black keys: blue
    }
}
/// The lane's accent color (for beams / key lights).
fn lane_accent(l: usize) -> &'static str {
    if l == 0 {
        "#ff5a52"
    } else if l % 2 == 1 {
        "#dfe7fb"
    } else {
        "#5b86ef"
    }
}

// ── the scene ──────────────────────────────────────────────────────────────────

fn scene(ctx: FrameCtx) -> Element {
    let t = ctx.t();
    let beat = 60.0 / BPM;
    let notes = chart(SECS as f32 - 2.0);
    let total = notes.len();
    let fr = field_right();
    let fw = fr - FIELD_LEFT;
    let fcx = field_center();

    // One pass: combo, per-lane last hit (for beams), and the most recent hit.
    let mut last_hit = [f32::NEG_INFINITY; 8];
    let mut combo = 0usize;
    let mut global_last = f32::NEG_INFINITY;
    for n in &notes {
        if n.t <= t {
            combo += 1;
            if n.t > last_hit[n.lane] {
                last_hit[n.lane] = n.t;
            }
            if n.t > global_last {
                global_last = n.t;
            }
        }
    }
    let global_since = t - global_last;

    // Start with the BGA tunnel behind everything, then paint the gameplay.
    let mut layers: Vec<Element> = bga_layers(t);

    // Thin accent bars top & bottom.
    layers.push(
        div()
            .absolute()
            .pos(0.0, 0.0)
            .w(Px(W))
            .h(Px(4.0))
            .gradient(hex("#5eead4"), hex("#a78bfa"), 0.0)
            .opacity(0.7),
    );

    // Playfield strip.
    layers.push(
        div()
            .absolute()
            .pos(FIELD_LEFT - 10.0, 0.0)
            .w(Px(fw + 20.0))
            .h(Px(H))
            .gradient(hex("#070b16"), hex("#03050c"), FRAC_PI_2)
            .opacity(0.94),
    );

    // Per-lane fills + dividers.
    for l in 0..8 {
        let lx = lane_left(l);
        let bg = if l == 0 {
            "#170a10"
        } else if l % 2 == 1 {
            "#0d1322"
        } else {
            "#0a0f1e"
        };
        layers.push(
            div()
                .absolute()
                .pos(lx, 0.0)
                .w(Px(LANE_W[l]))
                .h(Px(H))
                .bg(hex(bg)),
        );
        if l > 0 {
            layers.push(
                div()
                    .absolute()
                    .pos(lx - LANE_GAP, 0.0)
                    .w(Px(LANE_GAP))
                    .h(Px(H))
                    .bg(hex("#1b2740"))
                    .opacity(0.7),
            );
        }
    }

    // Beat pulse over the field (subtle brighten on each downbeat).
    let pulse = interpolate((t / beat).rem_euclid(1.0), [0.0, 0.14], [1.0, 0.0]);
    layers.push(
        div()
            .absolute()
            .pos(FIELD_LEFT - 10.0, 0.0)
            .w(Px(fw + 20.0))
            .h(Px(H))
            .bg(hex("#5eead4"))
            .opacity(pulse * 0.035),
    );

    // Beams + key lights per recently-hit lane.
    for l in 0..8 {
        let since = t - last_hit[l];
        if since < 0.0 || since > 0.18 {
            continue;
        }
        let bo = 1.0 - since / 0.18;
        let lx = lane_left(l);
        let acc = lane_accent(l);
        // Rising beam column.
        layers.push(
            div()
                .absolute()
                .pos(lx + 2.0, JUDGE_Y - 170.0)
                .w(Px(LANE_W[l] - 4.0))
                .h(Px(170.0))
                .gradient(hex("#05070d"), hex(acc), FRAC_PI_2)
                .rounded_px(3.0)
                .opacity(bo * 0.55),
        );
    }

    // Judgment line: soft glow + a bright core.
    layers.push(
        div()
            .absolute()
            .pos(FIELD_LEFT - 10.0, JUDGE_Y - 22.0)
            .w(Px(fw + 20.0))
            .h(Px(44.0))
            .bg(hex("#5eead4"))
            .rounded_px(6.0)
            .opacity(0.09),
    );
    layers.push(
        div()
            .absolute()
            .pos(FIELD_LEFT - 10.0, JUDGE_Y - 2.0)
            .w(Px(fw + 20.0))
            .h(Px(3.0))
            .bg(hex("#8ff7e2"))
            .opacity(0.85),
    );

    // Falling notes.
    for n in &notes {
        let rel = n.t - t;
        if rel < 0.0 || rel > 1.4 {
            continue;
        }
        let cy = JUDGE_Y - rel * SPEED;
        let (top, base) = note_colors(n.lane);
        let nh = if n.lane == 0 { 18.0 } else { 14.0 };
        let nw = LANE_W[n.lane] - 8.0;
        let x = lane_left(n.lane) + 4.0;
        let fade = interpolate(cy, [16.0, 74.0], [0.0, 1.0]); // ease in near the top
        layers.push(
            div()
                .absolute()
                .pos(x, cy - nh * 0.5)
                .w(Px(nw))
                .h(Px(nh))
                .gradient(hex(top), hex(base), FRAC_PI_2)
                .rounded_px(3.0)
                .opacity(fade),
        );
    }

    // Key lights just below the line.
    for l in 0..8 {
        let since = t - last_hit[l];
        let flash = if since >= 0.0 && since < 0.18 {
            1.0 - since / 0.18
        } else {
            0.0
        };
        let lx = lane_left(l);
        layers.push(
            div()
                .absolute()
                .pos(lx + 2.0, JUDGE_Y + 6.0)
                .w(Px(LANE_W[l] - 4.0))
                .h(Px(26.0))
                .bg(hex(lane_accent(l)))
                .rounded_px(3.0)
                .opacity(0.1 + flash * 0.85),
        );
    }

    // Judgment popup — "P-GREAT" flashing above the line on each hit.
    if global_since >= 0.0 && global_since < 0.16 {
        let po = 1.0 - global_since / 0.16;
        layers.push(
            div()
                .absolute()
                .pos(FIELD_LEFT - 40.0, JUDGE_Y - 96.0)
                .w(Px(fw + 80.0))
                .flex_col()
                .items_center()
                .child(
                    text("P-GREAT")
                        .font_size(30.0)
                        .bold()
                        .letter_spacing(2.0)
                        .color(hex("#7ff5df"))
                        .opacity(po)
                        .ty(-(po) * 8.0),
                ),
        );
    }

    // ── turntable (left) ──
    let d = 176.0;
    let tt_left = 148.0;
    let tt_top = 372.0;
    let theta = t * 3.4;
    let r = 70.0;
    let cxy = d * 0.5;
    let scratch_hot = {
        let since = t - last_hit[0];
        if since >= 0.0 && since < 0.2 {
            1.0 - since / 0.2
        } else {
            0.0
        }
    };
    let disc_border = if scratch_hot > 0.4 { "#ff5a52" } else { "#2b3350" };
    let turntable = div()
        .absolute()
        .pos(tt_left, tt_top)
        .w(Px(d))
        .h(Px(d))
        .children(vec![
            div()
                .absolute()
                .pos(0.0, 0.0)
                .w(Px(d))
                .h(Px(d))
                .rounded_px(d * 0.5)
                .gradient(hex("#101a2c"), hex("#04070e"), PI * 0.25)
                .border(3.0, hex(disc_border)),
            div()
                .absolute()
                .pos(28.0, 28.0)
                .w(Px(d - 56.0))
                .h(Px(d - 56.0))
                .rounded_px((d - 56.0) * 0.5)
                .border(1.0, hex("#1e2b46")),
            // spinning diameter marker
            polyline()
                .points(vec![
                    (cxy + r * theta.cos(), cxy + r * theta.sin()),
                    (cxy - r * theta.cos(), cxy - r * theta.sin()),
                ])
                .stroke_width(5.0)
                .stroke_color(hex("#7ff5df"))
                .w(Px(d))
                .h(Px(d))
                .absolute()
                .pos(0.0, 0.0)
                .opacity(0.85),
            // perpendicular tick (dimmer, for a record look)
            polyline()
                .points(vec![
                    (cxy + r * (theta + FRAC_PI_2).cos(), cxy + r * (theta + FRAC_PI_2).sin()),
                    (cxy - r * (theta + FRAC_PI_2).cos(), cxy - r * (theta + FRAC_PI_2).sin()),
                ])
                .stroke_width(2.0)
                .stroke_color(hex("#3a5a8a"))
                .w(Px(d))
                .h(Px(d))
                .absolute()
                .pos(0.0, 0.0)
                .opacity(0.7),
            // hub
            div()
                .absolute()
                .pos(cxy - 13.0, cxy - 13.0)
                .w(Px(26.0))
                .h(Px(26.0))
                .rounded_px(13.0)
                .bg(hex("#5eead4"))
                .opacity(0.85 + scratch_hot * 0.15),
        ]);
    layers.push(turntable);

    // Title (top-left) + info under the turntable.
    layers.push(
        div()
            .absolute()
            .pos(48.0, 44.0)
            .flex_col()
            .gap(6.0)
            .children(vec![
                text("SABITORI REEL")
                    .font_size(30.0)
                    .bold()
                    .letter_spacing(1.0)
                    .color(hex("#eef1f8")),
                text("AUTO PLAY")
                    .font_size(14.0)
                    .bold()
                    .letter_spacing(4.0)
                    .color(hex("#5eead4")),
            ]),
    );
    layers.push(
        div()
            .absolute()
            .pos(tt_left - 27.0, tt_top + d + 18.0)
            .w(Px(d + 54.0))
            .flex_col()
            .items_center()
            .gap(4.0)
            .children(vec![
                text(format!("♪ BPM {}", BPM as u32))
                    .font_size(20.0)
                    .bold()
                    .color(hex("#8b95ad")),
                text("☆ LEVEL 12")
                    .font_size(16.0)
                    .letter_spacing(2.0)
                    .color(hex("#5eead4")),
            ]),
    );

    // ── combo / score HUD (right) ──
    let pop = if global_since >= 0.0 && global_since < 0.1 {
        1.0 - global_since / 0.1
    } else {
        0.0
    };
    let score = combo * 250;
    layers.push(
        div()
            .absolute()
            .pos(908.0, 232.0)
            .w(Px(320.0))
            .flex_col()
            .items_center()
            .gap(2.0)
            .children(vec![
                text("COMBO")
                    .font_size(16.0)
                    .bold()
                    .letter_spacing(4.0)
                    .color(hex("#5eead4")),
                text(combo.to_string())
                    .font_size(104.0)
                    .bold()
                    .letter_spacing(-2.0)
                    .color(hex("#eef1f8"))
                    .ty(-pop * 7.0),
                div().h(Px(20.0)),
                text("SCORE")
                    .font_size(14.0)
                    .bold()
                    .letter_spacing(3.0)
                    .color(hex("#8b95ad")),
                text(score.to_string())
                    .font_size(42.0)
                    .bold()
                    .color(hex("#7ff5df")),
                text(format!("MAX {total}"))
                    .font_size(14.0)
                    .letter_spacing(2.0)
                    .color(hex("#5a6480")),
            ]),
    );

    // ── FULL COMBO banner (once every note has landed) ──
    if combo == total && global_last.is_finite() {
        let ft = (t - global_last).max(0.0);
        let rise = spring_with(ft, FPS, Spring::bouncy());
        let bo = interpolate(ft, [0.0, 0.3], [0.0, 1.0]);
        layers.push(
            div()
                .absolute()
                .pos(fcx - 260.0, 250.0)
                .w(Px(520.0))
                .flex_col()
                .items_center()
                .gap(6.0)
                .opacity(bo)
                .ty((1.0 - rise) * 26.0)
                .children(vec![
                    text("FULL COMBO")
                        .font_size(72.0)
                        .bold()
                        .letter_spacing(-1.0)
                        .color(hex("#7ff5df")),
                    text("cleared · pure Rust")
                        .font_size(22.0)
                        .letter_spacing(3.0)
                        .color(hex("#8b95ad")),
                ]),
        );
    }

    div()
        .w(Px(W))
        .h(Px(H))
        .gradient(hex("#04050b"), hex("#090f1e"), FRAC_PI_2)
        .children(layers)
}

fn main() {
    let reel = Reel::new(W as u32, H as u32, FPS, FPS * SECS);
    let out = std::env::temp_dir().join("sabitori-iidx.mp4");

    println!(
        "rendering + encoding {} frames ({}x{} @ {FPS}fps) …",
        reel.frames, reel.width, reel.height
    );
    match reel.render_mp4(&scene, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => {
            eprintln!("encode failed: {e}");
            let dir = std::env::temp_dir().join("sabitori-iidx");
            let paths = reel.render_pngs(&scene, &dir).expect("render failed");
            eprintln!("wrote {} PNGs instead -> {}", paths.len(), dir.display());
        }
    }
}
