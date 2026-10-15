//! sabitori-reel — a **30-second promo** that is also the tutorial: the code
//! on the left is typed out line by line, and the panel on the right plays what
//! that code draws. Made for a landing page (H.264 MP4 + a poster PNG).
//!
//! 1. *Write a scene* — a function from the frame to an `Element` tree.
//! 2. *Animate anything* — `.per_glyph`, `.rotate`, `.blur`, `.mask`,
//!    `path().trim(..)`; each line lights up as the preview shows it.
//! 3. *Render* — `cargo run`, a progress bar, an MP4.
//!
//!   cargo run -p sabitori-reel --example promo --release
//!
//! Output: `target/reel/promo.mp4` and `target/reel/promo-poster.png`
//! (`--out <dir>` to change). Frame numbers write just those frames as PNGs.

#[path = "common/out.rs"]
mod out;

use out::Args;
use sabitori_reel::{
    div, interpolate, interpolate_eased, path, spring_with, text, Color, Easing, Element, FrameCtx,
    GlyphMotion, MotionBlur, Percent, Px, Reel, Spring,
};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 30.0;

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

/// 時刻 `t` が `[a, b]` のどこか (0..1、外は端に留まる)。
fn phase(t: f32, a: f32, b: f32) -> f32 {
    interpolate(t, [a, b], [0.0, 1.0])
}

// ── 背景 ─────────────────────────────────────────────────────────────────

fn backdrop(_t: f32, frame: u32) -> Vec<Element> {
    let kids = vec![div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(W))
        .h(Px(H))
        .radial_gradient_at(hex("#1f1916"), hex("#0c0a09"), 0.65, 0.15)
        .grain(0.035)
        .grain_seed(frame)];
    kids
}

// ── 1. つかみ (0〜4 秒) ──────────────────────────────────────────────────

fn hook(t: f32) -> Element {
    let out = interpolate_eased(t, [3.3, 4.0], [0.0, 1.0], Easing::EaseIn);
    div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(W))
        .h(Px(H))
        .flex_col()
        .items_center()
        .justify_center()
        .gap(22.0)
        .child(
            text("Motion graphics,")
                .font_size(112.0)
                .font_weight(800)
                .color(hex("#efe6d8"))
                .per_glyph(|i, _| rise(t, 0.2 + i as f32 * 0.03)),
        )
        .child(
            text("written in Rust.")
                .font_size(112.0)
                .font_weight(800)
                .color(hex("#d9673a"))
                .per_glyph(|i, _| rise(t, 0.8 + i as f32 * 0.03)),
        )
        .blur(out * 30.0)
        .opacity(1.0 - out)
}

/// 下から回りながら立ち上がる 1 字。
fn rise(t: f32, start: f32) -> GlyphMotion {
    let s = spring_at(t, start, Spring::bouncy());
    GlyphMotion { dy: (1.0 - s) * 70.0, rotation: (1.0 - s) * 0.5, opacity: s.clamp(0.0, 1.0), ..Default::default() }
}

// ── コード (左) ──────────────────────────────────────────────────────────

/// 画面に出すコード。`(行, 打ち始める秒, 光らせる区間)`。
const CODE: &[(&str, f32, Option<(f32, f32)>)] = &[
    ("fn scene(ctx: FrameCtx) -> Element {", 4.6, None),
    ("    let t = ctx.t();", 5.4, None),
    ("    div().w(Px(720.0)).h(Px(560.0))", 6.0, None),
    ("        .radial_gradient(INK, NIGHT)", 6.9, None),
    ("        .child(text(\"Hello\")", 7.8, None),
    ("            .per_glyph(|i, _| hop(t, i)))", 8.6, Some((12.0, 14.0))),
    ("        .child(card()", 9.6, None),
    ("            .rotate(spin(t))", 10.2, Some((14.0, 16.0))),
    ("            .blur(focus(t))", 10.8, Some((16.0, 18.0))),
    ("            .mask(wipe(t)))", 11.4, Some((18.0, 19.5))),
    ("        .child(path().svg_path(CHECK)", 12.0, None),
    ("            .stroke(WHITE, 6.0).trim(0.0, t))", 12.6, Some((19.5, 21.0))),
    ("}", 13.2, None),
];

