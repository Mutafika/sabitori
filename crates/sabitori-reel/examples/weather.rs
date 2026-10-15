//! sabitori-reel — **a weather app intro** (22 s) for "hareoto", an app that
//! does not exist. The app screen is the star: the temperature counts up, clouds
//! drift across, rain falls and stops, the sun comes out, and the week's
//! forecast deals in card by card. The ground behind the phone follows the
//! weather — overcast grey, wet slate, then warm yellow.
//!
//! - the phone screen is an ordinary element tree, rounded off with a `mask`;
//! - raindrops and cloud puffs are plain boxes (single-box `.rotation(..)`, no
//!   groups), so a hundred of them stay cheap;
//! - the sun's rays are one `path()` that turns;
//! - the copy on the left and the closing line come in with `per_glyph`.
//!
//!   cargo run -p sabitori-reel --example weather --release
//!
//! Output: `target/reel/weather.mp4` (`--out <dir>` to change). Frame numbers
//! write just those frames as PNGs.

#[path = "common/out.rs"]
mod out;

use std::f32::consts::TAU;

use out::Args;
use sabitori_reel::{
    div, interpolate, interpolate_eased, path, spring_with, text, Color, Easing, Element, FrameCtx, GlyphMotion,
    MotionBlur, Percent, Px, Reel, Spring,
};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 22.0;

/// 電話の画面 (論理 px)。右寄りに置く。
const PX: f32 = 1130.0;
const PY: f32 = 70.0;
const PW: f32 = 520.0;
const PH: f32 = 940.0;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}
fn ink() -> Color {
    hex("#1d1b18")
}

fn spring_at(t: f32, start: f32, cfg: Spring) -> f32 {
    if t < start {
        0.0
    } else {
        spring_with(t - start, FPS, cfg)
    }
}

fn ease(t: f32, a: f32, b: f32) -> f32 {
    interpolate_eased(t, [a, b], [0.0, 1.0], Easing::EaseInOut)
}

fn hash(i: u32, k: u32) -> f32 {
    let mut h = i.wrapping_mul(0x9e37_79b1) ^ k.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^= h >> 13;
    (h & 0xffff) as f32 / 65535.0
}

// ── 天気の段階 ───────────────────────────────────────────────────────────

/// 雨の強さ (0..1): 3 秒から降り出し、8.5 秒から止む。
fn rain(t: f32) -> f32 {
    ease(t, 3.0, 4.0) * (1.0 - ease(t, 8.5, 9.6))
}

/// 晴れ具合 (0..1): 9.4 秒から晴れていく。
fn sun(t: f32) -> f32 {
    ease(t, 9.4, 11.0)
}

/// 地の色: くもりの灰 → 濡れた石の灰 → 暖かい黄。
fn ground(t: f32) -> Color {
    let cloudy = hex("#a9a8a2");
    let wet = hex("#73767a");
    let sunny = hex("#f1c64b");
    cloudy.lerp(wet, rain(t) * (1.0 - sun(t))).lerp(sunny, sun(t))
}

/// 画面の地の色 (地の色を淡くしたもの)。
fn screen_bg(t: f32) -> Color {
    let cloudy = hex("#ecebe7");
    let wet = hex("#dcdee0");
    let sunny = hex("#fff6d8");
    cloudy.lerp(wet, rain(t) * (1.0 - sun(t))).lerp(sunny, sun(t))
}

/// 気温: 0 → 14° (くもり) → 雨で 11° → 晴れて 23°。
fn temperature(t: f32) -> f32 {
    let up = interpolate_eased(t, [0.8, 2.6], [0.0, 14.0], Easing::EaseOutCubic);
    let down = interpolate_eased(t, [4.0, 6.5], [0.0, -3.0], Easing::EaseInOut);
    let warm = interpolate_eased(t, [9.6, 11.6], [0.0, 12.0], Easing::EaseOutCubic);
    up + down + warm
}

// ── 絵の部品 ─────────────────────────────────────────────────────────────

