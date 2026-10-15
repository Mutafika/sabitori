//! sabitori-reel — **a little bakery** (22 s), one of the made-up-business
//! series: "Hearth & Hum", a corner bakery that does not exist.
//!
//! - flour drifts down through the whole piece (plain dots, no blur);
//! - a wheat stalk and a loaf are drawn as if by hand (`path().trim(..)`), the
//!   loaf fills in from the bottom like it is rising (`.mask(..)`), steam curls
//!   up off it;
//! - the name drops in letter by letter like dough (`per_glyph`) and bakes:
//!   each letter turns from raw cream to golden brown, the edges first
//!   (`color_spans` re-coloured every frame);
//! - the menu is three line icons with paper price tags slapped on;
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
    MotionBlur, Percent, Px, Reel, Spring,
};

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

// ── 1. 麦 (0〜4.6 秒) ────────────────────────────────────────────────────

fn wheat(t: f32) -> Element {
    let stem = interpolate_eased(t, [0.4, 1.8], [0.0, 1.0], Easing::EaseInOut);
    let leave = interpolate_eased(t, [4.0, 4.6], [0.0, 1.0], Easing::EaseIn);
    let mut d = div().absolute().pos(W * 0.5 - 160.0, H * 0.5 - 330.0).w(Px(320.0)).h(Px(620.0));
    // 茎: 下から上へ、少し曲がって描かれる。
    d = d.child(
        path()
            .svg_path("M160 600 Q150 420 165 250 Q175 150 160 60")
            .stroke(crust(), 6.0)
            .trim(0.0, stem)
            .w(Px(320.0))
            .h(Px(620.0))
            .absolute()
            .pos(0.0, 0.0),
    );
    // 穂: 茎の上の方に、左右交互に粒が付いていく。
    for k in 0..10 {
        let side = if k % 2 == 0 { -1.0 } else { 1.0 };
        let y = 90.0 + k as f32 * 26.0;
        let s = spring_at(t, 1.4 + k as f32 * 0.12, Spring::bouncy());
        d = d.child(
            div()
                .absolute()
                .pos(160.0 + side * 22.0 - 15.0, y)
                .w(Px(30.0))
                .h(Px(54.0))
                .rounded_px(15.0)
                .bg(hex("#d9a54e"))
                .border(2.5, crust())
                .rotate(side * 0.55)
                .scaled(s.max(0.0))
                .opacity(s.clamp(0.0, 1.0)),
        );
    }
    // 葉: 線で 1 本。
    d = d.child(
        path()
            .svg_path("M162 470 Q220 430 262 330")
            .stroke(crust(), 4.0)
            .trim(0.0, interpolate(t, [1.6, 2.4], [0.0, 1.0]))
            .w(Px(320.0))
            .h(Px(620.0))
            .absolute()
            .pos(0.0, 0.0),
    );
    d.ty(leave * -60.0).opacity(1.0 - leave)
}

// ── 2. パン (4.4〜10.4 秒) ───────────────────────────────────────────────

const LOAF: &str = "M30 170 Q24 90 120 66 Q210 44 300 66 Q396 90 390 170 Q392 196 360 198 L60 198 Q28 196 30 170 Z";

