//! Landing-page demo — a *routed, animated* GPU app, built entirely in Sabitori.
//!
//! This is the "what React needs a router + framer-motion for, Sabitori does
//! natively" demo. It's an app shell with four screens and spring-driven
//! slide-and-fade transitions between them:
//!
//!   * a persistent nav whose active underline springs to the current tab,
//!   * screens that slide + crossfade as you navigate (`on_click` → state),
//!   * a working animated toggle (its knob eases across on state change),
//!   * a soft aurora + parallax starfield behind everything,
//!   * a headline whose accent hue cycles forever.
//!
//! Driven by `is_animating() -> true` (continuous redraw) + a `tick()` clock.
//! The whole tree is rebuilt every frame on the GPU. No canvas, no HTML, no JS.
//!
//! `cargo run --example landing`

use sabitori::*;

// ── Palette (Tokyo Night-ish) ────────────────────────────────────────────
const BG0: &str = "#0a0c16";
const BG1: &str = "#12162c";
const SURFACE: &str = "#171b2e";
const SURFACE_HI: &str = "#20263f";
const PANEL: &str = "#0d1020";
const BORDER: &str = "#2a3152";
const TEXT_HI: &str = "#eef1ff";
const TEXT_MID: &str = "#a7afd6";
const TEXT_DIM: &str = "#6b74a0";
const METHOD: &str = "#8aa2d8";
const ACCENT_BLUE: &str = "#7aa2f7";
const ACCENT_PURPLE: &str = "#bb9af7";
const ACCENT_CYAN: &str = "#7dcfff";
const ACCENT_GREEN: &str = "#9ece6a";
const ACCENT_AMBER: &str = "#e0af68";

const TABS: [&str; 4] = ["Home", "Features", "Showcase", "Get started"];

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn ease_out_cubic(x: f32) -> f32 {
    1.0 - (1.0 - x).powi(3)
}
/// Ease-out-back — overshoots slightly, gives the springy "settle" feel.
fn ease_back(x: f32) -> f32 {
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (x - 1.0).powi(3) + c1 * (x - 1.0).powi(2)
}
fn hash01(n: u32) -> f32 {
    let mut x = n.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    x = ((x >> ((x >> 28).wrapping_add(4))) ^ x).wrapping_mul(277_803_737);
    x = (x >> 22) ^ x;
    (x as f32) / (u32::MAX as f32)
}

// ── Aurora backdrop ───────────────────────────────────────────────────────
// A soft radial light is a *small* core with a *large* glow radius — NOT a big
// filled ellipse (that reads as a solid "lemon"). Glow is an SDF blur, so a
// 60px dot with a 240px halo is genuine ambient light.
fn glow_blob(cx: f32, cy: f32, r: f32, col: Color, op: f32, glow_r: f32) -> Element {
    div()
        .absolute()
        .pos(cx - r, cy - r)
        .w(Px(r * 2.0))
        .h(Px(r * 2.0))
        .rounded_px(r)
        .bg(col)
        .opacity(op)
        .glow(col, glow_r)
}

