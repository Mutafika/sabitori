//! sabitori-reel — **an architects' office, drawn** (22 s): "Linework Atelier
//! No.7", a practice that does not exist, introduced the way it works — on
//! paper, in graphite.
//!
//! - a faint grid appears, then the floor plan is drawn stroke by stroke
//!   (`path().trim(..)`): walls, partitions, door swings, windows, a stair;
//! - dimension lines run out along the plan and their figures count up;
//! - beside it the elevation rises: walls fill up from the ground line (a
//!   growing `mask`), windows drop in, and a slanted cast shadow stretches
//!   across the ground;
//! - one accent colour (vermilion) marks the entrance and the north arrow;
//! - the title block draws itself and carries the name; the whole sheet eases
//!   in with `.zoom(..)` so the fine lines never shimmer.
//!
//!   cargo run -p sabitori-reel --example architects --release
//!
//! Output: `target/reel/architects.mp4` (`--out <dir>` to change). Frame
//! numbers write just those frames as PNGs.

#[path = "common/out.rs"]
mod out;

use std::f32::consts::FRAC_PI_2;

use out::Args;
use sabitori_reel::{
    div, interpolate, interpolate_eased, path, text, Color, Easing, Element, FrameCtx, MotionBlur, Percent, Px, Reel,
};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 22.0;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}
fn paper() -> Color {
    hex("#f5f2ea")
}
fn graphite() -> Color {
    hex("#2c2a27")
}
fn faint() -> Color {
    hex("#2c2a27").with_alpha(0.07)
}
fn vermilion() -> Color {
    hex("#d2452d")
}

fn ease(t: f32, a: f32, b: f32) -> f32 {
    interpolate_eased(t, [a, b], [0.0, 1.0], Easing::EaseInOut)
}

/// 紙いっぱいの座標で書いた線 (SVG の d)。`p` (0..1) まで描く。
fn stroke(d: &str, width: f32, color: Color, p: f32) -> Element {
    path()
        .svg_path(d)
        .stroke(color, width)
        .trim(0.0, p)
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(W))
        .h(Px(H))
}

fn fill(d: &str, color: Color) -> Element {
    path().svg_path(d).fill(color).absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H))
}

/// 小さな文字 (図面の注記)。等幅で、少し字間を空ける。
fn note(s: &str, size: f32, color: Color) -> Element {
    text(s.to_string()).mono().font_size(size).letter_spacing(1.5).color(color)
}

// ── 方眼 ─────────────────────────────────────────────────────────────────

fn grid(t: f32) -> Vec<Element> {
    let mut out = Vec::new();
    let mut k = 0;
    let mut x = 40.0;
    while x < W {
        let p = ease(t, 0.3 + k as f32 * 0.012, 1.6 + k as f32 * 0.012);
        out.push(stroke(&format!("M{x} 0 V{H}"), 1.0, faint(), p));
        x += 40.0;
        k += 1;
    }
    let mut y = 40.0;
    k = 0;
    while y < H {
        let p = ease(t, 0.4 + k as f32 * 0.015, 1.7 + k as f32 * 0.015);
        out.push(stroke(&format!("M0 {y} H{W}"), 1.0, faint(), p));
        y += 40.0;
        k += 1;
    }
    out
}

// ── 平面図 (左) ──────────────────────────────────────────────────────────

/// 平面図の線: (d, 太さ, 描き始め, 描き終わり)。
const PLAN: &[(&str, f32, f32, f32)] = &[
    // 外壁
    ("M220 200 H900 V700 H220 Z", 4.0, 1.2, 3.2),
    // 間仕切り
    ("M520 200 V420", 2.5, 2.8, 3.4),
    ("M520 500 V700", 2.5, 3.0, 3.5),
    ("M220 440 H450", 2.5, 3.1, 3.7),
    ("M600 440 H900", 2.5, 3.3, 3.9),
    // 扉の開き (葉と弧)
    ("M450 440 L450 380", 1.5, 3.8, 4.1),
    ("M450 380 A60 60 0 0 1 510 440", 1.0, 4.0, 4.4),
    ("M520 420 L580 420", 1.5, 4.0, 4.3),
    ("M580 420 A60 60 0 0 1 520 480", 1.0, 4.2, 4.6),
    // 玄関の扉
    ("M640 700 L640 640", 1.5, 4.3, 4.6),
    ("M640 640 A60 60 0 0 1 700 700", 1.0, 4.5, 4.9),
    // 窓 (壁の中の細い 2 本)
    ("M290 196 H440 M290 204 H440", 1.0, 4.4, 4.9),
    ("M620 196 H820 M620 204 H820", 1.0, 4.6, 5.1),
    ("M896 260 V380 M904 260 V380", 1.0, 4.8, 5.3),
    ("M216 520 V640 M224 520 V640", 1.0, 5.0, 5.5),
];