/// とても小さな色付け: キーワード・文字列・数・`.メソッド`。
fn highlight(line: &str) -> Vec<(std::ops::Range<usize>, Color)> {
    let mut spans = Vec::new();
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'"' {
            let j = line[i + 1..].find('"').map_or(b.len(), |k| i + 1 + k + 1);
            spans.push((i..j, hex("#8fb59f")));
            i = j;
        } else if c.is_ascii_digit() {
            let j = (i..b.len()).find(|&k| !(b[k].is_ascii_digit() || b[k] == b'.')).unwrap_or(b.len());
            spans.push((i..j, hex("#d9a85b")));
            i = j;
        } else if c == b'.' && i + 1 < b.len() && b[i + 1].is_ascii_alphabetic() {
            let j = (i + 1..b.len()).find(|&k| !(b[k].is_ascii_alphanumeric() || b[k] == b'_')).unwrap_or(b.len());
            spans.push((i + 1..j, hex("#efe6d8")));
            i = j;
        } else if c.is_ascii_alphabetic() {
            let j = (i..b.len()).find(|&k| !(b[k].is_ascii_alphanumeric() || b[k] == b'_')).unwrap_or(b.len());
            let word = &line[i..j];
            if matches!(word, "fn" | "let" | "move") {
                spans.push((i..j, hex("#e0703a")));
            } else if word.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                spans.push((i..j, hex("#cf9a7a")));
            }
            i = j;
        } else {
            i += 1;
        }
    }
    spans
}

fn code_panel(t: f32) -> Element {
    let s = spring_at(t, 4.0, Spring::gentle());
    let mut lines = div().flex_col().gap(6.0);
    for &(line, start, lit) in CODE {
        // 1 秒に 48 字の速さで打つ。
        let shown = ((t - start) * 48.0).clamp(0.0, line.len() as f32) as usize;
        let mut cut = shown;
        while !line.is_char_boundary(cut) {
            cut -= 1;
        }
        let typed = &line[..cut];
        let spans: Vec<_> = highlight(line).into_iter().filter(|(r, _)| r.start < cut).map(|(r, c)| (r.start..r.end.min(cut), c)).collect();
        let glow = lit.map_or(0.0, |(a, b)| {
            interpolate(t, [a, a + 0.25], [0.0, 1.0]) * interpolate(t, [b - 0.25, b], [1.0, 0.0])
        });
        let caret = shown < line.len() && t >= start;
        lines = lines.child(
            div()
                .w(Percent(100.0))
                .flex_row()
                .px_pad(Px(14.0))
                .py(Px(3.0))
                .rounded_px(8.0)
                .bg(hex("#d9673a").with_alpha(0.22 * glow))
                .child(text(typed).mono().font_size(25.0).color(hex("#b9ac99")).color_spans(spans))
                .child(div().w(Px(13.0)).h(Px(28.0)).bg(hex("#e6dccb")).opacity(if caret { 0.9 } else { 0.0 })),
        );
    }
    div()
        .absolute()
        .pos(96.0, 200.0)
        .w(Px(860.0))
        .h(Px(680.0))
        .p(Px(30.0))
        .rounded_px(22.0)
        .bg(hex("#151210").with_alpha(0.86))
        .border(1.0, hex("#33291f"))
        .child(lines)
        .ty((1.0 - s) * 80.0)
        .opacity(s.clamp(0.0, 1.0))
}