fn backdrop(w: f32, h: f32, t: f32) -> Element {
    let mut kids: Vec<Element> = vec![
        glow_blob(
            w * 0.22 + 60.0 * (t * 0.11).sin(),
            h * 0.30 + 40.0 * (t * 0.09).cos(),
            64.0, hex(ACCENT_PURPLE), 0.22, 240.0,
        ),
        glow_blob(
            w * 0.80 + 70.0 * (t * 0.08 + 1.7).cos(),
            h * 0.26 + 48.0 * (t * 0.12 + 0.5).sin(),
            56.0, hex(ACCENT_CYAN), 0.18, 220.0,
        ),
        glow_blob(
            w * 0.56 + 84.0 * (t * 0.07 + 3.0).sin(),
            h * 0.74 + 56.0 * (t * 0.10 + 2.0).cos(),
            72.0, hex(ACCENT_BLUE), 0.17, 260.0,
        ),
        glow_blob(
            w * 0.40 + 50.0 * (t * 0.13 + 1.0).cos(),
            h * 0.52 + 44.0 * (t * 0.06 + 4.0).sin(),
            44.0, hex(ACCENT_BLUE), 0.13, 180.0,
        ),
    ];

    // Starfield — drifting upward, wrapping, twinkling. (No cursor parallax:
    // it read as the whole page swaying, which undercut the calm.)
    for i in 0..38u32 {
        let rx = hash01(i * 2 + 1);
        let ry = hash01(i * 7 + 3);
        let depth = 0.35 + 0.65 * hash01(i * 5 + 2);
        let speed = 5.0 + 22.0 * hash01(i * 11 + 4);
        let y = (ry * h - t * speed).rem_euclid(h + 24.0) - 12.0;
        let size = 1.1 + 2.6 * depth;
        let twinkle = 0.08 + 0.30 * (0.5 + 0.5 * (t * (1.0 + 2.0 * hash01(i * 13 + 6)) + rx * 6.28).sin());
        kids.push(
            div()
                .absolute()
                .pos(rx * w, y)
                .w(Px(size))
                .h(Px(size))
                .rounded_px(size)
                .bg(hex(TEXT_HI))
                .opacity(twinkle * depth),
        );
    }

    div().absolute().pos(0.0, 0.0).w(Px(w)).h(Px(h)).children(kids)
}

// ── Shared bits ───────────────────────────────────────────────────────────
fn eyebrow(txt: &str) -> Element {
    div()
        .flex_row()
        .items_center()
        .px_pad(Px(14.0))
        .py(Px(7.0))
        .rounded_px(999.0)
        .bg(hex(SURFACE))
        .border(1.0, hex(BORDER))
        .children([text(txt)
            .font_size(11.0)
            .font_weight(600)
            .letter_spacing(1.6)
            .color(hex(TEXT_DIM))])
}

fn pill(label: &str) -> Element {
    div()
        .px_pad(Px(12.0))
        .py(Px(5.0))
        .rounded_px(999.0)
        .bg(hex(SURFACE))
        .border(1.0, hex(BORDER))
        .children([text(label)
            .font_size(11.5)
            .font_weight(500)
            .letter_spacing(0.3)
            .color(hex(TEXT_MID))])
}

fn live_badge(t: f32) -> Element {
    let pulse = 0.30 + 0.70 * (0.5 + 0.5 * (t * 2.2).sin());
    div()
        .flex_row()
        .items_center()
        .gap(7.0)
        .px_pad(Px(10.0))
        .py(Px(5.0))
        .rounded_px(999.0)
        .bg(hex(SURFACE))
        .border(1.0, hex(BORDER))
        .children([
            div()
                .w(Px(7.0))
                .h(Px(7.0))
                .rounded_px(999.0)
                .bg(hex(ACCENT_GREEN))
                .glow_sm(hex(ACCENT_GREEN))
                .opacity(pulse),
            text("60 FPS")
                .font_size(10.5)
                .font_weight(600)
                .letter_spacing(1.0)
                .color(hex(TEXT_MID)),
        ])
}

fn cta_primary(id: &str, label: &str) -> Element {
    div()
        .id(id)
        .cursor(Cursor::Pointer)
        .flex_row()
        .items_center()
        .justify_center()
        .px_pad(Px(22.0))
        .py(Px(13.0))
        .rounded_px(11.0)
        .gradient(hex(ACCENT_BLUE), hex(ACCENT_PURPLE), 0.0)
        .glow_sm(hex(ACCENT_BLUE))
        .hover(|s| s.translate_y(-2.0).glow(hex(ACCENT_PURPLE), 22.0))
        .spring_transition(260.0, 22.0)
        .children([text(label)
            .font_size(14.5)
            .font_weight(600)
            .letter_spacing(0.3)
            .color(hex("#0b0d16"))])
}

