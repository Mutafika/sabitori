//! sabitori-reel — **a made-up indie game studio** (22 s): "BITMOSS", and
//! their made-up game "HOP GARDEN", in the four greens of an old handheld.
//!
//! - the studio logo scrolls down in 8-pixel steps, like a console booting;
//! - a squishy blob (`goo`: the body and a trailing foot fused) hops across a
//!   tiled level, stretching on the way up and squashing on landing, and
//!   collects coins — each one pops and the score counts up;
//! - a hi-score counts up to a big number while pixel sparkles blink;
//! - the title drops in letter by letter (`per_glyph`), snapped to whole
//!   "pixels" so it moves like a sprite, not like a vector;
//! - "PRESS START" blinks, and the screen fades through the palette.
//!
//! Sprites are strings of characters turned into runs of squares. Monospace
//! text uses Menlo (`Reel::mono_family`).
//!
//!   cargo run -p sabitori-reel --example game_studio --release
//!
//! Output: `target/reel/game_studio.mp4` (`--out <dir>` to change). Frame
//! numbers write just those frames as PNGs.

#[path = "common/out.rs"]
mod out;

use out::Args;
use sabitori_core::{goo, Rect};
use sabitori_reel::{
    div, interpolate, interpolate_eased, spring_with, text, Color, Easing, Element, FrameCtx, GlyphMotion, MotionBlur,
    Px, Reel, Spring,
};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 22.0;

/// ドット 1 つの大きさ (画面の px)。動きもこの刻みに揃える。
const DOT: f32 = 8.0;
/// 床の上端。
const GROUND: f32 = 800.0;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}
// 昔の携帯ゲーム機の 4 階調の緑。濃い順。
fn c0() -> Color {
    hex("#0f380f")
}
fn c1() -> Color {
    hex("#306230")
}
fn c2() -> Color {
    hex("#8bac0f")
}
fn c3() -> Color {
    hex("#9bbc0f")
}

fn spring_at(t: f32, start: f32, cfg: Spring) -> f32 {
    if t < start {
        0.0
    } else {
        spring_with(t - start, FPS, cfg)
    }
}

/// ドットの刻みに丸める (ベクタでなくスプライトのように動かす)。
fn snap(v: f32) -> f32 {
    (v / DOT).round() * DOT
}

fn hash(i: u32, k: u32) -> f32 {
    let mut h = i.wrapping_mul(0x9e37_79b1) ^ k.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 13;
    (h & 0xffff) as f32 / 65535.0
}

/// 文字の並びをドット絵にする。同じ色が横に続く所は 1 つの四角にまとめる。
/// `'0'`〜`'3'` が 4 階調 (濃い順)、それ以外は透明。
fn sprite(rows: &[&str], px: f32) -> Element {
    let w = rows.iter().map(|r| r.len()).max().unwrap_or(0) as f32 * px;
    let mut d = div().w(Px(w)).h(Px(rows.len() as f32 * px));
    for (y, row) in rows.iter().enumerate() {
        let b = row.as_bytes();
        let mut x = 0;
        while x < b.len() {
            let c = b[x];
            let mut end = x + 1;
            while end < b.len() && b[end] == c {
                end += 1;
            }
            let color = match c {
                b'0' => Some(c0()),
                b'1' => Some(c1()),
                b'2' => Some(c2()),
                b'3' => Some(c3()),
                _ => None,
            };
            if let Some(col) = color {
                d = d.child(
                    div()
                        .absolute()
                        .pos(x as f32 * px, y as f32 * px)
                        .w(Px((end - x) as f32 * px))
                        .h(Px(px))
                        .bg(col),
                );
            }
            x = end;
        }
    }
    d
}

const COIN: [&str; 8] = [
    "..0000..", //
    ".022220.", //
    "02233220", //
    "02230220", //
    "02230220", //
    "02233220", //
    ".022220.", //
    "..0000..", //
];

const SPARKLE: [&str; 5] = [
    "..0..", //
    "..0..", //
    "00300", //
    "..0..", //
    "..0..", //
];

const BUSH: [&str; 6] = [
    "...0000...", //
    "..011110..", //
    ".01122110.", //
    "0111111110", //
    "0112112110", //
    "0111111110", //
];

