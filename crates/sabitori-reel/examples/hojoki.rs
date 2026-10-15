//! sabitori-reel — **ink on paper** (22 s): the opening of *Hōjōki* (方丈記,
//! 1212, public domain) written out in vertical columns, one character at a
//! time, each one bleeding into the paper as it lands. A seal is pressed at
//! the end.
//!
//! - each column is one `text()`, animated with `per_glyph` (no element per
//!   character, so 85 characters stay cheap);
//! - the bleed is a blurred copy of the column underneath (`.blur(..)`) whose
//!   glyphs flare when they appear and settle to a faint halo;
//! - the columns are vertical text (`.vertical()`): punctuation sits in the
//!   top-right of its cell and brackets turn, as vertical writing sets them;
//! - the paper is a fibre texture made from noise, the seal is multiplied onto
//!   it (`BlendMode::Multiply`) so the paper shows through the red.
//!
//! The face is Hiragino Mincho (macOS); elsewhere the system serif is used.
//!
//!   cargo run -p sabitori-reel --example hojoki --release
//!
//! Output: `target/reel/hojoki.mp4` (`--out <dir>` to change). Frame numbers
//! write just those frames as PNGs.

#[path = "common/out.rs"]
mod out;

use out::Args;
use sabitori_reel::{
    div, image, interpolate, interpolate_eased, spring_with, text, BlendMode, Color, Easing, Element, FrameCtx,
    GlyphMotion, ImageData, MotionBlur, Px, Reel, Spring,
};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 22.0;

const COLUMNS: [&str; 7] = [
    "ゆく河の流れは絶えずして、",
    "しかももとの水にあらず。",
    "よどみに浮かぶうたかたは、",
    "かつ消えかつ結びて、",
    "久しくとどまりたるためしなし。",
    "世の中にある人とすみかと、",
    "またかくのごとし。",
];
/// 字の大きさと、字の送り (行の高さ)、列の間。
const SIZE: f32 = 52.0;
const PITCH: f32 = 58.0;
const COL_GAP: f32 = 104.0;
/// 1 字を書く間 (秒)。句読点の後は少し息をつく。
const STEP: f32 = 0.11;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343) ^ (y as u32).wrapping_mul(0xd816_3841) ^ seed.wrapping_mul(0xcb1a_b31f);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5bd1_e995);
    h ^= h >> 15;
    (h & 0xff_ffff) as f32 / 0xff_ffff as f32
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (u, v) = (s(fx), s(fy));
    let (a, b) = (hash(xi, yi, seed), hash(xi + 1, yi, seed));
    let (c, d) = (hash(xi, yi + 1, seed), hash(xi + 1, yi + 1, seed));
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

/// 和紙: 生成りの地に、ゆるい雲 (むら) と、短い繊維の筋。
fn paper(w: u32, h: u32) -> ImageData {
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (x as f32, y as f32);
            let cloud = value_noise(u / 260.0, v / 260.0, 3) * 0.6 + value_noise(u / 60.0, v / 60.0, 4) * 0.4;
            // 繊維: 向きの揃わない短い筋 (細かい雑音を一方向に伸ばす)。
            let fiber = value_noise(u / 3.0 + v / 40.0, v / 3.0 - u / 55.0, 9);
            let fiber = ((fiber - 0.82) * 6.0).max(0.0);
            let speck = if hash(x as i32, y as i32, 21) > 0.9993 { 0.25 } else { 0.0 };
            let l = 0.93 + (cloud - 0.5) * 0.05 - fiber * 0.05 - speck;
            let c = [l, l * 0.975, l * 0.92];
            let i = ((y * w + x) * 4) as usize;
            for k in 0..3 {
                px[i + k] = (c[k].clamp(0.0, 1.0) * 255.0) as u8;
            }
            px[i + 3] = 255;
        }
    }
    ImageData::new(px, w, h)
}

/// 句読点は字の枠の右上へ寄せる (縦書きの組み方)。
fn is_punct(c: char) -> bool {
    matches!(c, '、' | '。')
}

/// 字 `i` (全部の列を通した番号) を書き始める秒。
fn starts() -> Vec<Vec<f32>> {
    let mut at = 1.6;
    COLUMNS
        .iter()
        .map(|col| {
            let v: Vec<f32> = col
                .chars()
                .map(|c| {
                    let s = at;
                    at += if is_punct(c) { STEP + 0.35 } else { STEP };
                    s
                })
                .collect();
            at += 0.5;
            v
        })
        .collect()
}

/// 1 列の縦書き (`.vertical()`)。`motion(字の番号, 字)` で 1 字ずつ動かす。字の送りは PITCH。
fn column(s: &str, color: Color, motion: impl Fn(usize, char) -> GlyphMotion) -> Element {
    let chars: Vec<char> = s.chars().collect();
    text(s.to_string())
        .vertical()
        .font_size(SIZE)
        .font_weight(600)
        .letter_spacing(PITCH - SIZE)
        .color(color)
        .per_glyph(move |i, _| motion(i, chars[i]))
}

fn spring_at(t: f32, start: f32, cfg: Spring) -> f32 {
    if t < start {
        0.0
    } else {
        spring_with(t - start, FPS, cfg)
    }
}