fn cta_ghost(id: &str, label: &str) -> Element {
    div()
        .id(id)
        .cursor(Cursor::Pointer)
        .flex_row()
        .items_center()
        .justify_center()
        .px_pad(Px(22.0))
        .py(Px(13.0))
        .rounded_px(11.0)
        .bg(hex(SURFACE))
        .border(1.0, hex(BORDER))
        .hover(|s| s.bg(hex(SURFACE_HI)).border_color(hex(ACCENT_BLUE)))
        .spring_transition(260.0, 22.0)
        .children([text(label)
            .font_size(14.5)
            .font_weight(500)
            .color(hex(TEXT_HI))])
}

// ── Nav bar with a spring-driven active underline ─────────────────────────
fn nav_bar(cw: f32, cur: usize, prev: usize, trans: f32, accent: Color, t: f32) -> Element {
    let tabw = 112.0;
    let gap = 6.0;
    let center = |i: usize| (i as f32) * (tabw + gap) + tabw / 2.0;

    // The underline slides between prev and cur tab centres with an
    // overshooting ease — pure spring feel, no CSS transition involved.
    let eb = ease_back(trans);
    let ux = lerp(center(prev), center(cur), eb);
    let uw = 22.0;
    let underline = div()
        .absolute()
        .pos(ux - uw / 2.0, 34.0)
        .w(Px(uw))
        .h(Px(3.0))
        .rounded_px(999.0)
        .bg(accent)
        .glow_sm(accent);

    let tabs: Vec<Element> = (0..4)
        .map(|i| {
            let active = i == cur;
            div()
                .id(format!("nav-{i}"))
                .cursor(Cursor::Pointer)
                .w(Px(tabw))
                .h(Px(30.0))
                .flex_row()
                .items_center()
                .justify_center()
                .children([text(TABS[i])
                    .font_size(13.5)
                    .font_weight(if active { 600 } else { 500 })
                    .letter_spacing(0.2)
                    .color(if active { hex(TEXT_HI) } else { hex(TEXT_MID) })])
        })
        .collect();

    let tabs_row = div().flex_row().gap(gap).children(tabs);
    let tabs_box = div()
        .flex_col()
        .w(Px(4.0 * tabw + 3.0 * gap))
        .h(Px(40.0))
        .children([tabs_row, underline]);

    let logo = div().flex_row().items_center().gap(10.0).children([
        div()
            .w(Px(22.0))
            .h(Px(22.0))
            .rounded_px(6.0)
            .gradient(hex(ACCENT_BLUE), hex(ACCENT_PURPLE), 45.0)
            .glow_sm(hex(ACCENT_BLUE)),
        text("sabitori")
            .font_size(18.0)
            .font_weight(600)
            .letter_spacing(0.2)
            .color(hex(TEXT_HI)),
    ]);

    let right = div()
        .flex_row()
        .items_center()
        .gap(18.0)
        .children([live_badge(t), tabs_box]);

    div()
        .w(Px(cw))
        .h(Px(52.0))
        .flex_row()
        .items_center()
        .justify_between()
        .children([logo, right])
}

// ── Screen 0: Home ────────────────────────────────────────────────────────
fn home(stars: u32, accent: Color) -> Element {
    let headline = div().flex_col().items_center().gap(0.0).children([
        text("Build UI in Rust that")
            .font_size(58.0)
            .font_weight(400)
            .letter_spacing(-1.6)
            .line_height(1.06)
            .color(hex(TEXT_HI)),
        text("renders like a shader.")
            .font_size(58.0)
            .font_weight(500)
            .letter_spacing(-1.6)
            .line_height(1.06)
            .color(accent),
    ]);

    let subhead = div().flex_col().items_center().gap(2.0).children([
        text("An app shell, routed screens, spring transitions — one declarative")
            .font_size(16.5)
            .line_height(1.5)
            .color(hex(TEXT_MID)),
        text("tree, rebuilt every frame on the GPU. No canvas. No HTML. No JS.")
            .font_size(16.5)
            .line_height(1.5)
            .color(hex(TEXT_MID)),
    ]);

    let cta = div().flex_row().gap(14.0).children([
        cta_primary("cta-star", &format!("★  Star   {stars}")),
        cta_ghost("nav-1", "Explore features  →"),
    ]);

    let trust = div().flex_row().gap(8.0).children([
        pill("13 crates"),
        pill("20 widgets"),
        pill("CommonMark + GFM"),
        pill("WebGPU · WebGL2"),
    ]);

    div()
        .flex_col()
        .items_center()
        .gap(26.0)
        .children([
            eyebrow("V0.2.8 · GPU-NATIVE UI FRAMEWORK"),
            headline,
            subhead,
            cta,
            trust,
        ])
}