fn step_label(t: f32) -> Element {
    let (n, label, a, b) = if t < 12.0 {
        ("1", "Write a scene — a function of the frame", 4.0, 12.0)
    } else if t < 21.4 {
        ("2", "Animate anything", 12.0, 21.4)
    } else {
        ("3", "Render to MP4", 21.4, 26.4)
    };
    let a_in = phase(t, a, a + 0.4);
    let a_out = 1.0 - phase(t, b - 0.4, b);
    div()
        .absolute()
        .pos(96.0, 110.0)
        .flex_row()
        .items_center()
        .gap(18.0)
        .child(
            div()
                .w(Px(46.0))
                .h(Px(46.0))
                .rounded_px(23.0)
                .bg(hex("#c4532c"))
                .items_center()
                .justify_center()
                .child(text(n).font_size(24.0).bold().color(Color::WHITE)),
        )
        .child(text(label).font_size(34.0).font_weight(700).color(hex("#efe6d8")))
        .opacity(a_in * a_out)
        .tx((1.0 - a_in) * -30.0)
}

// ── 結果 (右) ────────────────────────────────────────────────────────────

fn preview(t: f32) -> Element {
    let s = spring_at(t, 4.3, Spring::gentle());
    let (pw, ph) = (720.0, 560.0);
    let mut kids: Vec<Element> = Vec::new();

    // "Hello" — 7.8 秒から。12〜14 秒は跳ね続ける。
    if t >= 7.8 {
        let hop = phase(t, 12.0, 14.0);
        kids.push(
            text("Hello")
                .font_size(96.0)
                .font_weight(800)
                .color(hex("#efe6d8"))
                .per_glyph(|i, _| {
                    let s = spring_at(t, 7.9 + i as f32 * 0.06, Spring::bouncy());
                    let wave = (t * 7.0 - i as f32 * 0.7).sin() * 26.0 * (hop * (1.0 - hop) * 4.0).min(1.0);
                    GlyphMotion { dy: (1.0 - s) * -60.0 - wave.max(0.0), opacity: s.clamp(0.0, 1.0), ..Default::default() }
                })
                .absolute()
                .pos(60.0, 50.0),
        );
    }
    // カード — 9.6 秒から。14〜16 回る、16〜18 ぼけて戻る、18〜19.5 ワイプで現れ直す。
    if t >= 9.6 {
        let pop = spring_at(t, 9.7, Spring::bouncy());
        let spin = interpolate_eased(t, [14.0, 16.0], [0.0, std::f32::consts::TAU], Easing::EaseInOut);
        let focus = {
            let p = phase(t, 16.0, 18.0);
            (p * std::f32::consts::PI).sin() * 26.0
        };
        let wipe_p = if t < 18.0 { 1.0 } else { interpolate_eased(t, [18.0, 19.4], [0.0, 1.0], Easing::EaseInOut) };
        let soft = 120.0;
        let wipe = div()
            .w(Percent(100.0))
            .h(Percent(100.0))
            .flex_row()
            .child(div().w(Percent(wipe_p * 100.0)).h(Percent(100.0)).bg(Color::BLACK))
            .child(div().w(Px(soft)).h(Percent(100.0)).gradient(Color::BLACK, Color::TRANSPARENT, 0.0))
            .tx(-soft * (1.0 - wipe_p));
        kids.push(
            div()
                .absolute()
                .pos(60.0, 210.0)
                .w(Px(330.0))
                .h(Px(220.0))
                .rounded_px(24.0)
                .gradient(hex("#c4532c"), hex("#d4a45a"), 0.6)
                .p(Px(26.0))
                .flex_col()
                .gap(10.0)
                .child(text("card()").mono().font_size(28.0).bold().color(Color::WHITE))
                .child(text("rotate · blur · mask").font_size(22.0).color(hex("#fbeee0")))
                .rotate(spin + (1.0 - pop) * -0.4)
                .blur(focus)
                .mask(wipe)
                .opacity(pop.clamp(0.0, 1.0)),
        );
    }
    // チェック — 12 秒から形が出て、19.5〜21 で描き直す。
    if t >= 12.0 {
        let trim = if t < 19.5 {
            interpolate_eased(t, [12.2, 13.2], [0.0, 1.0], Easing::EaseOutCubic)
        } else {
            interpolate_eased(t, [19.6, 20.8], [0.0, 1.0], Easing::EaseOutCubic)
        };
        kids.push(
            path()
                .svg_path("M50 6 A44 44 0 1 1 49.99 6 Z")
                .view_box(0.0, 0.0, 100.0, 100.0)
                .fill(hex("#5f8f7f").with_alpha(0.18))
                .stroke(hex("#7fb39c"), 4.0)
                .trim(0.0, trim)
                .w(Px(220.0))
                .h(Px(220.0))
                .absolute()
                .pos(450.0, 210.0),
        );
        kids.push(
            path()
                .svg_path("M28 52 L44 67 L73 35")
                .view_box(0.0, 0.0, 100.0, 100.0)
                .stroke(Color::WHITE, 8.0)
                .trim(0.0, trim)
                .w(Px(220.0))
                .h(Px(220.0))
                .absolute()
                .pos(450.0, 210.0),
        );
    }
    let leave = interpolate_eased(t, [21.0, 21.6], [0.0, 1.0], Easing::EaseIn);
    div()
        .absolute()
        .pos(1010.0, 200.0)
        .w(Px(pw))
        .h(Px(ph))
        .rounded_px(22.0)
        .radial_gradient_at(hex("#241c17"), hex("#100d0b"), 0.3, 0.2)
        .border(1.0, hex("#33291f"))
        .overflow_hidden()
        .children(kids)
        .child(
            text("preview · frame-accurate")
                .font_size(18.0)
                .color(hex("#8a7d6c"))
                .absolute()
                .pos(24.0, ph - 40.0),
        )
        .ty((1.0 - s) * 80.0 + leave * 40.0)
        .opacity(s.clamp(0.0, 1.0) * (1.0 - leave))
}