/// 雲: 丸 3 つと底の板。幅 `w`、左上 (x, y)。
fn cloud(x: f32, y: f32, w: f32, color: Color) -> Vec<Element> {
    let h = w * 0.5;
    let puff = |cx: f32, cy: f32, r: f32| div().absolute().pos(x + cx - r, y + cy - r).w(Px(r * 2.0)).h(Px(r * 2.0)).rounded_px(r).bg(color);
    vec![
        puff(w * 0.30, h * 0.62, w * 0.20),
        puff(w * 0.55, h * 0.45, w * 0.26),
        puff(w * 0.76, h * 0.66, w * 0.17),
        div().absolute().pos(x + w * 0.12, y + h * 0.62).w(Px(w * 0.76)).h(Px(h * 0.38)).rounded_px(h * 0.19).bg(color),
    ]
}

/// 太陽: 丸と、回る光の筋 (1 本の `path`)。中心 (cx, cy)、半径 r。
fn sun_disc(cx: f32, cy: f32, r: f32, turn: f32, color: Color) -> Vec<Element> {
    let mut rays = path();
    for k in 0..10 {
        let a = k as f32 / 10.0 * TAU;
        let (c, s) = (a.cos(), a.sin());
        let (r0, r1) = (r * 1.35, r * 1.75);
        rays = rays.move_to(r * 2.0 + c * r0, r * 2.0 + s * r0).line_to(r * 2.0 + c * r1, r * 2.0 + s * r1);
    }
    vec![
        rays.stroke(color, r * 0.16)
            .w(Px(r * 4.0))
            .h(Px(r * 4.0))
            .absolute()
            .pos(cx - r * 2.0, cy - r * 2.0)
            .rotate(turn),
        div().absolute().pos(cx - r, cy - r).w(Px(r * 2.0)).h(Px(r * 2.0)).rounded_px(r).bg(color),
    ]
}

/// 小さな天気の絵 (週間予報のカード用)。中心 (cx, cy)。
fn icon(kind: u8, cx: f32, cy: f32) -> Vec<Element> {
    match kind {
        0 => {
            let r = 13.0;
            vec![div().absolute().pos(cx - r, cy - r).w(Px(r * 2.0)).h(Px(r * 2.0)).rounded_px(r).bg(hex("#e8a417"))]
        }
        1 => cloud(cx - 22.0, cy - 13.0, 44.0, hex("#8d8f93")),
        _ => {
            let mut v = cloud(cx - 22.0, cy - 18.0, 44.0, hex("#6d7176"));
            for k in 0..3 {
                v.push(
                    div()
                        .absolute()
                        .pos(cx - 12.0 + k as f32 * 11.0, cy + 8.0)
                        .w(Px(3.0))
                        .h(Px(10.0))
                        .rounded_px(1.5)
                        .bg(hex("#3d6fae"))
                        .rotation(0.25),
                );
            }
            v
        }
    }
}

// ── 電話の画面 ───────────────────────────────────────────────────────────

/// 雨粒 (画面の中、画面の座標)。
fn raindrops(t: f32, w: f32, h: f32, n: u32, seed: u32) -> Vec<Element> {
    let a = rain(t);
    if a <= 0.0 {
        return Vec::new();
    }
    (0..n)
        .filter(|&i| hash(i, seed) < a)
        .map(|i| {
            let speed = 900.0 + hash(i, seed + 1) * 500.0;
            let len = 22.0 + hash(i, seed + 2) * 20.0;
            let x0 = hash(i, seed + 3) * (w + 200.0) - 100.0;
            let y = ((t * speed + hash(i, seed + 4) * h) % (h + 200.0)) - 100.0;
            div()
                .absolute()
                .pos(x0 - y * 0.25, y)
                .w(Px(3.0))
                .h(Px(len))
                .rounded_px(1.5)
                .bg(hex("#3d6fae").with_alpha(0.55))
                .rotation(0.25)
        })
        .collect()
}

