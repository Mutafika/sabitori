//! sabitori-reel — an **app-promo opener**: a photo-like backdrop, with a
//! gradient title, translucent cards and a goo pill that grows into a panel,
//! all sliding in *on top of* the image.
//!
//! The backdrop is an ordinary `image(key, ImageData)` (its pixels are made
//! procedurally below — a dusk sky over layered ridges), and everything after
//! it in the tree simply covers it: since v0.41 Sabitori draws in **tree
//! order**, so a box or a line of text placed over an image or over other text
//! needs no `.layer(..)`.
//!
//! What it exercises: `image` (with a slow Ken Burns push), `color_spans` for a
//! per-character gradient title, `spring_with` for the entrances, and
//! `goo(..)` with `goo_gradient` / `goo_border` / `goo_shadow` for the pill →
//! panel morph. Pure function of the frame.
//!
//!   cargo run -p sabitori-reel --example intro --release
//!
//! Output: `target/reel/intro.mp4` (`--out <dir>` to change). Pass frame numbers
//! (`-- 0 60 120`) to write just those frames as PNGs instead.

#[path = "common/out.rs"]
mod out;

use out::Args;
use sabitori_core::{goo, BoxShadow, Point, Rect};
use sabitori_reel::{
    MotionBlur,
    div, image, interpolate, interpolate_color, interpolate_eased, spring_with, text, Color,
    Easing, Element, FrameCtx, ImageData, Px, Reel, Spring,
};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 8.0;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}

/// `t` 秒に `start` 秒から始まるばねの値 (始まる前は 0)。
fn spring_at(t: f32, start: f32, cfg: Spring) -> f32 {
    if t < start {
        0.0
    } else {
        spring_with(t - start, FPS, cfg)
    }
}

// ── 背景の「写真」 ─────────────────────────────────────────────────────────────

/// 夕暮れの空と、遠いほど霞む稜線を 4 枚重ねた絵を、画素から作る。
/// 一度だけ作って `ImageData` (中身は `Arc`) を毎コマ使い回す。
fn backdrop(w: u32, h: u32) -> ImageData {
    // 遠い順。(基準の高さ 0..1, 揺れ幅, 色)
    let ridges: [(f32, f32, [f32; 3]); 4] = [
        (0.60, 0.10, [0.58, 0.46, 0.64]),
        (0.68, 0.10, [0.40, 0.31, 0.50]),
        (0.76, 0.09, [0.23, 0.18, 0.34]),
        (0.86, 0.07, [0.10, 0.08, 0.17]),
    ];
    let lerp3 = |a: [f32; 3], b: [f32; 3], t: f32| {
        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
    };
    // 三角波を周波数違いで重ねると、斜面がまっすぐで頂が尖った山並みになる。
    let tri = |x: f32| 1.0 - 2.0 * (x.rem_euclid(1.0) - 0.5).abs();
    let (sun_x, sun_y) = (0.58, 0.60);
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        let v = y as f32 / h as f32;
        // 空: 上は群青、地平線へ向けて桃色から橙へ。
        let sky = if v < 0.35 {
            lerp3([0.08, 0.09, 0.25], [0.40, 0.28, 0.52], v / 0.35)
        } else {
            lerp3([0.40, 0.28, 0.52], [1.0, 0.66, 0.50], ((v - 0.35) / 0.25).min(1.0))
        };
        for x in 0..w {
            let u = x as f32 / w as f32;
            let mut c = sky;
            // 太陽の光暈 (稜線より奥なので、先に足す)。
            let d = ((u - sun_x) * 1.78).hypot(v - sun_y);
            let glow = (-d * d * 18.0).exp() * 0.6 + (-d * d * 700.0).exp() * 0.7;
            c = lerp3(c, [1.0, 0.88, 0.70], glow.min(1.0));
            for (i, &(base, amp, col)) in ridges.iter().enumerate() {
                let k = i as f32 + 1.0;
                let ridge = base
                    - amp * tri(u * (1.3 + 0.6 * k) + k * 0.37) * 0.65
                    - amp * tri(u * (3.7 + 1.1 * k) + k * 0.61) * 0.25
                    - amp * tri(u * 11.0 + k * 0.23) * 0.10;
                if v > ridge {
                    // 稜線の下ほど濃く、上端は空の色に少し溶かす (大気の遠近)。
                    let depth = ((v - ridge) * 5.0).min(1.0);
                    c = lerp3(lerp3(col, sky, 0.3 * (1.0 - depth)), col, depth * 0.5);
                    // 太陽に近い稜線の縁だけ、細く光らせる。
                    let rim = (1.0 - (v - ridge) * 400.0).max(0.0) * (-(u - sun_x).powi(2) * 9.0).exp();
                    c = lerp3(c, [1.0, 0.80, 0.62], rim * 0.55 / k);
                }
            }
            // 帯 (バンディング) を消すための、ごく弱いざらつき。
            let n = ((x.wrapping_mul(73_856_093) ^ y.wrapping_mul(19_349_663)) % 1000) as f32 / 1000.0;
            let i = ((y * w + x) * 4) as usize;
            for (ch, &val) in c.iter().enumerate() {
                px[i + ch] = ((val + (n - 0.5) * 0.012).clamp(0.0, 1.0) * 255.0) as u8;
            }
            px[i + 3] = 255;
        }
    }
    ImageData::new(px, w, h)
}

