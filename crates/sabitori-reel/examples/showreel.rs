//! sabitori-reel — a **10-second showreel** that uses every motion-graphics
//! tool the reel has, in one piece:
//!
//! - a title that flies together one glyph at a time (`per_glyph`) and pulls
//!   into focus (`blur`), over a radial glow with moving film grain
//!   (`radial_gradient_at`, `grain`) and light orbs added on top (`blend`);
//! - a check mark and a ring drawn on stroke by stroke (`path`, `trim`,
//!   `dash`, an SVG `d`), then filled;
//! - a video card (`Video`) that tilts in (`rotate`) and is revealed by a
//!   soft wipe (`mask`);
//! - an outro that drops out of focus;
//! - all of it with motion blur (`Reel::motion_blur`).
//!
//!   cargo run -p sabitori-reel --example showreel --release [-- <video>]
//!
//! Pass your own clip (a screen recording, say) as the argument; otherwise a
//! test pattern is made with `ffmpeg` (`testsrc2`). Output:
//! `target/reel/showreel.mp4` (`--out <dir>` to change). Frame numbers (as the
//! other examples take them) write just those frames as PNGs.

#[path = "common/out.rs"]
mod out;

use std::f32::consts::TAU;
use std::path::PathBuf;

use out::Args;
use sabitori_reel::{
    div, interpolate, interpolate_eased, path, spring_with, text, BlendMode, Color, Easing, Element, FrameCtx,
    GlyphMotion, MotionBlur, Percent, Px, Reel, Spring, Video,
};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 10.0;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}

/// `start` 秒から鳴り始めるばね (それより前は 0)。
fn spring_at(t: f32, start: f32, cfg: Spring) -> f32 {
    if t < start {
        0.0
    } else {
        spring_with(t - start, FPS, cfg)
    }
}

/// 字ごとに決まった、でたらめに見える数 (-1..1)。
fn jitter(i: usize, k: u32) -> f32 {
    let x = ((i as u32).wrapping_mul(2654435761) ^ k.wrapping_mul(40503)).wrapping_mul(2246822519);
    (x >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
}

/// 背景: 右上の光だまり、動く粒子、加算で重ねたぼけた光の玉。
fn backdrop(t: f32, frame: u32) -> Vec<Element> {
    let mut kids = vec![div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(W))
        .h(Px(H))
        .radial_gradient_at(hex("#1b1446"), hex("#05040d"), 0.72, 0.2)
        .grain(0.04)
        .grain_seed(frame)];
    let orbs = [("#7c3aed", 0.2, 0.7, 260.0), ("#06b6d4", 0.78, 0.3, 320.0), ("#f472b6", 0.55, 0.85, 220.0)];
    for (i, (c, x, y, r)) in orbs.into_iter().enumerate() {
        let a = t * 0.35 + i as f32 * 2.1;
        let (cx, cy) = (x * W + a.cos() * 60.0, y * H + a.sin() * 40.0);
        kids.push(
            div()
                .absolute()
                .pos(cx - r, cy - r)
                .w(Px(r * 2.0))
                .h(Px(r * 2.0))
                .rounded_px(r)
                .bg(hex(c).with_alpha(0.32))
                .blur(70.0)
                .blend(BlendMode::Add),
        );
    }
    kids
}

/// 1 場面目: 題字が 1 字ずつ集まり、ピントが合う。
fn title(t: f32) -> Element {
    let focus = interpolate_eased(t, [0.0, 1.4], [18.0, 0.0], Easing::EaseOutCubic);
    let leave = interpolate_eased(t, [2.3, 2.9], [0.0, 1.0], Easing::EaseInOut);
    div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(W))
        .h(Px(H))
        .flex_col()
        .items_center()
        .justify_center()
        .gap(18.0)
        .child(
            text("sabitori-reel")
                .font_size(150.0)
                .font_weight(800)
                .color(hex("#f5f3ff"))
                .per_glyph(|i, _| {
                    let s = spring_at(t, 0.1 + i as f32 * 0.05, Spring::gentle());
                    let k = 1.0 - s;
                    GlyphMotion {
                        dx: k * jitter(i, 1) * 420.0,
                        dy: k * jitter(i, 2) * 300.0,
                        rotation: k * jitter(i, 3) * 2.5,
                        scale: 1.0 + k * 1.5,
                        opacity: s.clamp(0.0, 1.0),
                    }
                }),
        )
        .child(
            text("motion graphics from an Element tree")
                .font_size(38.0)
                .letter_spacing(2.0)
                .color(hex("#c4b5fd"))
                .opacity(interpolate(t, [1.0, 1.6], [0.0, 1.0])),
        )
        .blur(focus + leave * 30.0)
        .opacity(1.0 - leave)
}