fn loaf(t: f32) -> Element {
    let outline = interpolate_eased(t, [4.6, 6.0], [0.0, 1.0], Easing::EaseInOut);
    let rise = interpolate_eased(t, [5.8, 7.4], [0.0, 1.0], Easing::EaseOutCubic);
    let scores = interpolate_eased(t, [7.2, 8.0], [0.0, 1.0], Easing::EaseOutCubic);
    let leave = interpolate_eased(t, [9.8, 10.4], [0.0, 1.0], Easing::EaseIn);
    let (lw, lh) = (420.0, 220.0);
    let shape = |p: Element| p.view_box(0.0, 0.0, 420.0, 220.0).w(Px(lw)).h(Px(lh)).absolute().pos(0.0, 0.0);
    // 中身は下から膨らむ: 塗った形を、下から伸びる箱でマスクする。
    let fill = div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(lw))
        .h(Px(lh))
        .child(shape(path().svg_path(LOAF).fill(golden())))
        .mask(
            div()
                .w(Percent(100.0))
                .h(Percent(100.0))
                .flex_col()
                .justify_end()
                .child(div().w(Percent(100.0)).h(Percent(rise * 100.0)).bg(Color::BLACK)),
        );
    let mut d = div()
        .absolute()
        .pos(W * 0.5 - lw * 0.5, H * 0.5 - 60.0)
        .w(Px(lw))
        .h(Px(lh))
        .child(fill)
        .child(shape(path().svg_path(LOAF).stroke(toast(), 7.0).trim(0.0, outline)));
    // 切れ目: 斜めに 3 本。
    for k in 0..3 {
        let x0 = 120.0 + k as f32 * 70.0;
        let d_k = interpolate(scores, [k as f32 / 3.0, (k as f32 + 1.0) / 3.0], [0.0, 1.0]);
        d = d.child(shape(
            path()
                .move_to(x0, 120.0)
                .quad_to(x0 + 30.0, 92.0, x0 + 52.0, 84.0)
                .stroke(hex("#f3dcae"), 7.0)
                .trim(0.0, d_k),
        ));
    }
    // 湯気: 3 本、ゆらゆら上る。
    if t > 7.6 {
        for k in 0..3 {
            let age = t - 7.6 - k as f32 * 0.35;
            if age < 0.0 {
                continue;
            }
            let cyc = (age * 0.6).fract();
            let x = 130.0 + k as f32 * 80.0;
            d = d.child(
                path()
                    .svg_path("M20 120 Q4 96 20 72 Q36 48 20 24 Q8 8 20 0")
                    .stroke(Color::WHITE.with_alpha(0.85), 5.0)
                    .trim(0.0, interpolate(cyc, [0.0, 0.5], [0.0, 1.0]))
                    .w(Px(40.0))
                    .h(Px(120.0))
                    .absolute()
                    .pos(x, -150.0 - cyc * 60.0)
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

fn menu(t: f32) -> Vec<Element> {
    let items: [(&str, &str, &str, f32); 3] = [
        // 三日月のクロワッサン、細長いバゲット、丸いパン。
        ("croissant", "¥280", "M20 110 Q30 40 100 30 Q170 40 180 110 Q150 80 100 78 Q50 80 20 110 Z M60 60 L74 92 M100 46 L100 82 M140 60 L126 92", 15.6),
        ("baguette", "¥360", "M14 120 Q10 100 40 88 L170 40 Q196 32 192 56 Q190 70 160 82 L36 130 Q16 136 14 120 Z M64 92 L84 80 M100 78 L120 66 M136 64 L156 52", 16.0),
        ("milk bun", "¥180", "M24 116 Q20 50 100 44 Q180 50 176 116 Q176 132 156 132 L44 132 Q24 132 24 116 Z M70 78 Q100 62 130 78", 16.4),
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
    for (i, (label, price, d, at)) in items.into_iter().enumerate() {
        let x = W * 0.5 - 600.0 + i as f32 * 420.0;
        let draw = interpolate_eased(t, [at, at + 1.1], [0.0, 1.0], Easing::EaseInOut);
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
            .gap(18.0)
            .child(
                path()
                    .svg_path(d)
                    .view_box(0.0, 0.0, 200.0, 150.0)
                    .stroke(crust(), 5.0)
                    .trim(0.0, draw)
                    .w(Px(260.0))
                    .h(Px(200.0)),
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

fn scene(ctx: FrameCtx) -> Element {
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
        kids.push(wheat(t));
    }
    if (4.4..10.5).contains(&t) {
        kids.push(loaf(t));
    }
    if (10.2..15.7).contains(&t) {
        kids.push(name(t));
    }
    if (15.3..21.1).contains(&t) {
        kids.extend(menu(t));
    }
    // 最後は名前に戻って締める。
    if t >= 20.8 {
        let s = spring_at(t, 20.8, Spring::gentle());
        kids.push(
            div()
                .absolute()
                .pos(0.0, H * 0.42)
                .w(Px(W))
                .justify_center()
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
