//! **The** Sabitori homepage — a single, definitive, GPU-native landing page.
//!
//! This consolidates the best of the experiment set into one routed site:
//!
//!   * a persistent aurora + starfield backdrop (ambient chrome),
//!   * a nav whose active underline springs to the current tab,
//!   * four sections that slide + crossfade as you navigate,
//!   * a **live Gallery** — an orbital physics field, a flow-field tile, and a
//!     spring-bar tile, all three integrating every `tick()` right on the page.
//!
//! The whole tree is rebuilt every frame on the GPU (`is_animating() -> true`).
//! No router library, no canvas, no HTML, no CSS. This is what the framework
//! looks like when it draws its own front door.
//!
//! `cargo run --example sabitori_home`

use sabitori::*;
use sabitori_markdown::{render_markdown, MarkdownOptions, MarkdownTheme};

// ── Palette (Tokyo Night-ish) ────────────────────────────────────────────
const BG0: &str = "#0a0c16";
const BG1: &str = "#12162c";
const SURFACE: &str = "#171b2e";
const SURFACE_HI: &str = "#20263f";
const PANEL: &str = "#0c0f1e";
const BORDER: &str = "#2a3152";
const TEXT_HI: &str = "#eef1ff";
const TEXT_MID: &str = "#a7afd6";
const TEXT_DIM: &str = "#6b74a0";
const ACCENT_BLUE: &str = "#7aa2f7";
const ACCENT_PURPLE: &str = "#bb9af7";
const ACCENT_CYAN: &str = "#7dcfff";
const ACCENT_GREEN: &str = "#9ece6a";
const ACCENT_AMBER: &str = "#e0af68";
const ACCENT_PINK: &str = "#f7768e";

const TABS: [&str; 7] = ["Home", "Why", "Features", "Widgets", "Gallery", "Code", "Start"];

/// Headlines the hero cycles through (with a quick fade at each swap).
const HEADS: [&str; 3] = ["renders like a shader.", "flows like a current.", "springs like physics."];

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn ease_out_cubic(x: f32) -> f32 {
    1.0 - (1.0 - x).powi(3)
}
fn ease_back(x: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}
fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    x = ((x >> ((x >> 28).wrapping_add(4))) ^ x).wrapping_mul(277_803_737);
    x = (x >> 22) ^ x;
    (x as f32) / (u32::MAX as f32)
}

// ── Aurora backdrop ───────────────────────────────────────────────────────
// A soft radial light is a *small* core with a *large* glow radius — NOT a big
// filled ellipse (that reads as a solid "lemon"). Glow is an SDF blur.
fn glow_blob(cx: f32, cy: f32, r: f32, col: Color, op: f32, glow_r: f32) -> Element {
    div()
        .absolute()
        .pos(cx - r, cy - r)
        .w(Px(r * 2.0))
        .h(Px(r * 2.0))
        .rounded_px(r)
        .bg(col)
        .opacity(op)
        .glow(col, glow_r)
}

// Planets orbiting the wordmark's "sun": (orbit radius as fraction of h, disc
// size px, angular speed, phase, colour). Inner planets orbit faster.
const PLANETS: [(f32, f32, f32, f32, &str); 6] = [
    (0.100, 7.0, 0.42, 0.0, ACCENT_CYAN),
    (0.155, 11.0, 0.30, 1.4, ACCENT_BLUE),
    (0.215, 9.0, 0.23, 2.9, ACCENT_GREEN),
    (0.285, 14.0, 0.165, 0.7, ACCENT_PURPLE),
    (0.355, 10.0, 0.120, 3.6, ACCENT_PINK),
    (0.425, 8.0, 0.090, 5.0, ACCENT_AMBER),
];

fn backdrop(w: f32, h: f32, t: f32) -> Element {
    use std::f32::consts::TAU;
    // Oblique "orrery" view: everything on the orbital plane is squashed
    // vertically by TILT, so orbits + rings read as ellipses, not top-down
    // circles. That's what lets a ring have real depth.
    const TILT: f32 = 0.42;
    let cx = w * 0.5;
    let cy = h * 0.5;
    let mut kids: Vec<Element> = Vec::new();

    // ── Nebula clouds — big soft gradient blobs, slowly drifting. ──
    let nebulae: [(f32, f32, f32, f32, &str, f32, f32); 5] = [
        (0.15, 0.22, 46.0, 300.0, ACCENT_PURPLE, 0.13, 0.0),
        (0.87, 0.16, 42.0, 280.0, ACCENT_CYAN, 0.10, 1.7),
        (0.74, 0.82, 50.0, 320.0, ACCENT_BLUE, 0.10, 3.1),
        (0.26, 0.84, 40.0, 260.0, ACCENT_PINK, 0.08, 4.6),
        (0.50, 0.28, 44.0, 300.0, "#3a2d6e", 0.15, 2.2),
    ];
    for (fx, fy, r, gr, hue, op, ph) in nebulae {
        let dx = (t * 0.05 + ph).sin() * 14.0;
        let dy = (t * 0.04 + ph * 1.3).cos() * 10.0;
        kids.push(glow_blob(w * fx + dx, h * fy + dy, r, hex(hue), op, gr));
    }

    // ── Starfield — parallax depth, drifting + twinkling; every 7th is a glint. ──
    for i in 0..96u32 {
        let rx = hash01(i * 2 + 1);
        let ry = hash01(i * 7 + 3);
        let depth = 0.30 + 0.70 * hash01(i * 5 + 2);
        let speed = 4.0 + 22.0 * hash01(i * 11 + 4);
        let y = (ry * h - t * speed).rem_euclid(h + 24.0) - 12.0;
        let x = rx * w;
        let size = 0.9 + 2.4 * depth;
        let twinkle = 0.08 + 0.34 * (0.5 + 0.5 * (t * (1.0 + 2.0 * hash01(i * 13 + 6)) + rx * 6.28).sin());
        if i % 7 == 0 {
            let g = hex(TEXT_HI);
            let a = (twinkle * depth * 1.4).min(1.0);
            kids.push(div().absolute().pos(x - size, y - size).w(Px(size * 2.0)).h(Px(size * 2.0)).rounded_px(size).bg(g).glow_sm(hex(ACCENT_CYAN)).opacity(a));
            let sp = size * 3.5;
            kids.push(div().absolute().pos(x - sp, y - 0.4).w(Px(sp * 2.0)).h(Px(0.8)).bg(g).opacity(twinkle * depth * 0.5));
            kids.push(div().absolute().pos(x - 0.4, y - sp).w(Px(0.8)).h(Px(sp * 2.0)).bg(g).opacity(twinkle * depth * 0.5));
        } else {
            kids.push(div().absolute().pos(x, y).w(Px(size)).h(Px(size)).rounded_px(size).bg(hex(TEXT_HI)).opacity(twinkle * depth));
        }
    }

    // ── Oblique orbit ellipses — faint dotted rings (front arc a touch brighter). ──
    for (frac, _, _, _, _) in PLANETS {
        let rr = h * frac;
        let n = 30u32;
        for k in 0..n {
            let th = k as f32 / n as f32 * TAU;
            let x = cx + rr * th.cos();
            let yy = cy + rr * TILT * th.sin();
            let fb = 0.5 + 0.5 * th.sin();
            kids.push(div().absolute().pos(x - 1.0, yy - 1.0).w(Px(2.0)).h(Px(2.0)).rounded_px(1.0).bg(hex("#2a3560")).opacity(0.10 + 0.13 * fb));
        }
    }

    // ── Planets — depth-sorted, with tilted rings whose back half the planet occludes. ──
    let pos: Vec<(f32, f32, f32)> = PLANETS
        .iter()
        .map(|p| {
            let (frac, _, spd, ph, _) = *p;
            let rr = h * frac;
            let ang = spd * t + ph;
            (cx + rr * ang.cos(), cy + rr * TILT * ang.sin(), 0.5 + 0.5 * ang.sin())
        })
        .collect();
    let mut order: Vec<usize> = (0..PLANETS.len()).collect();
    order.sort_by(|&a, &b| pos[a].2.partial_cmp(&pos[b].2).unwrap()); // far (top) first

    for &i in &order {
        let (frac, size, spd, ph, col) = PLANETS[i];
        let (px, py, depth) = pos[i];
        let rr = h * frac;
        let ang = spd * t + ph;
        let psize = size * (0.72 + 0.4 * depth);
        let pcol = hex(col);

        // Orbit trail behind the planet, along the (squashed) orbit.
        for k in 1..6u32 {
            let a2 = ang - k as f32 * 0.05;
            let tx = cx + a2.cos() * rr;
            let ty = cy + a2.sin() * rr * TILT;
            let f = 1.0 - k as f32 / 6.0;
            let sz = psize * (0.35 + 0.45 * f);
            kids.push(div().absolute().pos(tx - sz / 2.0, ty - sz / 2.0).w(Px(sz)).h(Px(sz)).rounded_px(sz / 2.0).bg(pcol).opacity(0.14 * f * (0.4 + 0.6 * depth)));
        }

        let planet = div()
            .absolute()
            .pos(px - psize / 2.0, py - psize / 2.0)
            .w(Px(psize))
            .h(Px(psize))
            .rounded_px(psize / 2.0)
            .gradient(pcol, pcol.lerp(hex("#000000"), 0.5), 55.0)
            .glow_sm(pcol)
            .opacity(0.6 + 0.4 * depth);

        if i == 3 || i == 5 {
            // Tilted elliptical ring, two bands. Back half (sinθ<0) is pushed
            // before the planet disc → occluded; front half after → in front.
            let tint = pcol.lerp(hex("#ffffff"), 0.4);
            let (mut back, mut front): (Vec<Element>, Vec<Element>) = (Vec::new(), Vec::new());
            for band in [1.75_f32, 2.25] {
                let a = psize * band;
                let n = 44u32;
                for k in 0..n {
                    let th = k as f32 / n as f32 * TAU;
                    let s = th.sin();
                    let rx = px + a * th.cos();
                    let ry = py + a * TILT * s;
                    let dsz = 1.9;
                    let op = (0.30 + 0.4 * (0.5 + 0.5 * s)) * (0.5 + 0.5 * depth);
                    let dot = div().absolute().pos(rx - dsz / 2.0, ry - dsz / 2.0).w(Px(dsz)).h(Px(dsz)).rounded_px(dsz / 2.0).bg(tint).opacity(op);
                    if s < 0.0 {
                        back.push(dot);
                    } else {
                        front.push(dot);
                    }
                }
            }
            kids.append(&mut back);
            kids.push(planet);
            kids.append(&mut front);
        } else {
            kids.push(planet);
        }
    }

    // ── The sun — a layered corona around a hot white core, gently breathing. ──
    let pulse = 1.0 + 0.06 * (t * 0.9).sin();
    kids.push(glow_blob(cx, cy, 40.0 * pulse, hex("#e8873a"), 0.16, 260.0));
    kids.push(glow_blob(cx, cy, 26.0 * pulse, hex(ACCENT_AMBER), 0.30, 180.0));
    kids.push(glow_blob(cx, cy, 15.0 * pulse, hex("#ffd98a"), 0.50, 110.0));
    kids.push(div().absolute().pos(cx - 15.0, cy - 15.0).w(Px(30.0)).h(Px(30.0)).rounded_px(15.0).bg(hex("#fff3d8")).glow(hex("#ffcf8f"), 70.0));

    // ── Shooting stars — occasional foreground streaks on their own cadence. ──
    let comets: [(f32, f32, f32, f32, f32, f32, &str); 2] = [
        (0.05, 0.08, 0.42, 0.36, 7.0, 0.0, ACCENT_CYAN),
        (0.96, 0.14, 0.60, 0.44, 9.5, 3.5, "#ffffff"),
    ];
    for (sxf, syf, exf, eyf, period, phase, col) in comets {
        let p = ((t + phase) / period).rem_euclid(1.0);
        if p < 0.32 {
            let q = p / 0.32;
            let (sx, sy) = (w * sxf, h * syf);
            let (ex, ey) = (w * exf, h * eyf);
            let hx = lerp(sx, ex, q);
            let hy = lerp(sy, ey, q);
            let (dvx, dvy) = (ex - sx, ey - sy);
            let len = (dvx * dvx + dvy * dvy).sqrt().max(1.0);
            let (ux, uy) = (dvx / len, dvy / len);
            let vis = (1.0 - q).min(q * 4.0);
            for k in 1..8u32 {
                let d = k as f32 * 9.0;
                let (tx, ty) = (hx - ux * d, hy - uy * d);
                let f = 1.0 - k as f32 / 8.0;
                let sz = 1.0 + 2.2 * f;
                kids.push(div().absolute().pos(tx - sz / 2.0, ty - sz / 2.0).w(Px(sz)).h(Px(sz)).rounded_px(sz).bg(hex(col)).opacity(0.5 * f * vis));
            }
            kids.push(div().absolute().pos(hx - 2.0, hy - 2.0).w(Px(4.0)).h(Px(4.0)).rounded_px(2.0).bg(hex("#ffffff")).glow_sm(hex(col)).opacity(vis));
        }
    }

    div().absolute().pos(0.0, 0.0).w(Px(w)).h(Px(h)).children(kids)
}