/// 2 場面目: チェックと輪が線から描かれ、塗られる。
fn drawing(t: f32) -> Element {
    let local = t - 2.6;
    let ring = interpolate_eased(local, [0.0, 1.2], [0.0, 1.0], Easing::EaseInOut);
    let check = interpolate_eased(local, [0.8, 1.6], [0.0, 1.0], Easing::EaseOutCubic);
    let fill = interpolate(local, [1.6, 2.1], [0.0, 1.0]);
    let fade = interpolate(local, [2.6, 3.0], [1.0, 0.0]);
    let size = 420.0;
    let disc = || {
        path()
            .svg_path("M50 4 A46 46 0 1 1 49.99 4 Z")
            .view_box(0.0, 0.0, 100.0, 100.0)
            .w(Px(size))
            .h(Px(size))
            .absolute()
            .pos(0.0, 0.0)
    };
    div()
        .absolute()
        .pos((W - size) * 0.5, (H - size) * 0.5 - 40.0)
        .w(Px(size))
        .h(Px(size))
        .opacity(fade)
        // 塗り (緑の円) が後から満ちる。
        .child(disc().fill(hex("#22c55e").with_alpha(fill)))
        // 外の輪: 太い線が一周し、その外を点線が回る。
        .child(disc().stroke(hex("#86efac"), 4.0).trim(0.0, ring))
        .child(
            path()
                .svg_path("M50 -4 A54 54 0 1 1 49.99 -4 Z")
                .view_box(0.0, 0.0, 100.0, 100.0)
                .stroke(hex("#86efac").with_alpha(0.6), 1.5)
                .dash(3.0, 4.0)
                .w(Px(size))
                .h(Px(size))
                .absolute()
                .pos(0.0, 0.0)
                .rotate(t * 0.6)
                .opacity(ring),
        )
        // チェック: 線で描かれていく。
        .child(
            path()
                .svg_path("M28 52 L44 67 L73 35")
                .view_box(0.0, 0.0, 100.0, 100.0)
                .stroke(Color::WHITE, 9.0)
                .trim(0.0, check)
                .w(Px(size))
                .h(Px(size))
                .absolute()
                .pos(0.0, 0.0),
        )
}

/// 3 場面目: 動画のカードが傾いて入り、ワイプで現れる。
fn video_card(t: f32, clip: Option<&Video>) -> Element {
    let local = t - 5.4;
    let s = spring_at(t, 5.4, Spring::gentle());
    let reveal = interpolate_eased(local, [0.1, 1.1], [0.0, 1.0], Easing::EaseInOut);
    let leave = interpolate_eased(local, [2.7, 3.2], [0.0, 1.0], Easing::EaseIn);
    let (cw, ch) = (1120.0, 630.0);
    let media = match clip {
        Some(v) => v.element(local.max(0.0)).w(Px(cw)).h(Px(ch)),
        None => div().w(Px(cw)).h(Px(ch)).gradient(hex("#0ea5e9"), hex("#6366f1"), 0.4),
    };
    // 左から拭き取るマスク。右端 220px は柔らかく。
    let soft = 220.0;
    let wipe = div()
        .w(Percent(100.0))
        .h(Percent(100.0))
        .flex_row()
        .child(div().w(Percent(reveal * 100.0)).h(Percent(100.0)).bg(Color::BLACK))
        .child(div().w(Px(soft)).h(Percent(100.0)).gradient(Color::BLACK, Color::TRANSPARENT, 0.0))
        .tx(-soft * (1.0 - reveal));
    // 中身 (動画と字幕) をワイプで現し、外側を角丸の箱でマスクして角を落とす
    // (`overflow_hidden` は四角にしか切らない)。縁取りは一番上に重ねる。
    let content = div()
        .w(Px(cw))
        .h(Px(ch))
        .child(media)
        .child(
            text("your screen recording goes here")
                .font_size(30.0)
                .color(Color::WHITE)
                .absolute()
                .pos(40.0, ch - 70.0),
        )
        .mask(wipe);
    let rounded = || div().w(Percent(100.0)).h(Percent(100.0)).rounded_px(28.0);
    div()
        .absolute()
        .pos((W - cw) * 0.5, (H - ch) * 0.5)
        .w(Px(cw))
        .h(Px(ch))
        .child(content)
        .child(rounded().border(2.0, Color::WHITE.with_alpha(0.3 * reveal)).absolute().pos(0.0, 0.0))
        .mask(rounded().bg(Color::BLACK))
        .rotate((1.0 - s) * -0.35 + leave * 0.2)
        .ty((1.0 - s) * 120.0 + leave * -60.0)
        .blur(leave * 40.0)
        .opacity(1.0 - leave)
}

