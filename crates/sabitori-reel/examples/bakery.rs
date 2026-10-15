//! sabitori-reel — **a little bakery** (22 s), one of the made-up-business
//! series: "Hearth & Hum", a corner bakery that does not exist.
//!
//! - flour drifts down through the whole piece (plain dots, no blur);
//! - the pictures are Fluent Emoji (MIT, `assets/fluent/`) read with `Svg`
//!   and taken apart with `element_with`: a sheaf of grain grows up and its
//!   ears pop on one by one, a loaf is built from its pieces (crust, top, cut
//!   face) and sprinkled, steam curls up off it;
//! - the name drops in letter by letter like dough (`per_glyph`) and bakes:
//!   each letter turns from raw cream to golden brown, the edges first
//!   (`color_spans` re-coloured every frame);
//! - the menu is three pastries dropped onto cards, squashing as they land,
//!   the bagel getting its seeds one at a time, with paper price tags;
//! - the camera leans in slowly the whole time (`.zoom(..)`).
//!
//!   cargo run -p sabitori-reel --example bakery --release
//!
//! Output: `target/reel/bakery.mp4` (`--out <dir>` to change). Frame numbers
//! write just those frames as PNGs.

#[path = "common/out.rs"]
mod out;

use out::Args;
use sabitori_reel::{
    div, interpolate, interpolate_eased, path, spring_with, text, Color, Easing, Element, FrameCtx, GlyphMotion,
    MotionBlur, Px, Reel, Spring, Svg,
};

/// 絵 (Fluent Emoji の Flat、MIT)。1 度だけ読む。
struct Art {
    sheaf: Svg,
    bread: Svg,
    croissant: Svg,
    baguette: Svg,
    bagel: Svg,
}

impl Art {
    fn load() -> Self {
        let svg = |s: &str| Svg::parse(s).expect("素材の SVG が読めない");
        Self {
            sheaf: svg(include_str!("assets/fluent/sheaf_of_rice_flat.svg")),
            bread: svg(include_str!("assets/fluent/bread_flat.svg")),
            croissant: svg(include_str!("assets/fluent/croissant_flat.svg")),
            baguette: svg(include_str!("assets/fluent/baguette_bread_flat.svg")),
            bagel: svg(include_str!("assets/fluent/bagel_flat.svg")),
        }
    }
}

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 22.0;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}
// 色: 粉の白・生地の生成り・焼き色・焦げ茶。
fn cream() -> Color {
    hex("#f4e9d4")
}
fn dough() -> Color {
    hex("#efdcb8")
}
fn golden() -> Color {
    hex("#c9822f")
}
fn crust() -> Color {
    hex("#8a4a1e")
}
fn toast() -> Color {
    hex("#3d2414")
}

fn spring_at(t: f32, start: f32, cfg: Spring) -> f32 {
    if t < start {
        0.0
    } else {
        spring_with(t - start, FPS, cfg)
    }
}

fn hash(i: u32, k: u32) -> f32 {
    let mut h = i.wrapping_mul(0x9e37_79b1) ^ k.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 13;
    (h & 0xffff) as f32 / 65535.0
}

// ── 粉 ───────────────────────────────────────────────────────────────────

/// ゆっくり落ちながら揺れる粉。画面の上から下へ、尽きたら上に戻る。
fn flour(t: f32) -> Vec<Element> {
    (0..90u32)
        .map(|i| {
            let speed = 22.0 + hash(i, 1) * 40.0;
            let y = (hash(i, 2) * (H + 40.0) + t * speed).rem_euclid(H + 40.0) - 20.0;
            let x = hash(i, 3) * W + (t * (0.6 + hash(i, 4)) + i as f32).sin() * 24.0;
            let r = 2.0 + hash(i, 5) * 4.5;
            div()
                .absolute()
                .pos(x - r, y - r)
                .w(Px(r * 2.0))
                .h(Px(r * 2.0))
                .rounded_px(r)
                .bg(Color::WHITE.with_alpha(0.35 + hash(i, 6) * 0.4))
        })
        .collect()
}

// ── 1. 稲穂 (0〜4.6 秒) ──────────────────────────────────────────────────