// ── 部品 ───────────────────────────────────────────────────────────────────────

/// 1 字ずつ色を変えた題字。`color_spans` は byte 範囲なので、ASCII 前提で 1 字 = 1 byte。
fn gradient_title(s: &str, size: f32, stops: [Color; 3], shift: f32) -> Element {
    let n = s.len().max(2) as f32 - 1.0;
    let spans: Vec<(std::ops::Range<usize>, Color)> = (0..s.len())
        .map(|i| {
            let p = (i as f32 / n + shift).rem_euclid(2.0);
            let p = if p > 1.0 { 2.0 - p } else { p }; // 端で折り返して往復させる
            let c = if p < 0.5 {
                stops[0].lerp(stops[1], p * 2.0)
            } else {
                stops[1].lerp(stops[2], (p - 0.5) * 2.0)
            };
            (i..i + 1, c)
        })
        .collect();
    text(s.to_string()).font_size(size).font_weight(800).letter_spacing(-2.0).color(stops[0]).color_spans(spans)
}

/// 背景の写真に重ねる半透明のカード。
fn card(icon: Color, title: &str, body: &str) -> Element {
    div()
        .w(Px(500.0))
        .flex_row()
        .items_center()
        .gap(22.0)
        .p_px(26.0)
        .rounded_px(22.0)
        .bg(hex("#0d0f22").with_alpha(0.55))
        .border(1.0, Color::WHITE.with_alpha(0.16))
        .shadow(BoxShadow { color: hex("#05060f").with_alpha(0.45), offset: Point::new(0.0, 18.0), blur: 40.0, spread: 0.0 })
        .child(
            div()
                .w(Px(52.0))
                .h(Px(52.0))
                .rounded_px(15.0)
                .gradient(icon, icon.darken(0.25), 0.8)
                .glow(icon.with_alpha(0.45), 18.0),
        )
        .child(
            div()
                .flex_col()
                .gap(6.0)
                .shrink(1.0)
                .child(text(title.to_string()).font_size(26.0).font_weight(700).color(hex("#f4f2ff")))
                .child(text(body.to_string()).font_size(19.0).color(hex("#c4c0dd"))),
        )
}

// ── 場面 ───────────────────────────────────────────────────────────────────────

