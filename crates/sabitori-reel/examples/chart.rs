//! sabitori-reel — a **data explainer**: a line chart that draws itself, rings
//! that fill while their numbers count up, a sentence whose key phrases get
//! highlighted as if by a marker, and a callout card pinned over the chart.
//!
//! The data (a year of monthly signups for an imaginary product) is a constant
//! table; every mark on screen is derived from it and from `t`, so this is a
//! pure function of the frame like every reel.
//!
//! What it exercises: `polyline()` (the line is cut at a fractional point so it
//! grows smoothly between samples), `arc()` rings, numbers formatted per frame,
//! a marker sweep over words (`.highlight(HighlightSpec)` with a range that
//! grows character by character),
//! and tree-order drawing — the callout card, the
//! value tag and the tip dot are later siblings, so they sit on top of the
//! grid lines, the area fill and the line without any `.layer(..)`.
//!
//!   cargo run -p sabitori-reel --example chart --release
//!
//! Output: `$TMPDIR/sabitori-chart.mp4`. Pass frame numbers
//! (`-- 0 60 120`) to write just those frames as PNGs instead.

use std::f32::consts::{FRAC_PI_2, TAU};

use sabitori_core::element::HighlightSpec;
use sabitori_core::{BoxShadow, Point};
use sabitori_reel::{
    arc, div, interpolate, interpolate_eased, polyline, spring_with, text, Color, Easing, Element,
    FrameCtx, Px, Reel, Spring,
};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 9.0;

/// 月ごとの新規登録 (千人)。
const DATA: [f32; 12] = [3.6, 4.1, 4.7, 6.9, 7.8, 8.4, 9.1, 9.6, 10.3, 10.9, 11.6, 12.4];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const Y_MAX: f32 = 14.0;

// グラフの描画域 (キャンバスの px)。
const CX0: f32 = 200.0;
const CX1: f32 = 1180.0;
const CY0: f32 = 360.0;
const CY1: f32 = 900.0;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}

fn accent() -> Color {
    hex("#38bdf8")
}

fn ink() -> Color {
    hex("#e8edf6")
}

/// 地の下の方の色 (面の塗りを溶かす先)。
fn bg_bottom() -> Color {
    hex("#0e1528")
}

fn muted() -> Color {
    hex("#8c97ab")
}

/// i 番目の点のキャンバス座標。
fn pt(i: usize) -> (f32, f32) {
    let x = CX0 + (CX1 - CX0) * i as f32 / (DATA.len() - 1) as f32;
    let y = CY1 - (CY1 - CY0) * DATA[i] / Y_MAX;
    (x, y)
}

/// 線を `p` (0..1) まで伸ばした点の列。最後の区間は途中で切る。
fn line_upto(p: f32) -> Vec<(f32, f32)> {
    let span = p * (DATA.len() - 1) as f32;
    let whole = span.floor() as usize;
    let mut pts: Vec<(f32, f32)> = (0..=whole.min(DATA.len() - 1)).map(pt).collect();
    if whole < DATA.len() - 1 {
        let (a, b) = (pt(whole), pt(whole + 1));
        let f = span - whole as f32;
        pts.push((a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f));
    }
    pts
}

/// 線の伸び具合 (0..1)。1.4 秒から 4 秒かけて左から右へ。
fn line_progress(t: f32) -> f32 {
    interpolate_eased(t, [1.4, 5.4], [0.0, 1.0], Easing::EaseInOut)
}

/// 3 桁ごとにカンマを打つ。
fn commas(n: u32) -> String {
    // 右から 3 桁ずつ切り出して、左から並べ直す。
    let s = n.to_string();
    let mut groups: Vec<&str> = Vec::new();
    let mut end = s.len();
    while end > 3 {
        groups.push(&s[end - 3..end]);
        end -= 3;
    }
    groups.push(&s[..end]);
    groups.reverse();
    groups.join(",")
}

