//! sabitori-reel — an EnchuDB explainer built around an animated **cylinder
//! database** hero: a translucent glass cylinder (generated image) with data
//! rows dropping in behind the glass and a scan glow sweeping through it. The
//! hero carries across shots for continuity; everything drifts continuously
//! (nothing freezes after it arrives), shots flow past each other with a
//! directional dissolve, and layout alternates centered / two-column.
//!
//!   cargo run -p sabitori-reel --example video --release
//!
//! Output: `$TMPDIR/enchu-explainer.mp4`

use std::f32::consts::FRAC_PI_2;

use sabitori_reel::{
    div, image, interpolate, interpolate_color, interpolate_eased, spring_with, text, Color, Easing,
    Element, FrameCtx, ImageData, Px, Reel, Spring,
};

const W: f32 = 1280.0;
const H: f32 = 720.0;
const FPS: u32 = 30;
const SECS: u32 = 17;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}

// ── motion ───────────────────────────────────────────────────────────────────

/// Eased rise from 20px below, settling to 0, starting at local delay `d`.
fn rz(lt: f32, d: f32) -> f32 {
    interpolate_eased(lt, [d, d + 0.55], [20.0, 0.0], Easing::EaseOutCubic)
}

/// Cross-dissolve envelope: fade in over TR at `start`, fade out over TR before
/// `start + dur`. Consecutive shots overlap by ~TR so they dissolve.
fn envelope(t: f32, start: f32, dur: f32) -> f32 {
    let fade_in = interpolate(t, [start, start + 0.55], [0.0, 1.0]);
    let fade_out = interpolate(t, [start + dur - 0.55, start + dur], [1.0, 0.0]);
    fade_in * fade_out
}

// ── the cylinder database hero ───────────────────────────────────────────────

/// Generate a translucent glass DB cylinder shell as RGBA: bright teal top rim,
/// a faintly lit lid, dim bottom curve, and a body tint shaded for curvature.
/// Data rows + scan are drawn separately (as rects) behind this, showing
/// through the translucent body.
fn cylinder_shell(w: u32, h: u32) -> ImageData {
    let (wf, hf) = (w as f32, h as f32);
    let cx = wf * 0.5;
    let rx = wf * 0.5 - 3.0;
    let ry = rx * 0.32;
    let top_cy = ry + 3.0;
    let bot_cy = hf - ry - 3.0;
    let mut px = vec![0u8; (w * h * 4) as usize];

    for y in 0..h {
        for x in 0..w {
            let (xf, yf) = (x as f32 + 0.5, y as f32 + 0.5);
            let dx = xf - cx;
            let hx = dx.abs() / rx; // 0 center .. 1 edge
            if hx > 1.03 {
                continue;
            }
            let cap = (1.0 - hx * hx).max(0.0).sqrt();
            let top_edge = top_cy - ry * cap;
            let bot_edge = bot_cy + ry * cap;
            if yf < top_edge - 1.2 || yf > bot_edge + 1.2 {
                continue;
            }
            let idx = ((y * w + x) * 4) as usize;

            // Body glass tint, brighter toward the center (curvature).
            let curve = cap; // 1 center .. 0 edge
            let mut r = 16.0 + 16.0 * curve;
            let mut g = 28.0 + 34.0 * curve;
            let mut b = 38.0 + 40.0 * curve;
            let mut a = 30.0 + 26.0 * curve;

            let et = ((dx / rx).powi(2) + ((yf - top_cy) / ry).powi(2) - 1.0).abs();
            let top_rim = (1.0 - et / 0.13).clamp(0.0, 1.0);
            if (dx / rx).powi(2) + ((yf - top_cy) / ry).powi(2) <= 1.0 {
                // top lid — a touch brighter than the body
                a = a.max(64.0);
                r = r.max(26.0);
                g = g.max(62.0);
                b = b.max(74.0);
            }
            if top_rim > 0.0 {
                r = r * (1.0 - top_rim) + 120.0 * top_rim;
                g = g * (1.0 - top_rim) + 240.0 * top_rim;
                b = b * (1.0 - top_rim) + 214.0 * top_rim;
                a = a.max(235.0 * top_rim);
            }

            // Bottom curve (lower half only), dim.
            let eb = ((dx / rx).powi(2) + ((yf - bot_cy) / ry).powi(2) - 1.0).abs();
            let bot_rim = if yf > bot_cy {
                (1.0 - eb / 0.13).clamp(0.0, 1.0)
            } else {
                0.0
            };
            if bot_rim > 0.0 {
                r = r * (1.0 - bot_rim) + 70.0 * bot_rim;
                g = g * (1.0 - bot_rim) + 104.0 * bot_rim;
                b = b * (1.0 - bot_rim) + 120.0 * bot_rim;
                a = a.max(170.0 * bot_rim);
            }
            // Vertical side edges.
            if hx > 0.98 {
                let s = (hx - 0.98) / 0.02;
                a = a.max(70.0 + 90.0 * s);
                r = r.max(40.0);
                g = g.max(80.0);
                b = b.max(96.0);
            }

            // Antialias the silhouette edges.
            let mut aa = 1.0;
            if yf < top_edge {
                aa = (1.0 - (top_edge - yf)).clamp(0.0, 1.0);
            }
            if yf > bot_edge {
                aa = (1.0 - (yf - bot_edge)).clamp(0.0, 1.0);
            }
            if hx > 1.0 {
                aa *= (1.0 - (hx - 1.0) / 0.03).clamp(0.0, 1.0);
            }
            a *= aa;

            px[idx] = r.clamp(0.0, 255.0) as u8;
            px[idx + 1] = g.clamp(0.0, 255.0) as u8;
            px[idx + 2] = b.clamp(0.0, 255.0) as u8;
            px[idx + 3] = a.clamp(0.0, 255.0) as u8;
        }
    }
    ImageData::new(px, w, h)
}

