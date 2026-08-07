//! LP pattern — **a full routed site** with a living GPU backdrop.
//!
//! Takes the `landing_flow` particle field and makes it an actual website: a
//! persistent nav bar whose active underline springs to the current tab, four
//! routed sections that slide + crossfade as you navigate, and the flow field
//! running continuously *behind* all of it so it never resets across screens.
//!
//! Everything — the flow sim, the section transitions, the nav underline — is
//! state advanced in `tick()` and recomposed every frame on the GPU. No router
//! library, no CSS transitions, no HTML.
//!
//! `cargo run --example landing_site`

use sabitori::*;

// ── Palette ────────────────────────────────────────────────────────────────
const BG0: &str = "#06070f";
const BG1: &str = "#0c1022";
const SURFACE: &str = "#141830";
const SURFACE_HI: &str = "#1b2140";
const PANEL: &str = "#0b0e1c";
const BORDER: &str = "#262c4e";
const TEXT_HI: &str = "#f2f5ff";
const TEXT_MID: &str = "#a7afd6";
const TEXT_DIM: &str = "#6b74a0";
const METHOD: &str = "#8aa2d8";
const BLUE: &str = "#6ea8ff";
const PURPLE: &str = "#b18cff";
const CYAN: &str = "#7de3ff";
const GREEN: &str = "#9ece6a";
const AMBER: &str = "#e0af68";

const TABS: [&str; 4] = ["Home", "Features", "Showcase", "Get started"];

// ── Flow field backdrop ──────────────────────────────────────────────────
const N: usize = 110;
const TRAIL: usize = 5;

fn hx(s: &str) -> Color {
    Color::from_hex(s)
}
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn ease_out_cubic(x: f32) -> f32 {
    1.0 - (1.0 - x).powi(3)
}
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
fn field(x: f32, y: f32, t: f32) -> (f32, f32) {
    let a = (x * 6.0 + t * 0.30).sin() + (y * 5.0 - t * 0.23).cos() + (x * 2.4 + y * 3.1 + t * 0.17).sin();
    let angle = a * 1.4;
    (angle.cos(), angle.sin())
}

fn backdrop(w: f32, h: f32, t: f32, parts: &[[f32; 2]]) -> Element {
    let blue = hx(BLUE);
    let cyan = hx(CYAN);
    let purple = hx(PURPLE);
    let mut dots: Vec<Element> = Vec::with_capacity(N * TRAIL);
    for p in parts {
        let (vx, vy) = field(p[0], p[1], t);
        let col = blue.lerp(cyan, 0.5 + 0.5 * vy).lerp(purple, (0.5 + 0.5 * vx) * 0.4);
        for k in 0..TRAIL {
            let kf = k as f32;
            let bx = p[0] - vx * 0.009 * kf;
            let by = p[1] - vy * 0.009 * kf;
            let fade = 1.0 - kf / TRAIL as f32;
            let size = 0.7 + 2.1 * fade;
            dots.push(
                div()
                    .absolute()
                    .pos(bx * w - size / 2.0, by * h - size / 2.0)
                    .w(Px(size))
                    .h(Px(size))
                    .rounded_px(size)
                    .bg(col)
                    .opacity(0.4 * fade),
            );
        }
    }
    div().absolute().pos(0.0, 0.0).w(Px(w)).h(Px(h)).children(dots)
}