// ── Shared bits ───────────────────────────────────────────────────────────
fn eyebrow(txt: &str) -> Element {
    div()
        .flex_row()
        .items_center()
        .px_pad(Px(14.0))
        .py(Px(7.0))
        .rounded_px(13.0)
        .bg(hex(SURFACE))
        .border(1.0, hex(BORDER))
        .children([text(txt).font_size(11.0).font_weight(600).letter_spacing(1.6).color(hex(TEXT_DIM))])
}
fn live_badge(t: f32, fps: f32) -> Element {
    let pulse = 0.30 + 0.70 * (0.5 + 0.5 * (t * 2.2).sin());
    div()
        .flex_row()
        .items_center()
        .gap(7.0)
        .px_pad(Px(10.0))
        .py(Px(5.0))
        .rounded_px(13.0)
        .bg(hex(SURFACE))
        .border(1.0, hex(BORDER))
        .children([
            div().w(Px(7.0)).h(Px(7.0)).rounded_px(3.5).bg(hex(ACCENT_GREEN)).glow_sm(hex(ACCENT_GREEN)).opacity(pulse),
            text(&format!("{fps:.0} FPS")).mono().font_size(10.5).font_weight(600).letter_spacing(1.0).color(hex(TEXT_MID)),
        ])
}
fn cta_primary(id: &str, label: &str) -> Element {
    div()
        .id(id)
        .cursor(Cursor::Pointer)
        .flex_row()
        .items_center()
        .justify_center()
        .px_pad(Px(22.0))
        .py(Px(13.0))
        .rounded_px(11.0)
        .gradient(hex(ACCENT_BLUE), hex(ACCENT_PURPLE), 0.0)
        .glow_sm(hex(ACCENT_BLUE))
        .hover(|s| s.translate_y(-2.0).glow(hex(ACCENT_PURPLE), 22.0))
        .spring_transition(260.0, 22.0)
        .children([text(label).font_size(14.5).font_weight(600).letter_spacing(0.3).color(hex("#0b0d16"))])
}
fn cta_ghost(id: &str, label: &str) -> Element {
    div()
        .id(id)
        .cursor(Cursor::Pointer)
        .flex_row()
        .items_center()
        .justify_center()
        .px_pad(Px(22.0))
        .py(Px(13.0))
        .rounded_px(11.0)
        .bg(hex(SURFACE))
        .border(1.0, hex(BORDER))
        .hover(|s| s.bg(hex(SURFACE_HI)).border_color(hex(ACCENT_BLUE)))
        .spring_transition(260.0, 22.0)
        .children([text(label).font_size(14.5).font_weight(500).color(hex(TEXT_HI))])
}

// ── Nav bar with a spring-driven active underline ─────────────────────────
fn nav_bar(cw: f32, cur: usize, prev: usize, trans: f32, accent: Color, t: f32, fps: f32) -> Element {
    let n = TABS.len();
    let tabw = 84.0;
    let gap = 6.0;
    let center = |i: usize| (i as f32) * (tabw + gap) + tabw / 2.0;
    let ux = lerp(center(prev), center(cur), ease_back(trans));
    let uw = 22.0;
    let underline = div()
        .absolute()
        .pos(ux - uw / 2.0, 34.0)
        .w(Px(uw))
        .h(Px(3.0))
        .rounded_px(1.5)
        .bg(accent)
        .glow_sm(accent);

    let tabs: Vec<Element> = (0..n)
        .map(|i| {
            let active = i == cur;
            div()
                .id(format!("nav-{i}"))
                .cursor(Cursor::Pointer)
                .w(Px(tabw))
                .h(Px(30.0))
                .rounded_px(8.0)
                .flex_row()
                .items_center()
                .justify_center()
                .hover(|s| s.bg(hex(SURFACE_HI)))
                .spring_transition(240.0, 24.0)
                .children([text(TABS[i])
                    .font_size(13.5)
                    .font_weight(if active { 600 } else { 500 })
                    .letter_spacing(0.2)
                    .color(if active { hex(TEXT_HI) } else { hex(TEXT_MID) })])
        })
        .collect();
    let tabs_box = div()
        .flex_col()
        .w(Px(n as f32 * tabw + (n as f32 - 1.0) * gap))
        .h(Px(40.0))
        .children([div().flex_row().gap(gap).children(tabs), underline]);

    let logo = div().flex_row().items_center().gap(10.0).children([
        div().w(Px(22.0)).h(Px(22.0)).rounded_px(6.0).gradient(hex(ACCENT_BLUE), hex(ACCENT_PURPLE), 45.0).glow_sm(hex(ACCENT_BLUE)),
        text("sabitori").font_size(18.0).font_weight(600).letter_spacing(0.2).color(hex(TEXT_HI)),
    ]);
    let right = div().flex_row().items_center().gap(18.0).children([live_badge(t, fps), tabs_box]);

    div().w(Px(cw)).h(Px(52.0)).flex_row().items_center().justify_between().children([logo, right])
}

// ── Section 0: Home ────────────────────────────────────────────────────────
/// One "big number + label" tile for the stats band.
fn stat(num: &str, label: &str, col: Color) -> Element {
    div().flex_col().items_center().gap(2.0).w(Px(120.0)).children([
        text(num).font_size(30.0).font_weight(600).letter_spacing(-1.0).color(col),
        text(label).font_size(11.5).font_weight(500).letter_spacing(0.6).color(hex(TEXT_DIM)),
    ])
}

fn home(accent: Color, t: f32, fps: f32) -> Element {
    // Cycle the second headline line, fading out/in at each swap.
    let period = 3.2;
    let ph = (t / period).rem_euclid(1.0);
    let idx = ((t / period) as usize) % HEADS.len();
    let fade = (ph.min(1.0 - ph) * 9.0).min(1.0);

    let divider = || div().w(Px(1.0)).h(Px(30.0)).bg(hex(BORDER));

    div().flex_col().items_center().gap(24.0).children([
        eyebrow("V0.2.8 · GPU-NATIVE UI FRAMEWORK FOR RUST"),
        div().flex_col().items_center().gap(0.0).children([
            text("The UI framework that")
                .font_size(58.0)
                .font_weight(400)
                .letter_spacing(-1.6)
                .line_height(1.08)
                .color(hex(TEXT_HI)),
            text(HEADS[idx])
                .font_size(58.0)
                .font_weight(500)
                .letter_spacing(-1.6)
                .line_height(1.08)
                .color(accent)
                .opacity(0.15 + 0.85 * fade),
        ]),
        div().flex_col().items_center().gap(2.0).children([
            text("Flexbox, SDF styling, spring physics and routing — one declarative")
                .font_size(16.5)
                .line_height(1.5)
                .color(hex(TEXT_MID)),
            text("tree, rebuilt every frame on the GPU. No DOM. No CSS. No JS.")
                .font_size(16.5)
                .line_height(1.5)
                .color(hex(TEXT_MID)),
        ]),
        div().flex_row().gap(14.0).children([
            cta_primary("goto-why", "Why not the DOM?"),
            cta_ghost("goto-live", "See it live  →"),
        ]),
        // Stats band — the framework at a glance.
        div().flex_row().items_center().gap(22.0).children([
            stat("13", "CRATES", hex(TEXT_HI)),
            divider(),
            stat("20", "WIDGETS", hex(TEXT_HI)),
            divider(),
            stat(&format!("{fps:.0}"), "FPS · LIVE", hex(ACCENT_GREEN)),
            divider(),
            stat("2", "GPU BACKENDS", hex(TEXT_HI)),
            divider(),
            stat("0", "LINES OF HTML", hex(ACCENT_BLUE)),
        ]),
    ])
}