/// The animated cylinder. Chips continuously **churn** — each rains in on its
/// own staggered loop to a fixed spot, sits, then fades and refreshes — so the
/// DB always looks busy ingesting, with no synchronized reset. `sort` (0→1)
/// blends that churn toward a tidy aligned grid (and cool→teal), so a shot can
/// make the data "index" exactly when the query is explained. `key` unique/size.
fn cylinder(_lt: f32, t: f32, cw: f32, ch: f32, key: &str, sort: f32) -> Element {
    const PROLOGUE: f32 = 92.0;
    let gh = ch + PROLOGUE;
    let oy = PROLOGUE;
    let cx = cw * 0.5;
    let rx = cw * 0.5 - 3.0;
    let ry = rx * 0.32;
    let top_cy = oy + ry + 3.0;
    let bot_cy = oy + ch - ry - 3.0;
    let inner_rx = rx * 0.68;
    let left = cx - inner_rx;
    let iw = inner_rx * 2.0;
    let region_top = top_cy + ry * 0.7;
    let region_bot = bot_cy - 6.0;
    let region_h = region_bot - region_top;

    // The sorted layout: a tidy 3×3 grid the chips align into, with airy gaps.
    let cols = 3usize;
    let rows = 3usize;
    let n = cols * rows;
    let margin = 6.0;
    let cell_w = iw / cols as f32;
    let cell_h = region_h / rows as f32;
    let chip_w = cell_w - 12.0;
    let chip_h = cell_h * 0.5;

    // Fixed spots each chip churns in (fractions of the body) — spread out.
    let scatter: [(f32, f32); 9] = [
        (0.08, 0.16),
        (0.72, 0.08),
        (0.44, 0.30),
        (0.12, 0.58),
        (0.82, 0.48),
        (0.58, 0.74),
        (0.28, 0.86),
        (0.88, 0.82),
        (0.64, 0.40),
    ];

    let mut kids: Vec<Element> = Vec::new();

    for i in 0..n {
        let (fx, fy) = scatter[i];
        let scat_x = left + margin + fx * (iw - chip_w - 2.0 * margin);
        let scat_y = region_top + margin + fy * (region_h - chip_h - 2.0 * margin);
        // Aligned grid slot: chip centered in its cell (row 0 at top).
        let c = (i % cols) as f32;
        let r = (i / cols) as f32;
        let grid_x = left + c * cell_w + (cell_w - chip_w) * 0.5;
        let grid_y = region_top + r * cell_h + (cell_h - chip_h) * 0.5;

        // Continuous churn: this chip's own staggered loop — rain in, sit, fade,
        // repeat. Never all-at-once, so the ingest reads as ongoing, not a reset.
        let ph = (t * 0.38 + i as f32 / n as f32).rem_euclid(1.0);
        let fall = interpolate_eased(ph, [0.0, 0.16], [0.0, 1.0], Easing::EaseOutCubic);
        let churn_x = scat_x;
        let churn_y = 6.0 + fall * (scat_y - 6.0);
        let churn_o = interpolate(ph, [0.0, 0.1], [0.0, 0.92])
            * interpolate(ph, [0.82, 1.0], [1.0, 0.0]);

        // Blend churn → aligned grid by `sort` (steady + teal once sorted).
        let x = churn_x + (grid_x - churn_x) * sort;
        let y = churn_y + (grid_y - churn_y) * sort;
        let o = churn_o + (0.95 - churn_o) * sort;

        let color = interpolate_color(sort, [0.0, 1.0], hex("#6f7be0"), hex("#5eead4"));
        kids.push(
            div()
                .absolute()
                .pos(x, y)
                .w(Px(chip_w))
                .h(Px(chip_h))
                .bg(color)
                .rounded_px(3.0)
                .opacity(o),
        );
    }

    // Glass shell on top — frames the chips (and occludes them at the lid as
    // they fall in) without any clip.
    kids.push(
        image(key, cylinder_shell(cw as u32, ch as u32))
            .w(Px(cw))
            .h(Px(ch))
            .absolute()
            .pos(0.0, oy),
    );

    div().w(Px(cw)).h(Px(gh)).children(kids)
}

