//! sabitori-reel — **a secondhand bookshop** (22 s) for 古書 鳩羽堂, a shop that
//! does not exist. Books and shelves are the stars:
//!
//! - cloth-bound spines slide onto a wooden shelf one by one, their titles set
//!   in vertical type (`.vertical()`);
//! - the camera leans in (`.zoom(..)`, smooth for text), one book is drawn out
//!   of the row and comes forward;
//! - it opens from the spine (`.mask(..)` wiping out from the gutter) onto two
//!   pages of paper made from noise; a bird is sketched on the left page with a
//!   pen line (`path().trim(..)`), and the shop's notice is inked down the right
//!   page character by character (`per_glyph`, with a blurred under-copy as the
//!   bleed);
//! - foxing stains and a red seal are multiplied onto the paper
//!   (`BlendMode::Multiply`).
//!
//! The face is Hiragino Mincho (macOS); elsewhere the system serif is used.
//!
//!   cargo run -p sabitori-reel --example old_books --release
//!
//! Output: `target/reel/old_books.mp4` (`--out <dir>` to change). Frame
//! numbers write just those frames as PNGs.

#[path = "common/out.rs"]
mod out;

use out::Args;
use sabitori_reel::{
    div, image, interpolate, interpolate_eased, path, spring_with, text, BlendMode, Color, Easing, Element,
    FrameCtx, GlyphMotion, ImageData, MotionBlur, Percent, Px, Reel, Spring,
};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 22.0;

/// 開いた本の 1 頁の大きさ。
const PW: f32 = 560.0;
const PH: f32 = 760.0;

/// 引き出す本 (棚の上の段の、この番号)。
const PICKED: usize = 6;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
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

/// 古い紙: 黄ばんだ地に、ゆるいむらと、縁ほど濃い焼け。
fn old_paper(w: u32, h: u32) -> ImageData {
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (x as f32, y as f32);
            let cloud = value_noise(u / 140.0, v / 140.0, 3) * 0.6 + value_noise(u / 30.0, v / 30.0, 4) * 0.4;
            // 縁からの近さ (0 = 真ん中, 1 = 縁)。
            let ex = (u / w as f32 - 0.5).abs() * 2.0;
            let ey = (v / h as f32 - 0.5).abs() * 2.0;
            let edge = ex.max(ey).powf(6.0);
            let fiber = ((value_noise(u / 2.5 + v / 30.0, v / 2.5, 9) - 0.85) * 5.0).max(0.0);
            let l = 0.90 + (cloud - 0.5) * 0.06 - edge * 0.12 - fiber * 0.04;
            let c = [l, l * 0.94, l * 0.80];
            let i = ((y * w + x) * 4) as usize;
            for k in 0..3 {
                px[i + k] = (c[k].clamp(0.0, 1.0) * 255.0) as u8;
            }
            px[i + 3] = 255;
        }
    }
    ImageData::new(px, w, h)
}

/// 板目: 横に流れる木の筋。
fn wood(w: u32, h: u32) -> ImageData {
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let (u, v) = (x as f32, y as f32);
            let grain = value_noise(u / 260.0, v / 7.0, 11) * 0.7 + value_noise(u / 40.0, v / 2.0, 12) * 0.3;
            let l = 0.18 + grain * 0.14;
            let c = [l * 1.35, l * 0.95, l * 0.62];
            let i = ((y * w + x) * 4) as usize;
            for k in 0..3 {
                px[i + k] = (c[k].clamp(0.0, 1.0) * 255.0) as u8;
            }
            px[i + 3] = 255;
        }
    }
    ImageData::new(px, w, h)
}

// ── 棚 ───────────────────────────────────────────────────────────────────

/// 1 冊の背: (幅, 高さ, 布の色, 題)。
struct Spine {
    w: f32,
    h: f32,
    cloth: Color,
    title: &'static str,
}