// ── 1. 起動 (0〜3.2 秒) ───────────────────────────────────────────────────

fn boot(t: f32) -> Vec<Element> {
    // ロゴは 8px 刻みで上から降りてくる。
    let y = snap(interpolate(t, [0.2, 2.0], [-260.0, H * 0.5 - 90.0]));
    // 消えるときは階調を 1 段ずつ明るくする (液晶の消え方)。
    let fade = ((t - 2.7) / 0.12).floor().clamp(0.0, 4.0) as usize;
    let color = [c0(), c1(), c2(), c3(), c3()][fade];
    let mut out = vec![div()
        .absolute()
        .pos(0.0, y)
        .w(Px(W))
        .flex_row()
        .justify_center()
        .child(text("BITMOSS").mono().bold().font_size(150.0).letter_spacing(12.0).color(color))];
    // 降り切った所で、ロゴの右上がきらっと光る。
    if (2.05..2.6).contains(&t) {
        let blink = ((t - 2.05) * 10.0) as i32 % 2 == 0;
        if blink {
            out.push(sprite(&SPARKLE, DOT).absolute().pos(W * 0.5 + 400.0, H * 0.5 - 120.0));
        }
    }
    out
}

// ── 2. 遊ぶ (3.2〜9.4 秒) ─────────────────────────────────────────────────

const PLAY: (f32, f32) = (3.2, 9.4);
/// 1 回跳ぶのにかかる秒。
const HOP: f32 = 0.78;

/// キャラの位置 (左右の真ん中, 足の y) と、伸び (+ で縦に伸び、- で潰れ)。
fn hopper(t: f32) -> (f32, f32, f32) {
    let local = (t - PLAY.0).max(0.0);
    let x = interpolate(local, [0.0, PLAY.1 - PLAY.0], [180.0, W - 260.0]);
    let ph = (local / HOP).fract();
    let lift = 4.0 * ph * (1.0 - ph);
    let y = GROUND - lift * 230.0;
    // 上りは伸び、下りの終わりと着地で潰れる。
    let stretch = if ph < 0.08 || ph > 0.94 { -0.3 } else { (1.0 - 2.0 * ph) * 0.25 };
    (snap(x), snap(y), stretch)
}

fn blob(x: f32, foot: f32, stretch: f32) -> Element {
    let (w, h) = (120.0 * (1.0 - stretch * 0.6), 104.0 * (1.0 + stretch));
    let body = Rect::new(x - w * 0.5, foot - h, w, h * 0.8);
    // 足は体の下で、伸びるほど遅れてついてくる (首でつながる)。
    let foot_h = 30.0;
    let lag = stretch.max(0.0) * 60.0;
    let feet = Rect::new(x - w * 0.35, foot - foot_h + lag * 0.3, w * 0.7, foot_h);
    let eye = |dx: f32| {
        div()
            .absolute()
            .pos(snap(x + dx), snap(foot - h + h * 0.22))
            .w(Px(DOT * 2.0))
            .h(Px(DOT * 3.0))
            .bg(c3())
    };
    div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(W))
        .h(Px(H))
        .child(goo(body, feet).goo_radii(w.min(h * 0.8) * 0.45, foot_h * 0.5).goo_smooth(40.0).goo_color(c0()).absolute().pos(0.0, 0.0))
        .child(eye(-30.0))
        .child(eye(16.0))
}

/// 床: 64px の土の升目を並べ、上の縁だけ濃くする。
fn ground() -> Element {
    let mut d = div().absolute().pos(0.0, GROUND).w(Px(W)).h(Px(H - GROUND));
    let tile = 64.0;
    for i in 0..(W / tile) as usize + 1 {
        let x = i as f32 * tile;
        let shade = if i % 2 == 0 { c1() } else { c2() };
        d = d
            .child(div().absolute().pos(x, 0.0).w(Px(tile)).h(Px(H - GROUND)).bg(shade))
            .child(div().absolute().pos(x, 0.0).w(Px(tile)).h(Px(DOT * 2.0)).bg(c0()))
            .child(div().absolute().pos(x + 16.0, 40.0).w(Px(DOT)).h(Px(DOT)).bg(c0()));
    }
    d
}