fn stair(t: f32) -> Vec<Element> {
    // 階段: 段の線が下から順に。
    let mut out = vec![stroke("M700 470 H880 V680 H700 Z", 1.5, graphite(), ease(t, 4.6, 5.2))];
    for k in 0..9 {
        let y = 680.0 - 21.0 * (k as f32 + 1.0);
        out.push(stroke(&format!("M700 {y} H880"), 1.0, graphite(), ease(t, 4.9 + k as f32 * 0.06, 5.2 + k as f32 * 0.06)));
    }
    // 上り口の矢印
    out.push(stroke("M790 670 V500 M780 515 L790 500 L800 515", 1.2, graphite(), ease(t, 5.4, 5.9)));
    out
}

fn plan(t: f32) -> Vec<Element> {
    let mut out: Vec<Element> = PLAN.iter().map(|&(d, w, a, b)| stroke(d, w, graphite(), ease(t, a, b))).collect();
    out.extend(stair(t));
    // 部屋の名前
    let rooms = [("LIVING", 300.0, 560.0), ("STUDY", 300.0, 300.0), ("KITCHEN", 620.0, 300.0), ("STAIR", 740.0, 450.0)];
    for (i, (name, x, y)) in rooms.into_iter().enumerate() {
        let a = interpolate(t, [5.6 + i as f32 * 0.15, 6.0 + i as f32 * 0.15], [0.0, 1.0]);
        out.push(note(name, 18.0, graphite().with_alpha(0.75)).absolute().pos(x, y).opacity(a));
    }
    out
}

// ── 寸法線 ───────────────────────────────────────────────────────────────

fn dimensions(t: f32) -> Vec<Element> {
    let mut out = Vec::new();
    let h = ease(t, 5.6, 6.8);
    let v = ease(t, 6.0, 7.2);
    // 横: 平面図の上、端に斜めの止め。
    out.push(stroke("M220 150 H900", 1.0, graphite(), h));
    out.push(stroke("M212 158 L228 142 M892 158 L908 142", 1.2, graphite(), h));
    out.push(stroke("M220 190 V140 M900 190 V140", 0.8, graphite(), ease(t, 5.4, 5.8)));
    let mm_w = (7200.0 * h).round() as u32;
    out.push(
        note(&format!("{mm_w}"), 20.0, graphite())
            .absolute()
            .pos(530.0, 118.0)
            .opacity(interpolate(t, [5.8, 6.1], [0.0, 1.0])),
    );
    // 縦: 平面図の左。
    out.push(stroke("M170 200 V700", 1.0, graphite(), v));
    out.push(stroke("M162 208 L178 192 M162 708 L178 692", 1.2, graphite(), v));
    out.push(stroke("M210 200 H160 M210 700 H160", 0.8, graphite(), ease(t, 5.8, 6.2)));
    let mm_h = (5200.0 * v).round() as u32;
    out.push(
        note(&format!("{mm_h}"), 20.0, graphite())
            .absolute()
            .pos(118.0, 470.0)
            .rotate(-FRAC_PI_2)
            .opacity(interpolate(t, [6.2, 6.5], [0.0, 1.0])),
    );
    out
}

// ── 立面図 (右) ──────────────────────────────────────────────────────────

const GROUND: f32 = 700.0;

/// 下から `p` (0..1) だけ見せるマスク。壁が地面から立ち上がる。
fn rise(p: f32) -> Element {
    div()
        .w(Percent(100.0))
        .h(Percent(100.0))
        .flex_col()
        .justify_end()
        .child(div().w(Percent(100.0)).h(Percent(p * 100.0)).bg(Color::BLACK))
}