/// 束が下から伸びて揺れ、穂 (黄色の形、3 番目から) が 1 つずつ弾んで付く。
fn sheaf(t: f32, art: &Art) -> Element {
    let grow = spring_at(t, 0.3, Spring::gentle());
    let leave = interpolate_eased(t, [4.0, 4.6], [0.0, 1.0], Easing::EaseIn);
    let sway = (t * 1.6).sin() * 0.05 * grow.min(1.0);
    let size = 640.0;
    let pict = art.sheaf.element_with(size, |part, e| {
        if part.index >= 2 {
            let s = spring_at(t, 1.3 + (part.index - 2) as f32 * 0.35, Spring::bouncy());
            e.scaled(s.max(0.0)).opacity(s.clamp(0.0, 1.0))
        } else {
            e
        }
    });
    div()
        .absolute()
        .pos(W * 0.5 - size * 0.5, H * 0.5 - size * 0.5)
        .child(pict)
        .rotate(sway)
        .ty((1.0 - grow) * 420.0 + leave * -60.0)
        .opacity(grow.clamp(0.0, 1.0) * (1.0 - leave))
}

// ── 2. パン (4.4〜10.4 秒) ───────────────────────────────────────────────

/// 食パンを組み立てる: 側面・上面・切り口の順に落ちてきて着地し、最後に粒 (4 番目から)
/// を 1 つずつ散らす。焼き上がったら湯気が上る。
fn bread(t: f32, art: &Art) -> Element {
    let leave = interpolate_eased(t, [9.8, 10.4], [0.0, 1.0], Easing::EaseIn);
    let size = 560.0;
    let pict = art.bread.element_with(size, |part, e| match part.index {
        0..=2 => {
            let s = spring_at(t, 4.6 + part.index as f32 * 0.45, Spring::bouncy());
            e.ty((1.0 - s) * -520.0).opacity(s.clamp(0.0, 1.0) * 2.0)
        }
        k => {
            let s = spring_at(t, 6.4 + (k - 3) as f32 * 0.07, Spring::bouncy());
            e.scaled(s.max(0.0) * 1.0).opacity(s.clamp(0.0, 1.0))
        }
    });
    // 着地の瞬間に全体が少し潰れる。
    let land = (1.0 - ((t - 5.75) * 5.0).abs()).max(0.0);
    let mut d = div()
        .absolute()
        .pos(W * 0.5 - size * 0.5, H * 0.5 - size * 0.5 + 40.0)
        .w(Px(size))
        .h(Px(size))
        .child(pict.scaled(1.0 - land * 0.04));
    // 湯気: 3 本、ゆらゆら上る。
    if t > 7.8 {
        for k in 0..3 {
            let age = t - 7.8 - k as f32 * 0.35;
            if age < 0.0 {
                continue;
            }
            let cyc = (age * 0.6).fract();
            let x = 170.0 + k as f32 * 90.0;
            d = d.child(
                path()
                    .svg_path("M20 120 Q4 96 20 72 Q36 48 20 24 Q8 8 20 0")
                    .stroke(Color::WHITE.with_alpha(0.9), 6.0)
                    .trim(0.0, interpolate(cyc, [0.0, 0.5], [0.0, 1.0]))
                    .w(Px(40.0))
                    .h(Px(120.0))
                    .absolute()
                    .pos(x, -40.0 - cyc * 70.0)
                    .opacity((1.0 - cyc) * interpolate(age, [0.0, 0.4], [0.0, 1.0])),
            );
        }
    }
    d.ty(leave * 60.0).opacity(1.0 - leave)
}

// ── 3. 名前が焼ける (10.2〜15.6 秒) ──────────────────────────────────────