fn spines() -> Vec<Spine> {
    let cloths = ["#6e2a24", "#2f4a3a", "#8a6a2e", "#3a3f52", "#5b4a3a", "#7a3b2e", "#2c3a2e", "#9a7b4f", "#4a2a2a"];
    let titles = [
        "海辺の記録", "灯の歳時記", "雨の手紙", "山小屋日誌", "古い港町", "草木図譜", "旅の手帖", "夜の星座", "町の地図帳",
        "冬の台所", "川の名前", "紙と墨", "鳥のこよみ", "坂道の家", "遠い汽笛",
    ];
    titles
        .iter()
        .enumerate()
        .map(|(i, t)| Spine {
            w: 62.0 + hash(i as i32, 0, 5) * 36.0,
            h: 300.0 + hash(i as i32, 1, 5) * 80.0,
            cloth: hex(cloths[i % cloths.len()]),
            title: t,
        })
        .collect()
}

/// 背表紙 1 冊: 布の地、上下の金の帯、縦書きの題。
fn spine_el(s: &Spine) -> Element {
    let gold = hex("#d8b46a");
    let band = |y: f32| div().absolute().pos(6.0, y).w(Px(s.w - 12.0)).h(Px(3.0)).bg(gold.with_alpha(0.8));
    div()
        .w(Px(s.w))
        .h(Px(s.h))
        .rounded_px(4.0)
        .bg(s.cloth)
        .grain(0.06)
        .grain_seed(2)
        .child(band(22.0))
        .child(band(30.0))
        .child(band(s.h - 33.0))
        .child(band(s.h - 25.0))
        .child(
            div()
                .absolute()
                .pos(0.0, 52.0)
                .w(Px(s.w))
                .h(Px(s.h - 104.0))
                .flex_row()
                .justify_center()
                .child(text(s.title).vertical().font_size(24.0).letter_spacing(4.0).color(gold)),
        )
}

/// 棚の上の段の本の左端 x と、下の段の基準線 (板の上面) y。
fn shelf_layout(books: &[Spine]) -> (Vec<f32>, f32) {
    let total: f32 = books.iter().map(|b| b.w + 4.0).sum();
    // 引き出す本が画面の真ん中に来るように並べる。
    let before: f32 = books[..PICKED].iter().map(|b| b.w + 4.0).sum();
    let start = W * 0.5 - before - books[PICKED].w * 0.5;
    let _ = total;
    let mut xs = Vec::new();
    let mut x = start;
    for b in books {
        xs.push(x);
        x += b.w + 4.0;
    }
    (xs, 700.0)
}

fn shelf(t: f32, books: &[Spine], wood_img: &ImageData) -> Element {
    let (xs, base) = shelf_layout(books);
    let mut kids: Vec<Element> = Vec::new();
    // 奥の壁と板。
    kids.push(image("wood-back", wood_img.clone()).absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H)).opacity(0.55));
    kids.push(div().absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H)).bg(hex("#120d0a").with_alpha(0.55)));
    kids.push(image("wood-plank", wood_img.clone()).absolute().pos(0.0, base).w(Px(W)).h(Px(46.0)));
    kids.push(div().absolute().pos(0.0, base + 46.0).w(Px(W)).h(Px(18.0)).bg(Color::BLACK.with_alpha(0.45)).blur(8.0));
    let pull = ease(t, 6.2, 7.4);
    for (i, b) in books.iter().enumerate() {
        // 右から順に滑り込む。
        let s = spring_at(t, 0.3 + i as f32 * 0.12, Spring::gentle());
        let x = xs[i] + (1.0 - s) * (W + 200.0 - xs[i]);
        let mut el = spine_el(b).absolute().pos(x, base - b.h);
        if i == PICKED {
            // 引き出す本は、少し上へ持ち上がってから手前へ (棚から離れる)。
            el = el.ty(-pull * 60.0);
        }
        kids.push(el);
    }
    div().absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H)).children(kids)
}

// ── 開いた本 ─────────────────────────────────────────────────────────────