// ── Section 1: Features (2×3 grid + click-through detail) ──────────────────
struct Feat {
    a: &'static str,
    b: &'static str,
    accent: &'static str,
    title: &'static str,
    l1: &'static str,
    l2: &'static str,
    detail: &'static str,
    bullets: [&'static str; 3],
}
const FEATS: [Feat; 6] = [
    Feat { a: ACCENT_BLUE, b: ACCENT_PURPLE, accent: ACCENT_BLUE, title: "Flexbox layout",
        l1: "Taffy engine: rows, columns, gap,", l2: "grow, wrap, absolute positioning.",
        detail: "A full Taffy flexbox engine drives every layout — the same algorithm browser engines use, compiled straight into the binary. No reflow passes, no CSS cascade.",
        bullets: ["row / column, wrap, gap, grow & shrink", "absolute & relative positioning", "min / max / aspect-ratio constraints"] },
    Feat { a: ACCENT_CYAN, b: ACCENT_BLUE, accent: ACCENT_CYAN, title: "SDF styling",
        l1: "Glow, gradients, shadows, rounded", l2: "borders — one GPU pass, no images.",
        detail: "Every visual — glow, gradient, shadow, rounded border — is a signed-distance field evaluated on the GPU in a single pass. No raster images, no pre-blurred textures.",
        bullets: ["soft glows & drop shadows", "linear gradients & solid fills", "per-corner rounded rectangles"] },
    Feat { a: ACCENT_GREEN, b: ACCENT_AMBER, accent: ACCENT_GREEN, title: "Spring physics",
        l1: "Real stiffness and damping on", l2: "hover, drag and transitions.",
        detail: "Hovers, drags and screen transitions run on real spring integrators with stiffness and damping — not fixed-duration easing curves. Interrupt one mid-flight and it carries its velocity.",
        bullets: ["configurable stiffness & damping", "velocity carries between gestures", "interruptible — never snaps"] },
    Feat { a: ACCENT_AMBER, b: ACCENT_GREEN, accent: ACCENT_AMBER, title: "Typography API",
        l1: "font_weight, letter_spacing and", l2: "line_height, straight to cosmic-text.",
        detail: "font_weight, letter_spacing and line_height map directly onto cosmic-text. Proportional and monospace faces are shaped, wrapped and laid out on the GPU.",
        bullets: ["variable weight & tracking", "proportional + monospace faces", "full unicode shaping & wrapping"] },
    Feat { a: ACCENT_PURPLE, b: ACCENT_CYAN, accent: ACCENT_PURPLE, title: "Markdown",
        l1: "CommonMark + GFM rendered by the", l2: "same SDF pipeline as everything else.",
        detail: "CommonMark plus GitHub extensions are parsed and rendered through the exact same SDF pipeline as the rest of the UI — headings, code, lists and tables, all native. No HTML round-trip.",
        bullets: ["CommonMark + GitHub extensions", "code blocks, lists & tables", "styled by the same engine"] },
    Feat { a: ACCENT_BLUE, b: ACCENT_CYAN, accent: ACCENT_BLUE, title: "Ships to the web",
        l1: "The exact same code targets desktop", l2: "and the browser via WebGPU / WebGL2.",
        detail: "The identical Rust code targets desktop and the browser. wgpu selects WebGPU where it's available and falls back to WebGL2, packaged with wasm-bindgen.",
        bullets: ["one codebase, two targets", "WebGPU with WebGL2 fallback", "wasm-bindgen packaged"] },
];

fn feature_card(i: usize, sel: usize) -> Element {
    let f = &FEATS[i];
    let active = i == sel;
    div()
        .id(format!("feat-{i}"))
        .cursor(Cursor::Pointer)
        .w(Px(300.0))
        .min_h(Px(148.0))
        .flex_col()
        .gap(11.0)
        .p(Px(20.0))
        .bg(hex(SURFACE))
        .rounded_px(16.0)
        .border(if active { 2.0 } else { 1.0 }, if active { hex(f.accent) } else { hex(BORDER) })
        .shadow_md(hex("#00000055"))
        .hover(|s| s.translate_y(-6.0).glow_sm(hex(f.accent)).border_color(hex(f.accent)))
        .spring_transition(240.0, 24.0)
        .children([
            div().w(Px(36.0)).h(Px(36.0)).rounded_px(11.0).gradient(hex(f.a), hex(f.b), 45.0).glow_sm(hex(f.a)),
            text(f.title).font_size(16.0).font_weight(600).letter_spacing(-0.2).color(hex(TEXT_HI)),
            div().flex_col().gap(1.0).children([
                text(f.l1).font_size(12.5).line_height(1.5).color(hex(TEXT_MID)),
                text(f.l2).font_size(12.5).line_height(1.5).color(hex(TEXT_MID)),
            ]),
        ])
}

/// Expanded detail for the selected feature card: description on the left, a
/// *live* viewport on the right that actually exercises the feature. Markdown
/// is special — it swaps in the full source→render split.
fn detail_panel(sel: usize, t: f32) -> Element {
    if sel == MD_FEAT {
        return markdown_demo();
    }
    let f = &FEATS[sel];
    let bullet = |txt: &str| {
        div().flex_row().items_center().gap(9.0).children([
            div().w(Px(6.0)).h(Px(6.0)).rounded_px(3.0).bg(hex(f.accent)),
            text(txt).font_size(12.5).line_height(1.5).color(hex(TEXT_MID)),
        ])
    };
    let left = div().flex_col().gap(12.0).w(Px(392.0)).children([
        text(f.title).font_size(19.0).font_weight(600).letter_spacing(-0.3).color(hex(f.accent)),
        text(f.detail).font_size(13.5).line_height(1.6).color(hex(TEXT_MID)),
        div().flex_col().gap(8.0).pt(Px(2.0)).children([bullet(f.bullets[0]), bullet(f.bullets[1]), bullet(f.bullets[2])]),
    ]);
    div()
        .w(Px(940.0))
        .flex_row()
        .items_start()
        .gap(28.0)
        .p(Px(22.0))
        .bg(hex(SURFACE))
        .rounded_px(16.0)
        .border(1.0, hex(BORDER))
        .children([left, feature_viewport(sel, t)])
}

/// The Markdown card is index 4 — selecting it swaps the detail panel for a
/// live source→render demo instead of the static bullet list.
const MD_FEAT: usize = 4;

/// A tiny article we feed straight into `render_markdown` on-screen. The left
/// half of the demo shows this verbatim; the right half shows the tree it
/// produces. Bold/italic flatten to plain text (the crate doesn't do mixed
/// inline styles yet), so this leans on block-level features that render fully.
const SAMPLE_MD: &str = r#"# Live markdown

Parsed and drawn by render_markdown() —
no HTML, no web view, no round-trip.

- CommonMark + GFM extensions
- Headings, lists, quotes & code
- Styled by the same SDF pipeline

> One string in, an Element tree out.

```rust
fn view(&self) -> Element {
    render_markdown(src, &opts)
}
```
"#;

/// Markdown theme re-skinned onto the page's Tokyo-Night palette so the
/// rendered article sits in the site instead of the crate's warm defaults.
fn md_theme() -> MarkdownTheme {
    MarkdownTheme {
        body: hex(TEXT_MID),
        dim: hex(TEXT_DIM),
        heading: hex(TEXT_HI),
        link: hex(ACCENT_PINK),
        code_fg: hex(ACCENT_CYAN),
        code_bg: hex(BG0),
        quote_bar: hex(ACCENT_PINK),
        rule: hex(BORDER),
        base_font_size: 13.0,
        code_font_size: 12.0,
        heading_sizes: [21.0, 17.0, 15.0, 14.0, 13.0, 12.0],
        paragraph_gap: 9.0,
        max_image_width: 360.0,
    }
}

/// The "show, don't tell" panel: the markdown string on the left, and the
/// *actual* `render_markdown()` output on the right — same crate, same frame.
fn markdown_demo() -> Element {
    let opts = MarkdownOptions { theme: md_theme(), ..Default::default() };
    let rendered = render_markdown(SAMPLE_MD, &opts);

    let label = |txt: &str, col: Color| {
        text(txt).mono().font_size(10.5).font_weight(600).letter_spacing(1.0).color(col)
    };
    let card = |bg: &str| div().w_full().p(Px(16.0)).bg(hex(bg)).rounded_px(10.0).border(1.0, hex(BORDER));

    let source = div().flex_col().gap(8.0).w(Px(392.0)).children([
        label("SOURCE · sample.md", hex(TEXT_DIM)),
        card(BG0).children([text(SAMPLE_MD).mono().font_size(11.5).line_height(1.75).color(hex(TEXT_MID))]),
    ]);

    let arrow = div().w(Px(34.0)).flex_col().items_center().pt(Px(104.0)).children([
        text("→").font_size(24.0).font_weight(600).color(hex(ACCENT_PINK)),
    ]);

    let output = div().flex_col().gap(8.0).w(Px(392.0)).children([
        label("RENDERED · render_markdown()", hex(ACCENT_PINK)),
        card(PANEL).children([div().w(Px(356.0)).children([rendered])]),
    ]);

    div()
        .w(Px(940.0))
        .flex_row()
        .items_start()
        .justify_center()
        .gap(18.0)
        .p(Px(22.0))
        .bg(hex(SURFACE))
        .rounded_px(16.0)
        .border(1.0, hex(BORDER))
        .children([source, arrow, output])
}

// ── Per-feature live viewports (the right half of the detail panel) ─────────
const VW: f32 = 468.0;
const VH: f32 = 232.0;

/// Shared viewport frame — a dark inset panel every demo draws into.
fn viewport(children: Vec<Element>) -> Element {
    div()
        .w(Px(VW))
        .h(Px(VH))
        .bg(hex(PANEL))
        .rounded_px(12.0)
        .border(1.0, hex(BORDER))
        .shadow_md(hex("#00000066"))
        .overflow_hidden()
        .children(children)
}
fn vp_caption(txt: &str) -> Element {
    div().absolute().pos(14.0, VH - 26.0).children([text(txt).mono().font_size(10.5).letter_spacing(0.4).color(hex(TEXT_DIM))])
}

/// Flexbox → a real nested Taffy tree: window bar, sidebar, a 2×2 content grid.
/// One tile breathes so the honest-flex layout is visibly alive.
fn flex_viewport(t: f32) -> Element {
    let dot = |c: &str| div().w(Px(7.0)).h(Px(7.0)).rounded_px(3.5).bg(hex(c));
    let bar = div().w_full().h(Px(22.0)).rounded_px(6.0).bg(hex(SURFACE_HI)).flex_row().items_center().gap(6.0).px_pad(Px(9.0)).children([dot("#ff5f57"), dot("#febc2e"), dot("#28c840")]);
    let tile = |accent: bool| {
        let pulse = 0.5 + 0.5 * (t * 1.8).sin();
        let base = hex(SURFACE_HI);
        div()
            .w(Px(172.0))
            .h(Px(58.0))
            .rounded_px(8.0)
            .bg(if accent { base.lerp(hex(ACCENT_BLUE), 0.10 + 0.16 * pulse) } else { base })
            .border(1.0, if accent { hex(ACCENT_BLUE) } else { hex(BORDER) })
    };
    let content = div().flex_col().gap(9.0).children([
        div().flex_row().gap(9.0).children([tile(false), tile(true)]),
        div().flex_row().gap(9.0).children([tile(false), tile(false)]),
    ]);
    let body = div().w_full().flex_row().gap(11.0).children([
        div().w(Px(58.0)).h(Px(129.0)).rounded_px(8.0).bg(hex(SURFACE)).border(1.0, hex(BORDER)),
        content,
    ]);
    let frame = div().absolute().pos(14.0, 14.0).w(Px(VW - 28.0)).flex_col().gap(11.0).children([bar, body]);
    viewport(vec![frame, vp_caption("row · column · gap · grow — real taffy")])
}

/// SDF styling → the four primitives side by side, glows breathing.
fn sdf_viewport(t: f32) -> Element {
    let pulse = 0.5 + 0.5 * (t * 1.6).sin();
    let n = 4usize;
    let sw = 84.0;
    let gap = 24.0;
    let total = n as f32 * sw + (n as f32 - 1.0) * gap;
    let x0 = (VW - total) / 2.0;
    let cy = 88.0;
    let cx = |i: usize| x0 + sw * i as f32 + gap * i as f32 + sw / 2.0;
    let label = |i: usize, s: &str| div().absolute().pos(cx(i) - 40.0, cy + 42.0).w(Px(80.0)).flex_row().justify_center().children([text(s).mono().font_size(10.0).color(hex(TEXT_DIM))]);

    let mut kids: Vec<Element> = Vec::new();
    // glow orb
    kids.push(div().absolute().pos(cx(0) - 16.0, cy - 16.0).w(Px(32.0)).h(Px(32.0)).rounded_px(16.0).bg(hex(ACCENT_CYAN)).glow(hex(ACCENT_CYAN), 22.0 + 20.0 * pulse));
    // gradient tile
    kids.push(div().absolute().pos(cx(1) - 30.0, cy - 30.0).w(Px(60.0)).h(Px(60.0)).rounded_px(14.0).gradient(hex(ACCENT_BLUE), hex(ACCENT_PURPLE), 45.0).glow_sm(hex(ACCENT_BLUE)));
    // drop shadow
    kids.push(div().absolute().pos(cx(2) - 30.0, cy - 30.0).w(Px(60.0)).h(Px(60.0)).rounded_px(14.0).bg(hex(SURFACE_HI)).border(1.0, hex(BORDER)).shadow_md(hex("#000000aa")));
    // rounded ring
    kids.push(div().absolute().pos(cx(3) - 30.0, cy - 30.0).w(Px(60.0)).h(Px(60.0)).rounded_px(30.0).border(2.0, hex(ACCENT_GREEN)).glow_sm(hex(ACCENT_GREEN)).opacity(0.55 + 0.45 * pulse));
    kids.push(label(0, "glow"));
    kids.push(label(1, "gradient"));
    kids.push(label(2, "shadow"));
    kids.push(label(3, "rounded"));
    kids.push(vp_caption("one GPU pass · no raster images"));
    viewport(kids)
}

/// Spring physics → a meter of bars riding layered springs. Unmistakably live.
fn springs_viewport(t: f32) -> Element {
    let n = 19usize;
    let bw = 12.0;
    let gap = 7.0;
    let total = n as f32 * (bw + gap) - gap;
    let x0 = (VW - total) / 2.0;
    let baseline = VH - 44.0;
    let mut kids: Vec<Element> = Vec::new();
    for i in 0..n {
        let fi = i as f32;
        let a = 0.5 + 0.5 * (t * 3.4 + fi * 0.5).sin();
        let b = 0.5 + 0.5 * (t * 1.9 - fi * 0.3 + 1.1).sin();
        let hgt = 12.0 + 120.0 * (0.55 * a + 0.45 * b);
        let col = hex(ACCENT_PURPLE).lerp(hex(ACCENT_GREEN), fi / (n as f32 - 1.0));
        kids.push(div().absolute().pos(x0 + fi * (bw + gap), baseline - hgt).w(Px(bw)).h(Px(hgt)).rounded_px(5.0).bg(col).glow_sm(col).opacity(0.92));
    }
    kids.push(vp_caption("stiffness · damping · integrated every frame"));
    viewport(kids)
}

/// Typography → the wordmark under a live tracking sweep, plus a weight ramp.
fn type_viewport(t: f32) -> Element {
    let tracking = 8.0 * (0.5 + 0.5 * (t * 1.1).sin());
    let word = div().absolute().pos(0.0, 44.0).w(Px(VW)).flex_row().justify_center().children([
        text("Sabitori").font_size(40.0).font_weight(700).letter_spacing(tracking).color(hex(TEXT_HI)),
    ]);
    let weights = div().absolute().pos(0.0, 118.0).w(Px(VW)).flex_row().justify_center().gap(20.0).children([
        text("light").font_size(19.0).font_weight(300).color(hex(TEXT_MID)),
        text("regular").font_size(19.0).font_weight(400).color(hex(TEXT_MID)),
        text("medium").font_size(19.0).font_weight(500).color(hex(TEXT_HI)),
        text("bold").font_size(19.0).font_weight(700).color(hex(TEXT_HI)),
    ]);
    let ls_label = div().absolute().pos(0.0, 158.0).w(Px(VW)).flex_row().justify_center().children([
        text(&format!("letter_spacing: {tracking:.1}px")).mono().font_size(11.0).color(hex(ACCENT_AMBER)),
    ]);
    viewport(vec![word, weights, ls_label, vp_caption("cosmic-text · weight · tracking")])
}

/// Ships to the web → the same tiny `view()` in a desktop window and a browser
/// tab, with the browser's GPU backend toggling WebGPU ⇄ WebGL2 over time.
fn web_viewport(t: f32) -> Element {
    let mini = || {
        div().flex_col().gap(6.0).p(Px(11.0)).children([
            div().w_full().h(Px(15.0)).rounded_px(4.0).gradient(hex(ACCENT_BLUE), hex(ACCENT_PURPLE), 0.0),
            div().w(Px(96.0)).h(Px(6.0)).rounded_px(3.0).bg(hex(SURFACE_HI)),
            div().w(Px(74.0)).h(Px(6.0)).rounded_px(3.0).bg(hex(SURFACE_HI)),
            div().w(Px(50.0)).h(Px(15.0)).rounded_px(5.0).bg(hex(ACCENT_BLUE)),
        ])
    };
    let badge = |txt: &str, col: &str| {
        div().flex_row().items_center().gap(6.0).px_pad(Px(9.0)).py(Px(5.0)).children([
            div().w(Px(6.0)).h(Px(6.0)).rounded_px(3.0).bg(hex(col)).glow_sm(hex(col)),
            text(txt).mono().font_size(9.5).font_weight(600).letter_spacing(0.5).color(hex(TEXT_MID)),
        ])
    };
    let dot = |c: &str| div().w(Px(6.0)).h(Px(6.0)).rounded_px(3.0).bg(hex(c));
    let win = |title: Element, backend: Element| {
        div().w(Px(184.0)).flex_col().bg(hex(SURFACE)).rounded_px(10.0).border(1.0, hex(BORDER)).shadow_md(hex("#00000077")).overflow_hidden().children([
            title,
            div().w_full().h(Px(1.0)).bg(hex(BORDER)),
            mini(),
            div().w_full().h(Px(1.0)).bg(hex(BORDER)),
            backend,
        ])
    };
    let desktop_bar = div().w_full().flex_row().items_center().gap(6.0).px_pad(Px(10.0)).py(Px(8.0)).children([dot("#ff5f57"), dot("#febc2e"), dot("#28c840"), div().w(Px(6.0)), text("desktop").mono().font_size(10.0).color(hex(TEXT_DIM))]);
    let browser_bar = div().w_full().flex_row().items_center().gap(7.0).px_pad(Px(10.0)).py(Px(7.0)).children([
        div().flex_row().items_center().px_pad(Px(8.0)).py(Px(3.0)).rounded_px(7.0).bg(hex(PANEL)).border(1.0, hex(BORDER)).children([
            text("sabitori.dev").mono().font_size(9.5).color(hex(TEXT_DIM)),
        ]),
    ]);
    // Browser backend flips every ~2.4s to show the WebGPU→WebGL2 fallback.
    let webgpu = (t / 2.4).rem_euclid(2.0) < 1.0;
    let (btxt, bcol) = if webgpu { ("WebGPU", ACCENT_GREEN) } else { ("WebGL2", ACCENT_AMBER) };

    let row = div().absolute().pos(0.0, 30.0).w(Px(VW)).flex_row().justify_center().items_start().gap(22.0).children([
        win(desktop_bar, badge("wgpu · Metal", ACCENT_BLUE)),
        win(browser_bar, badge(btxt, bcol)),
    ]);
    let arrow = div().absolute().pos(VW / 2.0 - 8.0, 96.0).children([text("⇆").font_size(18.0).color(hex(TEXT_DIM))]);
    viewport(vec![row, arrow, vp_caption("the exact same view() — two targets")])
}

fn feature_viewport(sel: usize, t: f32) -> Element {
    match sel {
        0 => flex_viewport(t),
        1 => sdf_viewport(t),
        2 => springs_viewport(t),
        3 => type_viewport(t),
        _ => web_viewport(t),
    }
}

fn features(sel: usize, t: f32) -> Element {
    let row1 = div().flex_row().gap(20.0).children([feature_card(0, sel), feature_card(1, sel), feature_card(2, sel)]);
    let row2 = div().flex_row().gap(20.0).children([feature_card(3, sel), feature_card(4, sel), feature_card(5, sel)]);
    div().flex_col().items_center().gap(16.0).children([
        eyebrow(if sel == MD_FEAT {
            "MARKDOWN · RENDERED LIVE, RIGHT HERE, BY THE MD CRATE"
        } else {
            "EVERYTHING IN THE BOX · CLICK A CARD — EACH ONE DEMOS ITSELF"
        }),
        div().flex_col().gap(20.0).children([row1, row2]),
        detail_panel(sel, t),
    ])
}

// ── Section: Live Gallery (real product UIs, rendered live) ─────────────────
// Three miniature-but-recognisable app UIs, each rebuilt on the GPU every
// frame: an analytics dashboard, a chat window, and a music player. This is
// the "you can build real products with this" proof — not abstract particles.

/// Analytics dashboard — KPI cards + a streaming area chart + a goal bar.
/// The chart values scroll every frame; nothing here is a static image.
fn dash_panel(t: f32) -> Element {
    let inner = 512.0;
    let kpi = |label: &str, val: &str, delta: &str, up: bool| {
        let col = if up { ACCENT_GREEN } else { ACCENT_PINK };
        div().flex_col().gap(3.0).w(Px(162.0)).p(Px(12.0)).bg(hex(SURFACE)).rounded_px(10.0).border(1.0, hex(BORDER)).children([
            text(label).font_size(10.0).letter_spacing(0.5).color(hex(TEXT_DIM)),
            text(val).font_size(22.0).font_weight(600).letter_spacing(-0.5).color(hex(TEXT_HI)),
            div().flex_row().items_center().gap(4.0).children([
                text(if up { "▲" } else { "▼" }).font_size(8.0).color(hex(col)),
                text(delta).font_size(11.0).font_weight(500).color(hex(col)),
            ]),
        ])
    };
    let kpis = div().w(Px(inner)).flex_row().justify_between().children([
        kpi("REVENUE", "$128.4k", "12.4%", true),
        kpi("ACTIVE USERS", "42.1k", "3.2%", true),
        kpi("CHURN", "1.8%", "0.4%", false),
    ]);

    // Streaming area chart.
    let ch = 128.0;
    let n = 52usize;
    let colw = inner / n as f32;
    let mut chart_kids: Vec<Element> = Vec::new();
    for g in 1..4u32 {
        let gy = ch * g as f32 / 4.0;
        chart_kids.push(div().absolute().pos(0.0, gy).w(Px(inner)).h(Px(1.0)).bg(hex("#1b2033")));
    }
    for i in 0..n {
        let fi = i as f32;
        let v = 0.44 + 0.30 * (fi * 0.34 - t * 1.3).sin() + 0.13 * (fi * 0.9 + t * 0.8).sin() + 0.06 * (fi * 1.7 - t * 2.1).sin();
        let v = v.clamp(0.06, 0.98);
        let h = v * ch;
        let x = fi * colw;
        chart_kids.push(div().absolute().pos(x, ch - h).w(Px(colw + 0.8)).h(Px(h)).gradient(hex(ACCENT_BLUE).with_alpha(0.5), hex(ACCENT_BLUE).with_alpha(0.03), 90.0));
        chart_kids.push(div().absolute().pos(x, ch - h - 1.0).w(Px(colw + 0.8)).h(Px(2.0)).bg(hex(ACCENT_CYAN)).opacity(0.9));
    }
    let chart = div().w(Px(inner)).h(Px(ch)).overflow_hidden().children(chart_kids);

    let goal = 0.72 + 0.05 * (t * 0.6).sin();
    let bar = div().w(Px(inner)).flex_col().gap(6.0).children([
        div().w_full().flex_row().justify_between().children([
            text("Quarterly goal").font_size(11.5).color(hex(TEXT_MID)),
            text(&format!("{:.0}%", goal * 100.0)).mono().font_size(11.5).color(hex(ACCENT_CYAN)),
        ]),
        div().w_full().h(Px(6.0)).rounded_px(3.0).bg(hex(SURFACE_HI)).children([
            div().w(Px(inner * goal)).h(Px(6.0)).rounded_px(3.0).gradient(hex(ACCENT_BLUE), hex(ACCENT_CYAN), 0.0),
        ]),
    ]);

    div().w(Px(548.0)).flex_col().bg(hex(PANEL)).rounded_px(14.0).border(1.0, hex(BORDER)).shadow_md(hex("#00000088")).overflow_hidden().children([
        mac_dots("Analytics · Revenue"),
        hline(),
        div().flex_col().items_center().gap(14.0).p(Px(18.0)).children([kpis, chart, bar]),
    ])
}

/// Chat window — message bubbles left/right, plus a live "typing…" indicator.
fn chat_panel(t: f32) -> Element {
    let avatar = |c: &str| div().w(Px(24.0)).h(Px(24.0)).rounded_px(12.0).gradient(hex(c), hex(ACCENT_PURPLE), 45.0);
    let recv = |txt: &str| {
        div().w_full().flex_row().items_end().gap(8.0).children([
            avatar(ACCENT_CYAN),
            div().max_w(Px(232.0)).px_pad(Px(12.0)).py(Px(8.0)).rounded_px(13.0).bg(hex(SURFACE_HI)).children([text(txt).font_size(12.5).line_height(1.35).color(hex(TEXT_HI))]),
        ])
    };
    let sent = |txt: &str| {
        div().w_full().flex_row().justify_end().children([
            div().max_w(Px(232.0)).px_pad(Px(12.0)).py(Px(8.0)).rounded_px(13.0).gradient(hex(ACCENT_BLUE), hex(ACCENT_PURPLE), 0.0).children([text(txt).font_size(12.5).line_height(1.35).color(hex("#0b0d16"))]),
        ])
    };
    let tdot = |ph: f32| {
        let o = 0.25 + 0.6 * (0.5 + 0.5 * (t * 4.5 + ph).sin());
        div().w(Px(6.0)).h(Px(6.0)).rounded_px(3.0).bg(hex(TEXT_MID)).opacity(o)
    };
    let typing = div().w_full().flex_row().items_center().gap(8.0).children([
        avatar(ACCENT_CYAN),
        div().flex_row().gap(4.0).px_pad(Px(13.0)).py(Px(10.0)).rounded_px(13.0).bg(hex(SURFACE_HI)).children([tdot(0.0), tdot(1.0), tdot(2.0)]),
    ]);

    div().w(Px(372.0)).flex_col().bg(hex(PANEL)).rounded_px(14.0).border(1.0, hex(BORDER)).shadow_md(hex("#00000088")).overflow_hidden().children([
        mac_dots("Messages · #ship-it"),
        hline(),
        div().flex_col().gap(9.0).p(Px(16.0)).children([
            recv("Is the GPU renderer ready?"),
            sent("Shipping it right now."),
            typing,
        ]),
    ])
}

/// Music player — album art, an animated scrubber, an EQ visualiser, controls.
fn player_panel(t: f32) -> Element {
    let art = div().w(Px(58.0)).h(Px(58.0)).rounded_px(11.0).gradient(hex(ACCENT_PURPLE), hex(ACCENT_PINK), 45.0).glow_sm(hex(ACCENT_PURPLE));
    // EQ visualiser — a small row of bars riding sines.
    let mut eq: Vec<Element> = Vec::new();
    for i in 0..14 {
        let fi = i as f32;
        let h = 4.0 + 16.0 * (0.5 + 0.5 * (t * 6.0 + fi * 0.7).sin());
        eq.push(div().w(Px(3.0)).h(Px(h)).rounded_px(1.5).bg(hex(ACCENT_PINK)).opacity(0.85));
    }
    let eq_row = div().flex_row().items_end().gap(2.5).h(Px(22.0)).children(eq);
    let meta = div().flex_col().gap(3.0).children([
        text("Signed Distance Fields").font_size(13.5).font_weight(600).color(hex(TEXT_HI)),
        text("sabitori · Single Pass EP").font_size(11.5).color(hex(TEXT_DIM)),
        eq_row,
    ]);

    let trackw = 308.0;
    let prog = (t * 0.05).rem_euclid(1.0);
    let total = 220.0;
    let cur = (prog * total) as u32;
    let scrub = div().w(Px(trackw)).h(Px(12.0)).children([
        div().absolute().pos(0.0, 4.0).w(Px(trackw)).h(Px(4.0)).rounded_px(2.0).bg(hex(SURFACE_HI)),
        div().absolute().pos(0.0, 4.0).w(Px(trackw * prog)).h(Px(4.0)).rounded_px(2.0).gradient(hex(ACCENT_PURPLE), hex(ACCENT_PINK), 0.0),
        div().absolute().pos(trackw * prog - 6.0, 0.0).w(Px(12.0)).h(Px(12.0)).rounded_px(6.0).bg(hex(TEXT_HI)).glow_sm(hex(ACCENT_PINK)),
    ]);
    let times = div().w(Px(trackw)).flex_row().justify_between().children([
        text(&format!("{}:{:02}", cur / 60, cur % 60)).mono().font_size(10.5).color(hex(TEXT_DIM)),
        text("3:40").mono().font_size(10.5).color(hex(TEXT_DIM)),
    ]);
    let btn = |glyph: &str, big: bool| {
        let d = if big { 40.0 } else { 30.0 };
        let base = div().w(Px(d)).h(Px(d)).rounded_px(d / 2.0).flex_row().items_center().justify_center();
        let base = if big { base.gradient(hex(ACCENT_PURPLE), hex(ACCENT_PINK), 45.0).glow_sm(hex(ACCENT_PURPLE)) } else { base.bg(hex(SURFACE_HI)) };
        base.children([text(glyph).font_size(if big { 15.0 } else { 12.0 }).color(hex(if big { "#0b0d16" } else { TEXT_HI }))])
    };
    let controls = div().flex_row().items_center().justify_center().gap(16.0).children([btn("◀◀", false), btn("▶", true), btn("▶▶", false)]);

    div().w(Px(372.0)).flex_col().bg(hex(PANEL)).rounded_px(14.0).border(1.0, hex(BORDER)).shadow_md(hex("#00000088")).overflow_hidden().children([
        mac_dots("Now Playing"),
        hline(),
        div().flex_col().gap(13.0).p(Px(16.0)).children([
            div().flex_row().items_center().gap(13.0).children([art, meta]),
            div().flex_col().items_center().gap(3.0).children([scrub, times]),
            controls,
        ]),
    ])
}

fn gallery(t: f32) -> Element {
    let right = div().flex_col().gap(16.0).children([chat_panel(t), player_panel(t)]);
    div().flex_col().items_center().gap(18.0).children([
        eyebrow("LIVE GALLERY · REAL UIs, EVERY PIXEL RENDERED THIS FRAME"),
        div().flex_row().items_start().gap(20.0).children([dash_panel(t), right]),
        text("A dashboard, a chat and a player — three real interfaces, each rebuilt on the GPU every frame. No video, no canvas, no HTML.")
            .font_size(13.0)
            .line_height(1.5)
            .color(hex(TEXT_MID)),
    ])
}

// ── Section 3: Get started ─────────────────────────────────────────────────
fn code_line(segs: &[(&str, &str)]) -> Element {
    let spans: Vec<Element> = segs.iter().map(|(s, c)| text(*s).mono().font_size(13.5).line_height(1.7).color(hex(c))).collect();
    div().flex_row().children(spans)
}
fn step(n: &str, title: &str, body: &str) -> Element {
    div().flex_row().items_center().gap(12.0).children([
        div().w(Px(26.0)).h(Px(26.0)).rounded_px(13.0).bg(hex(SURFACE_HI)).border(1.0, hex(BORDER)).flex_row().items_center().justify_center()
            .children([text(n).font_size(12.5).font_weight(600).color(hex(ACCENT_BLUE))]),
        div().flex_col().gap(1.0).children([
            text(title).font_size(14.0).font_weight(600).color(hex(TEXT_HI)),
            text(body).font_size(12.5).color(hex(TEXT_DIM)),
        ]),
    ])
}
fn start() -> Element {
    let terminal = div()
        .w(Px(440.0))
        .flex_col()
        .gap(6.0)
        .p(Px(20.0))
        .bg(hex(PANEL))
        .rounded_px(12.0)
        .border(1.0, hex(BORDER))
        .shadow_md(hex("#00000077"))
        .children([
            div().flex_row().gap(8.0).children([
                text("$").mono().font_size(13.5).color(hex(ACCENT_GREEN)),
                text("cargo add sabitori").mono().font_size(13.5).color(hex(TEXT_HI)),
            ]),
            div().h(Px(4.0)),
            code_line(&[("sabitori", ACCENT_BLUE), ("::", TEXT_DIM), ("run_declarative", "#8aa2d8"), ("(", TEXT_DIM), ("Home", ACCENT_CYAN), ("::", TEXT_DIM), ("new", "#8aa2d8"), ("());", TEXT_DIM)]),
        ]);
    div().flex_col().items_center().gap(24.0).children([
        eyebrow("SHIP IT IN THREE LINES"),
        div().flex_col().items_center().gap(6.0).children([
            text("Start building.").font_size(46.0).font_weight(500).letter_spacing(-1.4).color(hex(TEXT_HI)),
            text("The same code runs on the desktop and the web.").font_size(15.5).color(hex(TEXT_MID)),
        ]),
        div().flex_row().items_start().gap(28.0).children([
            terminal,
            div().flex_col().gap(14.0).children([
                step("1", "Add the crate", "One dependency pulls the whole workspace."),
                step("2", "impl DeclarativeApp", "Write view(), return an Element tree."),
                step("3", "run_declarative(app)", "Desktop today, WebGPU/WebGL2 tomorrow."),
            ]),
        ]),
        div().flex_row().gap(14.0).children([
            cta_primary("cta-star", "★  Star on GitHub"),
            cta_ghost("goto-home", "←  Back to home"),
        ]),
    ])
}

// ── Section 3: Code ────────────────────────────────────────────────────────
fn code() -> Element {
    let dot = |c: &str| div().w(Px(11.0)).h(Px(11.0)).rounded_px(5.5).bg(hex(c));
    let titlebar = div().w_full().flex_row().items_center().gap(8.0).px_pad(Px(16.0)).py(Px(12.0)).children([
        dot("#ff5f57"),
        dot("#febc2e"),
        dot("#28c840"),
        div().w(Px(12.0)),
        text("src/main.rs").mono().font_size(12.0).color(hex(TEXT_DIM)),
    ]);
    let kw = ACCENT_PURPLE;
    let m = "#8aa2d8";
    let body = div().flex_col().px_pad(Px(22.0)).py(Px(18.0)).children([
        code_line(&[("use ", kw), ("sabitori", ACCENT_BLUE), ("::*;", TEXT_DIM)]),
        code_line(&[(" ", TEXT_DIM)]),
        code_line(&[("struct ", kw), ("Hello", ACCENT_CYAN), (";", TEXT_DIM)]),
        code_line(&[(" ", TEXT_DIM)]),
        code_line(&[("impl ", kw), ("DeclarativeApp", ACCENT_CYAN), (" for ", kw), ("Hello", ACCENT_CYAN), (" {", TEXT_DIM)]),
        code_line(&[("    fn ", kw), ("view", m), ("(&", TEXT_DIM), ("self", kw), (", _: &", TEXT_DIM), ("ViewContext", ACCENT_CYAN), (") -> ", TEXT_DIM), ("Element", ACCENT_CYAN), (" {", TEXT_DIM)]),
        code_line(&[("        ", TEXT_DIM), ("text", ACCENT_BLUE), ("(", TEXT_DIM), ("\"hello, gpu\"", ACCENT_GREEN), (")", TEXT_DIM)]),
        code_line(&[("            .", TEXT_DIM), ("font_size", m), ("(", TEXT_DIM), ("64.0", ACCENT_AMBER), (")", TEXT_DIM)]),
        code_line(&[("            .", TEXT_DIM), ("gradient", m), ("(blue, purple, ", TEXT_HI), ("0.0", ACCENT_AMBER), (")", TEXT_DIM)]),
        code_line(&[("    }", TEXT_DIM)]),
        code_line(&[("}", TEXT_DIM)]),
    ]);
    let panel = div()
        .w(Px(520.0))
        .flex_col()
        .bg(hex(PANEL))
        .rounded_px(14.0)
        .border(1.0, hex(BORDER))
        .shadow_md(hex("#00000077"))
        .overflow_hidden()
        .children([titlebar, div().w_full().h(Px(1.0)).bg(hex(BORDER)), body]);

    let noneed = |txt: &str| {
        div().flex_row().items_center().gap(10.0).children([
            div().w(Px(6.0)).h(Px(6.0)).rounded_px(3.0).bg(hex(ACCENT_PINK)),
            text(txt).font_size(13.5).line_height(1.6).color(hex(TEXT_MID)),
        ])
    };
    let right = div().flex_col().gap(13.0).w(Px(250.0)).children([
        text("WHAT YOU DON'T WRITE").font_size(12.0).font_weight(600).letter_spacing(0.8).color(hex(TEXT_DIM)),
        noneed("No JSX, no template DSL"),
        noneed("No CSS, no stylesheet"),
        noneed("No bundler, no build step"),
        noneed("No virtual-DOM diffing"),
        noneed("No <canvas> for effects"),
    ]);

    div().flex_col().items_center().gap(20.0).children([
        eyebrow("THE WHOLE APP · ONE FILE · NO BUILD STEP"),
        div().flex_row().items_start().gap(30.0).children([panel, right]),
    ])
}

// ── Section: Why (vs the DOM / CSS) ─────────────────────────────────────────
/// The framework's strongest argument, taught side by side.
/// (aspect, the web way, the sabitori way)
const VS_ROWS: [(&str, &str, &str); 6] = [
    ("LANGUAGES", "HTML + CSS + JavaScript", "Rust — one language, one file"),
    ("STYLING", "cascade · specificity · !important", "methods on the node — always local"),
    ("EFFECTS", "raster images or a <canvas>", "GPU signed-distance fields, one pass"),
    ("ANIMATION", "@keyframes & transitions", "real spring physics, per frame"),
    ("BUILD", "bundler · dev server · HMR", "cargo build → a single binary"),
    ("DEBUGGING", "\"why is this 3px off?\"", "what you wrote is what you get"),
];

fn why() -> Element {
    let cell = |w: f32| div().w(Px(w));
    let header = div().flex_row().items_center().children([
        cell(150.0),
        cell(340.0).children([text("THE WEB WAY").mono().font_size(11.0).font_weight(600).letter_spacing(1.4).color(hex(ACCENT_PINK))]),
        cell(360.0).children([text("SABITORI").mono().font_size(11.0).font_weight(600).letter_spacing(1.4).color(hex(ACCENT_CYAN))]),
    ]);
    let row = |aspect: &str, web: &str, sab: &str| {
        div().flex_col().children([
            div().w_full().h(Px(1.0)).bg(hex(BORDER)),
            div().flex_row().items_center().py(Px(11.0)).children([
                cell(150.0).children([text(aspect).font_size(11.0).font_weight(600).letter_spacing(0.8).color(hex(TEXT_DIM))]),
                cell(340.0).flex_row().items_center().gap(9.0).children([
                    div().w(Px(5.0)).h(Px(5.0)).rounded_px(2.5).bg(hex(ACCENT_PINK)).opacity(0.7),
                    text(web).font_size(13.5).line_height(1.4).color(hex(TEXT_MID)),
                ]),
                cell(360.0).flex_row().items_center().gap(9.0).children([
                    div().w(Px(5.0)).h(Px(5.0)).rounded_px(2.5).bg(hex(ACCENT_CYAN)),
                    text(sab).font_size(13.5).line_height(1.4).font_weight(500).color(hex(TEXT_HI)),
                ]),
            ]),
        ])
    };
    let rows: Vec<Element> = VS_ROWS.iter().map(|(a, w, s)| row(a, w, s)).collect();
    let panel = div()
        .flex_col()
        .w(Px(940.0))
        .p(Px(24.0))
        .bg(hex(SURFACE))
        .rounded_px(16.0)
        .border(1.0, hex(BORDER))
        .children([header, div().flex_col().pt(Px(6.0)).children(rows)]);

    div().flex_col().items_center().gap(18.0).children([
        eyebrow("WHY NOT JUST USE THE DOM?"),
        div().flex_col().items_center().gap(2.0).children([
            text("One tree. One language.").font_size(44.0).font_weight(400).letter_spacing(-1.4).color(hex(TEXT_HI)),
            text("No cascade.").font_size(44.0).font_weight(500).letter_spacing(-1.4).color(hex(ACCENT_CYAN)),
        ]),
        panel,
        text("Honest trade-off: the web still wins on ecosystem, text maturity and accessibility. Sabitori trades those for a single-language, GPU-native stack.")
            .font_size(12.5).line_height(1.55).color(hex(TEXT_DIM)),
    ])
}

// ── Section: Widgets (the toolkit + input / IME) ────────────────────────────
/// Selectable page themes for the live Dropdown. (label shown, accent hex.)
const THEMES: [(&str, &str); 4] = [
    ("Tokyo Night", ACCENT_BLUE),
    ("Aurora", ACCENT_GREEN),
    ("Rosé Pine", ACCENT_PINK),
    ("Amber Dusk", ACCENT_AMBER),
];

/// 実在するものだけを並べること。 0.4.0 より前は、 `view()` から使えない
/// retained ウィジェット (`Card` / `Tabs` / 旧 `Table` など) もここに数えていた。
const WIDGET_NAMES: [&str; 16] = [
    "Table", "TreeView", "Modal", "Select", "DatePicker", "ColorPicker",
    "Slider", "Toast", "Tooltip", "ContextMenu", "TextInput", "SplitPane",
    "NumericInput", "MenuBar", "Panel", "VirtualList",
];

fn mac_dots(title: &str) -> Element {
    let dot = |c: &str| div().w(Px(8.0)).h(Px(8.0)).rounded_px(4.0).bg(hex(c));
    div().w_full().flex_row().items_center().gap(7.0).px_pad(Px(13.0)).py(Px(10.0)).children([
        dot("#ff5f57"), dot("#febc2e"), dot("#28c840"), div().w(Px(8.0)),
        text(title).mono().font_size(11.5).color(hex(TEXT_DIM)),
    ])
}
fn hline() -> Element {
    div().w_full().h(Px(1.0)).bg(hex(BORDER))
}

/// The Preferences window — now *real*: the theme `Dropdown` recolours the
/// whole page, the toggle flips (pausing the space backdrop), and clicking a
/// table row selects it. All driven by real widget state + `on_click`.
fn wg_montage(theme: &DropdownState, hovered: Option<&str>, anim_on: bool, table_sel: usize) -> Element {
    let th = |s: &str, w: f32| div().w(Px(w)).children([text(s).font_size(10.5).font_weight(600).letter_spacing(0.6).color(hex(TEXT_DIM))]);
    let head = div().w_full().flex_row().px_pad(Px(14.0)).py(Px(7.0)).children([th("WIDGET", 150.0), th("KIND", 120.0), th("ROWS", 70.0)]);
    let trow = |i: usize, a: &str, b: &str, c: &str| {
        let sel = i == table_sel;
        let base = div().id(format!("wg-row-{i}")).cursor(Cursor::Pointer).w_full().flex_row().items_center().px_pad(Px(14.0)).py(Px(7.0));
        let base = if sel { base.bg(hex(ACCENT_BLUE).with_alpha(0.14)) } else { base };
        base.children([
            div().w(Px(150.0)).children([text(a).font_size(12.0).font_weight(if sel { 600 } else { 400 }).color(hex(if sel { TEXT_HI } else { TEXT_MID }))]),
            div().w(Px(120.0)).children([text(b).font_size(12.0).color(hex(TEXT_DIM))]),
            div().w(Px(70.0)).children([text(c).mono().font_size(12.0).color(hex(TEXT_DIM))]),
        ])
    };
    let table = div().w_full().flex_col().children([
        head,
        hline(),
        trow(0, "Table", "grid", "10k"),
        trow(1, "TreeView", "tree", "240"),
        trow(2, "VirtualList", "list", "1M"),
    ]);

    let label = |s: &str| div().w(Px(96.0)).children([text(s).font_size(12.5).color(hex(TEXT_MID))]);
    // Real Dropdown: trigger + inline menu (expands in layout flow on open).
    let dd_style = DropdownStyle::default_dark();
    let mut theme_kids: Vec<Element> = vec![theme.trigger(&dd_style, hovered)];
    if let Some(menu) = theme.menu_inline(hovered, &dd_style) {
        theme_kids.push(menu);
    }
    let theme_ctrl = div().w(Px(150.0)).flex_col().gap(4.0).children(theme_kids);

    let frac = 0.62_f32;
    let sw = 150.0_f32;
    let slider = div().w(Px(sw)).h(Px(18.0)).children([
        div().absolute().pos(0.0, 7.0).w(Px(sw)).h(Px(4.0)).rounded_px(2.0).bg(hex(SURFACE_HI)),
        div().absolute().pos(0.0, 7.0).w(Px(sw * frac)).h(Px(4.0)).rounded_px(2.0).bg(hex(ACCENT_BLUE)),
        div().absolute().pos(sw * frac - 8.0, 1.0).w(Px(16.0)).h(Px(16.0)).rounded_px(8.0).bg(hex(TEXT_HI)).glow_sm(hex(ACCENT_BLUE)),
    ]);
    let knob_x = if anim_on { 20.0 } else { 2.0 };
    let toggle = div().id("wg-toggle").cursor(Cursor::Pointer).w(Px(40.0)).h(Px(22.0)).rounded_px(11.0)
        .bg(hex(if anim_on { ACCENT_GREEN } else { SURFACE_HI }))
        .children([div().absolute().pos(knob_x, 2.0).w(Px(18.0)).h(Px(18.0)).rounded_px(9.0).bg(hex(TEXT_HI))]);
    let ctl = |lab: Element, ctrl: Element| div().w_full().flex_row().items_start().justify_between().px_pad(Px(14.0)).py(Px(6.0)).children([lab, ctrl]);
    let controls = div().w_full().flex_col().gap(2.0).children([
        ctl(label("Theme"), theme_ctrl),
        ctl(label("Density"), slider),
        ctl(label("Animations"), toggle),
    ]);

    div().w(Px(470.0)).flex_col().bg(hex(PANEL)).rounded_px(12.0).border(1.0, hex(BORDER)).shadow_md(hex("#00000088")).overflow_hidden().children([
        mac_dots("Preferences"),
        hline(),
        table,
        hline(),
        div().h(Px(6.0)),
        controls,
        div().h(Px(8.0)),
    ])
}

/// A *real* editable text field backed by `TextInputState`. Click to focus,
/// then type — ASCII and Japanese IME both route through it (`on_char` /
/// `on_key` / `on_ime_preedit` / `on_ime_commit`). The OS draws its own IME
/// candidate window; the preedit shows inline.
fn ime_panel(input: &TextInputState, focused: bool, t: f32) -> Element {
    let composed = input.display_text_with_preedit();
    let (content, dim) = if composed.is_empty() {
        ("Click here, then type…  日本語 IME もOK".to_string(), true)
    } else {
        (composed, false)
    };
    let show_caret = focused && (t * 1.6).rem_euclid(1.0) < 0.55;
    let caret = div().w(Px(2.0)).h(Px(22.0)).bg(hex(TEXT_HI)).opacity(if show_caret { 1.0 } else { 0.0 });
    let field = div()
        .id("wg-input")
        .cursor(Cursor::Pointer)
        .w_full()
        .flex_row()
        .items_center()
        .gap(1.0)
        .px_pad(Px(14.0))
        .py(Px(13.0))
        .min_h(Px(52.0))
        .rounded_px(9.0)
        .bg(hex(BG0))
        .border(1.0, hex(if focused { ACCENT_BLUE } else { BORDER }))
        .children([text(&content).font_size(18.0).color(hex(if dim { TEXT_DIM } else { TEXT_HI })), caret]);

    let hint = if focused {
        "focused — keystrokes route to TextInputState · IME preedit inline"
    } else {
        "click the field to focus, then type"
    };
    div().w(Px(440.0)).flex_col().bg(hex(PANEL)).rounded_px(12.0).border(1.0, hex(BORDER)).shadow_md(hex("#00000088")).overflow_hidden().children([
        mac_dots("テキスト入力 · TextInput + IME"),
        hline(),
        div().flex_col().gap(12.0).p(Px(18.0)).children([
            field,
            text(hint).font_size(12.0).line_height(1.5).color(hex(if focused { ACCENT_CYAN } else { TEXT_DIM })),
            text("mouse · touch · pen — one Pointer. Japanese IME & preedit built in.").font_size(12.0).line_height(1.5).color(hex(TEXT_DIM)),
        ]),
    ])
}

fn widget_chips() -> Element {
    let chip = |s: &str| div().px_pad(Px(11.0)).py(Px(5.0)).rounded_px(12.0).bg(hex(SURFACE)).border(1.0, hex(BORDER)).children([
        text(s).font_size(11.5).color(hex(TEXT_MID)),
    ]);
    let chips: Vec<Element> = WIDGET_NAMES.iter().map(|s| chip(s)).collect();
    div().w(Px(940.0)).flex_row().wrap().justify_center().gap(8.0).children(chips)
}

#[allow(clippy::too_many_arguments)]
fn widgets_section(theme: &DropdownState, hovered: Option<&str>, anim_on: bool, table_sel: usize, input: &TextInputState, focused: bool, t: f32) -> Element {
    div().flex_col().items_center().gap(18.0).children([
        eyebrow("A REAL TOOLKIT · CLICK THE DROPDOWN · FLIP THE TOGGLE · TYPE IN THE FIELD"),
        text("Batteries included.").font_size(40.0).font_weight(500).letter_spacing(-1.2).color(hex(TEXT_HI)),
        div().flex_row().items_start().gap(20.0).children([wg_montage(theme, hovered, anim_on, table_sel), ime_panel(input, focused, t)]),
        widget_chips(),
    ])
}

// ── App ────────────────────────────────────────────────────────────────────
pub struct Home {
    t: f32,
    cur: usize,
    prev: usize,
    trans: f32,
    dir: f32,
    /// Smoothed live frame rate (render fps), measured from tick dt.
    fps: f32,
    /// Selected feature card on the Features section (drives the detail panel).
    feat_sel: usize,
    /// Backdrop time — only advances while `anim_on`, so the toggle pauses it.
    anim_t: f32,
    // ── Widgets section: real, interactive widget state ──
    /// Page theme picker (drives the whole-page accent colour).
    theme: DropdownState,
    /// Selected theme index → THEMES[accent_idx].
    accent_idx: usize,
    /// "Animations" toggle — pauses the space backdrop when off.
    anim_on: bool,
    /// Selected row in the demo Table.
    table_sel: usize,
    /// The live, editable text field (ASCII + Japanese IME).
    input: TextInputState,
    /// Whether the text field currently owns keyboard input.
    input_focused: bool,
    /// Currently hovered element id (for the dropdown's hover highlight).
    hovered: Option<String>,
}

impl Home {
    pub fn new() -> Self {
        let theme = DropdownState::new("wg-theme", THEMES.iter().map(|(n, _)| n.to_string()).collect());
        Home {
            t: 0.0,
            cur: 0,
            prev: 0,
            trans: 1.0,
            dir: 1.0,
            fps: 120.0,
            feat_sel: 0,
            anim_t: 0.0,
            theme,
            accent_idx: 0,
            anim_on: true,
            table_sel: 1,
            input: TextInputState::new("Type here…"),
            input_focused: false,
            hovered: None,
        }
    }
    fn goto(&mut self, i: usize) {
        if i < TABS.len() && i != self.cur {
            self.prev = self.cur;
            self.dir = if i > self.cur { 1.0 } else { -1.0 };
            self.cur = i;
            self.trans = 0.0;
        }
    }
    fn section(&self, idx: usize, t: f32, accent: Color) -> Element {
        match idx {
            0 => home(accent, t, self.fps),
            1 => why(),
            2 => features(self.feat_sel, t),
            3 => widgets_section(&self.theme, self.hovered.as_deref(), self.anim_on, self.table_sel, &self.input, self.input_focused, t),
            4 => gallery(t),
            5 => code(),
            _ => start(),
        }
    }
}

fn stage_screen(cw: f32, sh: f32, content: Element, dx: f32, op: f32) -> Element {
    div()
        .absolute()
        .pos(dx, 0.0)
        .w(Px(cw))
        .h(Px(sh))
        .flex_col()
        .items_center()
        .justify_center()
        .opacity(op)
        .children([content])
}

impl DeclarativeApp for Home {
    fn title(&self) -> &str {
        "Sabitori — GPU UI for Rust"
    }
    fn size(&self) -> (f32, f32) {
        (1160.0, 860.0)
    }
    // On the web there are no system fonts, so bundle them. Native keeps using
    // system fonts (empty = trait default), so desktop rendering is unchanged.
    #[cfg(target_arch = "wasm32")]
    fn fonts(&self) -> Vec<Vec<u8>> {
        vec![
            include_bytes!("../assets/fonts/NotoSansJP-Regular.otf").to_vec(),
            include_bytes!("../assets/fonts/Hack-Regular.ttf").to_vec(),
            include_bytes!("../assets/fonts/Hack-Bold.ttf").to_vec(),
        ]
    }
    fn is_animating(&self) -> bool {
        true
    }

