//! sabitori-reel — a **terminal session, recorded as video**: a prompt,
//! commands typed one character at a time, colored output that scrolls, a
//! progress bar redrawn in place, and finally a notification card that slides
//! over the terminal — straight across the glyphs.
//!
//! The terminal body is one `cell_grid(..)` element: a monospace grid that
//! pins each glyph to `col × cell_w`, so the columns never drift, and only the
//! visible rows are drawn. The session itself is a small script of timed
//! events; the frame just asks "what is on screen at `t`?" — a pure function
//! of the frame, ready to drop into a README.
//!
//! What it exercises: `cell_grid` (colors, bold / dim, a reverse-video caret),
//! smooth scrolling by clipping the grid with `overflow_hidden` and nudging it
//! with `.ty(..)`, and tree-order drawing — the card is a later sibling, so it
//! covers the grid's glyphs with no `.layer(..)`.
//!
//!   cargo run -p sabitori-reel --example terminal --release
//!
//! Output: `$TMPDIR/sabitori-terminal.mp4`. Pass frame numbers
//! (`-- 0 60 120`) to write just those frames as PNGs instead.

use std::sync::Arc;

use sabitori_core::{cell_grid, BoxShadow, CellFlags, CellGrid, Corners, GridCell, Point};
use sabitori_reel::{
    div, interpolate, interpolate_eased, spring_with, text, Color, Easing, Element, FrameCtx, Px,
    Reel, Spring,
};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 12.0;

// 端末の格子。字送りは等幅の書体のおおよその比 (0.6 em)。
const COLS: usize = 92;
const ROWS: usize = 18;
const FONT: f32 = 22.0;
const CELL_W: f32 = FONT * 0.6;
const CELL_H: f32 = 32.0;
const PAD: f32 = 28.0;
const BAR_H: f32 = 52.0;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}

// 端末の配色。
fn fg() -> Color { hex("#c9d1d9") }
fn dim() -> Color { hex("#6e7681") }
fn green() -> Color { hex("#7ee787") }
fn blue() -> Color { hex("#79c0ff") }
fn violet() -> Color { hex("#d2a8ff") }
fn yellow() -> Color { hex("#e3b341") }
fn cyan() -> Color { hex("#56d4dd") }

// ── 台本 ───────────────────────────────────────────────────────────────────────

/// 1 行の中の、同じ色の続き。
type Seg = (String, Color, CellFlags);

fn seg(s: &str, c: Color) -> Seg {
    (s.to_string(), c, CellFlags::NONE)
}
fn bold(s: &str, c: Color) -> Seg {
    (s.to_string(), c, CellFlags::BOLD)
}

enum Kind {
    /// プロンプトのあとに、`at` から 1 字ずつ打たれるコマンド。
    Cmd(&'static str),
    /// 出力の 1 行。
    Out(Vec<Seg>),
    /// その場で描き直される進み具合の行 (`dur` 秒で 0 → `total`)。
    Progress { dur: f32, total: u32 },
}

struct Ev {
    at: f32,
    kind: Kind,
}

/// i 字目を打つまでの秒数。決まった揺らぎ (ハッシュ) を足して、人の手っぽくする。
fn type_delay(i: usize) -> f32 {
    let h = ((i as u32).wrapping_mul(2_654_435_761) >> 16) % 100;
    0.026 + h as f32 / 100.0 * 0.036
}

fn typed_len(cmd: &str, since: f32) -> usize {
    let mut acc = 0.0;
    for (i, _) in cmd.char_indices() {
        acc += type_delay(i);
        if acc > since {
            return i;
        }
    }
    cmd.len()
}

fn type_time(cmd: &str) -> f32 {
    (0..cmd.len()).map(type_delay).sum()
}

fn script() -> Vec<Ev> {
    let mut evs = Vec::new();
    let mut t = 0.9;
    let cmd = |evs: &mut Vec<Ev>, t: &mut f32, c: &'static str| {
        evs.push(Ev { at: *t, kind: Kind::Cmd(c) });
        *t += type_time(c) + 0.35; // 打ち終えて、Enter までの間
    };
    let out = |evs: &mut Vec<Ev>, t: &mut f32, gap: f32, segs: Vec<Seg>| {
        *t += gap;
        evs.push(Ev { at: *t, kind: Kind::Out(segs) });
    };