// ── Shared bits ───────────────────────────────────────────────────────────
fn eyebrow(txt: &str) -> Element {
    div()
        .flex_row()
        .items_center()
        .px_pad(Px(14.0))
        .py(Px(7.0))
        .rounded_px(999.0)
        .bg(hx(SURFACE).with_alpha(0.7))
        .border(1.0, hx(BORDER))
        .children([text(txt).font_size(11.0).font_weight(600).letter_spacing(1.8).color(hx(TEXT_DIM))])
}
fn pill(label: &str) -> Element {
    div()
        .px_pad(Px(12.0))
        .py(Px(5.0))
        .rounded_px(999.0)
        .bg(hx(SURFACE).with_alpha(0.7))
        .border(1.0, hx(BORDER))
        .children([text(label).font_size(11.5).font_weight(500).letter_spacing(0.3).color(hx(TEXT_MID))])
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
        .bg(hx(SURFACE).with_alpha(0.7))
        .border(1.0, hx(BORDER))
        .children([
            div().w(Px(7.0)).h(Px(7.0)).rounded_px(999.0).bg(hx(GREEN)).glow_sm(hx(GREEN)).opacity(pulse),
            text("60 FPS").font_size(10.5).font_weight(600).letter_spacing(1.0).color(hx(TEXT_MID)),
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
        .gradient(hx(BLUE), hx(PURPLE), 0.0)
        .glow_sm(hx(BLUE))
        .hover(|s| s.translate_y(-2.0).glow(hx(PURPLE), 22.0))
        .spring_transition(260.0, 22.0)
        .children([text(label).font_size(14.5).font_weight(600).letter_spacing(0.3).color(hx("#0a0c16"))])
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
        .bg(hx(SURFACE).with_alpha(0.8))
        .border(1.0, hx(BORDER))
        .hover(|s| s.bg(hx(SURFACE_HI)).border_color(hx(BLUE)))
        .spring_transition(260.0, 22.0)
        .children([text(label).font_size(14.5).font_weight(500).color(hx(TEXT_HI))])
}

// ── Nav with spring-driven active underline ───────────────────────────────
fn nav_bar(cw: f32, cur: usize, prev: usize, trans: f32, accent: Color, t: f32) -> Element {
    let tabw = 112.0;
    let gap = 6.0;
    let center = |i: usize| (i as f32) * (tabw + gap) + tabw / 2.0;
    let ux = lerp(center(prev), center(cur), ease_back(trans));
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
                    .color(if active { hx(TEXT_HI) } else { hx(TEXT_MID) })])
        })
        .collect();
    let tabs_box = div()
        .flex_col()
        .w(Px(4.0 * tabw + 3.0 * gap))
        .h(Px(40.0))
        .children([div().flex_row().gap(gap).children(tabs), underline]);

    let logo = div().flex_row().items_center().gap(10.0).children([
        div().w(Px(22.0)).h(Px(22.0)).rounded_px(6.0).gradient(hx(BLUE), hx(PURPLE), 45.0).glow_sm(hx(BLUE)),
        text("sabitori").font_size(18.0).font_weight(600).letter_spacing(0.2).color(hx(TEXT_HI)),
    ]);
    let right = div().flex_row().items_center().gap(18.0).children([live_badge(t), tabs_box]);

    div().w(Px(cw)).h(Px(52.0)).flex_row().items_center().justify_between().children([logo, right])
}

// ── Sections ──────────────────────────────────────────────────────────────
fn home(stars: u32, accent: Color) -> Element {
    div().flex_col().items_center().gap(24.0).children([
        eyebrow("GPU-NATIVE · REBUILT EVERY FRAME"),
        div().flex_col().items_center().gap(4.0).children([
            text("Build UI in Rust that").font_size(56.0).font_weight(300).letter_spacing(-1.2).line_height(1.08).color(hx(TEXT_HI)),
            text("flows like a shader.").font_size(56.0).font_weight(500).letter_spacing(-1.2).line_height(1.08).color(accent),
        ]),
        div().flex_col().items_center().gap(2.0).children([
            text("A routed, animated site — nav, transitions, a live particle").font_size(16.5).line_height(1.5).color(hx(TEXT_MID)),
            text("field — all recomposed every frame. No router. No CSS. No HTML.").font_size(16.5).line_height(1.5).color(hx(TEXT_MID)),
        ]),
        div().flex_row().gap(14.0).children([
            cta_primary("cta-star", &format!("★  Star   {stars}")),
            cta_ghost("nav-1", "Explore features  →"),
        ]),
        div().flex_row().gap(8.0).children([pill("13 crates"), pill("Spring routing"), pill("WebGPU · WebGL2")]),
    ])
}

fn feature_card(a: &str, b: &str, accent: &str, title: &str, l1: &str, l2: &str) -> Element {
    div()
        .w(Px(300.0))
        .min_h(Px(184.0))
        .flex_col()
        .gap(13.0)
        .p(Px(24.0))
        .bg(hx(SURFACE).with_alpha(0.82))
        .rounded_px(16.0)
        .border(1.0, hx(BORDER))
        .shadow_md(hx("#00000066"))
        .hover(|s| s.translate_y(-6.0).glow_sm(hx(accent)).border_color(hx(accent)))
        .spring_transition(240.0, 24.0)
        .children([
            div().w(Px(40.0)).h(Px(40.0)).rounded_px(11.0).gradient(hx(a), hx(b), 45.0).glow_sm(hx(a)),
            text(title).font_size(16.5).font_weight(600).letter_spacing(-0.2).color(hx(TEXT_HI)),
            div().flex_col().gap(1.0).children([
                text(l1).font_size(13.0).line_height(1.5).color(hx(TEXT_MID)),
                text(l2).font_size(13.0).line_height(1.5).color(hx(TEXT_MID)),
            ]),
        ])
}
fn features() -> Element {
    div().flex_col().items_center().gap(22.0).children([
        eyebrow("WHAT THE DOM CAN'T DO"),
        div().flex_row().justify_center().gap(20.0).children([
            feature_card(BLUE, PURPLE, BLUE, "Living backdrops", "A particle flow field integrated", "every frame — not a video, not CSS."),
            feature_card(CYAN, BLUE, CYAN, "Spring routing", "Sections slide + crossfade on a", "physics curve. This nav does it now."),
            feature_card(GREEN, AMBER, GREEN, "SDF everything", "Glow, gradients, shadows, rounded", "borders — one GPU pass, no images."),
        ]),
    ])
}