// ── Screen 1: Features (2×3 grid) ─────────────────────────────────────────
fn feature_card(icon_a: &str, icon_b: &str, accent: &str, title: &str, l1: &str, l2: &str) -> Element {
    div()
        .w(Px(300.0))
        .min_h(Px(170.0))
        .flex_col()
        .gap(12.0)
        .p(Px(22.0))
        .bg(hex(SURFACE))
        .rounded_px(16.0)
        .border(1.0, hex(BORDER))
        .shadow_md(hex("#00000055"))
        .hover(|s| s.translate_y(-6.0).glow_sm(hex(accent)).border_color(hex(accent)))
        .spring_transition(240.0, 24.0)
        .children([
            div()
                .w(Px(38.0))
                .h(Px(38.0))
                .rounded_px(11.0)
                .gradient(hex(icon_a), hex(icon_b), 45.0)
                .glow_sm(hex(icon_a)),
            text(title)
                .font_size(16.5)
                .font_weight(600)
                .letter_spacing(-0.2)
                .color(hex(TEXT_HI)),
            div().flex_col().gap(1.0).children([
                text(l1).font_size(13.0).line_height(1.5).color(hex(TEXT_MID)),
                text(l2).font_size(13.0).line_height(1.5).color(hex(TEXT_MID)),
            ]),
        ])
}

fn features_screen() -> Element {
    let row1 = div().flex_row().gap(20.0).children([
        feature_card(ACCENT_BLUE, ACCENT_PURPLE, ACCENT_BLUE, "Flexbox layout",
            "Taffy engine: rows, columns, gap,", "grow, wrap, absolute positioning."),
        feature_card(ACCENT_CYAN, ACCENT_BLUE, ACCENT_CYAN, "SDF styling",
            "Glow, gradients, shadows, rounded", "borders — one GPU pass, no images."),
        feature_card(ACCENT_GREEN, ACCENT_AMBER, ACCENT_GREEN, "Spring physics",
            "Real stiffness and damping on", "hover, drag and transitions."),
    ]);
    let row2 = div().flex_row().gap(20.0).children([
        feature_card(ACCENT_AMBER, ACCENT_GREEN, ACCENT_AMBER, "Typography API",
            "font_weight, letter_spacing and", "line_height, straight to cosmic-text."),
        feature_card(ACCENT_PURPLE, ACCENT_CYAN, ACCENT_PURPLE, "Markdown",
            "CommonMark + GFM rendered by the", "same SDF pipeline as everything else."),
        feature_card(ACCENT_BLUE, ACCENT_CYAN, ACCENT_BLUE, "Ships to the web",
            "The exact same code targets desktop", "and the browser via WebGPU / WebGL2."),
    ]);

    div().flex_col().items_center().gap(22.0).children([
        eyebrow("EVERYTHING IN THE BOX · 13 CRATES"),
        div().flex_col().gap(20.0).children([row1, row2]),
    ])
}