// ── reusable pieces ──────────────────────────────────────────────────────────

fn kicker(s: &str, o: f32, y: f32) -> Element {
    text(s)
        .font_size(16.0)
        .bold()
        .letter_spacing(4.0)
        .color(hex("#5eead4"))
        .opacity(o)
        .ty(y)
}

fn bars(lt: f32) -> Element {
    let targets = [200.0_f32, 280.0, 150.0, 240.0];
    let rows: Vec<Element> = targets
        .iter()
        .enumerate()
        .map(|(i, &tw)| {
            let d = 0.5 + i as f32 * 0.1;
            let fill = interpolate_eased(lt, [d, d + 0.7], [0.0, tw], Easing::EaseOutCubic);
            let o = interpolate(lt, [d, d + 0.3], [0.0, 1.0]);
            div()
                .w(Px(280.0))
                .h(Px(11.0))
                .bg(hex("#141a26"))
                .rounded_px(6.0)
                .opacity(o)
                .child(
                    div()
                        .w(Px(fill))
                        .h(Px(11.0))
                        .gradient(hex("#5eead4"), hex("#a78bfa"), 0.0)
                        .rounded_px(6.0),
                )
        })
        .collect();
    div().flex_col().gap(10.0).mt(Px(18.0)).children(rows)
}

/// Big display text.
fn headline(s: &str, size: f32, o: f32, y: f32) -> Element {
    text(s)
        .font_size(size)
        .bold()
        .letter_spacing(-1.5)
        .color(hex("#eef1f8"))
        .opacity(o)
        .ty(y)
}

fn subline(s: &str, o: f32, y: f32) -> Element {
    text(s)
        .font_size(25.0)
        .color(hex("#8b95ad"))
        .opacity(o)
        .ty(y)
}

// ── shots (each returns the shot's content group) ────────────────────────────

fn shot_intro(lt: f32, t: f32) -> Element {
    let app = |d: f32| interpolate(lt, [d, d + 0.5], [0.0, 1.0]);
    div()
        .flex_col()
        .items_center()
        .gap(26.0)
        .child(cylinder(lt, t, 250.0, 300.0, "cyl-hero", 0.0))
        .child(
            text("EnchuDB")
                .font_size(96.0)
                .bold()
                .letter_spacing(-3.0)
                .color(hex("#eef1f8"))
                .opacity(app(0.5))
                .ty((1.0 - spring_with((lt - 0.5).max(0.0), FPS, Spring::bouncy())) * 18.0),
        )
        .child(
            text("a pure-Rust embedded database")
                .font_size(28.0)
                .letter_spacing(1.0)
                .color(hex("#5eead4"))
                .opacity(app(0.85)),
        )
}

fn shot_what(lt: f32, t: f32) -> Element {
    let app = |d: f32| interpolate(lt, [d, d + 0.5], [0.0, 1.0]);
    div()
        .flex_row()
        .items_center()
        .gap(80.0)
        .child(cylinder(lt, t, 200.0, 260.0, "cyl-side", 0.0))
        .child(
            div()
                .flex_col()
                .items_start()
                .gap(14.0)
                .child(kicker("WHAT IT IS", app(0.15), rz(lt, 0.15)))
                .child(headline("One file.", 78.0, app(0.3), rz(lt, 0.3)))
                .child(subline("No server. No daemon.", app(0.55), rz(lt, 0.55))),
        )
}

