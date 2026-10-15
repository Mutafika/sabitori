//! sabitori-reel — **a playful studio intro** (22 s) for "studio mutafika",
//! a studio that does not exist. Bouncing, sticking and slapping:
//!
//! - a ball drops in and bounces with squash and stretch, meets a second ball,
//!   and the two stretch into one blob (`goo`) before popping into the name,
//!   letter by letter (`per_glyph`);
//! - the motto is slapped on word by word like stickers, each at its own tilt
//!   (`rotate`);
//! - three made-up products fly in like a dealt hand of cards and fan out;
//! - a winking face (`path`) and confetti close it out.
//!
//! Paper-cream ground, four flat colours, light grain — no gradients from the
//! usual purple-to-cyan shelf.
//!
//!   cargo run -p sabitori-reel --example studio --release
//!
//! Output: `target/reel/studio.mp4` (`--out <dir>` to change). Frame numbers
//! write just those frames as PNGs.

#[path = "common/out.rs"]
mod out;

use std::f32::consts::{PI, TAU};

use out::Args;
use sabitori_core::{goo, Rect};
use sabitori_reel::{
    div, interpolate, interpolate_eased, path, spring_with, text, Color, Easing, Element, FrameCtx, GlyphMotion,
    MotionBlur, Px, Reel, Spring,
};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 22.0;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}
fn paper() -> Color {
    hex("#f3ece0")
}
fn ink() -> Color {
    hex("#1f1b17")
}
fn tomato() -> Color {
    hex("#f2553a")
}
fn mustard() -> Color {
    hex("#f0b03a")
}
fn cobalt() -> Color {
    hex("#2f4fd6")
}
fn mint() -> Color {
    hex("#3fae8f")
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

// ── 1. 弾む玉 (0〜6 秒) ───────────────────────────────────────────────────

/// 弾み: (床からの高さ 0..1, 潰れ -1..1)。跳ねるたびに高さと間隔が縮む。
fn bounce(t: f32) -> (f32, f32) {
    let mut start = 0.0;
    let mut dur = 0.9;
    let mut amp = 1.0;
    // 最初は上から落ちてくる (半分の弧)。
    if t < dur * 0.5 {
        let u = t / (dur * 0.5);
        let h = 1.0 - u * u;
        return (h, -0.25 * u);
    }
    start += dur * 0.5;
    for _ in 0..6 {
        dur *= 0.62;
        amp *= 0.38;
        if t < start + dur {
            let u = (t - start) / dur;
            let h = amp * 4.0 * u * (1.0 - u);
            // 床に近いほど潰れ、空中では縦に伸びる。
            let squash = if h < 0.04 { 0.35 * (1.0 - h / 0.04) * amp.sqrt().max(0.4) } else { -0.12 * amp.sqrt() };
            return (h, squash);
        }
        start += dur;
    }
    (0.0, 0.0)
}

fn ball(cx: f32, floor: f32, r: f32, t: f32, color: Color) -> Element {
    let (h, squash) = bounce(t);
    let (w, hh) = (r * 2.0 * (1.0 + squash), r * 2.0 * (1.0 - squash));
    let y = floor - h * 520.0 - hh;
    div()
        .absolute()
        .pos(cx - w * 0.5, y)
        .w(Px(w))
        .h(Px(hh))
        .rounded_px(w.min(hh) * 0.5)
        .bg(color)
}

/// 床の影 (玉が高いほど小さく薄い)。
fn shadow(cx: f32, floor: f32, r: f32, t: f32) -> Element {
    let (h, _) = bounce(t);
    let s = 1.0 - h * 0.6;
    div()
        .absolute()
        .pos(cx - r * s, floor - 8.0)
        .w(Px(r * 2.0 * s))
        .h(Px(16.0))
        .rounded_px(8.0)
        .bg(ink().with_alpha(0.12 * s))
        .blur(6.0)
}

fn opening(t: f32) -> Vec<Element> {
    let floor = H * 0.62;
    let r = 70.0;
    let mut out = Vec::new();
    if t < 3.6 {
        // 赤が左から、青が少し遅れて右から跳ねてくる。
        let ax = interpolate_eased(t, [0.0, 2.2], [W * 0.18, W * 0.42], Easing::EaseOut);
        let bx = interpolate_eased(t, [0.6, 2.6], [W * 0.84, W * 0.58], Easing::EaseOut);
        out.push(shadow(ax, floor, r, t));
        out.push(shadow(bx, floor, r, (t - 0.6).max(0.0)));
        out.push(ball(ax, floor, r, t, tomato()));
        if t > 0.6 {
            out.push(ball(bx, floor, r, t - 0.6, cobalt()));
        }
    } else if t < 5.2 {
        // くっついて伸び、真ん中へ寄って 1 つになる。
        let p = interpolate_eased(t, [3.6, 4.8], [0.0, 1.0], Easing::EaseInOut);
        let gap = (W * 0.16) * (1.0 - p);
        let cy = floor - r * 2.0;
        let a = Rect::new(W * 0.5 - gap * 0.5 - r, cy, r * 2.0, r * 2.0);
        let b = Rect::new(W * 0.5 + gap * 0.5 - r, cy, r * 2.0, r * 2.0);
        let col = tomato().lerp(cobalt(), p * 0.5);
        out.push(goo(a, b).goo_radii(r, r).goo_smooth(90.0).goo_color(col).absolute().pos(0.0, 0.0));
        // くっついた瞬間に少し膨らむ。
        let pop = spring_at(t, 4.8, Spring::bouncy());
        if t > 4.8 {
            let s = r * 2.0 * (1.0 + 0.4 * pop);
            out.push(
                div()
                    .absolute()
                    .pos(W * 0.5 - s * 0.5, cy + r - s * 0.5)
                    .w(Px(s))
                    .h(Px(s))
                    .rounded_px(s * 0.5)
                    .bg(col)
                    .opacity(1.0 - interpolate(t, [4.95, 5.2], [0.0, 1.0])),
            );
        }
    }
    out
}

// ── 名前 (5〜8.4 秒) ──────────────────────────────────────────────────────

fn name(t: f32) -> Element {
    let colors = [tomato(), mustard(), cobalt(), mint()];
    let word = "mutafika";
    let spans: Vec<(std::ops::Range<usize>, Color)> = (0..word.len()).map(|i| (i..i + 1, colors[i % 4])).collect();
    let leave = interpolate_eased(t, [8.0, 8.5], [0.0, 1.0], Easing::EaseIn);
    div()
        .absolute()
        .pos(0.0, H * 0.32)
        .w(Px(W))
        .flex_col()
        .items_center()
        .gap(10.0)
        .child(
            text(word)
                .font_size(230.0)
                .font_weight(900)
                .letter_spacing(-4.0)
                .color(ink())
                .color_spans(spans)
                .per_glyph(|i, n| {
                    // 真ん中から外へ、順に弾け出る。
                    let c = i as f32 - (n as f32 - 1.0) * 0.5;
                    let s = spring_at(t, 5.0 + c.abs() * 0.07, Spring::bouncy());
                    GlyphMotion {
                        dx: -c * 60.0 * (1.0 - s),
                        dy: (1.0 - s) * 40.0 - leave * (200.0 + hash(i as u32, 1) * 300.0),
                        rotation: (1.0 - s) * (hash(i as u32, 2) - 0.5) * 2.0 + leave * (hash(i as u32, 3) - 0.5) * 3.0,
                        scale: 0.2 + 0.8 * s,
                        opacity: s.clamp(0.0, 1.0) * (1.0 - leave),
                    }
                }),
        )
        .child(
            text("s t u d i o")
                .font_size(36.0)
                .font_weight(700)
                .color(ink())
                .opacity(interpolate(t, [6.0, 6.6], [0.0, 1.0]) * (1.0 - leave)),
        )
}

// ── 2. シールの言葉 (8.4〜13 秒) ──────────────────────────────────────────

fn stickers(t: f32) -> Vec<Element> {
    let words: [(&str, Color, Color, f32, f32, f32); 5] = [
        ("We", ink(), paper(), 520.0, 330.0, -0.08),
        ("make", tomato(), paper(), 760.0, 310.0, 0.06),
        ("small,", mustard(), ink(), 1080.0, 340.0, -0.04),
        ("sharp", cobalt(), paper(), 640.0, 520.0, 0.05),
        ("software.", mint(), paper(), 980.0, 540.0, -0.07),
    ];
    let leave = interpolate_eased(t, [12.4, 13.0], [0.0, 1.0], Easing::EaseIn);
    let mut out = Vec::new();
    for (i, (word, bg, fg, x, y, tilt)) in words.into_iter().enumerate() {
        let at = 8.6 + i as f32 * 0.42;
        if t < at {
            continue;
        }
        let s = spring_at(t, at, Spring::bouncy());
        // 貼る瞬間に大きく、少し回りすぎてから落ち着く。
        let sticker = div()
            .absolute()
            .pos(x + 110.0, y)
            .px_pad(Px(34.0))
            .py(Px(14.0))
            .rounded_px(26.0)
            .bg(bg)
            .shadow_md(ink().with_alpha(0.25))
            .child(text(word).font_size(96.0).font_weight(900).color(fg))
            .rotate(tilt + (1.0 - s) * 0.5)
            .scaled(0.7 + 0.3 * s + (1.0 - s.min(1.0)) * 0.6)
            .ty(leave * (300.0 + i as f32 * 60.0))
            .opacity(s.clamp(0.0, 1.0) * (1.0 - leave));
        out.push(sticker);
    }
    out
}

// ── 3. 作った物のカード (13〜18 秒) ──────────────────────────────────────

fn card(title: &str, sub: &str, color: Color, art: Element) -> Element {
    div()
        .w(Px(360.0))
        .h(Px(470.0))
        .rounded_px(28.0)
        .bg(Color::WHITE)
        .shadow_md(ink().with_alpha(0.2))
        .p(Px(24.0))
        .flex_col()
        .gap(14.0)
        .child(div().w(Px(312.0)).h(Px(300.0)).rounded_px(18.0).bg(color).items_center().justify_center().child(art))
        .child(text(title).font_size(32.0).font_weight(800).color(ink()))
        .child(text(sub).font_size(20.0).color(ink().with_alpha(0.6)))
}

fn cards(t: f32) -> Vec<Element> {
    let leave = interpolate_eased(t, [17.4, 18.0], [0.0, 1.0], Easing::EaseIn);
    let arts = [
        // 帳簿: 積み上がる棒。
        {
            let mut d = div().flex_row().items_end().gap(14.0);
            for (k, hgt) in [70.0, 120.0, 95.0, 170.0].into_iter().enumerate() {
                let g = spring_at(t, 14.0 + k as f32 * 0.12, Spring::bouncy());
                d = d.child(div().w(Px(40.0)).h(Px(hgt * g.max(0.0))).rounded_px(8.0).bg(paper()));
            }
            d
        },
        // 夜勤: 月と星 (線から描かれる)。
        path()
            .svg_path("M60 10 A40 40 0 1 0 100 60 A30 30 0 1 1 60 10 Z")
            .view_box(0.0, 0.0, 110.0, 110.0)
            .fill(paper())
            .stroke(paper(), 3.0)
            .trim(0.0, interpolate(t, [14.2, 15.0], [0.0, 1.0]))
            .w(Px(160.0))
            .h(Px(160.0)),
        // 庭: 回る花。
        {
            let mut d = div().w(Px(180.0)).h(Px(180.0));
            for k in 0..6 {
                let a = k as f32 / 6.0 * TAU + t * 0.8;
                d = d.child(
                    div()
                        .absolute()
                        .pos(90.0 + a.cos() * 48.0 - 30.0, 90.0 + a.sin() * 48.0 - 30.0)
                        .w(Px(60.0))
                        .h(Px(60.0))
                        .rounded_px(30.0)
                        .bg(paper()),
                );
            }
            d.child(div().absolute().pos(66.0, 66.0).w(Px(48.0)).h(Px(48.0)).rounded_px(24.0).bg(mustard()))
        },
    ];
    let info = [
        ("Pocket Ledger", "money, minus the spreadsheet", tomato()),
        ("Night Shift", "a calm editor for 2 a.m.", cobalt()),
        ("Paper Garden", "notes that grow into plans", mint()),
    ];
    let mut out = Vec::new();
    let heading_in = spring_at(t, 13.1, Spring::gentle());
    out.push(
        div()
            .absolute()
            .pos(0.0, 110.0)
            .w(Px(W))
            .items_center()
            .child(text("things we made (pretend)").font_size(44.0).font_weight(800).color(ink()))
            .opacity((heading_in * (1.0 - leave)).clamp(0.0, 1.0))
            .ty((1.0 - heading_in) * -30.0),
    );
    for (i, (art, (title, sub, color))) in arts.into_iter().zip(info).enumerate() {
        let at = 13.4 + i as f32 * 0.25;
        let deal = spring_at(t, at, Spring::gentle());
        let fan = spring_at(t, 15.0, Spring::bouncy());
        let k = i as f32 - 1.0;
        // 配る: 右下の画面の外から飛んで真ん中に重なり、15 秒で扇に開く。
        let (sx, sy) = (W + 300.0, H + 200.0);
        let (cx, cy) = (W * 0.5 - 180.0, H * 0.5 - 200.0);
        let x = sx + (cx - sx) * deal + k * 400.0 * fan;
        let y = sy + (cy - sy) * deal + k.abs() * 40.0 * fan;
        out.push(
            card(title, sub, color, art)
                .absolute()
                .pos(x, y)
                .rotate((1.0 - deal) * 1.2 + k * 0.12 * fan)
                .ty(leave * (500.0 + i as f32 * 80.0))
                .opacity(1.0 - leave),
        );
    }
    out
}

// ── 締め (18〜22 秒) ─────────────────────────────────────────────────────

fn face(t: f32) -> Element {
    // 片目は 20.2 秒でウィンク。
    let wink = 1.0 - ((t - 20.2) * 6.0).abs().min(1.0);
    let draw = interpolate_eased(t, [18.4, 19.4], [0.0, 1.0], Easing::EaseOutCubic);
    let eye_h = 26.0 * (1.0 - wink * 0.9);
    div()
        .w(Px(220.0))
        .h(Px(220.0))
        .rounded_px(110.0)
        .bg(mustard())
        .child(div().absolute().pos(62.0, 72.0).w(Px(22.0)).h(Px(26.0)).rounded_px(11.0).bg(ink()))
        .child(div().absolute().pos(136.0, 72.0 + (26.0 - eye_h) * 0.5).w(Px(22.0)).h(Px(eye_h)).rounded_px(eye_h.min(22.0) * 0.5).bg(ink()))
        .child(
            path()
                .svg_path("M55 125 Q110 185 165 125")
                .stroke(ink(), 12.0)
                .trim(0.0, draw)
                .w(Px(220.0))
                .h(Px(220.0))
                .absolute()
                .pos(0.0, 0.0),
        )
}

fn confetti(t: f32) -> Vec<Element> {
    let colors = [tomato(), mustard(), cobalt(), mint(), ink()];
    let born = 18.6;
    let age = t - born;
    if age < 0.0 {
        return Vec::new();
    }
    (0..60u32)
        .map(|i| {
            let a = hash(i, 1) * PI + PI; // 上半分へ
            let v = 600.0 + hash(i, 2) * 900.0;
            let (vx, vy) = (a.cos() * v * 0.9, a.sin() * v);
            let x = W * 0.5 + vx * age;
            let y = H * 0.5 + vy * age + 900.0 * age * age;
            let size = 12.0 + hash(i, 3) * 18.0;
            let spin = (hash(i, 4) - 0.5) * 14.0 * age;
            let shape = if i % 3 == 0 {
                div().w(Px(size)).h(Px(size)).rounded_px(size * 0.5)
            } else {
                div().w(Px(size * 1.6)).h(Px(size * 0.6)).rounded_px(3.0)
            };
            shape
                .bg(colors[i as usize % colors.len()])
                .absolute()
                .pos(x, y)
                .rotate(spin)
                .opacity(interpolate(age, [2.2, 3.2], [1.0, 0.0]))
        })
        .collect()
}

fn outro(t: f32) -> Vec<Element> {
    let s = spring_at(t, 18.2, Spring::bouncy());
    let words = spring_at(t, 18.9, Spring::gentle());
    let mut out = confetti(t);
    out.push(
        div()
            .absolute()
            .pos(0.0, 0.0)
            .w(Px(W))
            .h(Px(H))
            .flex_col()
            .items_center()
            .justify_center()
            .gap(36.0)
            .child(face(t).scaled(s.max(0.0)).rotate((1.0 - s) * -0.6))
            .child(
                text("studio mutafika")
                    .font_size(96.0)
                    .font_weight(900)
                    .color(ink())
                    .per_glyph(|i, _| {
                        let s = spring_at(t, 18.9 + i as f32 * 0.03, Spring::bouncy());
                        GlyphMotion { dy: (1.0 - s) * 50.0, opacity: s.clamp(0.0, 1.0), ..Default::default() }
                    }),
            )
            .child(
                text("made with care (and sabitori)")
                    .font_size(30.0)
                    .color(ink().with_alpha(0.6))
                    .opacity(words.clamp(0.0, 1.0)),
            ),
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
        .bg(paper())
        .grain(0.05)
        // 紙の粒は止めておく (コマごとに変えると、動画が何倍も重くなる)。
        .grain_seed(1)];
    if t < 5.3 {
        kids.extend(opening(t));
    }
    if (4.9..8.6).contains(&t) {
        kids.push(name(t));
    }
    if (8.5..13.1).contains(&t) {
        kids.extend(stickers(t));
    }
    if (13.0..18.1).contains(&t) {
        kids.extend(cards(t));
    }
    if t >= 18.1 {
        kids.extend(outro(t));
    }
    let fin = interpolate(t, [21.4, 22.0], [0.0, 1.0]);
    div().w(Px(W)).h(Px(H)).bg(paper()).children(kids).opacity(1.0 - fin)
}

fn reel(frames: u32) -> Reel {
    Reel { motion_blur: Some(MotionBlur::new(6)), crf: 24, ..Reel::new(W as u32, H as u32, FPS, frames) }
}

fn main() {
    let frames = (SECS * FPS as f32).round() as u32;
    let args = Args::parse();

    if !args.stills.is_empty() {
        let stills = &args.stills;
        let reel = reel(stills.len() as u32);
        let pick = |ctx: FrameCtx| scene(FrameCtx { frame: stills[ctx.frame as usize], ..ctx });
        let dir = args.frames_dir("studio");
        let paths = reel.render_pngs(&pick, &dir).expect("render failed");
        for (f, p) in stills.iter().zip(&paths) {
            println!("frame {f} -> {}", p.display());
        }
        return;
    }

    let reel = reel(frames);
    let out = args.mp4("studio");
    println!("rendering + encoding {frames} frames ({}x{} @ {FPS}fps) …", reel.width, reel.height);
    match reel.render_mp4(&scene, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