    cmd(&mut evs, &mut t, "cargo test -p sabitori-reel");
    for krate in ["sabitori-core", "sabitori-text", "sabitori-reel"] {
        out(&mut evs, &mut t, 0.16, vec![bold("   Compiling ", green()), seg(&format!("{krate} v0.41.1"), fg())]);
    }
    out(&mut evs, &mut t, 0.45, vec![
        bold("    Finished ", green()),
        seg("`test` profile [unoptimized + debuginfo] target(s) in 6.84s", fg()),
    ]);
    out(&mut evs, &mut t, 0.08, vec![bold("     Running ", green()), seg("unittests src/lib.rs", fg())]);
    out(&mut evs, &mut t, 0.10, vec![]);
    out(&mut evs, &mut t, 0.05, vec![seg("running 5 tests", fg())]);
    for name in [
        "anim::tests::interpolate_clamps",
        "anim::tests::spring_is_pure",
        "anim::tests::seq_shifts_local_clock",
        "tests::scale_doubles_pixels",
        "tests::scrim_gradient_is_drawn",
    ] {
        out(&mut evs, &mut t, 0.07, vec![seg(&format!("test {name} ... "), fg()), bold("ok", green())]);
    }
    out(&mut evs, &mut t, 0.05, vec![]);
    out(&mut evs, &mut t, 0.12, vec![
        seg("test result: ", fg()),
        bold("ok", green()),
        seg(". 5 passed; 0 failed; 0 ignored; finished in 0.41s", fg()),
    ]);
    out(&mut evs, &mut t, 0.05, vec![]);

    t += 0.5;
    cmd(&mut evs, &mut t, "cargo run -p sabitori-reel --example intro --release");
    out(&mut evs, &mut t, 0.25, vec![
        bold("    Finished ", green()),
        seg("`release` profile [optimized] target(s) in 0.31s", fg()),
    ]);
    out(&mut evs, &mut t, 0.08, vec![
        bold("     Running ", green()),
        seg("`target/release/examples/intro`", fg()),
    ]);
    out(&mut evs, &mut t, 0.12, vec![
        seg("rendering + encoding ", fg()),
        bold("240", yellow()),
        seg(" frames (1920x1080 @ 30fps) …", fg()),
    ]);
    t += 0.1;
    evs.push(Ev { at: t, kind: Kind::Progress { dur: 1.6, total: 240 } });
    t += 1.6;
    out(&mut evs, &mut t, 0.15, vec![seg("wrote ", fg()), bold("/tmp/sabitori-intro.mp4", cyan())]);
    evs.push(Ev { at: t + 0.1, kind: Kind::Cmd("") }); // 次のプロンプト (何も打たない)
    evs
}

/// 進み具合の行の、棒より右の字 `  128/240 frames`。棒そのものは格子の上に
/// 普通の要素として重ねる (字形を並べた棒は、字と字の継ぎ目が出る)。
const BAR_COLS: usize = 40;
fn progress_line(p: f32, total: u32) -> Vec<Seg> {
    let done = (p * total as f32).round() as u32;
    vec![
        seg(&" ".repeat(BAR_COLS + 4), fg()),
        seg(&format!("{done:>3}/{total}"), if p >= 1.0 { green() } else { fg() }),
        (" frames".to_string(), dim(), CellFlags::NONE),
    ]
}

fn prompt() -> Vec<Seg> {
    vec![bold("~/sabitori", blue()), seg(" on ", dim()), bold("main", violet()), bold(" ❯ ", green())]
}

/// `t` 秒の画面。
struct Screen {
    lines: Vec<Vec<Seg>>,
    /// 各行が着いた時刻 (スクロールを滑らかにするのに使う)。
    arrived: Vec<f32>,
    /// (行, 列, 打っている最中か)。
    cursor: Option<(usize, usize, bool)>,
    /// 進み具合の棒: (行, 0..1)。
    bar: Option<(usize, f32)>,
}

fn screen_at(evs: &[Ev], t: f32) -> Screen {
    let mut sc = Screen { lines: Vec::new(), arrived: Vec::new(), cursor: None, bar: None };
    for ev in evs.iter().filter(|e| e.at <= t) {
        sc.cursor = None;
        let row = sc.lines.len();
        let line = match &ev.kind {
            Kind::Cmd(c) => {
                let n = typed_len(c, t - ev.at);
                let mut l = prompt();
                l.push(seg(&c[..n], fg()));
                let col: usize = l.iter().map(|s| s.0.chars().count()).sum();
                sc.cursor = Some((row, col, t - ev.at < type_time(c) + 0.2));
                l
            }
            Kind::Out(segs) => segs.clone(),
            Kind::Progress { dur, total } => {
                let p = ((t - ev.at) / dur).min(1.0);
                sc.bar = Some((row, p));
                progress_line(p, *total)
            }
        };
        sc.lines.push(line);
        sc.arrived.push(ev.at);
    }
    sc
}