fn code_line(segs: &[(&str, &str)]) -> Element {
    let spans: Vec<Element> = segs.iter().map(|(s, c)| text(*s).mono().font_size(13.0).line_height(1.7).color(hx(c))).collect();
    div().flex_row().children(spans)
}
fn showcase(t: f32) -> Element {
    let dot = |c: &str| div().w(Px(11.0)).h(Px(11.0)).rounded_px(999.0).bg(hx(c));
    let caret_on = (t * 3.0).sin() > 0.0;
    let titlebar = div().w_full().flex_row().items_center().gap(8.0).px_pad(Px(16.0)).py(Px(12.0)).children([
        dot("#ff5f57"),
        dot("#febc2e"),
        dot("#28c840"),
        div().w(Px(14.0)),
        text("examples/landing_site.rs").mono().font_size(12.0).color(hx(TEXT_DIM)),
    ]);
    let body = div().flex_col().px_pad(Px(20.0)).py(Px(16.0)).children([
        code_line(&[("for", PURPLE), (" p ", TEXT_HI), ("in", PURPLE), (" &self.parts ", TEXT_HI), ("{", TEXT_DIM)]),
        code_line(&[("    let", PURPLE), (" (vx, vy) = ", TEXT_HI), ("field", METHOD), ("(p[0], p[1], t)", TEXT_HI), (";", TEXT_DIM)]),
        code_line(&[("    p[0] = (p[0] + vx * ", TEXT_HI), ("0.05", AMBER), (" * dt).", TEXT_HI), ("rem_euclid", METHOD), ("(", TEXT_DIM), ("1.0", AMBER), (");", TEXT_DIM)]),
        div().flex_row().items_center().gap(4.0).children([
            code_line(&[("}", TEXT_DIM)]),
            div().w(Px(8.0)).h(Px(16.0)).rounded_px(2.0).bg(hx(CYAN)).opacity(if caret_on { 0.9 } else { 0.05 }),
        ]),
    ]);
    let panel = div()
        .w(Px(560.0))
        .flex_col()
        .bg(hx(PANEL).with_alpha(0.9))
        .rounded_px(14.0)
        .border(1.0, hx(BORDER))
        .shadow_md(hx("#00000077"))
        .overflow_hidden()
        .children([titlebar, div().w_full().h(Px(1.0)).bg(hx(BORDER)), body]);
    div().flex_col().items_center().gap(16.0).children([
        eyebrow("THE BACKDROP DRIVING THIS PAGE"),
        panel,
        text("~15 lines advect 110 particles. The rest of the page is on top of it.").font_size(13.5).color(hx(TEXT_MID)),
    ])
}

fn step(n: &str, title: &str, body: &str) -> Element {
    div().flex_row().items_center().gap(12.0).children([
        div().w(Px(26.0)).h(Px(26.0)).rounded_px(999.0).bg(hx(SURFACE_HI)).border(1.0, hx(BORDER)).flex_row().items_center().justify_center()
            .children([text(n).font_size(12.5).font_weight(600).color(hx(BLUE))]),
        div().flex_col().gap(1.0).children([
            text(title).font_size(14.0).font_weight(600).color(hx(TEXT_HI)),
            text(body).font_size(12.5).color(hx(TEXT_DIM)),
        ]),
    ])
}
fn start(stars: u32) -> Element {
    let terminal = div()
        .w(Px(420.0))
        .flex_col()
        .gap(6.0)
        .p(Px(20.0))
        .bg(hx(PANEL).with_alpha(0.9))
        .rounded_px(12.0)
        .border(1.0, hx(BORDER))
        .shadow_md(hx("#00000077"))
        .children([
            div().flex_row().gap(8.0).children([text("$").mono().font_size(13.5).color(hx(GREEN)), text("cargo add sabitori").mono().font_size(13.5).color(hx(TEXT_HI))]),
            div().h(Px(4.0)),
            code_line(&[("sabitori", BLUE), ("::", TEXT_DIM), ("run_declarative", METHOD), ("(", TEXT_DIM), ("Site", CYAN), ("::", TEXT_DIM), ("new", METHOD), ("());", TEXT_DIM)]),
        ]);
    div().flex_col().items_center().gap(24.0).children([
        eyebrow("SHIP IT IN THREE LINES"),
        text("Start building.").font_size(44.0).font_weight(500).letter_spacing(-1.2).color(hx(TEXT_HI)),
        div().flex_row().items_start().gap(28.0).children([
            terminal,
            div().flex_col().gap(14.0).children([
                step("1", "Add the crate", "One dep pulls the whole workspace."),
                step("2", "impl DeclarativeApp", "Write view(), return an Element tree."),
                step("3", "run_declarative(app)", "Desktop today, the browser tomorrow."),
            ]),
        ]),
        div().flex_row().gap(14.0).children([
            cta_primary("cta-star", &format!("★  Star on GitHub   {stars}")),
            cta_ghost("nav-0", "←  Back to home"),
        ]),
    ])
}