/// 4 場面目: 締めの一言が 1 字ずつ立ち上がり、最後はぼけて消える。
fn outro(t: f32) -> Element {
    let out = interpolate_eased(t, [9.3, 10.0], [0.0, 1.0], Easing::EaseIn);
    div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(W))
        .h(Px(H))
        .items_center()
        .justify_center()
        .child(
            text("Motion, in code.")
                .font_size(120.0)
                .font_weight(800)
                .color(hex("#f5f3ff"))
                .per_glyph(|i, _| {
                    let s = spring_at(t, 8.6 + i as f32 * 0.03, Spring::bouncy());
                    GlyphMotion {
                        dy: (1.0 - s) * 80.0,
                        rotation: (1.0 - s) * 0.6,
                        opacity: s.clamp(0.0, 1.0),
                        ..Default::default()
                    }
                }),
        )
        .blur(out * 36.0)
        .opacity(1.0 - out)
}

fn scene(ctx: FrameCtx, clip: Option<&Video>) -> Element {
    let t = ctx.t();
    let mut kids = backdrop(t, ctx.frame);
    if t < 3.0 {
        kids.push(title(t));
    }
    if (2.6..5.7).contains(&t) {
        kids.push(drawing(t));
    }
    if (5.4..8.7).contains(&t) {
        kids.push(video_card(t, clip));
    }
    if t >= 8.6 {
        kids.push(outro(t));
    }
    // 回る細い輪 (どの場面にも薄く)。
    kids.push(
        path()
            .svg_path("M50 2 A48 48 0 1 1 49.99 2 Z")
            .view_box(0.0, 0.0, 100.0, 100.0)
            .stroke(Color::WHITE.with_alpha(0.06), 0.3)
            .dash(1.0, 3.0)
            .w(Px(H * 1.4))
            .h(Px(H * 1.4))
            .absolute()
            .pos((W - H * 1.4) * 0.5, -H * 0.2)
            .rotate(t * TAU / 40.0),
    );
    div().w(Px(W)).h(Px(H)).bg(hex("#05040d")).children(kids)
}

/// 貼る動画: 引数で渡された物か、無ければ ffmpeg の試験パターンを作る。
fn clip_path(out_dir: &std::path::Path) -> Option<PathBuf> {
    if let Some(p) = std::env::args().skip(1).find(|a| a.ends_with(".mp4") || a.ends_with(".mov")) {
        return Some(PathBuf::from(p));
    }
    let p = out_dir.join("showreel-source.mp4");
    if p.exists() {
        return Some(p);
    }
    std::fs::create_dir_all(out_dir).ok()?;
    let ok = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=s=1120x630:r=30:d=4", "-pix_fmt", "yuv420p"])
        .arg(&p)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    ok.then_some(p)
}

fn reel(frames: u32) -> Reel {
    Reel { motion_blur: Some(MotionBlur::new(8)), ..Reel::new(W as u32, H as u32, FPS, frames) }
}

fn main() {
    let frames = (SECS * FPS as f32).round() as u32;
    let args = Args::parse_with(|a| a.ends_with(".mp4") || a.ends_with(".mov"));
    let clip = clip_path(&args.out).and_then(|p| Video::open(p, 1120, 630, FPS).ok());
    if clip.is_none() {
        eprintln!("no video (ffmpeg missing?) — the card shows a gradient instead");
    }
    let clip = clip.as_ref();

    if !args.stills.is_empty() {
        let stills = &args.stills;
        let reel = reel(stills.len() as u32);
        let pick = |ctx: FrameCtx| scene(FrameCtx { frame: stills[ctx.frame as usize], ..ctx }, clip);
        let dir = args.frames_dir("showreel");
        let paths = reel.render_pngs(&pick, &dir).expect("render failed");
        for (f, p) in stills.iter().zip(&paths) {
            println!("frame {f} -> {}", p.display());
        }
        return;
    }

    let reel = reel(frames);
    let out = args.mp4("showreel");
    println!("rendering + encoding {frames} frames ({}x{} @ {FPS}fps, motion blur ×8) …", reel.width, reel.height);
    match reel.render_mp4(&|ctx: FrameCtx| scene(ctx, clip), &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