    fn tick(&mut self, dt: f32) {
        // Live render-fps, smoothed, from the raw delta (before the physics clamp)
        // so genuine drops actually show.
        if dt > 0.0 {
            self.fps = self.fps * 0.9 + (1.0 / dt) * 0.1;
        }
        let dt = dt.min(0.05);

        self.t += dt;
        // The "Animations" toggle pauses the space backdrop only — navigation
        // and section transitions keep running on `self.t`.
        if self.anim_on {
            self.anim_t += dt;
        }
        if self.trans < 1.0 {
            self.trans = (self.trans + dt * 2.6).min(1.0);
        }
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        let cw = ctx.width.min(1040.0);
        let t = self.t;
        let stage_h = (ctx.height - 158.0).max(320.0);
        let cyc = 0.5 + 0.5 * (t * 0.35).sin();
        // The Widgets → Theme dropdown recolours the whole page's accent.
        let accent = hex(THEMES[self.accent_idx].1).lerp(hex(TEXT_HI), cyc * 0.14);

        let stage_kids: Vec<Element> = if self.trans < 1.0 {
            let e = ease_out_cubic(self.trans);
            let off = 130.0;
            vec![
                stage_screen(cw, stage_h, self.section(self.prev, t, accent), -self.dir * e * off, 1.0 - e),
                stage_screen(cw, stage_h, self.section(self.cur, t, accent), self.dir * (1.0 - e) * off, e),
            ]
        } else {
            vec![stage_screen(cw, stage_h, self.section(self.cur, t, accent), 0.0, 1.0)]
        };
        let stage = div().w(Px(cw)).h(Px(stage_h)).children(stage_kids);

        // Real buttons: unique id + a uniform hover pill. External URLs can't
        // open a browser yet (no platform hook), so each routes to the closest
        // in-app screen instead.
        let footer_link = |id: &str, txt: &str| {
            div()
                .id(id.to_string())
                .cursor(Cursor::Pointer)
                .px_pad(Px(9.0))
                .py(Px(5.0))
                .rounded_px(7.0)
                .flex_row()
                .items_center()
                .hover(|s| s.bg(hex(SURFACE_HI)))
                .spring_transition(240.0, 24.0)
                .children([text(txt).font_size(12.0).letter_spacing(0.3).color(hex(TEXT_MID))])
        };
        let footer = div().w(Px(cw)).flex_row().items_center().justify_between().children([
            div().flex_row().items_center().gap(9.0).children([
                div().w(Px(14.0)).h(Px(14.0)).rounded_px(4.0).gradient(hex(ACCENT_BLUE), hex(ACCENT_PURPLE), 45.0),
                text("sabitori").font_size(12.5).font_weight(600).color(hex(TEXT_MID)),
                text("· MIT").font_size(11.5).color(hex(TEXT_DIM)),
            ]),
            div().flex_row().gap(4.0).children([
                footer_link("foot-docs", "Docs"),
                footer_link("foot-github", "GitHub"),
                footer_link("foot-crates", "crates.io"),
                footer_link("foot-examples", "Examples"),
            ]),
            text("RENDERED BY SABITORI ITSELF · NO HTML").font_size(10.5).letter_spacing(0.6).color(hex(TEXT_DIM)),
        ]);

        div()
            .w(Px(ctx.width))
            .h(Px(ctx.height))
            .gradient(hex(BG0), hex(BG1), 90.0)
            .flex_col()
            .items_center()
            .gap(10.0)
            .pt(Px(26.0))
            .pb(Px(20.0))
            .children([backdrop(ctx.width, ctx.height, self.anim_t), nav_bar(cw, self.cur, self.prev, self.trans, accent, t, self.fps), stage, footer])
    }