/// キャンバス全体を箱にした、絶対座標で点を置く線。
fn stroke(pts: Vec<(f32, f32)>, color: Color, w: f32) -> Element {
    polyline().points(pts).stroke_width(w).stroke_color(color).absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H))
}

/// 塗りつぶした輪 + 中央の数字 + 下の見出し。
fn ring(value: f32, color: Color, big: String, label: &str) -> Element {
    let size = 210.0;
    div()
        .flex_row()
        .items_center()
        .gap(34.0)
        .child(
            div()
                .w(Px(size))
                .h(Px(size))
                .child(
                    arc()
                        .absolute()
                        .pos(0.0, 0.0)
                        .w(Px(size))
                        .h(Px(size))
                        .arc_sweep(-FRAC_PI_2, TAU)
                        .arc_thickness(18.0)
                        .arc_colors(color, hex("#1b2540"))
                        .arc_value(value),
                )
                .child(
                    div()
                        .absolute()
                        .pos(0.0, 0.0)
                        .w(Px(size))
                        .h(Px(size))
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .child(text(big).font_size(52.0).font_weight(800).color(ink())),
                ),
        )
        .child(text(label.to_string()).font_size(24.0).line_height(1.35).color(muted()).w(Px(260.0)))
}

// ── 場面 ───────────────────────────────────────────────────────────────────────