// ── Screen 2: Showcase (code panel + live widgets) ────────────────────────
fn dot(color: &str) -> Element {
    div().w(Px(11.0)).h(Px(11.0)).rounded_px(999.0).bg(hex(color))
}
fn code_line(segs: &[(&str, &str)]) -> Element {
    let spans: Vec<Element> = segs
        .iter()
        .map(|(s, c)| text(*s).mono().font_size(13.0).line_height(1.7).color(hex(c)))
        .collect();
    div().flex_row().children(spans)
}
fn code_panel(t: f32) -> Element {
    let titlebar = div().w_full().flex_row().items_center().gap(8.0).px_pad(Px(16.0)).py(Px(12.0)).children([
        dot("#ff5f57"),
        dot("#febc2e"),
        dot("#28c840"),
        div().w(Px(14.0)),
        text("examples/landing.rs").mono().font_size(12.0).color(hex(TEXT_DIM)),
    ]);

    let caret_on = (t * 3.0).sin() > 0.0;
    let caret = div()
        .w(Px(8.0))
        .h(Px(16.0))
        .rounded_px(2.0)
        .bg(hex(ACCENT_CYAN))
        .opacity(if caret_on { 0.9 } else { 0.05 });
    let last = div().flex_row().items_center().gap(4.0).children([
        code_line(&[("    .color", METHOD), ("(", TEXT_DIM), ("accent", ACCENT_CYAN), (")", TEXT_DIM)]),
        caret,
    ]);

    let body = div().flex_col().px_pad(Px(20.0)).py(Px(16.0)).gap(0.0).children([
        code_line(&[("text", ACCENT_BLUE), ("(", TEXT_DIM), ("\"renders like a shader.\"", ACCENT_GREEN), (")", TEXT_DIM)]),
        code_line(&[("    .font_weight", METHOD), ("(", TEXT_DIM), ("500", ACCENT_AMBER), (")", TEXT_DIM)]),
        code_line(&[("    .letter_spacing", METHOD), ("(", TEXT_DIM), ("-1.6", ACCENT_AMBER), (")", TEXT_DIM)]),
        code_line(&[("    .line_height", METHOD), ("(", TEXT_DIM), ("1.06", ACCENT_AMBER), (")", TEXT_DIM)]),
        last,
    ]);

    div()
        .w(Px(520.0))
        .flex_col()
        .bg(hex(PANEL))
        .rounded_px(14.0)
        .border(1.0, hex(BORDER))
        .shadow_md(hex("#00000066"))
        .overflow_hidden()
        .children([titlebar, div().w_full().h(Px(1.0)).bg(hex(BORDER)), body])
}

fn widget_card(stars: u32, sw: f32) -> Element {
    // Animated toggle — its knob eases across on state change (see `sw`).
    let track_col = hex(BORDER).lerp(hex(ACCENT_GREEN), sw);
    let knob = div()
        .absolute()
        .pos(3.0 + sw * 24.0, 3.0)
        .w(Px(22.0))
        .h(Px(22.0))
        .rounded_px(999.0)
        .bg(hex(TEXT_HI));
    let track = div()
        .id("sw-anim")
        .cursor(Cursor::Pointer)
        .w(Px(52.0))
        .h(Px(28.0))
        .rounded_px(999.0)
        .bg(track_col)
        .children([knob]);
    let toggle_row = div().w_full().flex_row().items_center().justify_between().children([
        div().flex_col().gap(1.0).children([
            text("is_animating()").mono().font_size(13.0).color(hex(TEXT_HI)),
            text(if sw > 0.5 { "continuous redraw" } else { "redraw on input" })
                .font_size(11.0)
                .color(hex(TEXT_DIM)),
        ]),
        track,
    ]);

    let divider = div().w_full().h(Px(1.0)).bg(hex(BORDER));

    let star_row = div().w_full().flex_row().items_center().justify_between().children([
        text("GitHub stars").font_size(13.0).color(hex(TEXT_MID)),
        text(&format!("★ {stars}")).mono().font_size(13.0).color(hex(ACCENT_AMBER)),
    ]);

    let chip = div()
        .flex_row()
        .items_center()
        .justify_center()
        .px_pad(Px(14.0))
        .py(Px(9.0))
        .rounded_px(10.0)
        .bg(hex(SURFACE_HI))
        .border(1.0, hex(BORDER))
        .hover(|s| s.translate_y(-3.0).glow_sm(hex(ACCENT_PURPLE)).border_color(hex(ACCENT_PURPLE)))
        .spring_transition(240.0, 20.0)
        .children([text("Hover me — real spring").font_size(12.5).font_weight(500).color(hex(TEXT_HI))]);

    div()
        .w(Px(320.0))
        .flex_col()
        .gap(16.0)
        .p(Px(22.0))
        .bg(hex(PANEL))
        .rounded_px(14.0)
        .border(1.0, hex(BORDER))
        .shadow_md(hex("#00000066"))
        .children([
            text("LIVE WIDGETS").font_size(11.0).font_weight(600).letter_spacing(1.4).color(hex(TEXT_DIM)),
            toggle_row,
            divider,
            star_row,
            chip,
        ])
}

