//! sabitori-reel — **a made-up indie game studio** (22 s): "BITMOSS", and
//! their made-up game "HOP GARDEN", in the four greens of an old handheld —
//! until the camera pulls back and the handheld turns out to be sitting on a
//! desk full of colour.
//!
//! - the studio logo scrolls down in 8-pixel steps, like a console booting;
//! - a squishy blob (`goo`) hops across a tiled level collecting coins, past a
//!   patrolling alien, with hearts and a score in the corner;
//! - a trophy bounces over a hi-score that counts up;
//! - the title drops in letter by letter (`per_glyph`), snapped to whole
//!   "pixels" so it moves like a sprite;
//! - "PRESS START" blinks, then the picture shrinks (`.zoom`) into the screen
//!   of a handheld, and the desk around it fills with the real thing.
//!
//! The coins, stars, hearts, trophy, alien, controller, joystick and gem are
//! Fluent Emoji (Flat, MIT — see `assets/fluent/LICENSE`) loaded with `Svg`.
//! Inside the game they are recoloured to the four greens (`element_with`
//! rewrites each shape's fill by its brightness); on the desk they keep their
//! own colours.
//!
//!   cargo run -p sabitori-reel --example game_studio --release
//!
//! Output: `target/reel/game_studio.mp4` (`--out <dir>` to change). Frame
//! numbers write just those frames as PNGs.

#[path = "common/out.rs"]
mod out;

use out::Args;
use sabitori_core::element::ElementKind;
use sabitori_core::{goo, Rect};
use sabitori_reel::{
    div, interpolate, interpolate_eased, spring_with, text, Color, Easing, Element, FrameCtx, GlyphMotion, MotionBlur,
    Px, Reel, Spring, Svg,
};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 22.0;

/// ドット 1 つの大きさ (画面の px)。動きもこの刻みに揃える。
const DOT: f32 = 8.0;
/// 床の上端。
const GROUND: f32 = 800.0;

/// 引いた後、ゲームの画面が窓 (ゲーム機の液晶) に収まる倍率。
const SCREEN: f32 = 0.30;
/// 引き始めと引き終わり。
const PULL: (f32, f32) = (17.6, 19.6);

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

// ── 素材 ─────────────────────────────────────────────────────────────────

/// 使う絵 (Fluent Emoji の Flat、MIT)。
struct Art {
    coin: Svg,
    star: Svg,
    heart: Svg,
    trophy: Svg,
    alien: Svg,
    pad: Svg,
    joystick: Svg,
    gem: Svg,
}

impl Art {
    fn load() -> Self {
        let p = |s: &str| Svg::parse(s).expect("SVG が読めない");
        Self {
            coin: p(include_str!("assets/fluent/coin_flat.svg")),
            star: p(include_str!("assets/fluent/star_flat.svg")),
            heart: p(include_str!("assets/fluent/red_heart_flat.svg")),
            trophy: p(include_str!("assets/fluent/trophy_flat.svg")),
            alien: p(include_str!("assets/fluent/alien_monster_flat.svg")),
            pad: p(include_str!("assets/fluent/video_game_flat.svg")),
            joystick: p(include_str!("assets/fluent/joystick_flat.svg")),
            gem: p(include_str!("assets/fluent/gem_stone_flat.svg")),
        }
    }
}

/// 目で見た明るさ (0..1)。
fn lightness(c: Color) -> f32 {
    c.luminance().powf(1.0 / 2.2)
}

/// 色を、その絵の中での明るさで緑に落とす (`lo..hi` はその絵の明るさの幅)。
/// 3 段目 (c2) は地 (c3) とほとんど同じ緑で見えなくなるので使わず、明るい側を c1、
/// 暗い側を c0 の 2 段にする (昔の携帯機の絵も、地の色を入れて 3 色が普通)。
fn to_green(c: Color, lo: f32, hi: f32) -> Color {
    let r = if hi > lo { (lightness(c) - lo) / (hi - lo) } else { 1.0 };
    let g = if r > 0.45 { c1() } else { c0() };
    g.with_alpha(c.a)
}

/// 緑に落とした絵 (ゲームの中の物)。先に 1 度なめて、絵の明るさの幅を測る。
fn green(svg: &Svg, width: f32) -> Element {
    let range = std::cell::Cell::new((1.0f32, 0.0f32));
    let _ = svg.element_with(width, |_, e| {
        if let ElementKind::Path(p) = &e.kind {
            for c in p.fill.iter().chain(p.stroke.iter()) {
                let (lo, hi) = range.get();
                range.set((lo.min(lightness(*c)), hi.max(lightness(*c))));
            }
        }
        e
    });
    let (lo, hi) = range.get();
    svg.element_with(width, |_, mut e| {
        if let ElementKind::Path(p) = &mut e.kind {
            p.fill = p.fill.map(|c| to_green(c, lo, hi));
            p.stroke = p.stroke.map(|c| to_green(c, lo, hi));
        }
        e
    })
}