    fn on_click(&mut self, id: &str) {
        // The live text field: clicking it takes focus; any other click blurs.
        if id == "wg-input" {
            self.input_focused = true;
            return;
        }
        self.input_focused = false;

        // Real theme Dropdown — open / close / select. Selecting recolours the page.
        match self.theme.handle_click(id) {
            DropdownEvent::Selected(i) => {
                self.accent_idx = i;
                return;
            }
            DropdownEvent::Opened | DropdownEvent::Closed => return,
            DropdownEvent::Ignored => {}
        }
        if id == "wg-toggle" {
            self.anim_on = !self.anim_on;
            return;
        }
        if let Some(rest) = id.strip_prefix("wg-row-") {
            if let Ok(i) = rest.parse::<usize>() {
                self.table_sel = i;
            }
            return;
        }
        // Section CTAs navigate but must NOT reuse a `nav-{i}` id: the
        // StyleAnimator is keyed by id, so a duplicate id bleeds the button's
        // background/border onto the same-numbered nav tab.
        if let Some(target) = match id {
            "goto-why" => Some(1),
            "goto-live" => Some(4),
            "goto-home" => Some(0),
            // Footer links (external URLs → closest in-app screen for now).
            "foot-docs" | "foot-github" => Some(5), // Code
            "foot-crates" => Some(6),               // Start (cargo add)
            "foot-examples" => Some(4),             // Gallery
            _ => None,
        } {
            self.goto(target);
            return;
        }
        if let Some(rest) = id.strip_prefix("nav-") {
            if let Ok(i) = rest.parse::<usize>() {
                self.goto(i);
            }
        } else if let Some(rest) = id.strip_prefix("feat-") {
            if let Ok(i) = rest.parse::<usize>() {
                self.feat_sel = i;
            }
        }
    }

    fn on_hover_change(&mut self, id: Option<&str>) {
        self.hovered = id.map(|s| s.to_string());
    }

    // 0.4.0 で `on_input` によるテキスト欄への手動配線を削除した。
    // `text_input(..)` を `view()` に置いた時点でランタイムが面倒を見るので、
    // ここに書くことは何も無い (書くと二重処理になる)。
}

// Native example entry. On wasm the page is driven by `web/sabitori-home`,
// which pulls this file in as a module and provides its own entry, so this
// `main` is gated off there.
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    sabitori::run_declarative(Home::new());
}