fn name(t: f32) -> Element {
    let word = "Hearth & Hum";
    let n = word.chars().count();
    // 焼き色: 端の字から先に色づく。
    let spans: Vec<(std::ops::Range<usize>, Color)> = word
        .char_indices()
        .enumerate()
        .map(|(i, (b, c))| {
            let edge = ((i as f32 - (n as f32 - 1.0) * 0.5).abs() / (n as f32 * 0.5)).min(1.0);
            let bake = interpolate_eased(t, [11.6 + (1.0 - edge) * 1.0, 13.6 + (1.0 - edge) * 1.0], [0.0, 1.0], Easing::EaseInOut);
            let col = if bake < 0.6 { dough().lerp(golden(), bake / 0.6) } else { golden().lerp(crust(), (bake - 0.6) / 0.4) };
            (b..b + c.len_utf8(), col)
        })
        .collect();
    let leave = interpolate_eased(t, [15.0, 15.6], [0.0, 1.0], Easing::EaseIn);
    div()
        .absolute()
        .pos(0.0, H * 0.34)
        .w(Px(W))
        .flex_col()
        .items_center()
        .gap(18.0)
        .child(
            text(word)
                .font_size(180.0)
                .font_weight(800)
                .color(dough())
                .color_spans(spans)
                .per_glyph(|i, _| {
                    // 生地が落ちて、潰れてから戻る。
                    let s = spring_at(t, 10.3 + i as f32 * 0.07, Spring::bouncy());
                    let land = (1.0 - (s - 1.0).abs() * 4.0).clamp(0.0, 1.0);
                    GlyphMotion {
                        dy: (1.0 - s.min(1.0)) * -260.0,
                        scale: 1.0 + 0.06 * land * (1.0 - s.min(1.0)),
                        // color_spans の色には要素の opacity が掛からないので、ここで消す。
                        opacity: (s * 2.0).clamp(0.0, 1.0) * (1.0 - leave),
                        ..Default::default()
                    }
                }),
        )
        .child(
            text("corner bakery · since 1987")
                .font_size(34.0)
                .letter_spacing(5.0)
                .color(toast().with_alpha(0.7))
                .opacity(interpolate(t, [12.6, 13.4], [0.0, 1.0])),
        )
        .ty(leave * -40.0)
        .opacity(1.0 - leave)
}

// ── 4. 品書き (15.4〜21 秒) ──────────────────────────────────────────────

fn menu(t: f32, art: &Art) -> Vec<Element> {
    let items: [(&str, &str, &Svg, f32); 3] = [
        ("croissant", "¥280", &art.croissant, 15.6),
        ("baguette", "¥360", &art.baguette, 16.0),
        ("bagel", "¥220", &art.bagel, 16.4),
    ];
    let leave = interpolate_eased(t, [20.4, 21.0], [0.0, 1.0], Easing::EaseIn);
    let mut out = Vec::new();
    let head = spring_at(t, 15.4, Spring::gentle());
    out.push(
        div()
            .absolute()
            .pos(0.0, 150.0)
            .w(Px(W))
            .justify_center()
            .child(text("today's oven").font_size(56.0).font_weight(800).color(toast()))
            .ty((1.0 - head) * -30.0)
            .opacity(head.clamp(0.0, 1.0) * (1.0 - leave)),
    );
    for (i, (label, price, svg, at)) in items.into_iter().enumerate() {
        let x = W * 0.5 - 600.0 + i as f32 * 420.0;
        // パンが上から落ちてきて、着地で潰れてから戻る。
        let drop = spring_at(t, at + 0.2, Spring::bouncy());
        let squash = (1.0 - ((t - at - 0.55) * 4.0).abs()).max(0.0);
        let is_bagel = i == 2;
        let pict = svg.element_with(270.0, |part, e| {
            // ベーグルの胡麻 (4 番目から) は着地のあとに 1 粒ずつ。
            if is_bagel && part.index >= 4 {
                let s = spring_at(t, at + 0.9 + (part.index - 4) as f32 * 0.06, Spring::bouncy());
                e.scaled(s.max(0.0)).opacity(s.clamp(0.0, 1.0))
            } else {
                e
            }
        });
        let tag = spring_at(t, at + 1.0, Spring::bouncy());
        let card = div()
            .absolute()
            .pos(x, 340.0)
            .w(Px(360.0))
            .h(Px(420.0))
            .rounded_px(20.0)
            .bg(hex("#fbf4e6"))
            .border(2.0, hex("#e6d3ad"))
            .flex_col()
            .items_center()
            .p(Px(30.0))
            .gap(14.0)
            .justify_center()
            .child(
                pict.ty((1.0 - drop) * -360.0)
                    .scaled(1.0 + squash * 0.06)
                    .opacity((drop * 3.0).clamp(0.0, 1.0)),
            )
            .child(text(label).font_size(40.0).font_weight(700).color(toast()))
            .ty(leave * 80.0)
            .opacity(interpolate(t, [at - 0.3, at], [0.0, 1.0]) * (1.0 - leave));
        out.push(card);
        // 値札: 紙の札を斜めに貼る。
        out.push(
            div()
                .absolute()
                .pos(x + 230.0, 300.0)
                .px_pad(Px(18.0))
                .py(Px(8.0))
                .rounded_px(6.0)
                .bg(hex("#fffaf0"))
                .border(2.0, crust())
                .child(text(price).font_size(34.0).font_weight(800).color(crust()))
                .rotate(0.18 - i as f32 * 0.12 + (1.0 - tag) * 0.6)
                .scaled(0.6 + 0.4 * tag.max(0.0))
                .ty(leave * 80.0)
                .opacity(tag.clamp(0.0, 1.0) * (1.0 - leave)),
        );
    }
    out.push(
        div()
            .absolute()
            .pos(0.0, 840.0)
            .w(Px(W))
            .justify_center()
            .child(text("open 7–3 · tuesday to sunday").font_size(32.0).letter_spacing(3.0).color(toast().with_alpha(0.75)))
            .opacity(interpolate(t, [17.6, 18.4], [0.0, 1.0]) * (1.0 - leave)),
    );
    out
}