// ── 3. 書き出し (21.4〜26.4 秒) ──────────────────────────────────────────

fn render_step(t: f32) -> Element {
    let s = spring_at(t, 21.5, Spring::gentle());
    let cmd = "$ cargo run --release";
    let typed = ((t - 21.8) * 30.0).clamp(0.0, cmd.len() as f32) as usize;
    let prog = interpolate_eased(t, [22.8, 25.2], [0.0, 1.0], Easing::EaseInOut);
    let leave = interpolate_eased(t, [26.0, 26.5], [0.0, 1.0], Easing::EaseIn);
    let mut term = div()
        .flex_col()
        .gap(14.0)
        .child(text(&cmd[..typed]).mono().font_size(30.0).color(hex("#e6dccb")).color_spans([(0..1.min(typed), hex("#7fb39c"))]));
    if t > 22.7 {
        term = term.child(
            text(format!("rendering + encoding  {:>3}/900 frames", (prog * 900.0) as u32))
                .mono()
                .font_size(26.0)
                .color(hex("#8a7d6c")),
        );
        term = term.child(
            div()
                .w(Px(760.0))
                .h(Px(14.0))
                .rounded_px(7.0)
                .bg(hex("#2a221c"))
                .child(div().w(Px(760.0 * prog)).h(Px(14.0)).rounded_px(7.0).gradient(hex("#c4532c"), hex("#d4a45a"), 0.0)),
        );
    }
    if t > 25.3 {
        let d = spring_at(t, 25.3, Spring::bouncy());
        term = term.child(
            text("✓ wrote target/reel/promo.mp4")
                .mono()
                .font_size(30.0)
                .bold()
                .color(hex("#7fb39c"))
                .opacity(d.clamp(0.0, 1.0))
                .tx((1.0 - d) * -20.0),
        );
    }
    div()
        .absolute()
        .pos(1010.0, 300.0)
        .w(Px(820.0))
        .p(Px(30.0))
        .rounded_px(22.0)
        .bg(hex("#151210").with_alpha(0.9))
        .border(1.0, hex("#33291f"))
        .child(term)
        .ty((1.0 - s) * 80.0)
        .opacity(s.clamp(0.0, 1.0) * (1.0 - leave))
        .blur(leave * 20.0)
}

// ── 締め (26〜30 秒) ─────────────────────────────────────────────────────