/// 硬貨の x 位置。キャラが通ると拾われる。
fn coin_xs() -> Vec<f32> {
    (0..6).map(|i| 420.0 + i as f32 * 240.0).collect()
}

/// キャラが `x` を通る秒。
fn passes_at(x: f32) -> f32 {
    PLAY.0 + (x - 180.0) / (W - 260.0 - 180.0) * (PLAY.1 - PLAY.0)
}

fn score_at(t: f32) -> u32 {
    coin_xs().iter().filter(|&&x| t >= passes_at(x)).count() as u32 * 100
}

fn play(t: f32) -> Vec<Element> {
    let mut out = vec![ground()];
    for (i, bx) in [260.0, 900.0, 1500.0].into_iter().enumerate() {
        out.push(sprite(&BUSH, DOT).absolute().pos(bx + i as f32 * 20.0, GROUND - BUSH.len() as f32 * DOT));
    }
    for (i, x) in coin_xs().into_iter().enumerate() {
        let got = passes_at(x);
        // 浮いて上下に揺れ、拾われたら跳ね上がって点滅しながら消える。
        let bob = snap((t * 3.0 + i as f32).sin() * 10.0);
        let y = GROUND - 240.0 + bob;
        if t < got {
            out.push(sprite(&COIN, DOT).absolute().pos(x - 32.0, y));
        } else if t < got + 0.45 {
            let age = t - got;
            if ((age * 16.0) as i32) % 2 == 0 {
                out.push(sprite(&COIN, DOT).absolute().pos(x - 32.0, snap(y - age * 260.0)));
            }
            out.push(
                text("+100")
                    .mono()
                    .bold()
                    .font_size(34.0)
                    .color(c0())
                    .absolute()
                    .pos(x - 40.0, snap(y - 60.0 - age * 120.0)),
            );
        }
    }
    let (x, foot, stretch) = hopper(t);
    out.push(blob(x, foot, stretch));
    out.push(
        text(format!("SCORE {:06}", score_at(t)))
            .mono()
            .bold()
            .font_size(44.0)
            .color(c0())
            .absolute()
            .pos(80.0, 70.0),
    );
    out.push(text("WORLD 1-1").mono().bold().font_size(44.0).color(c0()).absolute().pos(W - 400.0, 70.0));
    out
}

// ── 3. 最高点 (9.4〜14 秒) ───────────────────────────────────────────────

fn hiscore(t: f32) -> Vec<Element> {
    let n = interpolate_eased(t, [9.8, 12.2], [0.0, 98_765.0], Easing::EaseOut) as u32;
    let pop = spring_at(t, 9.5, Spring::bouncy());
    let mut out = vec![div()
        .absolute()
        .pos(0.0, H * 0.5 - 170.0)
        .w(Px(W))
        .flex_col()
        .items_center()
        .gap(24.0)
        .child(text("HI-SCORE").mono().bold().font_size(64.0).letter_spacing(8.0).color(c1()))
        .child(text(format!("{n:06}")).mono().bold().font_size(200.0).color(c0()))
        .ty(snap((1.0 - pop) * 120.0))];
    // きらきら: 決まった場所で、それぞれの間合いで点滅する。
    for i in 0..14u32 {
        let phase = (t * 3.0 + hash(i, 1) * 4.0) as i32;
        if t > 10.0 && phase % 3 == 0 {
            let (x, y) = (snap(160.0 + hash(i, 2) * (W - 320.0)), snap(120.0 + hash(i, 3) * (H - 240.0)));
            out.push(sprite(&SPARKLE, DOT).absolute().pos(x, y));
        }
    }
    out
}

// ── 4. 題名 (14〜19 秒) ──────────────────────────────────────────────────