/// 左の頁に、ペンで描かれていく鳩。
fn bird(t: f32) -> Element {
    let draw = interpolate_eased(t, [10.4, 12.6], [0.0, 1.0], Easing::EaseInOut);
    let ink = hex("#2a2420");
    let body = "M40 150 C60 95 120 80 170 92 C200 70 240 70 262 92 C250 98 246 108 248 118 C290 130 300 170 270 200 \
                C220 240 120 236 70 200 C55 190 45 172 40 150 Z";
    let wing = "M120 140 C150 120 200 120 230 150 C205 165 160 172 120 160";
    let legs = "M150 232 L145 262 M175 232 L180 262";
    let tail = "M48 158 L6 128 L18 166 L2 184 L50 176";
    let beak = "M262 92 L290 102 L264 108";
    let mk = |d: &str, w: f32| {
        path()
            .svg_path(d)
            .view_box(0.0, 0.0, 320.0, 300.0)
            .stroke(ink, w)
            .trim(0.0, draw)
            .w(Px(380.0))
            .h(Px(356.0))
            .absolute()
            .pos(0.0, 0.0)
    };
    div()
        .w(Px(380.0))
        .h(Px(356.0))
        .child(mk(body, 3.0))
        .child(mk(wing, 2.5))
        .child(mk(legs, 2.5))
        .child(mk(tail, 2.5))
        .child(mk(beak, 2.5))
        .child(
            div()
                .absolute()
                .pos(260.0 * 380.0 / 320.0 - 4.0, 96.0 * 356.0 / 300.0 - 4.0)
                .w(Px(8.0))
                .h(Px(8.0))
                .rounded_px(4.0)
                .bg(ink)
                .opacity(interpolate(t, [12.4, 12.7], [0.0, 1.0])),
        )
}

/// 案内文。右の列から読む。
const NOTICE: &str = "古書　鳩羽堂\n一冊ずつ手で拭いて、\n棚に戻しております。\n探している本は、\nどうぞお尋ねください。\n正午から日暮れまで。";
const INK_START: f32 = 12.6;
const INK_STEP: f32 = 0.085;

/// 字 `i` (改行も数える) を書き始める秒。改行の後は少し間を置く。
fn ink_times() -> Vec<f32> {
    let mut at = INK_START;
    NOTICE
        .chars()
        .map(|c| {
            let s = at;
            at += match c {
                '\n' => 0.25,
                '、' | '。' => INK_STEP + 0.2,
                _ => INK_STEP,
            };
            s
        })
        .collect()
}

fn notice(t: f32, times: &[f32], bleed: bool) -> Element {
    let ink = hex("#231c17");
    let el = text(NOTICE)
        .vertical()
        .font_size(38.0)
        .font_weight(600)
        .letter_spacing(6.0)
        .line_height(1.7)
        .h(Px(560.0))
        .color(ink)
        .per_glyph(|i, _| {
            let age = t - times[i];
            if age < 0.0 {
                return GlyphMotion { opacity: 0.0, ..Default::default() };
            }
            if bleed {
                GlyphMotion {
                    scale: 1.0 + 0.3 * (-age * 3.0).exp(),
                    opacity: 0.14 + 0.5 * (-age * 2.4).exp(),
                    ..Default::default()
                }
            } else {
                let s = interpolate_eased(age, [0.0, 0.3], [0.0, 1.0], Easing::EaseOutCubic);
                GlyphMotion { scale: 1.06 - 0.06 * s, opacity: 0.3 + 0.7 * s, ..Default::default() }
            }
        });
    if bleed {
        el.blur(5.0)
    } else {
        el
    }
}

/// 紙の染み (茶色のにじみ) を掛け合わせる。
fn foxing(t: f32) -> Vec<Element> {
    let a = interpolate(t, [17.6, 19.4], [0.0, 1.0]);
    let spots = [(80.0, 600.0, 90.0), (470.0, 90.0, 60.0), (300.0, 680.0, 40.0), (40.0, 120.0, 30.0)];
    spots
        .iter()
        .map(|&(x, y, r)| {
            div()
                .absolute()
                .pos(x - r, y - r)
                .w(Px(r * 2.0))
                .h(Px(r * 2.0))
                .radial_gradient(hex("#c9a46a").with_alpha(0.55 * a), hex("#c9a46a").with_alpha(0.0))
                .blend(BlendMode::Multiply)
        })
        .collect()
}