fn scene(ctx: FrameCtx, photo: &ImageData) -> Element {
    let t = ctx.t();
    let mut kids: Vec<Element> = Vec::new();

    // 写真: ゆっくり寄りながら少し右へ流す (Ken Burns)。
    let zoom = interpolate_eased(t, [0.0, SECS], [1.04, 1.12], Easing::EaseInOut);
    let (iw, ih) = (W * zoom, H * zoom);
    let drift = interpolate(t, [0.0, SECS], [0.0, 1.0]);
    kids.push(
        image("intro-backdrop", photo.clone())
            .absolute()
            .pos(-(iw - W) * (0.65 - 0.3 * drift), -(ih - H) * 0.5)
            .w(Px(iw))
            .h(Px(ih)),
    );
    // 左側を暗くする幕 (題字を読ませる)。写真の上に普通の箱として重ねるだけ。
    kids.push(
        div().absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H)).gradient(
            hex("#070818").with_alpha(0.78),
            hex("#070818").with_alpha(0.0),
            0.0,
        ),
    );

    // 題字: 左からばねで滑り込む。色の帯は時間で少しずつ流れる。
    let s_title = spring_at(t, 0.3, Spring::snappy());
    let stops = [hex("#7dd3fc"), hex("#c4b5fd"), hex("#fda4af")];
    kids.push(
        div()
            .absolute()
            .pos(150.0, 300.0)
            .flex_col()
            .gap(18.0)
            .tx((1.0 - s_title) * -140.0)
            // ぼけた所からピントが合う。`.blur(..)` は中身 (色を変えた字ごと) ぼかす。
            .blur((1.0 - s_title).max(0.0) * 24.0)
            .opacity(s_title.min(1.0))
            .child(
                text("SABITORI · REEL")
                    .font_size(22.0)
                    .font_weight(700)
                    .letter_spacing(6.0)
                    .color(hex("#a5b4fc")),
            )
            .child(gradient_title("Motion, in code.", 128.0, stops, t * 0.12))
            .child(
                text("An Element tree per frame, rendered on the GPU — offscreen.")
                    .font_size(32.0)
                    .color(hex("#dcd8f0"))
                    .opacity(interpolate(t, [0.8, 1.4], [0.0, 1.0])),
            ),
    );

    // カード 3 枚: 時間をずらして、傾いた状態から起き上がりつつ下から。写真にも、
    // 互いの影にも重なる。`.rotate(..)` は中身 (文字・光る点) ごと回す。
    let cards = [
        ("#5eead4", "A pure function of the frame", "Scrub backwards, land on the same pixels."),
        ("#a78bfa", "Tree-order drawing", "Boxes over images and text just cover them."),
        ("#fb923c", "PNG sequence or MP4", "Straight to ffmpeg, no window needed."),
    ];
    for (i, (icon, title, body)) in cards.iter().enumerate() {
        let s = spring_at(t, 1.3 + i as f32 * 0.22, Spring::bouncy());
        kids.push(
            card(hex(icon), title, body)
                .absolute()
                .pos(1270.0, 200.0 + i as f32 * 150.0)
                .ty((1.0 - s) * 90.0)
                .rotate((1.0 - s) * 0.22)
                .opacity(s.clamp(0.0, 1.0)),
        );
    }

    // goo: 小さな錠剤から、下へ雫が垂れて板に育つ。
    let (gx, gy) = (150.0, 640.0);
    let pill = Rect::new(0.0, 0.0, 250.0, 68.0);
    let s_pill = spring_at(t, 3.0, Spring::bouncy());
    // 2 段: まず雫が首を伸ばして下へ垂れ (drip)、着いたらばねで板に広がる (grow)。
    // 首 (smooth) は広がりきってから細り、最後は錠剤と板が離れる。
    let drip = interpolate_eased(t, [3.7, 4.5], [0.0, 1.0], Easing::EaseInOut);
    let grow = spring_at(t, 4.35, Spring::snappy());
    let body = Rect::new(
        interpolate(grow, [0.0, 1.0], [45.0, 0.0]),
        interpolate(drip, [0.0, 1.0], [10.0, 92.0]),
        interpolate(grow, [0.0, 1.0], [160.0, 760.0]),
        interpolate(grow, [0.0, 1.0], [52.0, 196.0]),
    );
    if s_pill > 0.001 {
        kids.push(
            goo(pill, body)
                .absolute()
                .pos(gx, gy)
                .goo_radii(34.0, interpolate(grow, [0.0, 1.0], [26.0, 28.0]))
                .goo_smooth(interpolate_eased(t, [4.9, 5.6], [64.0, 20.0], Easing::EaseInOut))
                .goo_gradient(hex("#312e81"), hex("#1e1b4b"))
                .goo_border(1.5, hex("#a5b4fc").with_alpha(0.55))
                .goo_shadow(BoxShadow { color: hex("#020313").with_alpha(0.55), offset: Point::new(0.0, 20.0), blur: 44.0, spread: 0.0 })
                .opacity(s_pill.clamp(0.0, 1.0))
                .ty((1.0 - s_pill) * 40.0),
        );
        // 錠剤の中身 (goo より後の兄弟なので、その上に描かれる)。
        kids.push(
            div()
                .absolute()
                .pos(gx, gy)
                .w(Px(pill.size.width))
                .h(Px(pill.size.height))
                .flex_row()
                .items_center()
                .justify_center()
                .gap(14.0)
                .opacity(s_pill.clamp(0.0, 1.0))
                .ty((1.0 - s_pill) * 40.0)
                .child(div().w(Px(14.0)).h(Px(14.0)).rounded_px(7.0).bg(hex("#5eead4")).glow(hex("#5eead4"), 10.0))
                .child(text("Render").font_size(26.0).font_weight(700).color(hex("#eef0ff"))),
        );
    }

    // 板の中身: 書き出しの進み具合。板が育ちきる頃に現れる。
    let panel_in = interpolate(t, [4.7, 5.1], [0.0, 1.0]);
    if panel_in > 0.0 {
        let done = interpolate_eased(t, [4.9, 7.2], [0.0, 1.0], Easing::EaseInOut);
        let frames = (done * 240.0).round() as u32;
        let bar_w = 680.0;
        let bar_col = interpolate_color(done, [0.0, 1.0], hex("#7dd3fc"), hex("#5eead4"));
        kids.push(
            div()
                .absolute()
                .pos(gx + 40.0, gy + 92.0 + 36.0)
                .w(Px(bar_w))
                .flex_col()
                .gap(20.0)
                .opacity(panel_in)
                .ty((1.0 - panel_in) * 12.0)
                .child(
                    div()
                        .flex_row()
                        .justify_between()
                        .items_center()
                        .child(text("intro.mp4").font_size(30.0).font_weight(700).color(hex("#f4f2ff")))
                        .child(
                            text(if done >= 1.0 { "Done".to_string() } else { format!("{:.0}%", done * 100.0) })
                                .font_size(28.0)
                                .font_weight(700)
                                .color(bar_col),
                        ),
                )
                .child(
                    div()
                        .w(Px(bar_w))
                        .h(Px(12.0))
                        .rounded_px(6.0)
                        .bg(Color::WHITE.with_alpha(0.12))
                        .child(
                            div()
                                .w(Px((bar_w * done).max(12.0)))
                                .h(Px(12.0))
                                .rounded_px(6.0)
                                .gradient(hex("#7dd3fc"), bar_col, 0.0)
                                .glow(bar_col.with_alpha(0.5), 12.0),
                        ),
                )
                .child(
                    text(format!("{frames} / 240 frames  ·  1920 × 1080  ·  30 fps"))
                        .font_size(22.0)
                        .color(hex("#b9b6d6")),
                ),
        );
    }

    // 終わりに少し暗転。
    let fade = interpolate(t, [SECS - 0.5, SECS], [0.0, 1.0]);
    if fade > 0.0 {
        kids.push(div().absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H)).bg(hex("#05060f")).opacity(fade));
    }

    div().w(Px(W)).h(Px(H)).bg(hex("#070818")).children(kids)
}

/// 速い動き (入場・伸びる線) が滑らかに見えるよう、1 コマを 8 回描いて平均する。
fn reel(frames: u32) -> Reel {
    Reel { motion_blur: Some(MotionBlur::new(8)), ..Reel::new(W as u32, H as u32, FPS, frames) }
}

fn main() {
    let frames = (SECS * FPS as f32).round() as u32;
    let photo = backdrop(W as u32, H as u32);
    let args = Args::parse();
    let stills = &args.stills;

    if !stills.is_empty() {
        // 指定のコマだけ PNG に書く (見た目の確認用)。
        let reel = reel(stills.len() as u32);
        let pick = |ctx: FrameCtx| scene(FrameCtx { frame: stills[ctx.frame as usize], ..ctx }, &photo);
        let dir = args.frames_dir("intro");
        let paths = reel.render_pngs(&pick, &dir).expect("render failed");
        for (f, p) in stills.iter().zip(&paths) {
            println!("frame {f} -> {}", p.display());
        }
        return;
    }

    let reel = reel(frames);
    let out = args.mp4("intro");
    println!("rendering + encoding {frames} frames ({}x{} @ {FPS}fps) …", reel.width, reel.height);
    match reel.render_mp4(&|ctx| scene(ctx, &photo), &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