fn scene(ctx: FrameCtx, art: &Art) -> Element {
    let t = ctx.t();
    let mut kids: Vec<Element> = vec![div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(W))
        .h(Px(H))
        .radial_gradient_at(hex("#fbf3e3"), hex("#e9d6b4"), 0.5, 0.42)
        .grain(0.05)
        .grain_seed(3)];
    if t < 4.7 {
        kids.push(sheaf(t, art));
    }
    if (4.4..10.5).contains(&t) {
        kids.push(bread(t, art));
    }
    if (10.2..15.7).contains(&t) {
        kids.push(name(t));
    }
    if (15.3..21.1).contains(&t) {
        kids.extend(menu(t, art));
    }
    // 最後は名前に戻って締める。
    if t >= 20.8 {
        let s = spring_at(t, 20.8, Spring::gentle());
        kids.push(
            div()
                .absolute()
                .pos(0.0, H * 0.28)
                .w(Px(W))
                .flex_col()
                .items_center()
                .gap(10.0)
                .child(art.bread.element(220.0).rotate((1.0 - s) * -0.5))
                .child(text("Hearth & Hum").font_size(120.0).font_weight(800).color(crust()))
                .opacity(s.clamp(0.0, 1.0))
                .ty((1.0 - s) * 30.0),
        );
    }
    kids.extend(flour(t));
    let lean = interpolate_eased(t, [0.0, SECS], [0.0, 1.0], Easing::EaseInOut);
    let fin = interpolate(t, [21.5, 22.0], [0.0, 1.0]);
    div()
        .w(Px(W))
        .h(Px(H))
        .bg(cream())
        // 終わりは中身だけを消して、紙の地を残す (全体を薄くすると黒に溶けて濁る)。
        .child(div().absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H)).children(kids).zoom(1.0 + lean * 0.05).opacity(1.0 - fin))
}

fn reel(frames: u32) -> Reel {
    Reel { motion_blur: Some(MotionBlur::new(5)), crf: 24, ..Reel::new(W as u32, H as u32, FPS, frames) }
}

fn main() {
    let frames = (SECS * FPS as f32).round() as u32;
    let args = Args::parse();
    let art = Art::load();
    let scene = |ctx: FrameCtx| scene(ctx, &art);

    if !args.stills.is_empty() {
        let stills = &args.stills;
        let reel = reel(stills.len() as u32);
        let pick = |ctx: FrameCtx| scene(FrameCtx { frame: stills[ctx.frame as usize], ..ctx });
        let dir = args.frames_dir("bakery");
        let paths = reel.render_pngs(&pick, &dir).expect("render failed");
        for (f, p) in stills.iter().zip(&paths) {
            println!("frame {f} -> {}", p.display());
        }
        return;
    }

    let reel = reel(frames);
    let out = args.mp4("bakery");
    println!("rendering + encoding {frames} frames ({}x{} @ {FPS}fps) …", reel.width, reel.height);
    match reel.render_mp4(&scene, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
