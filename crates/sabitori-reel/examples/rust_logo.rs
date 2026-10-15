//! sabitori-reel — **a logo that sheds its rust** (9 s). "sabitori" (錆取り)
//! means *rust remover*, so the wordmark starts out corroded and the rust comes
//! off in patches, flakes falling away, until bare brushed steel is left and a
//! glint runs across it.
//!
//! Everything is drawn from pixels and the element tree — no image assets:
//!
//! - the steel and the rust are textures made once from fractal noise;
//! - the letters are a **mask** (`.mask(text(..))`) over those textures;
//! - the rust patches are a second mask: the same noise field thresholded at a
//!   level that rises over time, rebuilt every frame;
//! - flakes are irregular `path()` polygons that drop and tumble from where the
//!   rust has just let go;
//! - the glint is a soft band laid over the steel with `BlendMode::Screen`.
//!
//!   cargo run -p sabitori-reel --example rust_logo --release
//!
//! Output: `target/reel/rust_logo.mp4` (`--out <dir>` to change). Frame
//! numbers write just those frames as PNGs.

#[path = "common/out.rs"]
mod out;

use std::sync::Arc;

use out::Args;
use sabitori_reel::{
    div, image, interpolate, interpolate_eased, path, text, BlendMode, Color, Easing, Element, FrameCtx, ImageData,
    MotionBlur, Percent, Px, Reel,
};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 9.0;

/// 文字を載せる板 (画面の真ん中)。質感の画像もこの大きさで作る。
const BW: f32 = 1500.0;
const BH: f32 = 420.0;
/// 錆の斑の画像は板の半分の解像度で作り、引き伸ばして縁を柔らかくする。
const MW: u32 = 750;
const MH: u32 = 210;

/// 錆が剥がれ始める秒と、剥がれ終わる秒。
const PEEL: (f32, f32) = (1.0, 5.2);

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}

// ── 雑音 ─────────────────────────────────────────────────────────────────

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xff_ffff) as f32 / 0xff_ffff as f32
}

/// 格子の値を滑らかにつないだ雑音 (0..1)。
fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v) = (s(fx), s(fy));
    let a = hash(xi, yi, seed);
    let b = hash(xi + 1, yi, seed);
    let c = hash(xi, yi + 1, seed);
    let d = hash(xi + 1, yi + 1, seed);
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