/// 絵の高さ (幅 `width` で置いたとき)。
fn height_of(svg: &Svg, width: f32) -> f32 {
    let (w, h) = svg.size();
    width * h / w
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

fn play(t: f32, art: &Art) -> Vec<Element> {
    let mut out = vec![ground()];
    for (i, bx) in [260.0, 900.0, 1500.0].into_iter().enumerate() {
        out.push(sprite(&BUSH, DOT).absolute().pos(bx + i as f32 * 20.0, GROUND - BUSH.len() as f32 * DOT));
    }
    // 空の星: ゆっくりまたたく (大きさを 2 段で切り替える)。
    for (i, (sx, sy)) in [(600.0, 250.0), (1180.0, 200.0), (1660.0, 300.0)].into_iter().enumerate() {
        let big = ((t * 2.0 + i as f32 * 0.7) as i32) % 2 == 0;
        let w = if big { 56.0 } else { 40.0 };
        out.push(green(&art.star, w).absolute().pos(snap(sx - w * 0.5), snap(sy - w * 0.5)));
    }
    // 宇宙人: 2 つ目の茂みの先を行ったり来たりする。キャラが上を跳び越える。
    let patrol = (t * 0.9).sin();
    let aw = 96.0;
    out.push(
        green(&art.alien, aw)
            .absolute()
            .pos(snap(1180.0 + patrol * 140.0), GROUND - height_of(&art.alien, aw) - snap(((t * 6.0).sin().abs()) * 8.0)),
    );
    let cw = 64.0;
    for (i, x) in coin_xs().into_iter().enumerate() {
        let got = passes_at(x);
        // 浮いて上下に揺れ、拾われたら跳ね上がって点滅しながら消える。
        let bob = snap((t * 3.0 + i as f32).sin() * 10.0);
        let y = GROUND - 240.0 + bob;
        if t < got {
            out.push(green(&art.coin, cw).absolute().pos(x - cw * 0.5, y));
        } else if t < got + 0.45 {
            let age = t - got;
            if ((age * 16.0) as i32) % 2 == 0 {
                out.push(green(&art.coin, cw).absolute().pos(x - cw * 0.5, snap(y - age * 260.0)));
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
    // 残りの命: ハート 3 つ。
    for k in 0..3 {
        out.push(green(&art.heart, 44.0).absolute().pos(80.0 + k as f32 * 56.0, 132.0));
    }
    out.push(text("WORLD 1-1").mono().bold().font_size(44.0).color(c0()).absolute().pos(W - 400.0, 70.0));
    out
}

// ── 3. 最高点 (9.4〜14 秒) ───────────────────────────────────────────────

fn hiscore(t: f32, art: &Art) -> Vec<Element> {
    // 数はコマの頭の時刻で決める (コマの間の時刻で数が変わると、モーションブラーで
    // 2 つの数が重なって読めなくなる)。
    let at_frame = (t * FPS as f32).floor() / FPS as f32;
    let n = interpolate_eased(at_frame, [9.8, 12.2], [0.0, 98_765.0], Easing::EaseOut) as u32;
    let pop = spring_at(t, 9.5, Spring::bouncy());
    // 優勝杯: 上から落ちてきて弾み、そのあと小さく跳ね続ける。
    let cup = spring_at(t, 9.7, Spring::bouncy());
    let hop = snap(((t * 3.2).sin().abs()) * 18.0 * interpolate(t, [10.6, 11.0], [0.0, 1.0]));
    let tw = 150.0;
    let mut out = vec![
        green(&art.trophy, tw)
            .absolute()
            .pos(W * 0.5 - tw * 0.5, snap(H * 0.5 - 360.0 - (1.0 - cup) * 500.0) - hop),
        div()
            .absolute()
            .pos(0.0, H * 0.5 - 170.0)
            .w(Px(W))
            .flex_col()
            .items_center()
            .gap(24.0)
            .child(text("HI-SCORE").mono().bold().font_size(64.0).letter_spacing(8.0).color(c1()))
            .child(text(format!("{n:06}")).mono().bold().font_size(200.0).color(c0()))
            .ty(snap((1.0 - pop) * 120.0) + 60.0),
    ];
    // きらきら: 決まった場所で、それぞれの間合いで点滅する。
    for i in 0..14u32 {
        let phase = (t * 3.0 + hash(i, 1) * 4.0) as i32;
        if t > 10.0 && phase % 3 == 0 {
            let (x, y) = (snap(160.0 + hash(i, 2) * (W - 320.0)), snap(120.0 + hash(i, 3) * (H - 240.0)));
            // 真ん中 (優勝杯と数字) には出さない。
            if (x - W * 0.5).abs() < 560.0 && (y - H * 0.5).abs() < 360.0 {
                continue;
            }
            out.push(sprite(&SPARKLE, DOT).absolute().pos(x, y));
        }
    }
    out
}

// ── 4. 題名 (14〜) ───────────────────────────────────────────────────────

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

/// PRESS START (16.8 秒から点滅)。
fn press_start(t: f32) -> Element {
    let on = ((t - 16.8) * 2.0) as i32 % 2 == 0;
    div()
        .absolute()
        .pos(0.0, H - 250.0)
        .w(Px(W))
        .flex_row()
        .justify_center()
        .child(text("PRESS START").mono().bold().font_size(64.0).letter_spacing(10.0).color(c0()))
        .opacity(if on { 1.0 } else { 0.0 })
}

/// ゲームの画面 (窓いっぱいの大きさで組む。引くときは丸ごと縮める)。
fn game(t: f32, art: &Art) -> Element {
    let mut kids: Vec<Element> = Vec::new();
    if t < 3.2 {
        kids.extend(boot(t));
    } else if t < PLAY.1 {
        kids.extend(play(t, art));
    } else if t < 14.0 {
        kids.extend(hiscore(t, art));
    } else {
        kids.extend(title(t));
        if t >= 16.8 {
            kids.push(press_start(t));
        }
    }
    // 寄りは絵ごと (字が揺れない `.zoom`)。題名の間だけ、ゆっくり寄る。
    let push = interpolate_eased(t, [14.0, PULL.0], [1.0, 1.04], Easing::EaseInOut);
    div()
        .w(Px(W))
        .h(Px(H))
        .bg(c3())
        .grain(0.04)
        .grain_seed(3)
        .overflow_hidden()
        .child(div().absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H)).children(kids).zoom(push))
}

// ── 5. 引くと机 (17.6〜22 秒) ────────────────────────────────────────────

/// 机の上の物: (絵, 幅, 真ん中の x, y, 傾き)。ゲーム機を囲むように置く。
fn desk_items(art: &Art) -> Vec<(&Svg, f32, f32, f32, f32)> {
    vec![
        (&art.pad, 300.0, 300.0, 300.0, -0.25),
        (&art.joystick, 220.0, 1650.0, 760.0, 0.18),
        (&art.star, 150.0, 1620.0, 240.0, 0.2),
        (&art.heart, 140.0, 250.0, 760.0, -0.15),
        (&art.trophy, 170.0, 1700.0, 470.0, 0.0),
        (&art.gem, 130.0, 420.0, 560.0, 0.3),
        (&art.coin, 100.0, 1480.0, 950.0, -0.2),
        (&art.alien, 150.0, 520.0, 960.0, 0.12),
    ]
}

/// ゲーム機の本体 (引き終わった時の大きさで、画面の真ん中を窓の真ん中に)。
fn handheld(t: f32) -> Element {
    let (sw, sh) = (W * SCREEN, H * SCREEN);
    let (cx, cy) = (W * 0.5, H * 0.5);
    let body = (cx - 380.0, cy - sh * 0.5 - 80.0, 760.0, sh + 380.0);
    let bezel = (cx - sw * 0.5 - 36.0, cy - sh * 0.5 - 36.0, sw + 72.0, sh + 86.0);
    let ink = hex("#2f2a26");
    let shell = hex("#e9e1d3");
    let dpad = |x: f32, y: f32| {
        div()
            .absolute()
            .pos(x, y)
            .w(Px(150.0))
            .h(Px(150.0))
            .child(div().absolute().pos(50.0, 0.0).w(Px(50.0)).h(Px(150.0)).rounded_px(8.0).bg(ink))
            .child(div().absolute().pos(0.0, 50.0).w(Px(150.0)).h(Px(50.0)).rounded_px(8.0).bg(ink))
    };
    let button = |x: f32, y: f32, label: &str| {
        div()
            .absolute()
            .pos(x, y)
            .w(Px(78.0))
            .h(Px(78.0))
            .rounded_px(39.0)
            .bg(hex("#b8325a"))
            .shadow_md(Color::BLACK.with_alpha(0.25))
            .flex_row()
            .items_center()
            .justify_center()
            .child(text(label).font_size(28.0).bold().color(hex("#f4d9e2")))
    };
    // 電源の明かり: 引き終わる頃に点く。
    let led = interpolate(t, [PULL.1 - 0.4, PULL.1], [0.0, 1.0]);
    div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(W))
        .h(Px(H))
        .child(
            div()
                .absolute()
                .pos(body.0, body.1)
                .w(Px(body.2))
                .h(Px(body.3))
                .rounded_px(48.0)
                .bg(shell)
                .shadow_md(Color::BLACK.with_alpha(0.35)),
        )
        .child(div().absolute().pos(bezel.0, bezel.1).w(Px(bezel.2)).h(Px(bezel.3)).rounded_px(22.0).bg(hex("#4a4650")))
        .child(
            div()
                .absolute()
                .pos(bezel.0 + 14.0, bezel.1 + 14.0)
                .w(Px(14.0))
                .h(Px(14.0))
                .rounded_px(7.0)
                .bg(hex("#e8402f").with_alpha(0.25 + 0.75 * led)),
        )
        .child(
            text("BITMOSS  POCKET")
                .mono()
                .bold()
                .font_size(22.0)
                .letter_spacing(4.0)
                .color(hex("#bdb6c4"))
                .absolute()
                .pos(bezel.0 + 30.0, bezel.1 + bezel.3 - 36.0),
        )
        .child(dpad(cx - 290.0, cy + sh * 0.5 + 70.0))
        .child(button(cx + 160.0, cy + sh * 0.5 + 130.0, "B"))
        .child(button(cx + 260.0, cy + sh * 0.5 + 72.0, "A"))
        .child(
            div()
                .absolute()
                .pos(cx - 70.0, cy + sh * 0.5 + 240.0)
                .w(Px(140.0))
                .h(Px(22.0))
                .rounded_px(11.0)
                .bg(hex("#b5ad9f")),
        )
}

/// 机: 暖かい木の色の地と、カラフルな物 (元の色の絵文字)。
fn desk(t: f32, art: &Art) -> Element {
    let mut d = div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(W))
        .h(Px(H))
        .radial_gradient_at(hex("#e8a35e"), hex("#a8562b"), 0.5, 0.45)
        .grain(0.05)
        .grain_seed(9);
    for (i, (svg, w, x, y, tilt)) in desk_items(art).into_iter().enumerate() {
        // 引いて画面に入ってくる頃に、ぽんと弾んで現れ、そのあとゆらゆら。
        let s = spring_at(t, PULL.0 + 0.6 + i as f32 * 0.12, Spring::bouncy());
        let sway = (t * 1.3 + i as f32).sin() * 0.06;
        let float = (t * 1.7 + i as f32 * 0.9).sin() * 8.0;
        let h = height_of(svg, w);
        d = d.child(
            svg.element(w)
                .absolute()
                .pos(x - w * 0.5, y - h * 0.5 + float)
                .rotate(tilt + sway + (1.0 - s) * 0.8)
                .zoom(s.max(0.001))
                .opacity(s.clamp(0.0, 1.0)),
        );
    }
    d.child(handheld(t))
}