fn scene(ctx: FrameCtx) -> Element {
    let t = ctx.t();
    let mut kids: Vec<Element> = Vec::new();

    // 見出し。
    let head_in = spring_with(t + 0.1, FPS, Spring::gentle()); // 最初のコマから少し見える
    // 蛍光ペン: 句に掛けた `HighlightSpec` の範囲を、1 字ずつ左から伸ばす。
    // 2 つ目の句は線を描き終えてから。
    const SENTENCE: &str = "Growth picked up after the March launch and never looked back.";
    let marker = |phrase: &str, from: f32| -> HighlightSpec {
        let p = interpolate_eased(t, [from, from + 0.7], [0.0, 1.0], Easing::EaseInOut);
        let start = SENTENCE.find(phrase).expect("句が文に無い");
        // 伸ばす単位は字 (バイトの途中で切らない)。
        let n = (phrase.chars().count() as f32 * p).round() as usize;
        let len: usize = phrase.chars().take(n).map(char::len_utf8).sum();
        HighlightSpec {
            ranges: vec![(start, start + len)],
            color: hex("#f59e0b").with_alpha(0.42),
            ..Default::default()
        }
    };
    let sentence = text(SENTENCE)
        .font_size(30.0)
        .color(hex("#dfe4ee"))
        .highlight(marker("after the March launch", 1.0))
        .highlight(marker("never looked back", 5.6));
    kids.push(
        div()
            .absolute()
            .pos(CX0 - 80.0, 96.0)
            .flex_col()
            .gap(14.0)
            .opacity(head_in.min(1.0))
            .ty((1.0 - head_in) * 24.0)
            .child(text("ANNUAL REPORT · SIGNUPS").font_size(20.0).font_weight(700).letter_spacing(4.0).color(accent()))
            .child(text("Signups grew 3.4× this year").font_size(68.0).font_weight(800).letter_spacing(-1.0).color(ink()))
            .child(sentence),
    );

    // 線の伸び具合と、その下の面。面は目盛りの線より先に描いて、目盛りを上に見せる。
    let p = line_progress(t);
    let pts = line_upto(p);
    if p > 0.0 {
        // 面の塗り: 線の下を細い縦の帯で埋める (上ほど濃い縦の階調)。帯は 1px ずつ
        // 重ね、色は地に混ぜた不透明の色にする (半透明だと、重なりや縁が筋になる)。
        let tip_x = pts.last().map_or(CX0, |q| q.0);
        let step = 6.0;
        let mut x = CX0;
        while x < tip_x {
            let y = y_at(&pts, x + step * 0.5);
            kids.push(
                div()
                    .absolute()
                    .pos(x, y)
                    .w(Px((step + 1.0).min(tip_x - x)))
                    .h(Px(CY1 - y))
                    .gradient(bg_bottom().lerp(accent(), 0.24), bg_bottom(), FRAC_PI_2),
            );
            x += step;
        }
    }

    // 軸と目盛り: 横線を下から順に引く。
    let grid_in = interpolate(t, [0.5, 1.2], [0.0, 1.0]);
    for (k, v) in [0.0_f32, 4.0, 8.0, 12.0].iter().enumerate() {
        let y = CY1 - (CY1 - CY0) * v / Y_MAX;
        let len = interpolate_eased(grid_in, [k as f32 * 0.15, 0.55 + k as f32 * 0.15], [0.0, 1.0], Easing::EaseOut);
        kids.push(div().absolute().pos(CX0, y).w(Px((CX1 - CX0) * len)).h(Px(1.0)).bg(Color::WHITE.with_alpha(if k == 0 { 0.28 } else { 0.08 })));
        kids.push(
            text(if *v == 0.0 { "0".to_string() } else { format!("{v:.0}k") })
                .font_size(20.0)
                .color(muted())
                .opacity(len)
                .absolute()
                .pos(CX0 - 60.0, y - 13.0),
        );
    }
    for (i, m) in MONTHS.iter().enumerate() {
        let (x, _) = pt(i);
        kids.push(
            text(*m)
                .font_size(20.0)
                .color(muted())
                .opacity(interpolate(t, [0.8 + i as f32 * 0.04, 1.2 + i as f32 * 0.04], [0.0, 1.0]))
                .absolute()
                .pos(x - 18.0, CY1 + 18.0),
        );
    }

    // 線 (面の塗りは目盛りより前に済ませてあるので、ここは線と札だけ)。
    if p > 0.0 {
        kids.push(stroke(pts.clone(), accent().with_alpha(0.25), 12.0)); // にじみ
        kids.push(stroke(pts.clone(), accent(), 4.0));

        // 先端の点と、今の値の札。
        let (tx, ty) = *pts.last().unwrap_or(&pt(0));
        let value = (CY1 - ty) / (CY1 - CY0) * Y_MAX;
        kids.push(div().absolute().pos(tx - 9.0, ty - 9.0).w(Px(18.0)).h(Px(18.0)).rounded_px(9.0).bg(Color::WHITE).glow(accent(), 16.0));
        kids.push(
            div()
                .absolute()
                .pos((tx - 52.0).max(CX0 - 8.0), ty - 62.0) // 縦軸の目盛りに被らないよう寄せる
                .w(Px(104.0))
                .h(Px(40.0))
                .rounded_px(10.0)
                .bg(hex("#0f172a"))
                .border(1.0, accent().with_alpha(0.6))
                .flex_row()
                .items_center()
                .justify_center()
                .child(text(format!("{:.1}k", value)).font_size(20.0).font_weight(700).color(ink())),
        );
    }

    // 吹き出し: 線が 4 月に届いたら、跳ねる手前の 3 月の点に札を立てる。グラフの上に重なる。
    let reach = 3.0 / (DATA.len() - 1) as f32;
    let april_at = (0..(SECS * FPS as f32) as u32).map(|f| f as f32 / FPS as f32).find(|&s| line_progress(s) >= reach).unwrap_or(SECS);
    let s_call = if t < april_at { 0.0 } else { spring_with(t - april_at, FPS, Spring::bouncy()) };
    if s_call > 0.0 {
        let (mx, my) = pt(2);
        let (bx, by) = (mx - 30.0, my - 340.0);
        kids.push(div().absolute().pos(mx - 1.0, by + 120.0).w(Px(2.0)).h(Px((my - by - 120.0) * s_call.min(1.0))).bg(hex("#facc15").with_alpha(0.7)));
        kids.push(div().absolute().pos(mx - 7.0, my - 7.0).w(Px(14.0)).h(Px(14.0)).rounded_px(7.0).bg(hex("#facc15")).opacity(s_call.min(1.0)));
        kids.push(
            div()
                .absolute()
                .pos(bx, by)
                .w(Px(340.0))
                .flex_col()
                .gap(8.0)
                .p_px(22.0)
                .rounded_px(16.0)
                .bg(hex("#1e293b"))
                .border(1.0, hex("#facc15").with_alpha(0.5))
                .shadow(BoxShadow { color: hex("#000000").with_alpha(0.45), offset: Point::new(0.0, 14.0), blur: 32.0, spread: 0.0 })
                .opacity(s_call.min(1.0))
                .scaled(0.85 + 0.15 * s_call)
                .child(text("March launch").font_size(26.0).font_weight(700).color(hex("#fde68a")))
                .child(text("+47% in the following month").font_size(21.0).color(hex("#cbd5e1"))),
        );
    }

    // 右の列: 輪 2 つと、数え上がる合計。
    let r1 = interpolate_eased(t, [2.2, 4.6], [0.0, 1.0], Easing::EaseOutCubic);
    let r2 = interpolate_eased(t, [2.8, 5.2], [0.0, 1.0], Easing::EaseOutCubic);
    let total: f32 = DATA.iter().sum::<f32>() * 1000.0;
    let col_in = interpolate(t, [1.8, 2.4], [0.0, 1.0]);
    kids.push(
        div()
            .absolute()
            .pos(1300.0, 330.0)
            .flex_col()
            .gap(46.0)
            .opacity(col_in)
            .tx((1.0 - col_in) * 40.0)
            .child(ring(0.84 * r1, accent(), format!("{:.0}%", 84.0 * r1), "of new users still active after 30 days"))
            .child(ring(r2 * 3.4 / 4.0, hex("#a78bfa"), format!("{:.1}×", 1.0 + 2.4 * r2), "December vs. January signups"))
            .child(
                div()
                    .flex_col()
                    .gap(4.0)
                    .child(text(commas((total * r2).round() as u32)).font_size(64.0).font_weight(800).color(ink()))
                    .child(text("signups in total").font_size(24.0).color(muted())),
            ),
    );

    div().w(Px(W)).h(Px(H)).gradient(hex("#0b1120"), hex("#0f1629"), FRAC_PI_2).children(kids)
}

