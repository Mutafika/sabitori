//! sabitori-reel — **a late-night radio station ident** (22 s) for "Night
//! Lantern 82.4", a station that does not exist.
//!
//! - the tuner needle sweeps through static and springs onto 82.4 MHz while
//!   the knob turns with it;
//! - the waveform (`polyline`) goes from noise to a calm voice-like wave once
//!   the station locks;
//! - the neon sign flickers on: each glyph is lit (`per_glyph`) and doubled by
//!   a blurred copy added on top (`BlendMode::Add`), so the tubes glow;
//! - the night's programme scrolls past like an LED board, then the camera
//!   eases back (`zoom`) and the sign hums out.
//!
//! Warm amber and vermilion neon on a deep green-black, fixed film grain.
//!
//!   cargo run -p sabitori-reel --example night_radio --release
//!
//! Output: `target/reel/night_radio.mp4` (`--out <dir>` to change). Frame
//! numbers write just those frames as PNGs.

#[path = "common/out.rs"]
mod out;

use out::Args;
use sabitori_reel::{
    div, interpolate, interpolate_eased, polyline, spring_with, text, BlendMode, Color, Easing, Element, FrameCtx,
    GlyphMotion, MotionBlur, Px, Reel, Spring,
};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 22.0;

/// 局の周波数と、目盛りの端。
const STATION: f32 = 82.4;
const LO: f32 = 76.0;
const HI: f32 = 95.0;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}
fn night() -> Color {
    hex("#070c0a")
}
fn panel() -> Color {
    hex("#0f1814")
}
fn amber() -> Color {
    hex("#ffb547")
}
fn vermilion() -> Color {
    hex("#ff5a36")
}
fn dim() -> Color {
    hex("#6f5a35")
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

// ── 合わせる (0〜5 秒) ───────────────────────────────────────────────────

/// 針の周波数: 静電気の中を行ったり来たりして、4 秒目からばねで 82.4 に止まる。
fn needle(t: f32) -> f32 {
    if t < 3.6 {
        let sweep = interpolate_eased(t, [0.2, 3.6], [0.0, 1.0], Easing::EaseInOut);
        // 端から端へ 1 往復半、局の手前で迷う。
        LO + 2.0 + (HI - LO - 4.0) * (0.5 - 0.5 * (sweep * std::f32::consts::PI * 2.6).cos()) * (1.0 - sweep * 0.3)
    } else {
        let from = needle(3.599);
        let s = spring_at(t, 3.6, Spring::bouncy());
        from + (STATION - from) * s
    }
}

/// 合っている度合い (0 = 雑音, 1 = 局)。
fn lock(t: f32) -> f32 {
    let d = (needle(t) - STATION).abs();
    (1.0 - d / 1.2).clamp(0.0, 1.0) * interpolate(t, [3.4, 4.6], [0.0, 1.0])
}

fn dial(t: f32) -> Element {
    let (x0, x1, y) = (260.0, 1660.0, 0.0);
    let to_x = |f: f32| x0 + (f - LO) / (HI - LO) * (x1 - x0);
    let mut d = div().absolute().pos(0.0, 0.0).w(Px(W)).h(Px(170.0));
    // 目盛り: 0.2 MHz ごと、整数で長く、偶数に数字。
    let mut f = LO;
    let mut k = 0;
    while f <= HI + 1e-3 {
        let major = k % 5 == 0;
        let x = to_x(f);
        let near = (1.0 - (f - needle(t)).abs() / 2.0).clamp(0.0, 1.0);
        let c = dim().lerp(amber(), near * 0.8);
        d = d.child(div().absolute().pos(x - 1.0, y + 70.0 - if major { 26.0 } else { 12.0 }).w(Px(2.0)).h(Px(if major { 26.0 } else { 12.0 })).bg(c));
        if major && (f.round() as i32) % 2 == 0 {
            d = d.child(
                text(format!("{}", f.round() as i32))
                    .font_size(22.0)
                    .mono()
                    .color(c)
                    .absolute()
                    .pos(x - 14.0, y + 82.0),
            );
        }
        f += 0.2;
        k += 1;
    }
    // 針: 朱の線と、加算で重ねた光。
    let nx = to_x(needle(t));
    d = d
        .child(div().absolute().pos(nx - 2.0, y + 18.0).w(Px(4.0)).h(Px(110.0)).rounded_px(2.0).bg(vermilion()))
        .child(
            div()
                .absolute()
                .pos(nx - 14.0, y + 10.0)
                .w(Px(28.0))
                .h(Px(126.0))
                .rounded_px(14.0)
                .bg(vermilion().with_alpha(0.55))
                .blur(14.0)
                .blend(BlendMode::Add),
        );
    d
}

/// つまみ: 針と一緒に回る。
fn knob(t: f32) -> Element {
    let r = 70.0;
    let a = (needle(t) - LO) / (HI - LO) * 5.0 - 2.5;
    let (dx, dy) = (a.sin() * r * 0.68, -a.cos() * r * 0.68);
    div()
        .w(Px(r * 2.0))
        .h(Px(r * 2.0))
        .rounded_px(r)
        .bg(panel())
        .border(3.0, dim())
        .child(div().absolute().pos(r + dx - 7.0, r + dy - 7.0).w(Px(14.0)).h(Px(14.0)).rounded_px(7.0).bg(amber()))
}

// ── 波形 ─────────────────────────────────────────────────────────────────

fn wave(t: f32, width: f32, height: f32) -> Element {
    let l = lock(t);
    let n = 240;
    let pts: Vec<(f32, f32)> = (0..n)
        .map(|i| {
            let u = i as f32 / (n - 1) as f32;
            // 声らしい波: 遅いうねりにいくつかの周波数を重ね、端は絞る。
            let env = (u * std::f32::consts::PI).sin();
            let voice = ((u * 9.0 - t * 2.1).sin() * 0.55 + (u * 23.0 + t * 3.7).sin() * 0.3 + (u * 51.0 - t * 6.3).sin() * 0.15)
                * (0.6 + 0.4 * (t * 1.3).sin().abs());
            // 雑音: コマごとに変わるでたらめ。
            let noise = hash(i as u32, (t * FPS as f32) as u32) * 2.0 - 1.0;
            let v = voice * l + noise * (1.0 - l) * 0.9;
            (u * width, height * 0.5 + v * env * height * 0.42)
        })
        .collect();
    div()
        .w(Px(width))
        .h(Px(height))
        .child(polyline().points(pts.clone()).stroke_width(3.0).stroke_color(amber()).absolute().pos(0.0, 0.0).w(Px(width)).h(Px(height)))
        .child(
            div()
                .absolute()
                .pos(0.0, 0.0)
                .w(Px(width))
                .h(Px(height))
                .child(polyline().points(pts).stroke_width(10.0).stroke_color(amber().with_alpha(0.5)).absolute().pos(0.0, 0.0).w(Px(width)).h(Px(height)))
                .blur(10.0)
                .blend(BlendMode::Add),
        )
}

// ── ネオン ───────────────────────────────────────────────────────────────

/// 管が灯る具合: `on` 秒から 0.6 秒、ちらつきながら点く。
fn flicker(t: f32, on: f32, seed: u32) -> f32 {
    let age = t - on;
    if age < 0.0 {
        return 0.0;
    }
    if age > 0.6 {
        // 点いた後も、ごくまれに瞬く。
        let blink = hash(seed, (t * 12.0) as u32) > 0.985;
        return if blink { 0.55 } else { 1.0 };
    }
    let step = (t * 24.0) as u32;
    if hash(seed, step) < age / 0.6 { 1.0 } else { 0.12 }
}

/// ネオンの文字: 字の芯と、ぼかして足す光の 2 枚重ね。字ごとに点き方が違う。
fn neon(s: &str, size: f32, color: Color, t: f32, on: f32, seed: u32) -> Element {
    let motion = |i: usize, _: usize| GlyphMotion {
        opacity: flicker(t, on + hash(seed + i as u32, 7) * 0.5, seed + i as u32),
        ..Default::default()
    };
    let core = text(s.to_string()).font_size(size).font_weight(700).letter_spacing(size * 0.08).color(color.lerp(Color::WHITE, 0.35)).per_glyph(motion);
    let glow = text(s.to_string()).font_size(size).font_weight(700).letter_spacing(size * 0.08).color(color).per_glyph(motion);
    div()
        .child(glow.blur(size * 0.18).blend(BlendMode::Add))
        .child(core.absolute().pos(0.0, 0.0))
}

// ── 番組表 (14〜19 秒) ───────────────────────────────────────────────────

fn schedule(t: f32) -> Element {
    let rows = [
        ("01:00", "Slow Tape", "records nobody asked for"),
        ("02:00", "Letters from Night Owls", "read out, half asleep"),
        ("03:30", "Harbor Weather", "fog, then more fog"),
        ("04:45", "Last Train Home", "quiet songs till dawn"),
    ];
    let mut col = div().flex_col().gap(18.0);
    for (i, (time, title, sub)) in rows.into_iter().enumerate() {
        let s = spring_at(t, 14.2 + i as f32 * 0.35, Spring::gentle());
        col = col.child(
            div()
                .flex_row()
                .items_center()
                .gap(30.0)
                .child(text(time).mono().font_size(40.0).color(vermilion()))
                .child(
                    div()
                        .flex_col()
                        .gap(4.0)
                        .child(text(title).font_size(40.0).font_weight(700).color(amber()))
                        .child(text(sub).font_size(22.0).color(dim().lerp(amber(), 0.3))),
                )
                .tx((1.0 - s) * 120.0)
                .opacity(s.clamp(0.0, 1.0)),
        );
    }
    col
}

fn scene(ctx: FrameCtx) -> Element {
    let t = ctx.t();
    let fade_in = interpolate(t, [0.0, 0.8], [0.0, 1.0]);
    let out = interpolate_eased(t, [20.6, 22.0], [0.0, 1.0], Easing::EaseIn);
    // 番組表の間は看板が左上へ退く。
    let shift = interpolate_eased(t, [13.4, 14.4], [0.0, 1.0], Easing::EaseInOut);

    let mut stage: Vec<Element> = Vec::new();
    // 上: 目盛り。
    stage.push(dial(t).ty(80.0).opacity(1.0 - shift * 0.7));
    // 左下: つまみ。
    stage.push(knob(t).absolute().pos(160.0, 840.0).opacity(1.0 - shift));
    // 下: 波形。
    stage.push(wave(t, 1300.0, 200.0).absolute().pos(310.0, 780.0).opacity(1.0 - shift * 0.4));

    // ON AIR (局に合った所で点く)。
    let on_air = div()
        .px_pad(Px(26.0))
        .py(Px(10.0))
        .rounded_px(12.0)
        .border(4.0, vermilion().with_alpha(flicker(t, 4.5, 3)))
        .child(neon("ON AIR", 46.0, vermilion(), t, 4.5, 3));
    stage.push(on_air.absolute().pos(W - 380.0, 300.0).opacity(1.0 - shift));

    // 看板。
    let sign = div()
        .flex_col()
        .gap(14.0)
        .child(neon("NIGHT LANTERN", 124.0, amber(), t, 6.4, 11))
        .child(
            div()
                .flex_row()
                .items_center()
                .gap(28.0)
                .child(neon("82.4", 92.0, vermilion(), t, 7.6, 31))
                .child(neon("MHz", 46.0, vermilion(), t, 7.9, 41))
                .child(
                    text("radio for people still awake")
                        .font_size(34.0)
                        .color(dim().lerp(amber(), 0.55))
                        .opacity(interpolate(t, [8.8, 9.6], [0.0, 1.0])),
                ),
        );
    stage.push(
        sign.absolute()
            .pos(220.0 - shift * 150.0, 330.0 - shift * 110.0)
            .zoom(1.0 - shift * 0.35),
    );
    if t > 13.8 {
        stage.push(schedule(t).absolute().pos(1120.0, 330.0).opacity(1.0 - out));
    }
    // 終わり: 一言。
    if t > 18.8 {
        stage.push(
            div()
                .absolute()
                .pos(0.0, H - 108.0)
                .w(Px(W))
                .flex_row()
                .justify_center()
                .child(neon("tune in, softly", 54.0, amber(), t, 19.0, 71)),
        );
    }

    let world = div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(W))
        .h(Px(H))
        .children(stage)
        // 終わりにゆっくり引く。
        .zoom(1.0 - out * 0.08);
    div()
        .w(Px(W))
        .h(Px(H))
        .bg(night())
        .child(
            div()
                .absolute()
                .pos(0.0, 0.0)
                .w(Px(W))
                .h(Px(H))
                .radial_gradient_at(hex("#13201a"), night(), 0.45, 0.4)
                .grain(0.05)
                .grain_seed(9),
        )
        .child(world)
        .opacity(fade_in * (1.0 - out))
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
        let dir = args.frames_dir("night_radio");
        let paths = reel.render_pngs(&pick, &dir).expect("render failed");
        for (f, p) in stills.iter().zip(&paths) {
            println!("frame {f} -> {}", p.display());
        }
        return;
    }

    let reel = reel(frames);
    let out = args.mp4("night_radio");
    println!("rendering + encoding {frames} frames ({}x{} @ {FPS}fps) …", reel.width, reel.height);
    match reel.render_mp4(&scene, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