fn scene(ctx: FrameCtx, art: &Art) -> Element {
    let t = ctx.t();
    let screen = game(t, art);
    let root = if t < PULL.0 {
        // ゲームの画面だけ (縮めずにそのまま描く: ドットがくっきりする)。
        div().w(Px(W)).h(Px(H)).child(screen)
    } else {
        // カメラを引く: 机は `z` 倍から 1 倍へ、ゲームの画面は `SCREEN × z` 倍へ。どちらも
        // 窓の真ん中を中心に縮めるので、画面はいつもゲーム機の窓にぴったり収まる。
        let p = interpolate_eased(t, [PULL.0, PULL.1], [0.0, 1.0], Easing::EaseInOut);
        let z = (1.0 / SCREEN).powf(1.0 - p);
        div()
            .w(Px(W))
            .h(Px(H))
            .bg(hex("#a8562b"))
            .child(desk(t, art).zoom(z))
            .child(screen.absolute().pos(0.0, 0.0).zoom(SCREEN * z))
    };
    // 終わりは暖かい暗さへ落とす。
    let out = interpolate(t, [21.3, 22.0], [0.0, 1.0]);
    root.child(div().absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H)).bg(hex("#1c120c")).opacity(out))
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
    let art = Art::load();
    let draw = |ctx: FrameCtx| scene(ctx, &art);

    if !args.stills.is_empty() {
        let stills = &args.stills;
        let reel = reel(stills.len() as u32);
        let pick = |ctx: FrameCtx| draw(FrameCtx { frame: stills[ctx.frame as usize], ..ctx });
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
    match reel.render_mp4(&draw, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