/// 折れ線の `x` での高さ。
fn y_at(pts: &[(f32, f32)], x: f32) -> f32 {
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if x <= b.0 {
            let f = ((x - a.0) / (b.0 - a.0).max(1e-3)).clamp(0.0, 1.0);
            return a.1 + (b.1 - a.1) * f;
        }
    }
    pts.last().map_or(CY1, |q| q.1)
}

fn main() {
    let frames = (SECS * FPS as f32).round() as u32;
    let stills: Vec<u32> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();

    if !stills.is_empty() {
        // 指定のコマだけ PNG に書く (見た目の確認用)。
        let reel = Reel::new(W as u32, H as u32, FPS, stills.len() as u32);
        let pick = |ctx: FrameCtx| scene(FrameCtx { frame: stills[ctx.frame as usize], ..ctx });
        let dir = std::env::temp_dir().join("sabitori-chart");
        let paths = reel.render_pngs(&pick, &dir).expect("render failed");
        for (f, p) in stills.iter().zip(&paths) {
            println!("frame {f} -> {}", p.display());
        }
        return;
    }

    let reel = Reel::new(W as u32, H as u32, FPS, frames);
    let out = std::env::temp_dir().join("sabitori-chart.mp4");
    println!("rendering + encoding {frames} frames ({}x{} @ {FPS}fps) …", reel.width, reel.height);
    match reel.render_mp4(&scene, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