fn title(t: f32) -> Vec<Element> {
    let drop = |shadow: bool| {
        text("HOP GARDEN")
            .mono()
            .bold()
            .font_size(170.0)
            .letter_spacing(6.0)
            .color(if shadow { c1().with_alpha(0.45) } else { c0() })
            .per_glyph(move |i, _| {
                let s = spring_at(t, 14.2 + i as f32 * 0.08, Spring::bouncy());
                GlyphMotion { dy: snap((1.0 - s) * -700.0), ..Default::default() }
            })
    };
    let (x, foot, stretch) = {
        // 題名の右下で、その場で跳ねる。
        let ph = ((t - 14.0) / HOP).fract();
        let lift = 4.0 * ph * (1.0 - ph);
        let stretch = if ph < 0.08 || ph > 0.94 { -0.3 } else { (1.0 - 2.0 * ph) * 0.25 };
        (W * 0.5 + 730.0, snap(H * 0.5 + 230.0 - lift * 140.0), stretch)
    };
    let by = interpolate(t, [15.6, 16.3], [0.0, 1.0]);
    vec![
        // 影 (1 ドットずらした明るい色) と本体。
        div().absolute().pos(DOT * 2.0, H * 0.5 - 150.0 + DOT * 2.0).w(Px(W)).flex_row().justify_center().child(drop(true)),
        div().absolute().pos(0.0, H * 0.5 - 150.0).w(Px(W)).flex_row().justify_center().child(drop(false)),
        div()
            .absolute()
            .pos(0.0, H * 0.5 + 70.0)
            .w(Px(W))
            .flex_row()
            .justify_center()
            .child(text("a game by BITMOSS").mono().font_size(40.0).color(c1()))
            .opacity(if by > 0.5 { 1.0 } else { 0.0 }),
        div().absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H)).child(blob(x, foot, stretch)),
    ]
}

// ── 5. PRESS START (19〜22 秒) ───────────────────────────────────────────

fn press_start(t: f32) -> Element {
    let on = ((t - 19.0) * 2.0) as i32 % 2 == 0;
    div()
        .absolute()
        .pos(0.0, H - 250.0)
        .w(Px(W))
        .flex_row()
        .justify_center()
        .child(text("PRESS START").mono().bold().font_size(64.0).letter_spacing(10.0).color(c0()))
        .opacity(if on { 1.0 } else { 0.0 })
}

fn scene(ctx: FrameCtx) -> Element {
    let t = ctx.t();
    let mut kids: Vec<Element> = Vec::new();
    if t < 3.2 {
        kids.extend(boot(t));
    } else if t < PLAY.1 {
        kids.extend(play(t));
    } else if t < 14.0 {
        kids.extend(hiscore(t));
    } else {
        kids.extend(title(t));
        if t >= 19.0 {
            kids.push(press_start(t));
        }
    }
    // 終わりは階調を 1 段ずつ濃くして暗転 (液晶が落ちる感じ)。
    let end = ((t - 21.2) / 0.15).floor().clamp(-1.0, 3.0);
    let screen = if end >= 0.0 {
        let c = [c2(), c1(), c0(), c0()][end as usize];
        Some(div().absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H)).bg(c))
    } else {
        None
    };
    // 寄りは絵ごと (字が揺れない `.zoom`)。題名の間だけ、ゆっくり寄る。
    let push = interpolate_eased(t, [14.0, 21.0], [1.0, 1.05], Easing::EaseInOut);
    let mut root = div()
        .w(Px(W))
        .h(Px(H))
        .bg(c3())
        .grain(0.04)
        .grain_seed(3)
        .child(div().absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H)).children(kids).zoom(push));
    if let Some(s) = screen {
        root = root.child(s);
    }
    root
}

fn reel(frames: u32) -> Reel {
    Reel {
        motion_blur: Some(MotionBlur::new(5)),
        mono_family: Some("Menlo".into()),
        crf: 24,
        ..Reel::new(W as u32, H as u32, FPS, frames)
    }
}

fn main() {
    let frames = (SECS * FPS as f32).round() as u32;
    let args = Args::parse();

    if !args.stills.is_empty() {
        let stills = &args.stills;
        let reel = reel(stills.len() as u32);
        let pick = |ctx: FrameCtx| scene(FrameCtx { frame: stills[ctx.frame as usize], ..ctx });
        let dir = args.frames_dir("game_studio");
        let paths = reel.render_pngs(&pick, &dir).expect("render failed");
        for (f, p) in stills.iter().zip(&paths) {
            println!("frame {f} -> {}", p.display());
        }
        return;
    }

    let reel = reel(frames);
    let out = args.mp4("game_studio");
    println!("rendering + encoding {frames} frames ({}x{} @ {FPS}fps) …", reel.width, reel.height);
    match reel.render_mp4(&scene, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