/// 行の列を、`first` 行目から `ROWS + 1` 行ぶんの格子にする。
fn grid_of(lines: &[Vec<Seg>], first: usize, cursor: Option<(usize, usize)>) -> CellGrid {
    let mut g = CellGrid::new(COLS, ROWS + 1, fg());
    for (row, line) in lines.iter().skip(first).take(ROWS + 1).enumerate() {
        let mut col = 0;
        for (s, c, f) in line {
            g.put_str(col, row, s, *c, None, *f);
            col += s.chars().count();
        }
    }
    if let Some((r, c)) = cursor.filter(|&(r, c)| r >= first && r - first <= ROWS && c < COLS) {
        // 反転表示のカーソル: 地を前景色で塗り、字は地の色で。
        let ch = g.get(c, r - first).map(|x| x.ch).unwrap_or(' ');
        g.set(c, r - first, GridCell { ch, fg: hex("#0b0e14"), bg: Some(fg()), flags: CellFlags::NONE });
    }
    g
}

// ── 場面 ───────────────────────────────────────────────────────────────────────

fn scene(ctx: FrameCtx, evs: &[Ev]) -> Element {
    let t = ctx.t();
    let sc = screen_at(evs, t);

    // 滑らかなスクロール: 行は着いてから 0.12 秒かけて場所を空ける。
    let filled: f32 = sc.arrived.iter().map(|&a| interpolate_eased(t - a, [0.0, 0.12], [0.0, 1.0], Easing::EaseOut)).sum();
    let scroll = (filled - ROWS as f32).max(0.0);
    let first = scroll.floor() as usize;
    // 打っている間は点けっぱなし、待っている間は点滅。
    let caret = sc.cursor.filter(|&(_, _, typing)| typing || (t * 1.6).fract() < 0.6).map(|(r, c, _)| (r, c));
    let grid = Arc::new(grid_of(&sc.lines, first, caret));
    let shift = -(scroll - first as f32) * CELL_H;

    let win_w = COLS as f32 * CELL_W + PAD * 2.0;
    let win_h = BAR_H + ROWS as f32 * CELL_H + PAD * 2.0;
    let (wx, wy) = ((W - win_w) * 0.5, (H - win_h) * 0.5 + 10.0);
    let s_win = spring_with(t + 0.1, FPS, Spring::snappy()); // 最初のコマから少し見える

    let dot = |c: &str| div().w(Px(14.0)).h(Px(14.0)).rounded_px(7.0).bg(hex(c));
    let window = div()
        .absolute()
        .pos(wx, wy)
        .w(Px(win_w))
        .h(Px(win_h))
        .rounded_px(16.0)
        .bg(hex("#0b0e14"))
        .border(1.0, hex("#2a3140"))
        .shadow(BoxShadow { color: hex("#000000").with_alpha(0.55), offset: Point::new(0.0, 30.0), blur: 70.0, spread: 0.0 })
        .overflow_hidden()
        .scaled(0.94 + 0.06 * s_win)
        .opacity(s_win.min(1.0))
        .child(
            // 題の帯: 書体は等幅にしない (枠の文字は本文ではない)。
            div()
                .w(Px(win_w))
                .h(Px(BAR_H))
                // 窓の切り抜きは角を丸めないので、帯の上の角は自分で丸める。
                .corner_radius(Corners::new(16.0, 16.0, 0.0, 0.0))
                .bg(hex("#141922"))
                .flex_row()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .absolute()
                        .pos(22.0, (BAR_H - 14.0) * 0.5)
                        .flex_row()
                        .gap(10.0)
                        .child(dot("#ff5f57"))
                        .child(dot("#febc2e"))
                        .child(dot("#28c840")),
                )
                .child(text("sabitori — zsh — 92×18").font_size(18.0).font_weight(600).color(hex("#8b949e"))),
        )
        .child(
            div()
                .absolute()
                .pos(PAD, BAR_H + PAD)
                .w(Px(COLS as f32 * CELL_W))
                .h(Px(ROWS as f32 * CELL_H))
                .overflow_hidden()
                .child(
                    // 既定の等幅 (OS の Monospace) は macOS だと Courier になるので、名指しする。
                    // 無い環境では等幅の既定に落ちる。
                    cell_grid(grid, CELL_W, CELL_H).font_size(FONT).font_family("Menlo").ty(shift),
                )
                .children(sc.bar.map(|(row, p)| {
                    // 進み具合の棒: 格子より後の兄弟なので、格子の上に描かれる。
                    let y = (row as f32 - first as f32) * CELL_H + shift + CELL_H * 0.5 - 4.0;
                    let len = BAR_COLS as f32 * CELL_W;
                    div()
                        .absolute()
                        .pos(2.0 * CELL_W, y)
                        .w(Px(len))
                        .h(Px(8.0))
                        .rounded_px(4.0)
                        .bg(hex("#262c36"))
                        .child(
                            div()
                                .w(Px((len * p).max(8.0)))
                                .h(Px(8.0))
                                .rounded_px(4.0)
                                .gradient(hex("#3b82f6"), cyan(), 0.0),
                        )
                })),
        );

    let mut kids = vec![
        // 背後の、ぼけた色の玉 2 つ。
        // 影 (glow) を大きくぼかして、縁を溶かす。
        div().absolute().pos(-80.0, -180.0).w(Px(640.0)).h(Px(640.0)).rounded_px(320.0).bg(hex("#3b2a7a")).glow(hex("#3b2a7a"), 220.0).opacity(0.45),
        div().absolute().pos(1350.0, 640.0).w(Px(640.0)).h(Px(640.0)).rounded_px(320.0).bg(hex("#0f5a63")).glow(hex("#0f5a63"), 220.0).opacity(0.40),
        window,
    ];

    // 通知: 書き出しが終わったら右から滑り込み、端末の字の上に重なる。
    let done_at = evs.iter().rev().find(|e| matches!(e.kind, Kind::Cmd(""))).map_or(SECS, |e| e.at);
    let s_note = if t < done_at { 0.0 } else { spring_with(t - done_at, FPS, Spring::bouncy()) };
    if s_note > 0.0 {
        let out = interpolate(t, [SECS - 0.6, SECS - 0.1], [0.0, 1.0]);
        kids.push(
            div()
                .absolute()
                // 字の並んだ行の真上に置く (行末の字を隠すのが狙い)。
                .pos(wx + win_w - 640.0, wy + BAR_H + PAD + 8.5 * CELL_H)
                .w(Px(580.0))
                .flex_row()
                .gap(20.0)
                .items_center()
                .p_px(22.0)
                .rounded_px(20.0)
                .bg(hex("#1b2130"))
                .border(1.0, hex("#56d4dd").with_alpha(0.35))
                .shadow(BoxShadow { color: hex("#000000").with_alpha(0.5), offset: Point::new(0.0, 16.0), blur: 40.0, spread: 0.0 })
                .tx((1.0 - s_note) * 160.0 + out * 40.0)
                .opacity((s_note.min(1.0) - out).max(0.0))
                .child(
                    // 書き出した動画の小さな見本 (題字の色の帯)。
                    div()
                        .w(Px(112.0))
                        .h(Px(64.0))
                        .rounded_px(10.0)
                        .gradient(hex("#7dd3fc"), hex("#fda4af"), 0.6)
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .child(text("▶").font_size(22.0).color(Color::WHITE.with_alpha(0.9))),
                )
                .child(
                    div()
                        .flex_col()
                        .gap(6.0)
                        .child(text("Render finished").font_size(24.0).font_weight(700).color(hex("#f0f3f6")))
                        .child(text("sabitori-intro.mp4 · 240 frames · 8.0 s").font_size(18.0).color(hex("#9aa4b2"))),
                ),
        );
    }

    div().w(Px(W)).h(Px(H)).gradient(hex("#0d1020"), hex("#141029"), 1.2).children(kids)
}

fn main() {
    let evs = script();
    let frames = (SECS * FPS as f32).round() as u32;
    let stills: Vec<u32> = std::env::args().skip(1).filter_map(|a| a.parse().ok()).collect();

    if !stills.is_empty() {
        // 指定のコマだけ PNG に書く (見た目の確認用)。
        let reel = Reel::new(W as u32, H as u32, FPS, stills.len() as u32);
        let pick = |ctx: FrameCtx| scene(FrameCtx { frame: stills[ctx.frame as usize], ..ctx }, &evs);
        let dir = std::env::temp_dir().join("sabitori-terminal");
        let paths = reel.render_pngs(&pick, &dir).expect("render failed");
        for (f, p) in stills.iter().zip(&paths) {
            println!("frame {f} -> {}", p.display());
        }
        return;
    }

    let reel = Reel::new(W as u32, H as u32, FPS, frames);
    let out = std::env::temp_dir().join("sabitori-terminal.mp4");
    println!("rendering + encoding {frames} frames ({}x{} @ {FPS}fps) …", reel.width, reel.height);
    match reel.render_mp4(&|ctx| scene(ctx, &evs), &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
