//! sabitori-reel demo: a Sabitori-native EnchuDB explainer, authored as a
//! declarative Element tree (React/Remotion style) and rendered offscreen to a
//! PNG sequence. Exercises the full primitive set — text (incl. CJK), rounded
//! rects, **gradients**, a generated **image**, an animated **polyline**, plus
//! the `interpolate` / `spring` / `Seq` helpers. Layout is all Sabitori flex.
//!
//!   cargo run -p sabitori-reel --example demo --release
//!
//! Output: `$TMPDIR/sabitori-reel-demo/frame_00000.png` …

use std::f32::consts::FRAC_PI_2;

use sabitori_reel::{
    div, image, interpolate, interpolate_eased, polyline, spring_with, text, Color, Easing, Element,
    FrameCtx, ImageData, Px, Reel, Seq, Spring,
};

const FPS: u32 = 30;
const SECS: u32 = 4;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}

/// Generate a small round brand mark procedurally (no asset file needed): a
/// radial teal→purple disc on a transparent background, as raw RGBA pixels.
fn logo(size: u32) -> ImageData {
    let n = size as usize;
    let mut px = vec![0u8; n * n * 4];
    let c = (size as f32 - 1.0) * 0.5;
    let radius = c * 0.94;
    for y in 0..n {
        for x in 0..n {
            let (dx, dy) = (x as f32 - c, y as f32 - c);
            let dist = (dx * dx + dy * dy).sqrt();
            let g = (dist / radius).clamp(0.0, 1.0); // 0 center → 1 edge
            let idx = (y * n + x) * 4;
            // teal #5eead4 (94,234,212) → purple #a78bfa (167,139,250)
            px[idx] = (94.0 + (167.0 - 94.0) * g) as u8;
            px[idx + 1] = (234.0 + (139.0 - 234.0) * g) as u8;
            px[idx + 2] = (212.0 + (250.0 - 212.0) * g) as u8;
            // 1px-soft circular edge for antialiasing
            px[idx + 3] = (255.0 * (radius - dist + 0.5).clamp(0.0, 1.0)) as u8;
        }
    }
    ImageData::new(px, size, size)
}

/// An animated sparkline — a `polyline` whose points are a sine wave that
/// flows with time `t`. Points are logical px inside its own sized box.
fn sparkline(t: f32) -> Element {
    let (w, h, n) = (420.0_f32, 56.0_f32, 48usize);
    let pts: Vec<(f32, f32)> = (0..=n)
        .map(|i| {
            let x = i as f32 / n as f32 * w;
            let phase = i as f32 * 0.45 + t * 2.4;
            (x, h * 0.5 - phase.sin() * (h * 0.34))
        })
        .collect();
    polyline()
        .points(pts)
        .stroke_width(2.5)
        .stroke_color(hex("#5eead4"))
        .w(Px(w))
        .h(Px(h))
}

/// A solid capability card: title + subtitle in a bordered, rounded box.
fn card(title: &str, sub: &str, accent: &str) -> Element {
    div()
        .flex_col()
        .gap(6.0)
        .p_px(30.0)
        .w(Px(300.0))
        .bg(hex("#12151f"))
        .rounded_px(18.0)
        .border(1.5, hex("#242a38"))
        .children([
            text(title).font_size(34.0).bold().color(hex(accent)),
            text(sub).font_size(19.0).color(hex("#818da6")),
        ])
}

/// Same card, but with a diagonal gradient fill instead of a flat background.
fn grad_card(title: &str, sub: &str, from: &str, to: &str) -> Element {
    div()
        .flex_col()
        .gap(6.0)
        .p_px(30.0)
        .w(Px(300.0))
        .gradient(hex(from), hex(to), 0.6)
        .rounded_px(18.0)
        .border(1.5, hex("#39304f"))
        .children([
            text(title).font_size(34.0).bold().color(hex("#eaeef7")),
            text(sub).font_size(19.0).color(hex("#c3b7e6")),
        ])
}

/// The EnchuDB explainer scene — a pure function of the frame. Shared verbatim
/// with the `preview` example.
fn scene(ctx: FrameCtx) -> Element {
    let t = ctx.t();
    // Opacity ramp 0→1 over 0.5s, starting at `delay`.
    let appear = |delay: f32| interpolate(t, [delay, delay + 0.5], [0.0, 1.0]);
    // Eased rise from 24px below, settling to 0.
    let rise = |delay: f32| interpolate_eased(t, [delay, delay + 0.6], [24.0, 0.0], Easing::EaseOutCubic);

    let brand = image("logo", logo(88)).w(Px(88.0)).h(Px(88.0)).opacity(appear(0.0));

    // Title: fades in while a bouncy spring drives an overshooting rise.
    let title = text("EnchuDB")
        .font_size(104.0)
        .bold()
        .letter_spacing(-3.0)
        .color(hex("#eaeef7"))
        .opacity(appear(0.1))
        .ty((1.0 - spring_with(t, ctx.fps, Spring::bouncy())) * 22.0);

    let subtitle = text("円柱 database · embedded · single-file")
        .font_size(30.0)
        .color(hex("#5eead4"))
        .opacity(appear(0.4))
        .ty(rise(0.4));

    let cards = div()
        .flex_row()
        .gap(24.0)
        .mt(Px(24.0))
        .opacity(appear(0.7))
        .children([
            card("SQL", "SQLite-superset frontend", "#5eead4"),
            card("C ABI", "12-function FFI", "#a78bfa"),
            grad_card("RAG", "vector store · meta-filtered", "#134e48", "#3b2764"),
        ]);

    let signal = div().opacity(appear(0.95)).child(sparkline(t));

    // Tagline as its own Sequence: mounts at t=1.4s with a local clock, in a
    // fixed-height slot so mounting doesn't reflow the centered column.
    let tagline = {
        let slot = div().h(Px(34.0)).items_center().justify_center();
        match Seq::new(1.4, 10.0).enter(ctx) {
            Some(local) => slot.child(
                text("multi-condition AND — in nanoseconds")
                    .font_size(26.0)
                    .color(hex("#818da6"))
                    .opacity(interpolate(local.t(), [0.0, 0.4], [0.0, 1.0]))
                    .ty(interpolate_eased(local.t(), [0.0, 0.4], [16.0, 0.0], Easing::EaseOutCubic)),
            ),
            None => slot,
        }
    };

    div()
        .w(Px(ctx.width))
        .h(Px(ctx.height))
        .gradient(hex("#05070d"), hex("#0b1120"), FRAC_PI_2)
        .flex_col()
        .items_center()
        .justify_center()
        .gap(16.0)
        .children([brand, title, subtitle, cards, signal, tagline])
}

fn main() {
    let reel = Reel::new(1280, 720, FPS, FPS * SECS);
    let out = std::env::temp_dir().join("sabitori-reel-demo.mp4");

    println!(
        "rendering + encoding {} frames ({}x{} @ {FPS}fps) …",
        reel.frames, reel.width, reel.height
    );
    match reel.render_mp4(&scene, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => {
            eprintln!("encode failed: {e}");
            // Frames still rendered — fall back to writing them somewhere handy.
            let dir = std::env::temp_dir().join("sabitori-reel-demo");
            let paths = reel.render_pngs(&scene, &dir).expect("render failed");
            eprintln!("wrote {} PNGs instead -> {}", paths.len(), dir.display());
        }
    }
}