fn scene(ctx: FrameCtx, paper: &ImageData, starts: &[Vec<f32>]) -> Element {
    let t = ctx.t();
    let ink = hex("#1b1714");
    let fade_in = interpolate(t, [0.0, 1.2], [0.0, 1.0]);
    // 本文の右端。列は右から左へ。
    let right = W - 330.0;
    let top = 96.0;

    let mut kids: Vec<Element> = vec![image("paper", paper.clone()).absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H))];

    for (ci, col) in COLUMNS.iter().enumerate() {
        let at = &starts[ci];
        let x = right - ci as f32 * COL_GAP - SIZE;
        // にじみ: ぼかした写し。書いた瞬間に濃く広がり、薄い暈 (かさ) になって残る。
        kids.push(
            column(col, ink, |k, _| {
                let age = t - at[k];
                let a = if age < 0.0 { 0.0 } else { 0.16 + 0.5 * (-age * 2.2).exp() };
                GlyphMotion { scale: 1.0 + 0.25 * (-age * 3.0).exp().min(1.0), opacity: a, ..Default::default() }
            })
            .absolute()
            .pos(x, top)
            .blur(7.0),
        );
        // 墨: 置いた瞬間は少し大きく淡く、すぐ締まる。
        kids.push(
            column(col, ink, |k, _| {
                let age = t - at[k];
                if age < 0.0 {
                    return GlyphMotion { opacity: 0.0, ..Default::default() };
                }
                let s = interpolate_eased(age, [0.0, 0.35], [0.0, 1.0], Easing::EaseOutCubic);
                GlyphMotion { scale: 1.08 - 0.08 * s, opacity: 0.35 + 0.65 * s, ..Default::default() }
            })
            .absolute()
            .pos(x, top),
        );
    }

    // 書名と著者 (左)、落款。
    let end = starts.last().and_then(|c| c.last()).copied().unwrap_or(12.0) + 0.8;
    let title_in = interpolate_eased(t, [end, end + 1.2], [0.0, 1.0], Easing::EaseOut);
    let title_x = right - COLUMNS.len() as f32 * COL_GAP - 160.0;
    kids.push(
        column("方丈記", ink, |_, _| GlyphMotion::default())
            .font_size(80.0)
            .line_height(1.2)
            .absolute()
            .pos(title_x, top + 40.0)
            .blur((1.0 - title_in) * 10.0)
            .opacity(title_in),
    );
    kids.push(
        column("鴨長明", hex("#5a524a"), |_, _| GlyphMotion::default())
            .font_size(34.0)
            .line_height(1.24)
            .absolute()
            .pos(title_x + 20.0, top + 360.0)
            .opacity(interpolate(t, [end + 0.6, end + 1.6], [0.0, 1.0])),
    );
    let press = spring_at(t, end + 2.0, Spring::bouncy());
    if t > end + 2.0 {
        let red = hex("#b8322a");
        let seal = div()
            .w(Px(104.0))
            .h(Px(104.0))
            .rounded_px(10.0)
            .bg(red)
            .grain(0.18)
            .grain_seed(7)
            .flex_row()
            .items_center()
            .justify_center()
            .gap(4.0)
            // 右の列から読む: 錆 / 取。
            .child(column("取", Color::WHITE, |_, _| GlyphMotion::default()).font_size(40.0).line_height(1.1))
            .child(column("錆", Color::WHITE, |_, _| GlyphMotion::default()).font_size(40.0).line_height(1.1));
        kids.push(
            seal.absolute()
                .pos(title_x - 8.0, top + 560.0)
                .scaled(1.6 - 0.6 * press)
                .rotate(-0.05)
                .opacity((press * 1.5).clamp(0.0, 0.92))
                .blend(BlendMode::Multiply),
        );
    }

    // ゆっくり寄る (頁が少しずつ近づく)。
    let drift = interpolate_eased(t, [0.0, SECS], [0.0, 1.0], Easing::EaseInOut);
    div()
        .w(Px(W))
        .h(Px(H))
        .bg(hex("#efe9dc"))
        .child(
            div()
                .absolute()
                .pos(0.0, 0.0)
                .w(Px(W))
                .h(Px(H))
                .children(kids)
                // 絵として寄せる (`.scaled` だと字を描き直すので、画素の刻みで揺れる)。
                .zoom(1.0 + drift * 0.04),
        )
        // 四隅を少し落とす。
        .child(
            div()
                .absolute()
                .pos(0.0, 0.0)
                .w(Px(W))
                .h(Px(H))
                .radial_gradient(Color::TRANSPARENT, hex("#3a2e22").with_alpha(0.35)),
        )
        .opacity(fade_in)
}

fn reel(frames: u32) -> Reel {
    Reel {
        motion_blur: Some(MotionBlur::new(4)),
        family: Some("ヒラギノ明朝 ProN".into()),
        crf: 20,
        ..Reel::new(W as u32, H as u32, FPS, frames)
    }
}

fn main() {
    let frames = (SECS * FPS as f32).round() as u32;
    let args = Args::parse();
    let paper = paper(W as u32, H as u32);
    let starts = starts();
    let draw = |ctx: FrameCtx| scene(ctx, &paper, &starts);

    if !args.stills.is_empty() {
        let stills = &args.stills;
        let reel = reel(stills.len() as u32);
        let pick = |ctx: FrameCtx| draw(FrameCtx { frame: stills[ctx.frame as usize], ..ctx });
        let dir = args.frames_dir("hojoki");
        let paths = reel.render_pngs(&pick, &dir).expect("render failed");
        for (f, p) in stills.iter().zip(&paths) {
            println!("frame {f} -> {}", p.display());
        }
        return;
    }

    let reel = reel(frames);
    let out = args.mp4("hojoki");
    println!("rendering + encoding {frames} frames ({}x{} @ {FPS}fps) …", reel.width, reel.height);
    match reel.render_mp4(&draw, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