fn showcase_screen(stars: u32, sw: f32, t: f32) -> Element {
    div().flex_col().items_center().gap(20.0).children([
        eyebrow("DOGFOODED · THIS PANEL RENDERS ITSELF"),
        div().flex_row().items_start().gap(20.0).children([code_panel(t), widget_card(stars, sw)]),
    ])
}

// ── Screen 3: Get started ─────────────────────────────────────────────────
fn shell_line(prompt: &str, cmd: &str, cmd_color: &str) -> Element {
    div().flex_row().items_center().gap(8.0).children([
        text(prompt).mono().font_size(13.5).color(hex(ACCENT_GREEN)),
        text(cmd).mono().font_size(13.5).color(hex(cmd_color)),
    ])
}

fn step(n: &str, title: &str, body: &str) -> Element {
    div().flex_row().items_center().gap(12.0).children([
        div().w(Px(26.0)).h(Px(26.0)).rounded_px(999.0).bg(hex(SURFACE_HI)).border(1.0, hex(BORDER))
            .flex_row().items_center().justify_center()
            .children([text(n).font_size(12.5).font_weight(600).color(hex(ACCENT_BLUE))]),
        div().flex_col().gap(1.0).children([
            text(title).font_size(14.0).font_weight(600).color(hex(TEXT_HI)),
            text(body).font_size(12.5).color(hex(TEXT_DIM)),
        ]),
    ])
}

fn start_screen(stars: u32) -> Element {
    let terminal = div()
        .w(Px(440.0))
        .flex_col()
        .gap(6.0)
        .p(Px(20.0))
        .bg(hex(PANEL))
        .rounded_px(12.0)
        .border(1.0, hex(BORDER))
        .shadow_md(hex("#00000066"))
        .children([
            shell_line("$", "cargo add sabitori", TEXT_HI),
            shell_line(" ", "", TEXT_DIM),
            code_line(&[("sabitori", ACCENT_BLUE), ("::", TEXT_DIM), ("run_declarative", METHOD), ("(", TEXT_DIM), ("App", ACCENT_CYAN), (");", TEXT_DIM)]),
        ]);

    let steps = div().flex_col().gap(14.0).children([
        step("1", "Add the crate", "One dependency pulls the whole workspace."),
        step("2", "impl DeclarativeApp", "Write view(), return an Element tree."),
        step("3", "run_declarative(app)", "Desktop today, WebGPU/WebGL2 tomorrow."),
    ]);

    div().flex_col().items_center().gap(24.0).children([
        eyebrow("SHIP IT IN THREE LINES"),
        div().flex_col().items_center().gap(6.0).children([
            text("Start building.").font_size(46.0).font_weight(500).letter_spacing(-1.4).color(hex(TEXT_HI)),
            text("The same code runs on the desktop and the web.").font_size(15.5).color(hex(TEXT_MID)),
        ]),
        div().flex_row().items_start().gap(28.0).children([terminal, steps]),
        div().flex_row().gap(14.0).children([
            cta_primary("cta-star", &format!("★  Star on GitHub   {stars}")),
            cta_ghost("nav-0", "←  Back to home"),
        ]),
    ])
}

// ── App ────────────────────────────────────────────────────────────────────
struct Landing {
    stars: u32,
    t: f32,
    cur: usize,
    prev: usize,
    /// 0.0 → 1.0 progress of the current screen transition (1.0 = settled).
    trans: f32,
    /// +1 navigating forward (higher tab), −1 backward. Sets slide direction.
    dir: f32,
    toggle: bool,
    /// Eased 0..1 mirror of `toggle` — drives the switch knob.
    sw: f32,
}