fn outro(t: f32) -> Element {
    let s = spring_at(t, 26.6, Spring::gentle());
    let url = phase(t, 27.6, 28.2);
    let out = interpolate(t, [29.4, 30.0], [0.0, 1.0]);
    div()
        .absolute()
        .pos(0.0, 0.0)
        .w(Px(W))
        .h(Px(H))
        .flex_col()
        .items_center()
        .justify_center()
        .gap(26.0)
        .child(
            text("sabitori-reel")
                .font_size(140.0)
                .font_weight(800)
                .color(hex("#efe6d8"))
                .per_glyph(|i, n| {
                    let c = i as f32 - n as f32 * 0.5;
                    let s = spring_at(t, 26.6 + c.abs() * 0.03, Spring::gentle());
                    GlyphMotion { dx: (1.0 - s) * c * 40.0, scale: 0.6 + 0.4 * s, opacity: s.clamp(0.0, 1.0), ..Default::default() }
                }),
        )
        .child(
            text("github.com/Mutafika/sabitori")
                .mono()
                .font_size(34.0)
                .color(hex("#d4a45a"))
                .mask(
                    div()
                        .w(Percent(url * 100.0))
                        .h(Percent(100.0))
                        .bg(Color::BLACK),
                ),
        )
        .blur((1.0 - s).max(0.0) * 20.0)
        .opacity(s.clamp(0.0, 1.0) * (1.0 - out))
}

fn scene(ctx: FrameCtx) -> Element {
    let t = ctx.t();
    let mut kids = backdrop(t, ctx.frame);
    if t < 4.1 {
        kids.push(hook(t));
    }
    if (4.0..26.6).contains(&t) {
        let leave = interpolate_eased(t, [26.0, 26.6], [0.0, 1.0], Easing::EaseIn);
        kids.push(code_panel(t).opacity(1.0 - leave).blur(leave * 24.0));
        kids.push(step_label(t));
    }
    if (4.0..21.7).contains(&t) {
        kids.push(preview(t));
    }
    if (21.4..26.6).contains(&t) {
        kids.push(render_step(t));
    }
    if t >= 26.5 {
        kids.push(outro(t));
    }
    div().w(Px(W)).h(Px(H)).bg(hex("#0c0a09")).children(kids)
}

fn reel(frames: u32) -> Reel {
    Reel {
        motion_blur: Some(MotionBlur::new(6)),
        mono_family: Some("Menlo".into()),
        // ウェブに載せるので少し軽く (平らな画面は 22 でも十分きれい)。
        crf: 22,
        ..Reel::new(W as u32, H as u32, FPS, frames)
    }
}

fn main() {
    let frames = (SECS * FPS as f32).round() as u32;
    let args = Args::parse();
    let stills = &args.stills;

    if !stills.is_empty() {
        let reel = reel(stills.len() as u32);
        let pick = |ctx: FrameCtx| scene(FrameCtx { frame: stills[ctx.frame as usize], ..ctx });
        let dir = args.frames_dir("promo");
        let paths = reel.render_pngs(&pick, &dir).expect("render failed");
        for (f, p) in stills.iter().zip(&paths) {
            println!("frame {f} -> {}", p.display());
        }
        return;
    }

    // LP の再生前に見せる 1 枚 (コードと結果が両方出ている所)。
    let poster = Reel { frames: 1, ..reel(1) };
    let pick = |ctx: FrameCtx| scene(FrameCtx { frame: (19.0 * FPS as f32) as u32, ..ctx });
    let dir = args.out.join("promo-poster");
    if let Ok(paths) = poster.render_pngs(&pick, &dir) {
        let to = args.out.join("promo-poster.png");
        let _ = std::fs::rename(&paths[0], &to);
        let _ = std::fs::remove_dir_all(&dir);
        println!("poster -> {}", to.display());
    }

    let reel = reel(frames);
    let out = args.mp4("promo");
    println!("rendering + encoding {frames} frames ({}x{} @ {FPS}fps) …", reel.width, reel.height);
    match reel.render_mp4(&scene, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
