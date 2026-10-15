//! sabitori-reel — **an app tour** (30 s): a file manager built from the same
//! `div()` / `text()` elements a sabitori app uses, driven through a short
//! session — the window focuses in, a click swaps the folder, a row is picked
//! and previewed, renamed letter by letter, then the camera pushes in on the
//! column view while callouts draw themselves, and pulls back to the title.
//!
//! Nothing here is a screenshot: the "app" is an `Element` tree like any other,
//! so it can be animated at the level of single rows and characters. The
//! cursor is a `path()`, the camera is `.scaled(..)` / `.tx(..)` on the window,
//! the callouts are `path().trim(..)`.
//!
//!   cargo run -p sabitori-reel --example app_tour --release
//!
//! Output: `target/reel/app_tour.mp4` (`--out <dir>` to change). Frame
//! numbers write just those frames as PNGs.

#[path = "common/out.rs"]
mod out;

use out::Args;
use sabitori_reel::{
    div, interpolate, interpolate_eased, path, spring_with, text, Color, Easing, Element, FrameCtx, GlyphMotion,
    MotionBlur, Percent, Px, Reel, Spring,
};

const W: f32 = 1920.0;
const H: f32 = 1080.0;
const FPS: u32 = 30;
const SECS: f32 = 30.0;

/// 窓の大きさ (論理 px)。画面の真ん中に置く。
const WW: f32 = 1280.0;
const WH: f32 = 780.0;
const SIDEBAR: f32 = 230.0;
const ROW: f32 = 40.0;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}