fn week(t: f32) -> Element {
    let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let kinds = [0u8, 0, 1, 2, 2, 1, 0];
    let highs = [23, 24, 20, 16, 15, 19, 25];
    let lows = [12, 13, 12, 10, 9, 11, 14];
    let mut row = div().flex_row().gap(8.0);
    for i in 0..7 {
        let s = spring_at(t, 11.8 + i as f32 * 0.13, Spring::bouncy());
        let mut cardk = div()
            .w(Px(58.0))
            .h(Px(150.0))
            .rounded_px(16.0)
            .bg(Color::WHITE.with_alpha(0.75))
            .flex_col()
            .items_center()
            .p(Px(10.0))
            .gap(6.0)
            .child(text(days[i]).font_size(15.0).bold().color(ink()))
            .child(div().w(Px(44.0)).h(Px(40.0)))
            .child(text(format!("{}°", highs[i])).font_size(18.0).bold().color(ink()))
            .child(text(format!("{}°", lows[i])).font_size(15.0).color(ink().with_alpha(0.5)));
        for e in icon(kinds[i], 29.0, 64.0) {
            cardk = cardk.child(e);
        }
        row = row.child(cardk.ty((1.0 - s) * 140.0).opacity(s.clamp(0.0, 1.0)));
    }
    row
}

fn screen(t: f32) -> Element {
    let temp = temperature(t);
    let label = if sun(t) > 0.5 {
        "Sunny"
    } else if rain(t) > 0.3 {
        "Rain · stops at 3 pm"
    } else {
        "Cloudy"
    };
    let mut kids: Vec<Element> = Vec::new();
    // 太陽 (雲の奥)。
    let rise = sun(t);
    if rise > 0.0 {
        kids.extend(sun_disc(PW - 120.0, 260.0 - rise * 80.0 + (1.0 - rise) * 120.0, 58.0, t * 0.4, hex("#f0a91b").with_alpha(rise)));
    }
    // 雲が右から左へ流れる。晴れると上へはけて薄くなる。
    let leave = sun(t);
    for (i, (w, y, speed, shade)) in [(240.0, 90.0, 38.0, "#c9c9c6"), (170.0, 170.0, 60.0, "#b4b5b3"), (270.0, 215.0, 26.0, "#d4d3cf")]
        .into_iter()
        .enumerate()
    {
        let x = PW + 60.0 - ((t * speed + i as f32 * 210.0) % (PW + 400.0));
        let dark = hex(shade).lerp(hex("#8e9196"), rain(t));
        let c = dark.with_alpha(1.0 - leave);
        kids.extend(cloud(x, y - leave * 200.0, w, c));
    }
    kids.extend(raindrops(t, PW, PH, 90, 11));
    // 中身。
    let pop = spring_at(t, 0.6, Spring::gentle());
    let info = div()
        .absolute()
        .pos(40.0, 380.0)
        .flex_col()
        .gap(4.0)
        .child(text("Kitamachi").font_size(28.0).bold().color(ink()))
        .child(text(format!("{:.0}°", temp.max(0.0))).font_size(180.0).font_weight(800).letter_spacing(-6.0).color(ink()))
        .child(text(label).font_size(26.0).color(ink().with_alpha(0.7)))
        .opacity(pop.clamp(0.0, 1.0))
        .ty((1.0 - pop) * 30.0);
    kids.push(info);
    kids.push(week(t).absolute().pos(20.0, PH - 190.0));
    // 上の帯 (時刻と電池)。
    kids.push(
        div()
            .absolute()
            .pos(36.0, 26.0)
            .w(Px(PW - 72.0))
            .flex_row()
            .justify_between()
            .child(text("9:41").font_size(18.0).bold().color(ink()))
            .child(div().w(Px(30.0)).h(Px(14.0)).rounded_px(4.0).border(2.0, ink())),
    );
    div()
        .w(Px(PW))
        .h(Px(PH))
        .bg(screen_bg(t))
        .overflow_hidden()
        .children(kids)
        .mask(div().w(Percent(100.0)).h(Percent(100.0)).rounded_px(56.0).bg(Color::BLACK))
}

fn phone(t: f32) -> Element {
    let s = spring_at(t, 0.1, Spring::gentle());
    let out = ease(t, 17.4, 18.6);
    div()
        .absolute()
        .pos(PX - 14.0, PY - 14.0)
        .w(Px(PW + 28.0))
        .h(Px(PH + 28.0))
        .rounded_px(68.0)
        .bg(ink())
        .shadow_md(ink().with_alpha(0.35))
        .p(Px(14.0))
        .child(screen(t))
        .ty((1.0 - s) * 600.0 + out * 40.0)
        .zoom(1.0 - out * 0.12)
        .opacity(1.0 - out)
}