/// 周波数を倍々に重ねた雑音 (0..1)。
fn fbm(x: f32, y: f32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 0.5, 1.0, 0.0);
    for o in 0..octaves {
        sum += value_noise(x * freq, y * freq, seed + o * 17) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

fn to_image(w: u32, h: u32, f: impl Fn(u32, u32) -> [f32; 4]) -> ImageData {
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let c = f(x, y);
            let i = ((y * w + x) * 4) as usize;
            for k in 0..4 {
                px[i + k] = (c[k].clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }
    ImageData::new(px, w, h)
}

// ── 質感 ─────────────────────────────────────────────────────────────────

/// ヘアライン仕上げの鋼: 横に流れる細い筋と、上から下へのゆるい明暗。
fn steel() -> ImageData {
    to_image(BW as u32, BH as u32, |x, y| {
        let (u, v) = (x as f32, y as f32);
        let streak = fbm(u * 0.004, v * 0.9, 3, 7) * 0.6 + fbm(u * 0.02, v * 2.5, 2, 9) * 0.4;
        let band = 0.55 + 0.25 * (1.0 - (v / BH - 0.35).abs() * 2.2).max(0.0);
        let l = (band + (streak - 0.5) * 0.22).clamp(0.0, 1.0);
        let c = lerp3([0.30, 0.31, 0.33], [0.78, 0.79, 0.80], l);
        [c[0], c[1], c[2], 1.0]
    })
}

/// 錆: 暗い焦げ茶から橙、ところどころ黄土。穴 (暗い点) も散らす。
fn rust() -> ImageData {
    to_image(BW as u32, BH as u32, |x, y| {
        let (u, v) = (x as f32 / 90.0, y as f32 / 90.0);
        let n = fbm(u, v, 5, 3);
        let fine = fbm(u * 6.0, v * 6.0, 2, 11);
        let mut c = if n < 0.5 {
            lerp3([0.20, 0.08, 0.04], [0.55, 0.20, 0.07], n / 0.5)
        } else {
            lerp3([0.55, 0.20, 0.07], [0.80, 0.47, 0.18], (n - 0.5) / 0.5)
        };
        c = lerp3(c, [0.72, 0.56, 0.30], smoothstep(0.62, 0.8, fbm(u * 2.0, v * 2.0, 3, 21)) * 0.5);
        let pit = smoothstep(0.72, 0.85, fine);
        c = lerp3(c, [0.10, 0.04, 0.02], pit * 0.7);
        let grain = (hash(x as i32, y as i32, 5) - 0.5) * 0.08;
        [c[0] + grain, c[1] + grain * 0.6, c[2] + grain * 0.4, 1.0]
    })
}

/// 錆の斑の場: 値が小さい所から先に剥がれる。
fn rust_field() -> Arc<Vec<f32>> {
    let mut f = Vec::with_capacity((MW * MH) as usize);
    for y in 0..MH {
        for x in 0..MW {
            // 大きな斑 (低い周波数) に、縁を荒らす細かい揺れを少しだけ。
            let (u, v) = (x as f32 / 150.0, y as f32 / 150.0);
            f.push(fbm(u, v, 5, 42) * 0.92 + hash(x as i32, y as i32, 77) * 0.08);
        }
    }
    // 散らばり方を 0..1 にならす (下と上の 2% を端に)。剥がれる速さが時間に比例する。
    let mut sorted = f.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let (lo, hi) = (sorted[sorted.len() / 50], sorted[sorted.len() * 49 / 50]);
    for v in f.iter_mut() {
        *v = ((*v - lo) / (hi - lo)).clamp(0.0, 1.0);
    }
    Arc::new(f)
}

/// 剥がれ具合 `p` (0 = 全部錆, 1 = 全部剥がれた) の、錆が残っている所のマスク。
fn coverage(field: &[f32], p: f32) -> ImageData {
    let th = -0.03 + p * 1.06;
    to_image(MW, MH, |x, y| {
        let a = smoothstep(th - 0.015, th + 0.015, field[(y * MW + x) as usize]);
        [0.0, 0.0, 0.0, a]
    })
}

// ── 部品 ─────────────────────────────────────────────────────────────────

/// 板いっぱいの箱の真ん中に置いた字 (マスクに使う)。
fn word() -> Element {
    div()
        .w(Percent(100.0))
        .h(Percent(100.0))
        .items_center()
        .justify_center()
        .child(text("sabitori").font_size(300.0).font_weight(900).letter_spacing(-6.0).color(Color::BLACK))
}

fn layer(img: Element) -> Element {
    img.absolute().pos(0.0, 0.0).w(Px(BW)).h(Px(BH))
}

/// 剥がれ具合 (時刻から)。
fn peel(t: f32) -> f32 {
    interpolate_eased(t, [PEEL.0, PEEL.1], [0.0, 1.0], Easing::EaseInOut)
}

/// 錆の欠片: 剥がれた瞬間の場所から落ちる。場の値が閾値を超えた時刻に生まれる。
fn flakes(t: f32, field: &[f32]) -> Vec<Element> {
    let mut out = Vec::new();
    let (ox, oy) = ((W - BW) * 0.5, (H - BH) * 0.5);
    for i in 0..36u32 {
        let (hx, hy) = (hash(i as i32, 0, 901), hash(i as i32, 1, 901));
        // 字の帯 (板の中ほど) から拾う。
        let (mx, my) = ((0.08 + hx * 0.84) * MW as f32, (0.3 + hy * 0.45) * MH as f32);
        let v = field[(my as u32 * MW + mx as u32) as usize];
        // この値が閾値を下回る時刻 = 剥がれる時刻。peel を逆にたどる。
        let p_at = ((v + 0.03) / 1.06).clamp(0.0, 1.0);
        let born = (0..=200)
            .map(|k| PEEL.0 + (PEEL.1 - PEEL.0) * k as f32 / 200.0)
            .find(|&s| peel(s) >= p_at)
            .unwrap_or(PEEL.1);
        let age = t - born;
        if !(0.0..2.2).contains(&age) {
            continue;
        }
        let size = 5.0 + hash(i as i32, 2, 901) * 11.0;
        let drift = (hash(i as i32, 3, 901) - 0.5) * 90.0;
        let x = ox + mx / MW as f32 * BW + drift * age;
        let y = oy + my / MH as f32 * BH + 260.0 * age * age + 30.0 * age;
        let spin = (hash(i as i32, 4, 901) - 0.5) * 9.0 * age;
        // 不規則な 5 角形。
        let mut p = path();
        for k in 0..5 {
            let a = k as f32 / 5.0 * std::f32::consts::TAU + hash(i as i32, 10 + k, 901) * 0.9;
            let r = size * (0.55 + hash(i as i32, 20 + k, 901) * 0.45);
            let (px, py) = (size + a.cos() * r, size + a.sin() * r);
            p = if k == 0 { p.move_to(px, py) } else { p.line_to(px, py) };
        }
        let shade = hash(i as i32, 5, 901);
        let col = hex("#3e180a").lerp(hex("#8a3f17"), shade);
        out.push(
            p.close_path()
                .fill(col)
                .absolute()
                .pos(x - size, y - size)
                .w(Px(size * 2.0))
                .h(Px(size * 2.0))
                .rotate(spin)
                .opacity(interpolate(age, [1.4, 2.2], [1.0, 0.0])),
        );
    }
    out
}

fn scene(ctx: FrameCtx, steel: &ImageData, rust: &ImageData, field: &[f32]) -> Element {
    let t = ctx.t();
    let p = peel(t);
    let enter = interpolate_eased(t, [0.0, 0.9], [0.0, 1.0], Easing::EaseOutCubic);
    let glint = interpolate_eased(t, [5.3, 7.4], [-0.35, 1.25], Easing::EaseInOut);
    let tag = interpolate_eased(t, [6.2, 7.0], [0.0, 1.0], Easing::EaseOutCubic);
    let out = interpolate(t, [8.4, 9.0], [0.0, 1.0]);

    // 剥がれる前の錆 (字の形) を、残っている所だけ見せる。マスクの入れ子:
    // 内側で字の形に、外側で斑に切る。
    let rust_layer = div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(BW))
        .h(Px(BH))
        .child(layer(image("rust", rust.clone())).mask(word()))
        .mask(layer(image(format!("cover-{:.4}", p), coverage(field, p))));

    // 光の筋: 斜めの柔らかい帯を、鋼の字の上にスクリーンで重ねる。
    let band = div()
        .absolute()
        .pos(BW * glint - 280.0, -100.0)
        .w(Px(300.0))
        .h(Px(BH + 200.0))
        .gradient(Color::TRANSPARENT, Color::WHITE.with_alpha(0.85), 0.0)
        .rotate(0.35);
    let band = div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(BW))
        .h(Px(BH))
        .child(band)
        .child(
            div()
                .absolute()
                .pos(BW * glint + 20.0, -100.0)
                .w(Px(300.0))
                .h(Px(BH + 200.0))
                .gradient(Color::WHITE.with_alpha(0.85), Color::TRANSPARENT, 0.0)
                .rotate(0.35),
        )
        .mask(word())
        .blend(BlendMode::Screen);

    let plate = div()
        .absolute()
        .pos((W - BW) * 0.5, (H - BH) * 0.5 - 40.0)
        .w(Px(BW))
        .h(Px(BH))
        // 字の落とす影。
        .child(
            div()
                .absolute()
                .pos(0.0, 14.0)
                .w(Px(BW))
                .h(Px(BH))
                .bg(Color::BLACK.with_alpha(0.55))
                .mask(word())
                .blur(18.0),
        )
        .child(layer(image("steel", steel.clone())).mask(word()))
        .child(rust_layer)
        .child(band)
        .scaled(0.94 + 0.06 * enter)
        .blur((1.0 - enter) * 16.0)
        .opacity(enter);

    let mut kids = vec![
        div()
            .absolute()
            .pos(0.0, 0.0)
            .w(Px(W))
            .h(Px(H))
            .radial_gradient_at(hex("#2a221d"), hex("#0d0b0a"), 0.5, 0.42)
            .grain(0.05)
            .grain_seed(ctx.frame),
        plate,
    ];
    kids.extend(flakes(t, field));
    kids.push(
        div()
            .absolute()
            .pos(0.0, H * 0.5 + BH * 0.5 + 10.0)
            .w(Px(W))
            .items_center()
            .child(
                text("a GPU UI framework for Rust")
                    .font_size(46.0)
                    .letter_spacing(8.0)
                    .color(hex("#c9bfb2"))
                    .mask(div().w(Percent(tag * 100.0)).h(Percent(100.0)).bg(Color::BLACK)),
            ),
    );
    div().w(Px(W)).h(Px(H)).bg(hex("#0d0b0a")).children(kids).opacity(1.0 - out)
}

fn reel(frames: u32) -> Reel {
    Reel { motion_blur: Some(MotionBlur::new(6)), crf: 24, ..Reel::new(W as u32, H as u32, FPS, frames) }
}

fn main() {
    let frames = (SECS * FPS as f32).round() as u32;
    let args = Args::parse();
    let (steel, rust, field) = (steel(), rust(), rust_field());
    let draw = |ctx: FrameCtx| scene(ctx, &steel, &rust, &field);

    if !args.stills.is_empty() {
        let stills = &args.stills;
        let reel = reel(stills.len() as u32);
        let pick = |ctx: FrameCtx| draw(FrameCtx { frame: stills[ctx.frame as usize], ..ctx });
        let dir = args.frames_dir("rust_logo");
        let paths = reel.render_pngs(&pick, &dir).expect("render failed");
        for (f, p) in stills.iter().zip(&paths) {
            println!("frame {f} -> {}", p.display());
        }
        return;
    }

    let reel = reel(frames);
    let out = args.mp4("rust_logo");
    println!("rendering + encoding {frames} frames ({}x{} @ {FPS}fps) …", reel.width, reel.height);
    match reel.render_mp4(&draw, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