fn elevation(t: f32) -> Vec<Element> {
    let mut out = Vec::new();
    // 地盤線と、その下の帯 (地面)。
    out.push(stroke(&format!("M1040 {GROUND} H1820"), 3.0, graphite(), ease(t, 7.6, 8.6)));
    // 壁: 塗りが下から立ち上がる (母屋、少し遅れて下屋)。
    let main = ease(t, 8.6, 10.2);
    let wing = ease(t, 9.4, 10.8);
    let body = |x: f32, y: f32, w: f32, h: f32, p: f32, tone: Color| {
        div()
            .absolute()
            .pos(x, y)
            .w(Px(w))
            .h(Px(h))
            .bg(tone)
            .border(2.5, graphite())
            .mask(rise(p))
    };
    out.push(body(1140.0, 460.0, 420.0, GROUND - 460.0, main, hex("#e8e3d8")));
    out.push(body(1560.0, 560.0, 180.0, GROUND - 560.0, wing, hex("#ddd7ca")));
    // 屋根: 壁が立ってから線で。
    out.push(stroke("M1120 462 L1350 330 L1580 462", 2.5, graphite(), ease(t, 10.0, 10.9)));
    out.push(stroke("M1550 562 H1752", 2.5, graphite(), ease(t, 10.6, 11.0)));
    // 窓: 上から落ちて、ぴたりと止まる。
    let windows = [(1190.0, 520.0, 70.0, 100.0), (1290.0, 520.0, 70.0, 100.0), (1430.0, 520.0, 90.0, 180.0), (1600.0, 600.0, 100.0, 60.0)];
    for (i, (x, y, w, h)) in windows.into_iter().enumerate() {
        let at = 10.6 + i as f32 * 0.18;
        let s = interpolate_eased(t, [at, at + 0.45], [0.0, 1.0], Easing::EaseOutCubic);
        if s <= 0.0 {
            continue;
        }
        out.push(
            div()
                .absolute()
                .pos(x, y - (1.0 - s) * 24.0)
                .w(Px(w))
                .h(Px(h))
                .bg(graphite().with_alpha(0.85))
                .border(2.0, graphite())
                .opacity(s),
        );
    }
    // 影: 左上からの光。地面に斜めの影が右へ伸びる。
    let len = 150.0 * ease(t, 11.6, 13.4);
    if len > 1.0 {
        out.push(fill(
            &format!("M1740 {g} L{x1} {g} L{x2} {g2} L1780 {g2} Z", g = GROUND, g2 = GROUND + 34.0, x1 = 1740.0 + len, x2 = 1780.0 + len),
            graphite().with_alpha(0.22),
        ));
        // 母屋が下屋の壁に落とす影 (斜めの帯)。
        let k = ease(t, 11.8, 13.4);
        out.push(fill(
            &format!("M1560 562 L{a} 562 L1560 {b} Z", a = 1560.0 + 150.0 * k, b = 562.0 + 120.0 * k),
            graphite().with_alpha(0.18),
        ));
    }
    // 地面の帯 (斜線のハッチ)。
    for k in 0..25 {
        let x = 1050.0 + k as f32 * 30.0;
        out.push(stroke(&format!("M{x} {g} L{x2} {g2}", g = GROUND + 4.0, x2 = x - 16.0, g2 = GROUND + 20.0), 1.0, graphite().with_alpha(0.55), ease(t, 8.0 + k as f32 * 0.02, 8.4 + k as f32 * 0.02)));
    }
    let label = interpolate(t, [8.4, 8.9], [0.0, 1.0]);
    out.push(note("SOUTH ELEVATION  1:100", 18.0, graphite().with_alpha(0.75)).absolute().pos(1060.0, GROUND + 40.0).opacity(label));
    out.push(note("GROUND FLOOR PLAN  1:100", 18.0, graphite().with_alpha(0.75)).absolute().pos(220.0, 730.0).opacity(interpolate(t, [5.6, 6.1], [0.0, 1.0])));
    out
}

// ── 差し色: 玄関と北 ─────────────────────────────────────────────────────

fn accents(t: f32) -> Vec<Element> {
    let mut out = Vec::new();
    // 玄関: 朱の丸が描かれ、矢印と注記。
    let ring = ease(t, 13.8, 14.6);
    out.push(stroke("M670 738 A26 26 0 1 1 669.9 738 Z", 2.5, vermilion(), ring));
    out.push(stroke("M670 820 V770 M660 784 L670 770 L680 784", 2.5, vermilion(), ease(t, 14.4, 14.9)));
    out.push(
        note("ENTRANCE", 18.0, vermilion())
            .absolute()
            .pos(700.0, 800.0)
            .opacity(interpolate(t, [14.7, 15.1], [0.0, 1.0])),
    );
    // 北の矢印 (右上)。
    let n = ease(t, 15.0, 15.8);
    out.push(stroke("M960 230 A34 34 0 1 1 959.9 230 Z", 1.5, graphite(), n));
    out.push(fill("M960 236 L972 290 L960 280 L948 290 Z", vermilion().with_alpha(n)));
    out.push(note("N", 18.0, graphite()).absolute().pos(954.0, 200.0).opacity(n));
    out
}