fn page(paper: &ImageData, key: &str, left: bool) -> Element {
    // 綴じ目の側を暗くする影。
    let gutter = if left {
        div().absolute().pos(PW - 90.0, 0.0).w(Px(90.0)).h(Px(PH)).gradient(
            Color::TRANSPARENT,
            Color::BLACK.with_alpha(0.28),
            0.0,
        )
    } else {
        div().absolute().pos(0.0, 0.0).w(Px(90.0)).h(Px(PH)).gradient(Color::BLACK.with_alpha(0.28), Color::TRANSPARENT, 0.0)
    };
    div()
        .w(Px(PW))
        .h(Px(PH))
        .child(image(key.to_string(), paper.clone()).absolute().pos(0.0, 0.0).w(Px(PW)).h(Px(PH)))
        .child(gutter)
}

fn open_book(t: f32, paper: &ImageData, times: &[f32]) -> Element {
    // 綴じ目から両側へ開く: 真ん中から広がるマスク。
    let open = ease(t, 8.6, 10.0);
    let half = (PW + 16.0) * open;
    let mask = div()
        .w(Percent(100.0))
        .h(Percent(100.0))
        .child(div().absolute().pos(PW + 8.0 - half, 0.0).w(Px(half * 2.0)).h(Px(PH + 24.0)).bg(Color::BLACK));
    let left = page(paper, "page-l", true)
        .absolute()
        .pos(8.0, 12.0)
        .child(bird(t).absolute().pos(90.0, 190.0))
        .children(foxing(t));
    let right = page(paper, "page-r", false)
        .absolute()
        .pos(PW + 8.0, 12.0)
        .child(notice(t, times, true).absolute().pos(70.0, 90.0))
        .child(notice(t, times, false).absolute().pos(70.0, 90.0));
    // 落款。
    let press = spring_at(t, 18.6, Spring::bouncy());
    let mut book = div()
        .w(Px(PW * 2.0 + 16.0))
        .h(Px(PH + 24.0))
        .rounded_px(10.0)
        // 表紙 (布) が頁の外に少し見える。
        .bg(hex("#2c3a2e"))
        .child(left)
        .child(right)
        .child(div().absolute().pos(PW + 6.0, 12.0).w(Px(4.0)).h(Px(PH)).bg(Color::BLACK.with_alpha(0.25)));
    if t > 18.6 {
        let seal = div()
            .w(Px(92.0))
            .h(Px(92.0))
            .rounded_px(8.0)
            .bg(hex("#b8322a"))
            .grain(0.2)
            .grain_seed(9)
            .flex_row()
            .items_center()
            .justify_center()
            .gap(2.0)
            .child(text("羽").vertical().font_size(34.0).color(Color::WHITE))
            .child(text("鳩").vertical().font_size(34.0).color(Color::WHITE));
        book = book.child(
            seal.absolute()
                .pos(PW + 8.0 + 70.0, 12.0 + PH - 170.0)
                .scaled(1.5 - 0.5 * press)
                .rotate(0.06)
                .opacity((press * 1.4).clamp(0.0, 0.9))
                .blend(BlendMode::Multiply),
        );
    }
    book.mask(mask)
}

// ── 場面 ─────────────────────────────────────────────────────────────────

struct Assets {
    paper: ImageData,
    wood: ImageData,
    books: Vec<Spine>,
    times: Vec<f32>,
}