// 色: 明るい紙の画面に、錆色の差し色。
fn bg() -> Color {
    hex("#f7f4ef")
}
fn panel() -> Color {
    hex("#efebe4")
}
fn line() -> Color {
    hex("#ddd6cc")
}
fn ink() -> Color {
    hex("#2a2420")
}
fn dim() -> Color {
    hex("#8a8077")
}
fn accent() -> Color {
    hex("#c4532c")
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

// ── 中身 ─────────────────────────────────────────────────────────────────

struct Entry {
    name: &'static str,
    kind: &'static str,
    date: &'static str,
    size: &'static str,
}

const HOME: &[Entry] = &[
    Entry { name: "Desktop", kind: "dir", date: "Today 09:12", size: "—" },
    Entry { name: "Documents", kind: "dir", date: "Yesterday", size: "—" },
    Entry { name: "Downloads", kind: "dir", date: "Today 08:40", size: "—" },
    Entry { name: "Music", kind: "dir", date: "Sep 30", size: "—" },
    Entry { name: "Pictures", kind: "dir", date: "Oct 2", size: "—" },
    Entry { name: "notes.md", kind: "doc", date: "Oct 11", size: "4 KB" },
];

const PROJECTS: &[Entry] = &[
    Entry { name: "atlas", kind: "dir", date: "Today 10:02", size: "—" },
    Entry { name: "harbor-site", kind: "dir", date: "Yesterday", size: "—" },
    Entry { name: "cover.png", kind: "img", date: "Oct 12", size: "2.4 MB" },
    Entry { name: "draft-v3.pdf", kind: "pdf", date: "Oct 9", size: "880 KB" },
    Entry { name: "budget.csv", kind: "doc", date: "Oct 8", size: "12 KB" },
    Entry { name: "kickoff.mov", kind: "mov", date: "Sep 28", size: "214 MB" },
    Entry { name: "logo.svg", kind: "img", date: "Sep 21", size: "9 KB" },
];

fn kind_color(kind: &str) -> Color {
    match kind {
        "dir" => hex("#c9a26b"),
        "img" => hex("#6f9c8a"),
        "pdf" => hex("#c4532c"),
        "mov" => hex("#7d6aa8"),
        _ => hex("#9a918a"),
    }
}

fn icon(kind: &str) -> Element {
    let c = kind_color(kind);
    if kind == "dir" {
        // フォルダ: タブの付いた箱。
        path()
            .svg_path("M1 4 Q1 2 3 2 H8 L10 4 H17 Q19 4 19 6 V14 Q19 16 17 16 H3 Q1 16 1 14 Z")
            .view_box(0.0, 0.0, 20.0, 18.0)
            .fill(c)
            .w(Px(22.0))
            .h(Px(20.0))
    } else {
        div()
            .w(Px(18.0))
            .h(Px(22.0))
            .rounded_px(3.0)
            .bg(c.with_alpha(0.18))
            .border(1.5, c)
    }
}

// ── 窓 ───────────────────────────────────────────────────────────────────

fn traffic() -> Element {
    let dot = |c: &str| div().w(Px(13.0)).h(Px(13.0)).rounded_px(6.5).bg(hex(c));
    div().flex_row().gap(8.0).child(dot("#e0605a")).child(dot("#e3b341")).child(dot("#6bb36b"))
}

fn sidebar(t: f32, on_projects: f32) -> Element {
    let item = |label: &str, active: f32, kind: &str| {
        div()
            .flex_row()
            .items_center()
            .gap(10.0)
            .h(Px(34.0))
            .px_pad(Px(12.0))
            .rounded_px(8.0)
            .bg(accent().with_alpha(0.14 * active))
            .child(icon(kind))
            .child(text(label).font_size(16.0).color(ink().lerp(accent(), active)))
    };
    let tag = |c: &str, label: &str| {
        div()
            .flex_row()
            .items_center()
            .gap(10.0)
            .h(Px(30.0))
            .px_pad(Px(14.0))
            .child(div().w(Px(10.0)).h(Px(10.0)).rounded_px(5.0).bg(hex(c)))
            .child(text(label).font_size(15.0).color(dim()))
    };
    let _ = t;
    div()
        .w(Px(SIDEBAR))
        .h(Percent(100.0))
        .bg(panel())
        .p(Px(14.0))
        .flex_col()
        .gap(4.0)
        .child(text("FAVORITES").font_size(12.0).letter_spacing(1.5).color(dim()).mb(Px(6.0)))
        .child(item("Home", 1.0 - on_projects, "dir"))
        .child(item("Projects", on_projects, "dir"))
        .child(item("Downloads", 0.0, "dir"))
        .child(item("Archive", 0.0, "dir"))
        .child(text("TAGS").font_size(12.0).letter_spacing(1.5).color(dim()).mt(Px(18.0)).mb(Px(4.0)))
        .child(tag("#c4532c", "Urgent"))
        .child(tag("#6f9c8a", "Shared"))
        .child(tag("#c9a26b", "Review"))
}

/// 一覧の 1 行。`sel` (0..1) で選択の色、`name` は書き換え中の名前。
fn row(e: &Entry, name: Element, sel: f32) -> Element {
    div()
        .w(Percent(100.0))
        .h(Px(ROW))
        .flex_row()
        .items_center()
        .px_pad(Px(18.0))
        .gap(14.0)
        .rounded_px(8.0)
        .bg(accent().with_alpha(0.9 * sel))
        .child(icon(e.kind))
        .child(name.w(Px(380.0)))
        .child(text(e.date).font_size(15.0).color(dim().lerp(Color::WHITE, sel)).w(Px(170.0)))
        .child(text(e.size).font_size(15.0).color(dim().lerp(Color::WHITE, sel)))
}

fn list(t: f32) -> Element {
    // 3.6 秒のクリックで Home → Projects。行は上から順に入れ替わる。
    let swap = 3.6;
    let entries = if t < swap { HOME } else { PROJECTS };
    let selected = 2; // cover.png
    let sel = if t < swap { 0.0 } else { ease(t, 7.0, 7.25) };
    let mut col = div().flex_col().gap(2.0).w(Percent(100.0));
    col = col.child(
        div()
            .flex_row()
            .h(Px(30.0))
            .px_pad(Px(54.0))
            .gap(14.0)
            .child(text("Name").font_size(13.0).bold().color(dim()).w(Px(380.0)))
            .child(text("Modified").font_size(13.0).bold().color(dim()).w(Px(170.0)))
            .child(text("Size").font_size(13.0).bold().color(dim())),
    );
    for (i, e) in entries.iter().enumerate() {
        let s = if t < swap { 1.0 } else { spring_at(t, swap + 0.05 + i as f32 * 0.05, Spring::gentle()) };
        let is_sel = t >= swap && i == selected;
        let name = if is_sel && t > 13.4 { rename(t) } else { plain_name(e.name, if is_sel { sel } else { 0.0 }) };
        col = col.child(
            row(e, name, if is_sel { sel } else { 0.0 })
                .ty((1.0 - s) * 24.0)
                .opacity(s.clamp(0.0, 1.0)),
        );
    }
    col
}

fn plain_name(s: &str, sel: f32) -> Element {
    text(s).font_size(16.0).color(ink().lerp(Color::WHITE, sel))
}

/// 名前を打ち直す: 古い名前が選ばれ、新しい名前が 1 字ずつ跳ねて入る。
fn rename(t: f32) -> Element {
    let new = "cover-final.png";
    let typed = ((t - 14.2) * 14.0).clamp(0.0, new.len() as f32) as usize;
    let editing = t < 16.6;
    if t < 14.2 {
        // 名前が選ばれた所 (白い枠の中で反転)。
        return div()
            .px_pad(Px(6.0))
            .rounded_px(4.0)
            .bg(Color::WHITE)
            .child(text("cover.png").font_size(16.0).color(Color::WHITE).bg(accent().with_alpha(0.6)));
    }
    let shown = &new[..typed];
    let body = text(shown.to_string()).font_size(16.0).color(if editing { ink() } else { Color::WHITE }).per_glyph(|i, _| {
        let s = spring_at(t, 14.2 + i as f32 / 14.0, Spring::bouncy());
        GlyphMotion { dy: (1.0 - s) * -10.0, scale: 1.0 + (1.0 - s) * 0.6, ..Default::default() }
    });
    if editing {
        div()
            .flex_row()
            .items_center()
            .px_pad(Px(6.0))
            .rounded_px(4.0)
            .bg(Color::WHITE)
            .child(body)
            .child(div().w(Px(2.0)).h(Px(18.0)).bg(accent()).opacity(if (t * 2.0).fract() < 0.6 { 1.0 } else { 0.0 }))
    } else {
        body
    }
}

/// 右から滑り込むプレビュー (選んだ画像)。
fn preview(t: f32) -> Option<Element> {
    if t < 9.0 {
        return None;
    }
    let s = spring_at(t, 9.0, Spring::gentle());
    let out = ease(t, 17.6, 18.2);
    let reveal = ease(t, 9.2, 10.2);
    let art = div()
        .w(Px(330.0))
        .h(Px(230.0))
        .rounded_px(10.0)
        .radial_gradient_at(hex("#e9b77e"), hex("#5b6f68"), 0.35, 0.3)
        .grain(0.06)
        .grain_seed(3)
        .child(
            path()
                .svg_path("M0 230 L90 120 L150 170 L230 70 L330 200 L330 230 Z")
                .view_box(0.0, 0.0, 330.0, 230.0)
                .fill(hex("#3d4d48").with_alpha(0.85))
                .w(Px(330.0))
                .h(Px(230.0))
                .absolute()
                .pos(0.0, 0.0),
        )
        .mask(div().w(Percent(reveal * 100.0)).h(Percent(100.0)).bg(Color::BLACK));
    let meta = |k: &str, v: &str| {
        div()
            .flex_row()
            .justify_between()
            .w(Percent(100.0))
            .child(text(k).font_size(14.0).color(dim()))
            .child(text(v).font_size(14.0).color(ink()))
    };
    Some(
        div()
            .absolute()
            .pos(WW - 390.0, 64.0)
            .w(Px(370.0))
            .h(Px(WH - 84.0))
            .rounded_px(12.0)
            .bg(Color::WHITE)
            .border(1.0, line())
            .p(Px(20.0))
            .flex_col()
            .gap(14.0)
            .child(art)
            .child(text("cover.png").font_size(20.0).bold().color(ink()))
            .child(meta("Kind", "PNG image"))
            .child(meta("Size", "2.4 MB"))
            .child(meta("Dimensions", "3200 × 2240"))
            .child(meta("Modified", "Oct 12, 18:20"))
            .tx((1.0 - s) * 420.0 + out * 420.0)
            .opacity((s * (1.0 - out)).clamp(0.0, 1.0)),
    )
}

/// 列表示: 18.4 秒から、列が右から順に滑り込む。
fn columns(t: f32) -> Option<Element> {
    if t < 18.4 {
        return None;
    }
    let cols: [&[&str]; 3] = [
        &["atlas", "harbor-site", "cover.png", "draft-v3.pdf", "budget.csv"],
        &["src", "assets", "docs", "Cargo.toml", "README.md"],
        &["main.rs", "view.rs", "theme.rs", "files.rs"],
    ];
    let picked = [0usize, 0, 1];
    let mut rowbox = div().flex_row().h(Percent(100.0));
    for (ci, items) in cols.iter().enumerate() {
        let s = spring_at(t, 18.4 + ci as f32 * 0.35, Spring::gentle());
        let mut c = div().w(Px(260.0)).h(Percent(100.0)).border(1.0, line()).p(Px(8.0)).flex_col().gap(2.0);
        for (i, name) in items.iter().enumerate() {
            let on = i == picked[ci];
            c = c.child(
                div()
                    .h(Px(34.0))
                    .flex_row()
                    .items_center()
                    .px_pad(Px(10.0))
                    .rounded_px(6.0)
                    .bg(if on { accent().with_alpha(if ci == 2 { 0.9 } else { 0.16 }) } else { Color::TRANSPARENT })
                    .child(text(*name).font_size(15.0).color(if on && ci == 2 { Color::WHITE } else { ink() })),
            );
        }
        rowbox = rowbox.child(c.tx((1.0 - s) * 200.0).opacity(s.clamp(0.0, 1.0)));
    }
    let a = ease(t, 18.2, 18.6);
    Some(
        div()
            .absolute()
            .pos(SIDEBAR, 56.0)
            .w(Px(WW - SIDEBAR))
            .h(Px(WH - 56.0))
            .bg(bg())
            .child(rowbox)
            .opacity(a),
    )
}

fn window(t: f32) -> Element {
    let on_projects = ease(t, 3.5, 3.8);
    let toolbar = div()
        .w(Percent(100.0))
        .h(Px(56.0))
        .flex_row()
        .items_center()
        .px_pad(Px(18.0))
        .gap(20.0)
        .border(1.0, line())
        .child(traffic())
        .child(text(if t < 3.6 { "Home" } else { "Home  ›  Projects" }).font_size(16.0).bold().color(ink()))
        .child(div().grow(1.0))
        .child(
            div()
                .w(Px(220.0))
                .h(Px(32.0))
                .rounded_px(8.0)
                .bg(panel())
                .px_pad(Px(12.0))
                .items_center()
                .flex_row()
                .child(text("Search").font_size(14.0).color(dim())),
        );
    let mut win = div()
        .w(Px(WW))
        .h(Px(WH))
        .rounded_px(16.0)
        .bg(bg())
        .border(1.0, line())
        .overflow_hidden()
        .flex_col()
        .child(toolbar)
        .child(
            div()
                .flex_row()
                .grow(1.0)
                .child(sidebar(t, on_projects))
                .child(div().grow(1.0).p(Px(16.0)).child(list(t))),
        );
    if let Some(p) = preview(t) {
        win = win.child(p);
    }
    if let Some(c) = columns(t) {
        win = win.child(c);
    }
    win
}

// ── カメラ・カーソル・説明 ──────────────────────────────────────────────

/// カメラ: (倍率, 横, 縦)。窓の真ん中が画面の真ん中に来る位置が (1, 0, 0)。
fn camera(t: f32) -> (f32, f32, f32) {
    // 動画では字が小さいので、窓はふだん 1.18 倍で見せる。説明の間は左へ寄せ、
    // 空いた右の暗い所に説明を並べる。
    let side = ease(t, 19.6, 21.0) * (1.0 - ease(t, 26.2, 27.4));
    (1.18 - side * 0.16, side * -250.0, 0.0)
}

/// 窓の中の点 (窓の左上から) → 画面の点。
fn to_screen(t: f32, x: f32, y: f32) -> (f32, f32) {
    let (k, dx, dy) = camera(t);
    (W * 0.5 + (x - WW * 0.5) * k + dx, H * 0.5 + (y - WH * 0.5) * k + dy)
}

/// カーソルの通り道 (秒, 窓の中の x, y)。
const CURSOR: &[(f32, f32, f32)] = &[
    (1.8, 900.0, 640.0),
    (3.3, 120.0, 140.0),
    (3.7, 120.0, 140.0),
    (6.4, 520.0, 220.0),
    (7.0, 470.0, 218.0),
    (8.6, 470.0, 218.0),
    (12.8, 330.0, 218.0),
    (13.4, 330.0, 218.0),
    (17.0, 700.0, 520.0),
];

fn cursor(t: f32) -> Option<Element> {
    if !(1.8..18.0).contains(&t) {
        return None;
    }
    let (mut x, mut y) = (CURSOR[0].1, CURSOR[0].2);
    for w in CURSOR.windows(2) {
        let (a, b) = (w[0], w[1]);
        if t >= a.0 {
            let p = interpolate_eased(t, [a.0, b.0], [0.0, 1.0], Easing::EaseInOut);
            x = a.1 + (b.1 - a.1) * p;
            y = a.2 + (b.2 - a.2) * p;
        }
    }
    let (sx, sy) = to_screen(t, x, y);
    let clicks = [3.5, 7.0, 13.2];
    let press = clicks.iter().map(|&c| (1.0 - ((t - c) * 6.0).abs()).max(0.0)).fold(0.0, f32::max);
    let ring = clicks
        .iter()
        .filter_map(|&c| {
            let age = t - c;
            (0.0..0.5).contains(&age).then(|| {
                let r = 8.0 + age * 70.0;
                div()
                    .absolute()
                    .pos(sx - r, sy - r)
                    .w(Px(r * 2.0))
                    .h(Px(r * 2.0))
                    .rounded_px(r)
                    .border(2.0, accent().with_alpha(1.0 - age * 2.0))
            })
        })
        .next();
    let arrow = path()
        .svg_path("M0 0 L0 22 L6 17 L10 26 L14 24 L10 15 L17 15 Z")
        .fill(ink())
        .stroke(Color::WHITE, 1.5)
        .w(Px(26.0))
        .h(Px(36.0))
        .absolute()
        .pos(sx, sy)
        .scaled(1.0 - press * 0.15);
    let mut d = div().absolute().pos(0.0, 0.0).w(Px(W)).h(Px(H));
    if let Some(r) = ring {
        d = d.child(r);
    }
    Some(d.child(arrow))
}

/// 引き出し線と説明 (列表示に寄った所で)。
fn callouts(t: f32) -> Vec<Element> {
    let items = [
        (21.2, (330.0, 134.0), (1460.0, 300.0), "Every row is an Element", "the same div() / text() as your app"),
        (22.4, (620.0, 300.0), (1460.0, 500.0), "Columns slide in with springs", "pure functions of the frame"),
        (23.6, (900.0, 160.0), (1460.0, 700.0), "Drawn on the GPU", "and rendered straight to MP4"),
    ];
    let leave = ease(t, 26.0, 26.6);
    let mut out = Vec::new();
    for (start, anchor, label, title, sub) in items {
        if t < start {
            continue;
        }
        let (ax, ay) = to_screen(t, anchor.0, anchor.1);
        let draw = interpolate_eased(t, [start, start + 0.6], [0.0, 1.0], Easing::EaseOutCubic);
        let text_in = spring_at(t, start + 0.45, Spring::gentle());
        out.push(
            path()
                .move_to(ax, ay)
                .line_to(label.0 - 16.0, label.1 + 16.0)
                .stroke(accent(), 2.0)
                .trim(0.0, draw)
                .absolute()
                .pos(0.0, 0.0)
                .w(Px(W))
                .h(Px(H))
                .opacity(1.0 - leave),
        );
        out.push(
            div()
                .absolute()
                .pos(ax - 7.0, ay - 7.0)
                .w(Px(14.0))
                .h(Px(14.0))
                .rounded_px(7.0)
                .bg(accent())
                .opacity(draw * (1.0 - leave)),
        );
        out.push(
            div()
                .absolute()
                .pos(label.0, label.1)
                .w(Px(420.0))
                .flex_col()
                .gap(6.0)
                .child(text(title).font_size(28.0).bold().color(hex("#f3ede4")))
                .child(text(sub).font_size(20.0).color(hex("#b5aa9d")))
                .ty((1.0 - text_in) * 14.0)
                .opacity((text_in * (1.0 - leave)).clamp(0.0, 1.0)),
        );
    }
    out
}

fn scene(ctx: FrameCtx) -> Element {
    let t = ctx.t();
    let enter = interpolate_eased(t, [0.2, 1.6], [0.0, 1.0], Easing::EaseOutCubic);
    let (k, dx, dy) = camera(t);
    let outro = ease(t, 27.0, 28.0);
    let fin = interpolate(t, [29.4, 30.0], [0.0, 1.0]);

    let shadow = div()
        .absolute()
        .pos((W - WW) * 0.5, (H - WH) * 0.5 + 30.0)
        .w(Px(WW))
        .h(Px(WH))
        .rounded_px(16.0)
        .bg(Color::BLACK.with_alpha(0.55))
        .blur(40.0)
        .scaled(k * (0.92 + 0.08 * enter) * (1.0 - outro * 0.35))
        .tx(dx)
        .ty(dy - outro * 90.0)
        .opacity(enter);
    let win = window(t)
        .absolute()
        .pos((W - WW) * 0.5, (H - WH) * 0.5)
        .scaled(k * (0.92 + 0.08 * enter) * (1.0 - outro * 0.35))
        .tx(dx)
        .ty(dy - outro * 90.0)
        .rotate((1.0 - enter) * -0.05)
        .blur((1.0 - enter) * 18.0)
        .opacity(enter);

    let mut kids = vec![
        div()
            .absolute()
            .pos(0.0, 0.0)
            .w(Px(W))
            .h(Px(H))
            .radial_gradient_at(hex("#3a2f28"), hex("#141110"), 0.5, 0.35)
            .grain(0.04)
            .grain_seed(ctx.frame),
        shadow,
        win,
    ];
    if let Some(c) = cursor(t) {
        kids.push(c);
    }
    kids.extend(callouts(t));
    if t > 27.2 {
        let s = spring_at(t, 27.4, Spring::gentle());
        kids.push(
            div()
                .absolute()
                .pos(0.0, H - 250.0)
                .w(Px(W))
                .flex_col()
                .items_center()
                .gap(10.0)
                .child(text("Built with sabitori").font_size(56.0).font_weight(800).color(hex("#f3ede4")).per_glyph(
                    |i, _| {
                        let s = spring_at(t, 27.4 + i as f32 * 0.025, Spring::bouncy());
                        GlyphMotion { dy: (1.0 - s) * 30.0, opacity: s.clamp(0.0, 1.0), ..Default::default() }
                    },
                ))
                .child(text("a GPU UI framework for Rust").font_size(24.0).letter_spacing(4.0).color(hex("#b5aa9d")))
                .opacity(s.clamp(0.0, 1.0)),
        );
    }
    div().w(Px(W)).h(Px(H)).bg(hex("#141110")).children(kids).opacity(1.0 - fin)
}

fn reel(frames: u32) -> Reel {
    Reel { motion_blur: Some(MotionBlur::new(5)), crf: 20, ..Reel::new(W as u32, H as u32, FPS, frames) }
}

fn main() {
    let frames = (SECS * FPS as f32).round() as u32;
    let args = Args::parse();

    if !args.stills.is_empty() {
        let stills = &args.stills;
        let reel = reel(stills.len() as u32);
        let pick = |ctx: FrameCtx| scene(FrameCtx { frame: stills[ctx.frame as usize], ..ctx });
        let dir = args.frames_dir("app_tour");
        let paths = reel.render_pngs(&pick, &dir).expect("render failed");
        for (f, p) in stills.iter().zip(&paths) {
            println!("frame {f} -> {}", p.display());
        }
        return;
    }

    let reel = reel(frames);
    let out = args.mp4("app_tour");
    println!("rendering + encoding {frames} frames ({}x{} @ {FPS}fps) …", reel.width, reel.height);
    match reel.render_mp4(&scene, &out) {
        Ok(path) => println!("wrote {}", path.display()),
        Err(e) => eprintln!("encode failed: {e}"),
    }
}