// ── 表題欄 (右下) ────────────────────────────────────────────────────────

fn title_block(t: f32) -> Vec<Element> {
    let (x, y, w, h) = (1230.0, 830.0, 560.0, 190.0);
    let mut out = vec![
        stroke(&format!("M{x} {y} H{r} V{b} H{x} Z", r = x + w, b = y + h), 2.0, graphite(), ease(t, 15.6, 16.6)),
        stroke(&format!("M{x} {m} H{r}", m = y + 110.0, r = x + w), 1.0, graphite(), ease(t, 16.2, 16.8)),
        stroke(&format!("M{c} {m} V{b}", c = x + 300.0, m = y + 110.0, b = y + h), 1.0, graphite(), ease(t, 16.5, 16.9)),
    ];
    let a = |s: f32| interpolate(t, [s, s + 0.5], [0.0, 1.0]);
    out.push(
        text("Linework Atelier No.7")
            .font_size(46.0)
            .font_weight(700)
            .letter_spacing(-0.5)
            .color(graphite())
            .absolute()
            .pos(x + 26.0, y + 20.0)
            .opacity(a(16.8)),
    );
    out.push(note("ARCHITECTS  ·  A PRACTICE THAT DOES NOT EXIST", 15.0, graphite().with_alpha(0.6)).absolute().pos(x + 28.0, y + 78.0).opacity(a(17.1)));
    out.push(note("HOUSE IN THE PINES", 18.0, graphite()).absolute().pos(x + 26.0, y + 130.0).opacity(a(17.3)));
    out.push(note("A-01  ·  2026.10", 18.0, graphite()).absolute().pos(x + 326.0, y + 130.0).opacity(a(17.5)));
    // 差し色の細い帯 (表題欄の左端)。
    out.push(div().absolute().pos(x, y).w(Px(8.0)).h(Px(h * ease(t, 16.6, 17.2))).bg(vermilion()));
    out
}

fn tagline(t: f32) -> Element {
    let a = interpolate_eased(t, [18.4, 19.6], [0.0, 1.0], Easing::EaseOut);
    text("Quiet buildings, drawn precisely.")
        .font_size(38.0)
        .letter_spacing(0.5)
        .color(graphite())
        .absolute()
        .pos(1060.0, 200.0)
        .opacity(a)
        .ty((1.0 - a) * 12.0)
}

fn scene(ctx: FrameCtx) -> Element {
    let t = ctx.t();
    let mut sheet: Vec<Element> = Vec::new();
    sheet.extend(grid(t));
    sheet.extend(plan(t));
    sheet.extend(dimensions(t));
    // 立面図は少し左へ (影が紙の右端で切れないように)。
    sheet.push(div().absolute().pos(-70.0, 0.0).w(Px(W)).h(Px(H)).children(elevation(t)));
    sheet.extend(accents(t));
    sheet.extend(title_block(t));
    sheet.push(tagline(t));
    // 紙全体をゆっくり寄せる (絵として。`.scaled` だと細い線と字が揺れる)。
    let zoom = interpolate_eased(t, [0.0, SECS], [1.0, 1.05], Easing::EaseInOut);
    let fade_in = interpolate(t, [0.0, 0.8], [0.0, 1.0]);
    let fade_out = interpolate(t, [21.2, 22.0], [1.0, 0.0]);
    div()
        .w(Px(W))
        .h(Px(H))
        .bg(paper())
        .child(div().absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H)).bg(paper()).grain(0.03).grain_seed(7))
        .child(div().absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H)).children(sheet).zoom(zoom).opacity(fade_in * fade_out))
}

fn reel(frames: u32) -> Reel {
    Reel {
        motion_blur: Some(MotionBlur::new(5)),
        crf: 24,
        mono_family: Some("Menlo".into()),
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
        let dir = args.frames_dir("architects");
        let paths = reel.render_pngs(&pick, &dir).expect("render failed");
        for (f, p) in stills.iter().zip(&paths) {
            println!("frame {f} -> {}", p.display());
        }
        return;
    }

    let reel = reel(frames);
    let out = args.mp4("architects");
    println!("rendering + encoding {frames} frames ({}x{} @ {FPS}fps) …", reel.width, reel.height);
    match reel.render_mp4(&scene, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