fn shot_speed(lt: f32, t: f32) -> Element {
    let app = |d: f32| interpolate(lt, [d, d + 0.5], [0.0, 1.0]);
    div()
        .flex_row()
        .items_center()
        .gap(72.0)
        .child(cylinder(lt, t, 200.0, 260.0, "cyl-side", interpolate(lt, [0.7, 1.9], [0.0, 1.0])))
        .child(
            div()
                .flex_col()
                .items_start()
                .gap(12.0)
                .child(kicker("SPEED", app(0.0), rz(lt, 0.0)))
                .child(headline("multi-condition AND", 50.0, app(0.15), rz(lt, 0.15)))
                .child(
                    text("in nanoseconds")
                        .font_size(74.0)
                        .bold()
                        .letter_spacing(-1.0)
                        .color(hex("#5eead4"))
                        .opacity(app(0.35))
                        .ty((1.0 - spring_with((lt - 0.35).max(0.0), FPS, Spring::snappy())) * 20.0),
                )
                .child(bars(lt)),
        )
}

fn shot_outro(lt: f32, t: f32) -> Element {
    let app = |d: f32| interpolate(lt, [d, d + 0.5], [0.0, 1.0]);
    div()
        .flex_col()
        .items_center()
        .gap(22.0)
        .child(cylinder(lt, t, 170.0, 216.0, "cyl-out", 1.0))
        .child(
            text("EnchuDB")
                .font_size(84.0)
                .bold()
                .letter_spacing(-2.0)
                .color(hex("#eef1f8"))
                .opacity(app(0.3))
                .ty(rz(lt, 0.3)),
        )
        .child(
            text("embedded · fast · pure Rust")
                .font_size(26.0)
                .letter_spacing(1.0)
                .color(hex("#5eead4"))
                .opacity(app(0.55)),
        )
}

// ── composition ──────────────────────────────────────────────────────────────

/// Wrap a shot's content in a full-screen layer. Opacity is the dissolve
/// envelope; a vertical flow (enter from below, exit upward) makes the
/// transition directional rather than a flat crossfade.
fn stage(t: f32, start: f32, dur: f32, content: Element) -> Option<Element> {
    let o = envelope(t, start, dur);
    if o <= 0.001 {
        return None;
    }
    let fade_in = interpolate(t, [start, start + 0.55], [0.0, 1.0]);
    let exit = interpolate(t, [start + dur - 0.55, start + dur], [0.0, 1.0]);
    let flow = (1.0 - fade_in) * 40.0 - exit * 40.0;
    Some(
        div()
            .absolute()
            .pos(0.0, 0.0)
            .w(Px(W))
            .h(Px(H))
            .flex_col()
            .items_center()
            .justify_center()
            .opacity(o)
            .ty(flow)
            .child(content),
    )
}

fn scene(ctx: FrameCtx) -> Element {
    let t = ctx.t();
    // (start, dur) — consecutive shots overlap by ~0.55 for the dissolve.
    let shots: [(f32, f32); 4] = [
        // First shot is pre-rolled (negative start) so frame 0 already shows the
        // hero — the file opens on content, not the black fade-in.
        (-0.6, 4.8),
        (3.8, 4.2),
        (7.6, 5.2),
        (12.4, 4.4),
    ];

    let mut layers: Vec<Element> = Vec::new();
    for (i, &(start, dur)) in shots.iter().enumerate() {
        if envelope(t, start, dur) <= 0.001 {
            continue;
        }
        let lt = t - start;
        let content = match i {
            0 => shot_intro(lt, t),
            1 => shot_what(lt, t),
            2 => shot_speed(lt, t),
            _ => shot_outro(lt, t),
        };
        if let Some(layer) = stage(t, start, dur, content) {
            layers.push(layer);
        }
    }

    div()
        .w(Px(W))
        .h(Px(H))
        .gradient(hex("#05060d"), hex("#0a1120"), FRAC_PI_2)
        .children(layers)
}

fn main() {
    let reel = Reel::new(W as u32, H as u32, FPS, FPS * SECS);
    let out = std::env::temp_dir().join("enchu-explainer.mp4");

    println!(
        "rendering + encoding {} frames ({}x{} @ {FPS}fps) …",
        reel.frames, reel.width, reel.height
    );
    match reel.render_mp4(&scene, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => {
            eprintln!("encode failed: {e}");
            let dir = std::env::temp_dir().join("enchu-explainer");
            let paths = reel.render_pngs(&scene, &dir).expect("render failed");
            eprintln!("wrote {} PNGs instead -> {}", paths.len(), dir.display());
        }
    }
}