impl Landing {
    fn goto(&mut self, i: usize) {
        if i < TABS.len() && i != self.cur {
            self.prev = self.cur;
            self.dir = if i > self.cur { 1.0 } else { -1.0 };
            self.cur = i;
            self.trans = 0.0;
        }
    }

    fn screen(&self, idx: usize, t: f32, accent: Color) -> Element {
        match idx {
            0 => home(self.stars, accent),
            1 => features_screen(),
            2 => showcase_screen(self.stars, self.sw, t),
            _ => start_screen(self.stars),
        }
    }
}

fn stage_screen(cw: f32, sh: f32, content: Element, dx: f32, op: f32) -> Element {
    // Absolute wrapper — slide via its left inset (`pos.x`), fade via opacity.
    // `translate_x` only exists on the hover-style builder, not base Element.
    div()
        .absolute()
        .pos(dx, 0.0)
        .w(Px(cw))
        .h(Px(sh))
        .flex_col()
        .items_center()
        .justify_center()
        .opacity(op)
        .children([content])
}

impl DeclarativeApp for Landing {
    fn title(&self) -> &str {
        "Sabitori — GPU UI for Rust"
    }
    fn size(&self) -> (f32, f32) {
        (1160.0, 860.0)
    }

    fn is_animating(&self) -> bool {
        true
    }

    fn tick(&mut self, dt: f32) {
        self.t += dt;
        if self.trans < 1.0 {
            self.trans = (self.trans + dt * 3.2).min(1.0);
        }
        let target = if self.toggle { 1.0 } else { 0.0 };
        self.sw += (target - self.sw) * (dt * 11.0).min(1.0);
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        let cw = ctx.width.min(1040.0);
        let t = self.t;
        let stage_h = (ctx.height - 158.0).max(320.0);

        // Hue-cycling accent, shared by headline + nav underline.
        let cyc = 0.5 + 0.5 * (t * 0.35).sin();
        let accent = hex(ACCENT_BLUE).lerp(hex(ACCENT_CYAN), cyc * 0.85);

        // Two screens overlap only while transitioning; one when settled.
        let stage_kids: Vec<Element> = if self.trans < 1.0 {
            let e = ease_out_cubic(self.trans);
            let off = 54.0;
            vec![
                stage_screen(cw, stage_h, self.screen(self.prev, t, accent), -self.dir * e * off, 1.0 - e),
                stage_screen(cw, stage_h, self.screen(self.cur, t, accent), self.dir * (1.0 - e) * off, e),
            ]
        } else {
            vec![stage_screen(cw, stage_h, self.screen(self.cur, t, accent), 0.0, 1.0)]
        };

        let stage = div().w(Px(cw)).h(Px(stage_h)).children(stage_kids);

        let footer = text("MIT · RENDERED BY SABITORI ITSELF · NOT A SINGLE LINE OF HTML")
            .font_size(10.5)
            .font_weight(500)
            .letter_spacing(0.8)
            .color(hex(TEXT_DIM));

        div()
            .w(Px(ctx.width))
            .h(Px(ctx.height))
            .gradient(hex(BG0), hex(BG1), 90.0)
            .flex_col()
            .items_center()
            .gap(10.0)
            .pt(Px(26.0))
            .pb(Px(20.0))
            .children([
                backdrop(ctx.width, ctx.height, t),
                nav_bar(cw, self.cur, self.prev, self.trans, accent, t),
                stage,
                footer,
            ])
    }

    fn on_click(&mut self, id: &str) {
        if let Some(rest) = id.strip_prefix("nav-") {
            if let Ok(i) = rest.parse::<usize>() {
                self.goto(i);
            }
        } else if id == "cta-star" {
            self.stars += 1;
        } else if id == "sw-anim" {
            self.toggle = !self.toggle;
        }
    }
}

fn main() {
    sabitori::run_declarative(Landing {
        stars: 128,
        t: 0.0,
        cur: 0,
        prev: 0,
        trans: 1.0,
        dir: 1.0,
        toggle: true,
        sw: 1.0,
    });
}