// ── App ────────────────────────────────────────────────────────────────────
struct Site {
    t: f32,
    parts: Vec<[f32; 2]>,
    cur: usize,
    prev: usize,
    trans: f32,
    dir: f32,
    stars: u32,
}

impl Site {
    fn new() -> Self {
        let parts = (0..N as u32).map(|i| [hash01(i * 2 + 1), hash01(i * 3 + 7)]).collect();
        Site { t: 0.0, parts, cur: 0, prev: 0, trans: 1.0, dir: 1.0, stars: 128 }
    }
    fn goto(&mut self, i: usize) {
        if i < TABS.len() && i != self.cur {
            self.prev = self.cur;
            self.dir = if i > self.cur { 1.0 } else { -1.0 };
            self.cur = i;
            self.trans = 0.0;
        }
    }
    fn section(&self, idx: usize, t: f32, accent: Color) -> Element {
        match idx {
            0 => home(self.stars, accent),
            1 => features(),
            2 => showcase(t),
            _ => start(self.stars),
        }
    }
}

fn stage_screen(cw: f32, sh: f32, content: Element, dx: f32, op: f32) -> Element {
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

impl DeclarativeApp for Site {
    fn title(&self) -> &str {
        "Sabitori — Site"
    }
    fn size(&self) -> (f32, f32) {
        (1160.0, 800.0)
    }
    fn is_animating(&self) -> bool {
        true
    }

    fn tick(&mut self, dt: f32) {
        let dt = dt.min(0.05);
        let t = self.t;
        for p in &mut self.parts {
            let (vx, vy) = field(p[0], p[1], t);
            p[0] = (p[0] + vx * 0.05 * dt).rem_euclid(1.0);
            p[1] = (p[1] + vy * 0.05 * dt).rem_euclid(1.0);
        }
        self.t += dt;
        if self.trans < 1.0 {
            self.trans = (self.trans + dt * 3.2).min(1.0);
        }
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        let cw = ctx.width.min(1040.0);
        let t = self.t;
        let stage_h = (ctx.height - 150.0).max(320.0);
        let cyc = 0.5 + 0.5 * (t * 0.35).sin();
        let accent = hx(BLUE).lerp(hx(CYAN), cyc * 0.85);

        let stage_kids: Vec<Element> = if self.trans < 1.0 {
            let e = ease_out_cubic(self.trans);
            let off = 54.0;
            vec![
                stage_screen(cw, stage_h, self.section(self.prev, t, accent), -self.dir * e * off, 1.0 - e),
                stage_screen(cw, stage_h, self.section(self.cur, t, accent), self.dir * (1.0 - e) * off, e),
            ]
        } else {
            vec![stage_screen(cw, stage_h, self.section(self.cur, t, accent), 0.0, 1.0)]
        };
        let stage = div().w(Px(cw)).h(Px(stage_h)).children(stage_kids);

        let footer = text("MIT · RENDERED BY SABITORI ITSELF · NOT A SINGLE LINE OF HTML")
            .font_size(10.5)
            .font_weight(500)
            .letter_spacing(0.8)
            .color(hx(TEXT_DIM));

        div()
            .w(Px(ctx.width))
            .h(Px(ctx.height))
            .gradient(hx(BG0), hx(BG1), 90.0)
            .flex_col()
            .items_center()
            .gap(10.0)
            .pt(Px(26.0))
            .pb(Px(20.0))
            .children([
                backdrop(ctx.width, ctx.height, t, &self.parts),
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
        }
    }
}

fn main() {
    sabitori::run_declarative(Site::new());
}