fn scene(ctx: FrameCtx, a: &Assets) -> Element {
    let t = ctx.t();
    let mut kids: Vec<Element> = vec![div().absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H)).bg(hex("#140f0c"))];

    // 棚: 3〜7.6 秒でゆっくり寄り、7.4 秒から暗くぼけて退く。
    if t < 9.0 {
        let lean = ease(t, 3.0, 7.6);
        let leave = ease(t, 7.4, 8.6);
        kids.push(
            shelf(t, &a.books, &a.wood)
                .zoom(1.0 + lean * 0.35)
                .blur(leave * 14.0)
                .opacity(1.0 - leave * 0.85),
        );
        // 屋号: 棚の上に。
        let name_in = spring_at(t, 1.8, Spring::gentle());
        kids.push(
            div()
                .absolute()
                .pos(0.0, 150.0)
                .w(Px(W))
                .items_center()
                .flex_col()
                .gap(10.0)
                .child(text("古書　鳩羽堂").font_size(64.0).font_weight(600).letter_spacing(18.0).color(hex("#e9dcc3")))
                .child(text("HATOBA-DŌ  SECONDHAND BOOKS").font_size(20.0).letter_spacing(8.0).color(hex("#a8957a")))
                .opacity((name_in * (1.0 - ease(t, 3.4, 4.2))).clamp(0.0, 1.0))
                .ty((1.0 - name_in) * 20.0),
        );
    }

    // 引き出した本が手前に来て、開く。
    if (7.2..9.2).contains(&t) {
        let come = ease(t, 7.2, 8.8);
        let b = &a.books[PICKED];
        let k = 1.0 + come * 1.4;
        kids.push(
            spine_el(b)
                .absolute()
                .pos(W * 0.5 - b.w * 0.5, H * 0.5 - b.h * 0.5 + (1.0 - come) * 60.0)
                .zoom(k)
                .opacity(1.0 - ease(t, 8.6, 9.2)),
        );
    }
    if t > 8.5 {
        let settle = ease(t, 8.5, 9.6);
        // 開いた後はほんの少しずつ寄る (字が揺れないよう zoom で)。
        let drift = ease(t, 10.0, SECS);
        kids.push(
            open_book(t, &a.paper, &a.times)
                .absolute()
                .pos((W - PW * 2.0 - 16.0) * 0.5, (H - PH - 24.0) * 0.5)
                .zoom(0.96 + settle * 0.04 + drift * 0.035)
                .opacity(settle),
        );
        // 本の影。
    }

    let fin = interpolate(t, [21.3, 22.0], [0.0, 1.0]);
    div()
        .w(Px(W))
        .h(Px(H))
        .bg(hex("#140f0c"))
        .children(kids)
        // 四隅を落とす。
        .child(
            div()
                .absolute()
                .pos(0.0, 0.0)
                .w(Px(W))
                .h(Px(H))
                .radial_gradient(Color::TRANSPARENT, Color::BLACK.with_alpha(0.45)),
        )
        .opacity(1.0 - fin)
}

fn reel(frames: u32) -> Reel {
    Reel {
        motion_blur: Some(MotionBlur::new(5)),
        family: Some("ヒラギノ明朝 ProN".into()),
        crf: 24,
        ..Reel::new(W as u32, H as u32, FPS, frames)
    }
}

fn main() {
    let frames = (SECS * FPS as f32).round() as u32;
    let args = Args::parse();
    let assets =
        Assets { paper: old_paper(PW as u32, PH as u32), wood: wood(960, 540), books: spines(), times: ink_times() };
    let draw = |ctx: FrameCtx| scene(ctx, &assets);

    if !args.stills.is_empty() {
        let stills = &args.stills;
        let reel = reel(stills.len() as u32);
        let pick = |ctx: FrameCtx| draw(FrameCtx { frame: stills[ctx.frame as usize], ..ctx });
        let dir = args.frames_dir("old_books");
        let paths = reel.render_pngs(&pick, &dir).expect("render failed");
        for (f, p) in stills.iter().zip(&paths) {
            println!("frame {f} -> {}", p.display());
        }
        return;
    }

    let reel = reel(frames);
    let out = args.mp4("old_books");
    println!("rendering + encoding {frames} frames ({}x{} @ {FPS}fps) …", reel.width, reel.height);
    match reel.render_mp4(&draw, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