// ── 左の言葉 ─────────────────────────────────────────────────────────────

fn copy(t: f32) -> Element {
    let lines = [
        (0.6, 3.0, "Clouds roll in."),
        (3.2, 9.0, "Rain? It tells you\nwhen it stops."),
        (9.6, 11.6, "Then the sun."),
        (11.8, 17.2, "The whole week,\none glance."),
    ];
    let mut d = div().absolute().pos(150.0, 0.0).w(Px(860.0)).h(Px(H)).justify_center().flex_col();
    for (a, b, words) in lines {
        if t < a - 0.1 || t > b + 0.5 {
            continue;
        }
        let gone = ease(t, b, b + 0.4);
        d = d.child(
            text(words)
                .font_size(84.0)
                .font_weight(800)
                .line_height(1.1)
                .color(ink())
                .per_glyph(|i, _| {
                    let s = spring_at(t, a + i as f32 * 0.025, Spring::bouncy());
                    GlyphMotion {
                        dy: (1.0 - s) * 40.0 - gone * 30.0,
                        opacity: s.clamp(0.0, 1.0) * (1.0 - gone),
                        ..Default::default()
                    }
                }),
        );
    }
    d
}

fn outro(t: f32) -> Element {
    let s = spring_at(t, 18.4, Spring::bouncy());
    let icon_box = div()
        .w(Px(150.0))
        .h(Px(150.0))
        .rounded_px(40.0)
        .bg(hex("#fff6d8"))
        .shadow_md(ink().with_alpha(0.3));
    let mut icon_box = icon_box;
    for e in sun_disc(75.0, 75.0, 30.0, t * 0.5, hex("#f0a91b")) {
        icon_box = icon_box.child(e);
    }
    div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(W))
        .h(Px(H))
        .flex_col()
        .items_center()
        .justify_center()
        .gap(30.0)
        .child(icon_box.scaled(s.max(0.0)).rotate((1.0 - s) * -0.5))
        .child(
            text("hareoto")
                .font_size(110.0)
                .font_weight(900)
                .letter_spacing(-3.0)
                .color(ink())
                .per_glyph(|i, _| {
                    let s = spring_at(t, 18.8 + i as f32 * 0.05, Spring::bouncy());
                    GlyphMotion { dy: (1.0 - s) * 60.0, opacity: s.clamp(0.0, 1.0), ..Default::default() }
                }),
        )
        .child(
            text("weather, at a glance")
                .font_size(36.0)
                .color(ink().with_alpha(0.7))
                .opacity(interpolate(t, [19.4, 20.0], [0.0, 1.0])),
        )
}

fn scene(ctx: FrameCtx) -> Element {
    let t = ctx.t();
    let mut kids: Vec<Element> = vec![div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(W))
        .h(Px(H))
        .bg(ground(t))
        .grain(0.04)
        .grain_seed(1)];
    // 地にも雨 (画面の外)。
    kids.extend(raindrops(t, W, H, 70, 40).into_iter().map(|d| d.opacity(0.6)));
    if t < 18.7 {
        kids.push(phone(t));
        kids.push(copy(t));
    }
    if t > 18.2 {
        kids.push(outro(t));
    }
    let fin = interpolate(t, [21.4, 22.0], [0.0, 1.0]);
    div().w(Px(W)).h(Px(H)).bg(ground(t)).children(kids).opacity(1.0 - fin)
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
        let dir = args.frames_dir("weather");
        let paths = reel.render_pngs(&pick, &dir).expect("render failed");
        for (f, p) in stills.iter().zip(&paths) {
            println!("frame {f} -> {}", p.display());
        }
        return;
    }

    let reel = reel(frames);
    let out = args.mp4("weather");
    println!("rendering + encoding {frames} frames ({}x{} @ {FPS}fps) …", reel.width, reel.height);
    match reel.render_mp4(&scene, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
