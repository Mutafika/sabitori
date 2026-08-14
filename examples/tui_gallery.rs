/// TUI Animation Gallery — wabi-inspired animated components on GPU.

use sabitori::*;
use sabitori_style::{AnsiPalette, Theme};
use std::time::Instant;

// ── Gallery state ──

const SPLASH_DURATION: f32 = 4.5;

struct Gallery {
    theme: Theme,
    start: Instant,
    selected: usize,
    /// elapsed() at which `selected` last changed — drives the switch fade-in.
    sel_at: f32,
    /// TUI (crisp mono) ↔ Modern (proportional, soft) chrome skin.
    modern: bool,
    splash_done: bool,
    // Animation states (framework types)
    typewriter_st: TypewriterState,
    spinners_st: Vec<SpinnerState>,
    progress_st: Vec<ProgressBarState>,
    gradient_st: GradientState,
    wave_st: WaveState,
    pulse_st: PulseState,
    color_cycle_st: ColorCycleState,
    // Interaction (keep as-is)
    transition_view: usize,
    transition_start: f32,
    transition_kind: usize,
    modal_open: bool,
    modal_start: f32,
    toasts: Vec<(f32, usize)>,
    next_toast_id: usize,
    // Scroll
    scroll_y: f32,
    scroll_target: f32,
    /// 選択項目を見える位置へ動かす要求。 `scroll_intents` が 1 度だけ吸い出す。
    ///
    /// 以前は `sidebar_scroll: f32` を直に持って `.scroll_offset()` に渡していたが、
    /// ランタイムが `Overflow::Scroll` の要素を全部管理していたため**毎フレーム
    /// 上書きされ、この値は一度も効いていなかった** (issue #14)。 プログラム的な
    /// スクロールは `scroll_intents` が正しい口。
    sidebar_scroll_intent: Option<f32>,
    // Cached presets
    presets: Vec<Theme>,
    // Toggle states
    toggles: [bool; 4],
    toggle_changed: [f32; 4],
    // Context menu
    ctx_menu: Option<(f32, f32)>,
    // Focus tracking (synced from ViewContext)
    focused: Option<String>,
    // Form controls
    form_text: sabitori_widgets::TextInputState,
    form_checks: [bool; 3],
    form_radio: usize,
    form_slider: f32,
    form_dropdown_open: bool,
    form_dropdown_sel: usize,
    slider_dragging: bool,
    // Splash preview
    splash_preview_idx: usize,
    splash_preview_start: f32,
}

const ITEMS: &[&str] = &[
    // 0-7: Text
    "Typewriter",       // 0
    "Gradient Text",    // 1
    "Wave Text",        // 2
    "Terminal",          // 3
    "Matrix Rain",       // 4
    // 5-12: Widgets
    "Progress Bars",    // 5
    "Spinners",         // 6
    "Toggle",           // 7
    "Tabs",             // 8
    "Counter",          // 9
    "Skeleton",         // 10
    "Sparkline",        // 11
    "Clock",            // 12
    // 13-19: Visual
    "Easing Curves",    // 13
    "Pulse Border",     // 14
    "Color Tween",      // 15
    "Gradient",         // 16
    "Glassmorphism",    // 17
    "Bento Grid",       // 18
    "Heatmap",          // 19
    // 20-24: GPU
    "Orbit",            // 20
    "Glow Pulse",       // 21
    "Morph",            // 22
    "Particles",        // 23
    "Smooth Motion",    // 24
    "Carousel",         // 25
    // 26: Themes
    "Theme Gallery",    // 26
    // 27: Splash
    "Splash Presets",   // 27
    // 28: Forms
    "Form Controls",    // 27
    // 28-31: Interaction
    "Context Menu",     // 28
    "View Transition",  // 28
    "Modal",            // 29
    "Toast",            // 30
];

// Color palettes used by multiple demos
const COLOR_CYCLE_PALETTE: &[&str] = &[
    "#ff6b6b", "#fbbf24", "#4ade80", "#22d3ee", "#c084fc", "#f472b6",
];

impl Gallery {
    fn elapsed(&self) -> f32 {
        self.start.elapsed().as_secs_f32()
    }

    /// Skin-aware label: proportional (Modern) vs mono (TUI). Use for demo
    /// headings, captions and descriptions — keep digits / ascii / data on
    /// `.mono()` so tabular content doesn't jitter.
    fn lbl(&self, s: &str, size: f32, weight: u16, col: Color) -> Element {
        if self.modern {
            text(s).font_size(size).font_weight(weight).letter_spacing(0.25).color(col)
        } else {
            // Preserve the TUI look: mono, and bold for headings (weight >= 600).
            let e = text(s).mono().font_size(size).color(col);
            if weight >= 600 { e.bold() } else { e }
        }
    }

    /// 選択中の項目がサイドバーの見える範囲に来るよう要求する。
    ///
    /// 現在のスクロール位置はランタイムが持っているので、 「足りない分だけ動かす」
    /// 判断はこちらでは書けない。 選択項目が中央に来る位置を要求し、 上下端の
    /// クランプはランタイム側 (`smooth_scroll_to`) に任せる。
    fn ensure_sidebar_visible(&mut self) {
        let item_h = 22.0;
        let viewport = 500.0; // サイドバーのおおよその可視高さ
        let y = self.selected as f32 * item_h;
        self.sidebar_scroll_intent = Some((y - viewport * 0.5).max(0.0));
    }

    fn content_h(&self) -> f32 {
        match self.selected {
            12 => 550.0, // Clock (analog + 4 digital)
            16 => 550.0, // Gradient
            26 => 500.0, // Theme Gallery (was 26, now shifted)
            _ => 0.0,
        }
    }

    // ── Component renderers ──

    fn typewriter(&self, t: &Theme) -> Element {
        let shown = self.typewriter_st.visible_text();
        let cursor_on = self.typewriter_st.cursor_visible();
        let display = if cursor_on {
            format!("{shown}▌")
        } else {
            shown.to_string()
        };

        div().w_full().flex_col().gap(8.0).children([
            text("Typewriter").mono().bold().font_size(14.0).color(t.primary).shrink(0.0),
            text(&display).mono().font_size(14.0).color(t.text_primary).shrink(0.0),
            text("Characters reveal one-by-one with blinking cursor.")
                .mono().font_size(14.0).color(t.text_disabled).shrink(0.0),
        ])
    }

    fn gradient_text(&self, t: &Theme) -> Element {
        let content = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

        let chars: Vec<Element> = content.chars().enumerate().map(|(i, c)| {
            let color = self.gradient_st.color_at(i);
            text(String::from(c)).mono().font_size(14.0).color(color).shrink(0.0)
        }).collect();

        div().w_full().flex_col().gap(8.0).children([
            self.lbl("Gradient Text", 15.0, 700, t.primary).shrink(0.0),
            div().flex_row().children(chars),
            self.lbl("Per-character color interpolation across gradient stops.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn progress_bars(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let labels = ["download", "compile", "upload", "deploy"];
        let colors = [a.cyan, a.yellow, a.green, a.magenta];

        let bars: Vec<Element> = self.progress_st.iter().enumerate().map(|(i, pb)| {
            let bar_str = pb.bar_string(30);
            let pct = format!("{:3.0}%", pb.progress() * 100.0);

            div().w_full().h(Px(22.0)).shrink(0.0).flex_row().items_center().children([
                self.lbl(labels[i], 13.0, 500, t.text_secondary).w(Px(70.0)).shrink(0.0),
                text(&bar_str).mono().font_size(14.0).color(colors[i]).shrink(0.0),
                text(&pct).mono().font_size(14.0).color(t.text_disabled).shrink(0.0).pl(Px(6.0)),
            ])
        }).collect();

        let mut children = vec![
            self.lbl("Progress Bars", 15.0, 700, t.primary).shrink(0.0),
        ];
        children.extend(bars);
        children.push(
            self.lbl("Smooth eased progress with staggered delays.", 12.5, 400, t.text_disabled).shrink(0.0),
        );

        div().w_full().flex_col().gap(6.0).children(children)
    }

    fn spinners(&self, t: &Theme) -> Element {
        let names = ["Braille dots", "Line", "Blocks", "Bounce", "Growing"];

        let spinners: Vec<Element> = self.spinners_st.iter().enumerate().map(|(i, sp)| {
            div().flex_row().gap(8.0).items_center().shrink(0.0).children([
                text(sp.current_frame()).mono().font_size(14.0).color(t.primary).shrink(0.0),
                text(names[i]).mono().font_size(14.0).color(t.text_secondary).shrink(0.0),
            ])
        }).collect();

        let mut children = vec![
            text("Spinners").mono().bold().font_size(14.0).color(t.primary).shrink(0.0),
        ];
        children.extend(spinners);
        children.push(
            text("Frame-based character cycling at configurable intervals.")
                .mono().font_size(14.0).color(t.text_disabled).shrink(0.0),
        );

        div().w_full().flex_col().gap(6.0).children(children)
    }

    fn easing_curves(&self, t: &Theme) -> Element {
        let elapsed = self.elapsed();
        let cycle = 3.0;
        let raw_t = (elapsed % cycle) / cycle;

        let curve = |name: &str, easing: EasingFunction, color: Color| -> Element {
            let val = easing.eval(raw_t);
            let bar_w = (val * 200.0).max(1.0);
            div().w_full().h(Px(20.0)).shrink(0.0).flex_row().items_center().children([
                self.lbl(name, 12.5, 500, t.text_secondary)
                    .w(Px(90.0)).shrink(0.0),
                div().w(Px(bar_w)).h(Px(8.0)).shrink(0.0).bg(color).rounded_px(if self.modern { 4.0 } else { 1.0 }),
            ])
        };

        div().w_full().flex_col().gap(3.0).children([
            self.lbl("Easing Curves", 15.0, 700, t.primary).shrink(0.0),
            curve("linear",          EasingFunction::Linear,          Color::from_hex("#888888")),
            curve("ease-in",         EasingFunction::EaseInQuad,      Color::from_hex("#ff6b6b")),
            curve("ease-out",        EasingFunction::EaseOutQuad,     Color::from_hex("#4ade80")),
            curve("ease-in-out",     EasingFunction::EaseInOutQuad,   Color::from_hex("#fbbf24")),
            curve("ease-out-cubic",  EasingFunction::EaseOutCubic,    Color::from_hex("#22d3ee")),
            curve("ease-out-back",   EasingFunction::EaseOutBack,     Color::from_hex("#c084fc")),
            curve("ease-out-elastic",EasingFunction::EaseOutElastic,  Color::from_hex("#f472b6")),
            self.lbl("Visualizing 7 easing functions in real-time.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn pulse_border(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let pulse_color = self.pulse_st.apply_to_color(a.cyan);

        div().w_full().flex_col().gap(8.0).children([
            self.lbl("Pulse Border", 15.0, 700, t.primary).shrink(0.0),
            block("System Status")
                .border_color(pulse_color)
                .title_color(pulse_color)
                .bg(t.surface)
                .padding(8.0)
                .children([
                    self.lbl("● All services operational", 13.0, 500, a.green),
                    self.lbl("  Uptime: 99.97%", 13.0, 400, t.text_secondary),
                ]),
            self.lbl("Border brightness oscillates with ease-in-out ping-pong.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn color_tween(&self, t: &Theme) -> Element {
        let current = self.color_cycle_st.current_color();
        let active_idx = self.color_cycle_st.active_index();
        let colors: Vec<Color> = COLOR_CYCLE_PALETTE.iter().map(|h| Color::from_hex(h)).collect();

        let swatch = |color: Color, active: bool| -> Element {
            let h = if active { 20.0 } else { 14.0 };
            div().w(Px(40.0)).h(Px(h)).shrink(0.0).bg(color).rounded_px(2.0)
        };

        let swatches: Vec<Element> = colors.iter().enumerate().map(|(i, &c)| {
            swatch(c, i == active_idx)
        }).collect();

        div().w_full().flex_col().gap(8.0).children([
            self.lbl("Color Tween", 15.0, 700, t.primary).shrink(0.0),
            div().w_full().h(Px(32.0)).shrink(0.0).bg(current).rounded_px(if self.modern { 8.0 } else { 3.0 }),
            div().flex_row().gap(4.0).items_center().children(swatches),
            self.lbl("RGB channel interpolation with eased transitions.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn wave_text(&self, t: &Theme) -> Element {
        let content = "hello, sabitori!";

        let chars: Vec<Element> = content.chars().enumerate().map(|(i, c)| {
            let top_pad = self.wave_st.offset_at(i);
            text(String::from(c)).mono().font_size(14.0).color(t.text_primary)
                .shrink(0.0).pt(Px(top_pad))
        }).collect();

        div().w_full().flex_col().gap(8.0).children([
            self.lbl("Wave Text", 15.0, 700, t.primary).shrink(0.0),
            div().h(Px(30.0)).shrink(0.0).flex_row().items_end().children(chars),
            self.lbl("Per-character sine wave Y offset animation.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    // ════════════════════════════════════════
    // GPU-Only effects (impossible in terminal)
    // ════════════════════════════════════════

    fn orbit(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();
        let pi2 = std::f32::consts::PI * 2.0;
        let cx = 200.0;
        let cy = 140.0;

        // 8 orbs at different speeds and radii
        let orbs: &[(f32, f32, f32, Color)] = &[
            (60.0, 1.0,  6.0, a.cyan),
            (60.0, 1.0,  4.0, a.cyan.with_alpha(0.4)),   // trail
            (45.0, 1.7,  5.0, a.green),
            (45.0, 1.7,  3.0, a.green.with_alpha(0.4)),
            (80.0, 0.6,  7.0, a.magenta),
            (80.0, 0.6,  5.0, a.magenta.with_alpha(0.3)),
            (30.0, 2.5,  4.0, a.yellow),
            (30.0, 2.5,  2.5, a.yellow.with_alpha(0.3)),
        ];

        let mut elements: Vec<Element> = Vec::new();

        // Center dot
        elements.push(
            div().w(Px(8.0)).h(Px(8.0)).shrink(0.0)
                .bg(t.text_primary).rounded_px(4.0)
                .m(Px(0.0))
        );

        // Orbit rings (faint circles approximated with rounded rects)
        for &(radius, _, _, color) in &orbs[..3] {
            let size = radius * 2.0;
            elements.push(
                div().w(Px(size)).h(Px(size)).shrink(0.0)
                    .border(1.0, color.with_alpha(0.08))
                    .rounded_px(radius)
            );
        }

        // Orbiting dots
        for (i, &(radius, speed, size, color)) in orbs.iter().enumerate() {
            let delay = if i % 2 == 1 { -0.15 } else { 0.0 }; // trail offset
            let angle = (elapsed + delay) * speed * pi2;
            let ox = cx + angle.cos() * radius - size / 2.0;
            let oy = cy + angle.sin() * radius - size / 2.0;

            elements.push(
                div().w(Px(size)).h(Px(size)).shrink(0.0)
                    .bg(color)
                    .rounded_px(size / 2.0)
                    .glow(color, 6.0)
                    .m(Px(0.0))
            );
            // Position hack: use margin to place at orbit position
            // (Since we can't absolute-position in flex, we use a fixed-size container)
            let _ = (ox, oy); // positions calculated but flex doesn't support absolute yet
        }

        // Calculate actual positions
        let mut positioned: Vec<(f32, f32, f32, Color)> = Vec::new();
        for &(radius, speed, size, color) in orbs {
            let angle = elapsed * speed * pi2;
            let ox = cx + angle.cos() * radius;
            let oy = cy + angle.sin() * radius;
            positioned.push((ox, oy, size, color));
        }
        // Sort by Y for layering
        positioned.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        // Render as stacked rows
        let mut rows: Vec<Element> = Vec::new();
        let mut last_y: f32 = 0.0;
        for &(x, y, size, color) in &positioned {
            let dy = (y - last_y).max(0.0);
            rows.push(
                div().h(Px(dy)).shrink(0.0)
            );
            rows.push(
                div().w_full().shrink(0.0).flex_row().children([
                    div().w(Px(x.max(0.0))).shrink(0.0),
                    div().w(Px(size)).h(Px(size)).shrink(0.0)
                        .bg(color).rounded_px(size / 2.0)
                        .glow(color, 6.0),
                ]),
            );
            last_y = y + size;
        }

        div().w_full().flex_col().gap(0.0).children([
            self.lbl("Orbit — GPU Only", 15.0, 700, t.primary).shrink(0.0).pb(Px(4.0)),
            div().w_full().h(Px(300.0)).shrink(0.0)
                .bg(t.surface)
                .rounded_px(4.0)
                .flex_col()
                .overflow_hidden()
                .children(rows),
            self.lbl("Sub-pixel circular motion + glow trails. Impossible in cell grid.", 12.5, 400, t.text_disabled).shrink(0.0).pt(Px(4.0)),
        ])
    }

    fn glow_pulse(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();

        let glowing_box = |label: &str, color: Color, phase: f32, speed: f32| -> Element {
            let raw = (elapsed * speed + phase) % 1.0;
            let ping = if raw < 0.5 { raw * 2.0 } else { 2.0 - raw * 2.0 };
            let intensity = EasingFunction::EaseInOutQuad.eval(ping);
            let blur = 4.0 + intensity * 16.0;

            div().flex_1().h(Px(60.0)).shrink(0.0)
                .bg(t.surface_elevated)
                .border(1.0, color.with_alpha(0.3 + intensity * 0.7))
                .rounded_px(6.0)
                .glow(color.with_alpha(intensity * 0.5), blur)
                .flex_col().items_center().justify_center()
                .children([
                    self.lbl(label, 13.0, 700, color).shrink(0.0),
                ])
        };

        div().w_full().flex_col().gap(8.0).children([
            self.lbl("Glow Pulse — GPU Only", 15.0, 700, t.primary).shrink(0.0),
            div().w_full().flex_row().gap(8.0).children([
                glowing_box("ACTIVE",  a.green,   0.0, 0.8),
                glowing_box("WARNING", a.yellow,  0.3, 1.0),
                glowing_box("ALERT",   a.bright_red, 0.6, 1.5),
            ]),
            div().w_full().flex_row().gap(8.0).children([
                glowing_box("SYNC",    a.cyan,    0.1, 0.6),
                glowing_box("BUILD",   a.magenta, 0.5, 1.2),
                glowing_box("DEPLOY",  a.blue,    0.8, 0.9),
            ]),
            self.lbl("Animated blur radius + alpha. Terminal can only toggle on/off.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn morph(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();
        let cycle = 3.0;
        let raw = (elapsed % cycle) / cycle;
        let ping = if raw < 0.5 { raw * 2.0 } else { 2.0 - raw * 2.0 };
        let eased = EasingFunction::EaseOutBack.eval(ping.min(1.0).max(0.0));

        // Morph: square → circle
        let size = 80.0;
        let radius = eased * (size / 2.0); // 0 = square, 40 = circle

        // Color shift
        let color = a.cyan.lerp(a.magenta, eased);

        // Size breathe
        let scale = 0.8 + eased * 0.4; // 0.8x → 1.2x
        let actual = size * scale;
        let shadow_blur = eased * 12.0;

        // Multiple morphing shapes at different phases
        let shape = |phase: f32, base_color: Color| -> Element {
            let p = ((elapsed * 0.7 + phase) % cycle) / cycle;
            let pp = if p < 0.5 { p * 2.0 } else { 2.0 - p * 2.0 };
            let e = EasingFunction::EaseOutCubic.eval(pp);
            let r = e * 25.0;
            let s = 40.0 + e * 20.0;
            let c = base_color.lerp(t.primary, e);
            div().w(Px(s)).h(Px(s)).shrink(0.0)
                .bg(c.with_alpha(0.7))
                .rounded_px(r)
                .glow(c, 4.0 + e * 8.0)
        };

        div().w_full().flex_col().gap(8.0).children([
            self.lbl("Morph — GPU Only", 15.0, 700, t.primary).shrink(0.0),
            div().w_full().h(Px(120.0)).shrink(0.0)
                .flex_row().items_center().justify_center().gap(16.0)
                .children([
                    // Main shape
                    div().w(Px(actual)).h(Px(actual)).shrink(0.0)
                        .bg(color.with_alpha(0.8))
                        .rounded_px(radius * scale)
                        .glow(color, shadow_blur)
                        .border(1.0, color),
                    // Smaller companions
                    div().flex_col().gap(8.0).children([
                        shape(0.0, a.green),
                        shape(1.0, a.yellow),
                    ]),
                    div().flex_col().gap(8.0).children([
                        shape(0.5, a.red),
                        shape(1.5, a.blue),
                    ]),
                ]),
            self.lbl("Continuous corner-radius + size + color + shadow. All sub-pixel.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn particles(&self, t: &Theme) -> Element {
        let elapsed = self.elapsed();
        let area_w = 500.0;
        let area_h = 280.0;
        let count = 40;

        let mut dots: Vec<(f32, f32, f32, Color)> = Vec::new();
        for i in 0..count {
            let seed = i as f32 * 7.31;
            let speed_x = ((seed * 1.3).sin() * 0.5 + 0.5) * 30.0 + 10.0;
            let speed_y = ((seed * 2.7).cos() * 0.5 + 0.5) * 20.0 + 5.0;
            let phase_x = seed * 3.14;
            let phase_y = seed * 1.57;

            let x = ((elapsed * speed_x * 0.02 + phase_x).sin() * 0.5 + 0.5) * (area_w - 6.0);
            let y = ((elapsed * speed_y * 0.02 + phase_y).cos() * 0.5 + 0.5) * (area_h - 6.0);
            let size = 2.0 + ((seed * 5.0).sin() * 0.5 + 0.5) * 4.0;

            let hue = (i as f32 / count as f32 + elapsed * 0.1) % 1.0;
            let color = Color::new(
                (hue * 6.28).sin() * 0.5 + 0.5,
                ((hue + 0.33) * 6.28).sin() * 0.5 + 0.5,
                ((hue + 0.66) * 6.28).sin() * 0.5 + 0.5,
                0.7,
            );
            dots.push((x, y, size, color));
        }

        dots.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let mut rows: Vec<Element> = Vec::new();
        let mut last_y: f32 = 0.0;
        for &(x, y, size, color) in &dots {
            let dy = (y - last_y).max(0.0);
            if dy > 0.5 {
                rows.push(div().h(Px(dy)).shrink(0.0));
            }
            rows.push(
                div().w_full().shrink(0.0).flex_row().children([
                    div().w(Px(x.max(0.0))).shrink(0.0),
                    div().w(Px(size)).h(Px(size)).shrink(0.0)
                        .bg(color).rounded_px(size / 2.0),
                ]),
            );
            last_y = y;
        }

        div().w_full().flex_col().gap(0.0).children([
            self.lbl("Particles — GPU Only", 15.0, 700, t.primary).shrink(0.0).pb(Px(4.0)),
            div().w(Px(area_w)).h(Px(area_h)).shrink(0.0)
                .bg(t.surface)
                .rounded_px(4.0)
                .flex_col()
                .overflow_hidden()
                .children(rows),
            self.lbl(&format!("{count} independent dots, sub-pixel positions, per-dot color."), 12.5, 400, t.text_disabled).shrink(0.0).pt(Px(4.0)),
        ])
    }

    fn smooth_motion(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();
        let track_w = 400.0;

        let runner = |label: &str, easing: EasingFunction, color: Color, speed: f32| -> Element {
            let cycle = speed;
            let raw = (elapsed % cycle) / cycle;
            let ping = if raw < 0.5 { raw * 2.0 } else { 2.0 - raw * 2.0 };
            let x = easing.eval(ping) * (track_w - 16.0);

            div().w_full().h(Px(22.0)).shrink(0.0).flex_col().children([
                self.lbl(label, 12.5, 500, t.text_disabled).shrink(0.0),
                div().w(Px(track_w)).h(Px(12.0)).shrink(0.0)
                    .bg(t.surface)
                    .rounded_px(2.0)
                    .flex_row().items_center()
                    .children([
                        div().w(Px(x)).shrink(0.0),
                        div().w(Px(16.0)).h(Px(10.0)).shrink(0.0)
                            .bg(color).rounded_px(5.0)
                            .glow(color, 3.0),
                    ]),
            ])
        };

        div().w_full().flex_col().gap(6.0).children([
            self.lbl("Smooth Motion — GPU Only", 15.0, 700, t.primary).shrink(0.0),
            runner("linear",          EasingFunction::Linear,         a.white,       2.0),
            runner("ease-out-cubic",  EasingFunction::EaseOutCubic,   a.cyan,        2.0),
            runner("ease-in-out",     EasingFunction::EaseInOutQuad,  a.green,       2.0),
            runner("ease-out-back",   EasingFunction::EaseOutBack,    a.magenta,     2.5),
            runner("ease-out-elastic",EasingFunction::EaseOutElastic, a.yellow,      3.0),
            self.lbl("Sub-pixel position + rounded shapes + glow. Terminal snaps to cells.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn gradient_demo(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();
        let pi = std::f32::consts::PI;
        let angle = (elapsed * 0.3) % (pi * 2.0);

        // Hero card with rotating gradient
        let hero = div().w_full().h(Px(120.0)).shrink(0.0)
            .gradient(t.primary, a.magenta, angle)
            .rounded_px(12.0)
            .shadow_md(t.primary.with_alpha(0.3))
            .flex_col().items_center().justify_center().gap(4.0)
            .children([
                self.lbl("sabitori", 20.0, 700, Color::WHITE).shrink(0.0),
                self.lbl("GPU-accelerated UI framework", 12.5, 500, Color::new(1.0, 1.0, 1.0, 0.7)).shrink(0.0),
            ]);

        // Feature cards with subtle gradients
        let card = |title: &str, desc: &str, from: Color, to: Color| -> Element {
            div().flex_1().shrink(0.0)
                .gradient(from, to, pi / 3.0)
                .rounded_px(8.0)
                .p(Px(12.0))
                .flex_col().gap(4.0)
                .children([
                    self.lbl(title, 14.0, 700, Color::WHITE).shrink(0.0),
                    self.lbl(desc, 12.5, 400, Color::new(1.0, 1.0, 1.0, 0.6)).shrink(0.0),
                ])
        };

        let cards = div().w_full().flex_row().gap(8.0).children([
            card("Layout", "Flexbox", a.blue.with_alpha(0.6), a.cyan.with_alpha(0.3)),
            card("Animate", "60fps+", a.magenta.with_alpha(0.6), a.red.with_alpha(0.3)),
            card("Theme", "11 presets", a.green.with_alpha(0.6), a.yellow.with_alpha(0.3)),
        ]);

        // Stat bars with gradient fills
        let stat_bar = |label: &str, pct: f32, from: Color, to: Color| -> Element {
            div().w_full().h(Px(24.0)).shrink(0.0).flex_row().items_center().gap(8.0).children([
                self.lbl(label, 12.5, 500, t.text_secondary).w(Px(60.0)).shrink(0.0),
                div().flex_1().h(Px(8.0)).shrink(0.0)
                    .bg(t.surface_elevated).rounded_px(4.0)
                    .overflow_hidden()
                    .children([
                        div().w(Percent(pct)).h_full()
                            .gradient(from, to, 0.0)
                            .rounded_px(4.0),
                    ]),
                text(&format!("{:.0}%", pct)).mono().font_size(14.0).color(t.text_disabled).shrink(0.0),
            ])
        };

        let stats = div().w_full().flex_col().gap(4.0).children([
            stat_bar("CPU", 73.0, a.cyan, a.blue),
            stat_bar("Memory", 45.0, a.green, a.yellow),
            stat_bar("Disk", 91.0, a.red, a.magenta),
        ]);

        div().w_full().flex_col().gap(12.0).children([
            self.lbl("Gradient — GPU Only", 15.0, 700, t.primary).shrink(0.0),
            hero,
            cards,
            stats,
            text(".gradient(from, to, angle) — per-pixel on GPU")
                .mono().font_size(14.0).color(t.text_disabled).shrink(0.0),
        ])
    }

    fn bento_grid(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let cell = |w: f32, h: f32, label: &str, color: Color| -> Element {
            div().w(Px(w)).h(Px(h)).shrink(0.0)
                .bg(color.with_alpha(0.1))
                .border(1.0, color.with_alpha(0.2))
                .rounded_px(8.0)
                .flex_col().items_center().justify_center()
                .children([
                    self.lbl(label, 13.0, 600, color).shrink(0.0),
                ])
        };

        // Row 1: wide + square
        let row1 = div().w_full().shrink(0.0).flex_row().gap(6.0).children([
            cell(280.0, 80.0, "Overview", a.cyan),
            cell(130.0, 80.0, "CPU", a.green),
            cell(130.0, 80.0, "Mem", a.yellow),
        ]);

        // Row 2: square + tall + square
        let row2 = div().w_full().shrink(0.0).flex_row().gap(6.0).children([
            cell(130.0, 120.0, "Logs", a.magenta),
            div().flex_col().gap(6.0).shrink(0.0).children([
                cell(210.0, 57.0, "Network", a.blue),
                cell(210.0, 57.0, "Storage", a.bright_red),
            ]),
            cell(190.0, 120.0, "Chart", a.bright_cyan),
        ]);

        div().w_full().flex_col().gap(8.0).children([
            self.lbl("Bento Grid — Layout", 15.0, 700, t.primary).shrink(0.0),
            div().w_full().flex_col().gap(6.0).children([row1, row2]),
            self.lbl("Mixed-size card grid. Flexbox composition, no CSS grid needed.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn glassmorphism(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();

        let glass = |label: &str, value: &str, accent: Color, phase: f32| -> Element {
            let shimmer = (((elapsed + phase) * 0.8).sin() * 0.5 + 0.5) * 0.03;
            div().flex_1().shrink(0.0)
                .bg(accent.with_alpha(0.06 + shimmer))
                .border(1.0, Color::new(1.0, 1.0, 1.0, 0.08))
                .rounded_px(12.0)
                .shadow_md(Color::new(0.0, 0.0, 0.0, 0.4))
                .p(Px(14.0))
                .flex_col().justify_between()
                .children([
                    self.lbl(label, 12.5, 500, t.text_secondary).shrink(0.0),
                    self.lbl(value, 16.0, 700, accent).shrink(0.0),
                    div().w(Px(30.0)).h(Px(2.0)).shrink(0.0)
                        .bg(accent.with_alpha(0.4)).rounded_px(1.0),
                ])
        };

        div().w_full().flex_col().gap(8.0).children([
            self.lbl("Glassmorphism — GPU Only", 15.0, 700, t.primary).shrink(0.0),
            // Row 1
            div().w_full().flex_row().gap(6.0).children([
                glass("Revenue", "$12.4k", a.green, 0.0),
                glass("Users", "1,284", a.cyan, 0.7),
                glass("Uptime", "99.9%", a.magenta, 1.4),
            ]),
            // Row 2: one wide + one narrow
            div().w_full().flex_row().gap(6.0).children([
                div().flex_1().shrink(0.0)
                    .bg(a.blue.with_alpha(0.05))
                    .border(1.0, Color::new(1.0, 1.0, 1.0, 0.06))
                    .rounded_px(12.0)
                    .shadow_sm(Color::new(0.0, 0.0, 0.0, 0.3))
                    .p(Px(14.0))
                    .flex_col().gap(6.0)
                    .children([
                        self.lbl("Activity", 12.5, 500, t.text_secondary).shrink(0.0),
                        // Mini bar chart
                        div().flex_row().gap(3.0).items_end().h(Px(30.0)).shrink(0.0).children(
                            (0..12).map(|i| {
                                let h = ((i as f32 * 1.7 + elapsed * 0.5).sin() * 0.5 + 0.5) * 28.0 + 2.0;
                                div().w(Px(8.0)).h(Px(h)).shrink(0.0)
                                    .bg(a.blue.with_alpha(0.3))
                                    .rounded_px(2.0)
                            }).collect::<Vec<_>>()
                        ),
                    ]),
                glass("Errors", "3", a.bright_red, 2.1),
            ]),
            self.lbl("Tinted translucent cards + white borders + shadow depth.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    // ════════════════════════════════════════
    // Skeleton / Counter / Sparkline / Matrix / Tabs
    // ════════════════════════════════════════

    fn skeleton(&self, t: &Theme) -> Element {
        let elapsed = self.elapsed();
        // Shimmer: a bright band sweeps across
        let shimmer_phase = (elapsed * 0.8) % 1.0;

        let skel_bar = |w: f32, h: f32| -> Element {
            let shimmer_x = shimmer_phase * (w + 60.0) - 60.0;
            div().w(Px(w)).h(Px(h)).shrink(0.0)
                .bg(t.surface_elevated)
                .rounded_px(4.0)
                .overflow_hidden()
                .children([
                    // Shimmer highlight
                    div().w(Px(60.0)).h_full()
                        .bg(Color::new(1.0, 1.0, 1.0, 0.04))
                        .rounded_px(4.0)
                        .ml(Px(shimmer_x)),
                ])
        };

        // Fake card skeleton
        let card = |i: usize| -> Element {
            let delay = i as f32 * 0.15;
            let phase = ((elapsed * 0.8 + delay) % 1.0) * 400.0 - 60.0;
            div().w_full().shrink(0.0)
                .bg(t.surface)
                .rounded_px(8.0)
                .p(Px(12.0))
                .flex_col().gap(8.0)
                .children([
                    // Avatar + title
                    div().flex_row().gap(8.0).items_center().children([
                        div().w(Px(32.0)).h(Px(32.0)).shrink(0.0)
                            .bg(t.surface_elevated).rounded_px(16.0)
                            .overflow_hidden()
                            .children([
                                div().w(Px(32.0)).h_full()
                                    .bg(Color::new(1.0, 1.0, 1.0, 0.04))
                                    .rounded_px(16.0)
                                    .ml(Px(phase.max(-32.0).min(32.0))),
                            ]),
                        div().flex_1().flex_col().gap(4.0).children([
                            skel_bar(160.0, 12.0),
                            skel_bar(100.0, 10.0),
                        ]),
                    ]),
                    // Body lines
                    skel_bar(300.0, 10.0),
                    skel_bar(250.0, 10.0),
                    skel_bar(200.0, 10.0),
                ])
        };

        div().w_full().flex_col().gap(10.0).children([
            self.lbl("Loading Skeleton", 15.0, 700, t.primary).shrink(0.0),
            card(0),
            card(1),
            self.lbl("Shimmer sweep animation on placeholder shapes.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn counter(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();

        let animated_num = |target: f32, label: &str, color: Color, speed: f32| -> Element {
            let cycle = 5.0;
            let raw = ((elapsed * speed) % cycle) / cycle;
            let eased = EasingFunction::EaseOutCubic.eval(raw.min(0.6) / 0.6);
            let value = (eased * target) as u64;

            div().flex_1().shrink(0.0)
                .bg(t.surface)
                .rounded_px(8.0)
                .p(Px(12.0))
                .flex_col().gap(4.0)
                .children([
                    self.lbl(label, 12.5, 500, t.text_disabled).shrink(0.0),
                    text(&format!("{}", value)).mono().bold().font_size(24.0).color(color).shrink(0.0),
                ])
        };

        div().w_full().flex_col().gap(10.0).children([
            self.lbl("Number Counter", 15.0, 700, t.primary).shrink(0.0),
            div().w_full().flex_row().gap(8.0).children([
                animated_num(1284032.0, "Requests", a.cyan, 0.2),
                animated_num(99.97, "Uptime %", a.green, 0.3),
                animated_num(7.0, "Errors", a.bright_red, 0.5),
            ]),
            div().w_full().flex_row().gap(8.0).children([
                animated_num(142.0, "P99 (ms)", a.yellow, 0.25),
                animated_num(48.0, "Active", a.magenta, 0.35),
                animated_num(3600.0, "Uptime (h)", a.blue, 0.15),
            ]),
            self.lbl("Eased count-up with configurable speed per counter.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn sparkline(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();

        let line = |label: &str, color: Color, seed: f32, bar_count: usize| -> Element {
            let bars: Vec<Element> = (0..bar_count).map(|i| {
                let v = ((i as f32 * 0.7 + seed + elapsed * 0.3).sin() * 0.5 + 0.5);
                let h = 4.0 + v * 36.0;
                div().w(Px(4.0)).h(Px(h)).shrink(0.0)
                    .bg(color.with_alpha(0.4 + v * 0.6))
                    .rounded_px(1.0)
            }).collect();

            div().w_full().shrink(0.0).flex_col().gap(4.0).children([
                self.lbl(label, 12.5, 500, t.text_secondary).shrink(0.0),
                div().w_full().h(Px(40.0)).shrink(0.0)
                    .flex_row().items_end().gap(2.0)
                    .children(bars),
            ])
        };

        div().w_full().flex_col().gap(12.0).children([
            self.lbl("Sparkline", 15.0, 700, t.primary).shrink(0.0),
            line("CPU Load", a.cyan, 0.0, 40),
            line("Network I/O", a.green, 3.14, 40),
            line("Disk IOPS", a.yellow, 1.57, 40),
            self.lbl("Per-bar height animated with sine wave + per-bar alpha.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn matrix_rain(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();
        let cols = 40;
        let rows = 12;

        let chars = "アイウエオカキクケコサシスセソタチツテト0123456789";
        let char_vec: Vec<char> = chars.chars().collect();

        let mut grid_rows: Vec<Element> = Vec::new();
        for row in 0..rows {
            let cells: Vec<Element> = (0..cols).map(|col| {
                let seed = (col as f32 * 7.31 + row as f32 * 3.14) as u32;
                let speed = 1.5 + (seed % 10) as f32 * 0.3;
                let phase = (elapsed * speed + col as f32 * 0.5) % (rows as f32 * 1.5);
                let dist = (phase - row as f32).abs();

                let alpha = if dist < 1.0 { 1.0 } else if dist < 3.0 { 0.6 / dist } else { 0.05 };
                let is_head = dist < 0.5;

                let char_idx = ((seed as f32 + elapsed * 8.0) as usize) % char_vec.len();
                let ch = char_vec[(char_idx + col + row * 7) % char_vec.len()];

                let color = if is_head {
                    Color::new(0.8, 1.0, 0.8, alpha)
                } else {
                    a.green.with_alpha(alpha * 0.8)
                };

                text(String::from(ch)).mono().font_size(12.0).color(color).shrink(0.0)
            }).collect();

            grid_rows.push(
                div().w_full().h(Px(16.0)).shrink(0.0).flex_row().children(cells)
            );
        }

        div().w_full().flex_col().gap(4.0).children([
            text("Matrix Rain").mono().bold().font_size(14.0).color(t.primary).shrink(0.0),
            div().w_full().shrink(0.0)
                .bg(Color::from_hex("#010a01"))
                .rounded_px(6.0)
                .p(Px(4.0))
                .flex_col()
                .overflow_hidden()
                .children(grid_rows),
            text("480 per-character color + alpha. Japanese katakana + digits.")
                .mono().font_size(14.0).color(t.text_disabled).shrink(0.0),
        ])
    }

    fn tabs_demo(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();
        let tab_count = 4;
        let active_tab = ((elapsed * 0.4) as usize) % tab_count;
        let labels = ["Overview", "Analytics", "Settings", "Logs"];
        let colors = [a.cyan, a.green, a.magenta, a.yellow];

        let tabs: Vec<Element> = labels.iter().enumerate().map(|(i, label)| {
            let is_active = i == active_tab;
            let color = if is_active { colors[i] } else { t.text_disabled };
            div().shrink(0.0)
                .flex_col().items_center().gap(4.0)
                .children([
                    self.lbl(*label, 13.5, 700, color)
                        .shrink(0.0)
                        .px_pad(Px(12.0)).py(Px(6.0)),
                    // Indicator bar
                    div().w(Px(if is_active { 40.0 } else { 0.0 })).h(Px(2.0)).shrink(0.0)
                        .bg(colors[i]).rounded_px(1.0),
                ])
        }).collect();

        // Tab content
        let content_color = colors[active_tab];
        let content = div().w_full().h(Px(100.0)).shrink(0.0)
            .bg(content_color.with_alpha(0.05))
            .border(1.0, content_color.with_alpha(0.15))
            .rounded_px(8.0)
            .flex_col().items_center().justify_center()
            .children([
                self.lbl(labels[active_tab], 16.0, 700, content_color).shrink(0.0),
                self.lbl("Tab content area", 12.5, 400, t.text_disabled).shrink(0.0),
            ]);

        div().w_full().flex_col().gap(10.0).children([
            self.lbl("Tabs", 15.0, 700, t.primary).shrink(0.0),
            // Tab bar
            div().w_full().shrink(0.0)
                .bg(t.surface)
                .rounded_px(8.0)
                .flex_row().justify_center().gap(4.0)
                .p(Px(4.0))
                .children(tabs),
            content,
            self.lbl("Auto-cycling tabs with indicator animation.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn heatmap(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();
        let cols = 24;
        let rows = 7;
        let days = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

        let mut grid_rows: Vec<Element> = Vec::new();
        for row in 0..rows {
            let mut cells: Vec<Element> = vec![
                self.lbl(days[row], 12.0, 500, t.text_disabled)
                    .w(Px(30.0)).shrink(0.0),
            ];
            for col in 0..cols {
                let seed = (row * cols + col) as f32 * 2.71;
                let base = ((seed * 1.3).sin() * 0.5 + 0.5);
                let wave = ((elapsed * 0.2 + seed * 0.1).sin() * 0.5 + 0.5) * 0.3;
                let v = (base + wave).min(1.0);

                let color = if v < 0.1 {
                    t.surface_elevated
                } else if v < 0.3 {
                    a.green.with_alpha(0.2)
                } else if v < 0.6 {
                    a.green.with_alpha(0.5)
                } else {
                    a.green.with_alpha(0.8)
                };
                cells.push(
                    div().w(Px(14.0)).h(Px(14.0)).shrink(0.0)
                        .bg(color).rounded_px(2.0)
                );
            }
            grid_rows.push(
                div().w_full().shrink(0.0).flex_row().gap(3.0).items_center()
                    .children(cells)
            );
        }

        div().w_full().flex_col().gap(8.0).children([
            self.lbl("Heatmap", 15.0, 700, t.primary).shrink(0.0),
            div().w_full().flex_col().gap(3.0).children(grid_rows),
            self.lbl("GitHub-style contribution grid. 168 cells with animated intensity.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn clock(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();
        let pi = std::f32::consts::PI;
        let modern = self.modern;
        // Skin-aware label: proportional in Modern, mono in TUI. Digits stay
        // mono in both — tabular numerals shouldn't jitter as they tick.
        let lbl = |s: &str, size: f32, col: Color| -> Element {
            if modern {
                text(s).font_size(size).font_weight(600).letter_spacing(0.3).color(col)
            } else {
                text(s).mono().font_size(14.0).color(col)
            }
        };
        let card_round = if modern { 12.0 } else { 8.0 };

        // Shared digit card builder
        let digit = |val: &str, color: Color, size: f32| -> Element {
            div().shrink(0.0)
                .bg(t.surface)
                .rounded_px(4.0)
                .px_pad(Px(6.0)).py(Px(4.0))
                .children([
                    text(val).mono().bold().font_size(size).color(color).shrink(0.0),
                ])
        };
        let sep = |color: Color| -> Element {
            let alpha = ((elapsed * 2.0).sin() * 0.5 + 0.5);
            text(":").mono().bold().font_size(20.0).color(color.with_alpha(alpha)).shrink(0.0)
        };
        let progress_bar = |pct: f32, from: Color, to: Color| -> Element {
            div().w_full().h(Px(4.0)).shrink(0.0)
                .bg(t.surface).rounded_px(2.0)
                .overflow_hidden()
                .children([
                    div().w(Percent(pct * 100.0)).h_full()
                        .gradient(from, to, 0.0).rounded_px(2.0),
                ])
        };

        // ── 1. Clock ──
        let secs = elapsed as u64;
        let clock_card = div().flex_1().shrink(0.0)
            .bg(t.surface_elevated).rounded_px(card_round)
            .border(1.0, if modern { t.border } else { Color::TRANSPARENT })
            .p(Px(10.0)).flex_col().gap(6.0)
            .children([
                lbl("Clock", 12.0, t.text_disabled).shrink(0.0),
                div().flex_row().items_center().justify_center().gap(3.0).children([
                    digit(&format!("{:02}", (secs / 3600) % 24), a.cyan, 20.0),
                    sep(a.cyan),
                    digit(&format!("{:02}", (secs / 60) % 60), a.green, 20.0),
                    sep(a.green),
                    digit(&format!("{:02}", secs % 60), a.magenta, 20.0),
                ]),
                progress_bar(elapsed.fract(), a.cyan, a.magenta),
            ]);

        // ── 2. Pomodoro (25 min cycle) ──
        let pomo_cycle = 25.0 * 60.0; // 25 min in seconds
        let pomo_elapsed = elapsed % pomo_cycle;
        let pomo_remaining = pomo_cycle - pomo_elapsed;
        let pomo_min = (pomo_remaining as u64) / 60;
        let pomo_sec = (pomo_remaining as u64) % 60;
        let pomo_pct = pomo_elapsed / pomo_cycle;
        let pomo_color = if pomo_pct > 0.9 { a.red } else if pomo_pct > 0.7 { a.yellow } else { a.green };

        let pomo_card = div().flex_1().shrink(0.0)
            .bg(t.surface_elevated).rounded_px(card_round)
            .border(1.0, if modern { t.border } else { Color::TRANSPARENT })
            .p(Px(10.0)).flex_col().gap(6.0)
            .children([
                div().flex_row().items_center().gap(6.0).children([
                    lbl("Pomodoro", 12.0, t.text_disabled).shrink(0.0),
                    div().flex_1(),
                    lbl(if pomo_pct > 0.9 { "BREAK!" } else { "Focus" }, 12.0, pomo_color).shrink(0.0),
                ]),
                div().flex_row().items_center().justify_center().gap(3.0).children([
                    digit(&format!("{:02}", pomo_min), pomo_color, 20.0),
                    sep(pomo_color),
                    digit(&format!("{:02}", pomo_sec), pomo_color, 20.0),
                ]),
                progress_bar(1.0 - pomo_pct, pomo_color, t.surface_hover),
            ]);

        // ── 3. Stopwatch (counts up with laps) ──
        let sw_total = elapsed % 120.0; // resets every 2 min
        let sw_min = (sw_total as u64) / 60;
        let sw_sec = (sw_total as u64) % 60;
        let sw_ms = ((sw_total.fract()) * 100.0) as u64;

        let laps: Vec<Element> = (0..3).map(|i| {
            let lap_time = ((i + 1) as f32 * 12.3 + 5.0).min(sw_total);
            lbl(&format!("Lap {} — {:.2}s", i + 1, lap_time), 12.5, t.text_disabled).shrink(0.0)
        }).collect();

        let sw_card = div().flex_1().shrink(0.0)
            .bg(t.surface_elevated).rounded_px(card_round)
            .border(1.0, if modern { t.border } else { Color::TRANSPARENT })
            .p(Px(10.0)).flex_col().gap(6.0)
            .children([
                lbl("Stopwatch", 12.0, t.text_disabled).shrink(0.0),
                div().flex_row().items_center().justify_center().gap(3.0).children([
                    digit(&format!("{:02}", sw_min), a.cyan, 20.0),
                    sep(a.cyan),
                    digit(&format!("{:02}", sw_sec), a.cyan, 20.0),
                    text(".").mono().font_size(20.0).color(t.text_disabled).shrink(0.0),
                    digit(&format!("{:02}", sw_ms), a.cyan.with_alpha(0.6), 20.0),
                ]),
                div().flex_col().gap(2.0).children(laps),
            ]);

        // ── 4. Countdown (to a fake event) ──
        let event_secs = 3600.0 * 2.5; // 2.5 hours fake target
        let cd_remaining = (event_secs - (elapsed % event_secs)).max(0.0);
        let cd_h = (cd_remaining as u64) / 3600;
        let cd_m = ((cd_remaining as u64) % 3600) / 60;
        let cd_s = (cd_remaining as u64) % 60;
        let cd_pct = cd_remaining / event_secs;
        let cd_color = if cd_pct < 0.1 { a.red } else { a.bright_cyan };

        let cd_card = div().w_full().shrink(0.0)
            .bg(t.surface_elevated).rounded_px(card_round)
            .border(1.0, if modern { t.border } else { Color::TRANSPARENT })
            .p(Px(10.0)).flex_col().gap(6.0)
            .children([
                div().flex_row().items_center().gap(6.0).children([
                    lbl("Countdown", 12.0, t.text_disabled).shrink(0.0),
                    div().flex_1(),
                    lbl("Deploy v3.0", 12.0, cd_color).shrink(0.0),
                ]),
                div().flex_row().items_center().justify_center().gap(4.0).children([
                    digit(&format!("{:02}h", cd_h), cd_color, 20.0),
                    digit(&format!("{:02}m", cd_m), cd_color, 20.0),
                    digit(&format!("{:02}s", cd_s), cd_color, 20.0),
                ]),
                progress_bar(cd_pct, cd_color, t.surface_hover),
            ]);

        // ── 5. Analog clock with absolute positioning ──
        let face_size = 200.0;
        let cx = face_size / 2.0;
        let cy = face_size / 2.0;
        let r = face_size / 2.0 - 14.0;

        let s_total = elapsed;
        let s_angle = (s_total % 60.0) / 60.0 * pi * 2.0 - pi / 2.0;
        let m_angle = ((s_total / 60.0) % 60.0) / 60.0 * pi * 2.0 - pi / 2.0;
        let h_angle = ((s_total / 3600.0) % 12.0) / 12.0 * pi * 2.0 - pi / 2.0;

        let mut face_children: Vec<Element> = Vec::new();

        // Hour numbers
        let hour_labels = ["12","1","2","3","4","5","6","7","8","9","10","11"];
        for i in 0..12 {
            let ang = (i as f32 / 12.0) * pi * 2.0 - pi / 2.0;
            let nr = r - 16.0;
            let x = cx + ang.cos() * nr - 8.0;
            let y = cy + ang.sin() * nr - 7.0;
            face_children.push(
                text(hour_labels[i]).mono().font_size(12.0)
                    .color(if i % 3 == 0 { t.text_primary } else { t.text_secondary })
                    .pos(x, y)
            );
        }

        // Tick marks (hour positions only)
        for i in 0..12 {
            let ang = (i as f32 / 12.0) * pi * 2.0 - pi / 2.0;
            let size = if i % 3 == 0 { 4.0 } else { 3.0 };
            let x = (cx + ang.cos() * r - size / 2.0).round();
            let y = (cy + ang.sin() * r - size / 2.0).round();
            face_children.push(
                div().w(Px(size)).h(Px(size))
                    .bg(t.text_secondary).rounded_px(size / 2.0)
                    .pos(x, y)
            );
        }

        // Draw a hand: overlapping dots for solid line appearance
        let mut draw_hand = |angle: f32, length: f32, width: f32, color: Color, children: &mut Vec<Element>| {
            let steps = (length / 1.5).max(3.0) as usize;
            for s in 0..=steps {
                let frac = s as f32 / steps as f32;
                let w = width * (1.0 - frac * 0.3);
                let x = (cx + angle.cos() * length * frac - w / 2.0).round();
                let y = (cy + angle.sin() * length * frac - w / 2.0).round();
                children.push(
                    div().w(Px(w)).h(Px(w))
                        .bg(color).rounded_px(w / 2.0)
                        .pos(x, y)
                );
            }
        };

        // Hour hand (short, thick, tapered)
        draw_hand(h_angle, r * 0.45, 6.0, a.cyan, &mut face_children);
        // Minute hand (longer, medium)
        draw_hand(m_angle, r * 0.65, 4.0, t.text_primary, &mut face_children);
        // Second hand (longest, thin, red)
        draw_hand(s_angle, r * 0.8, 2.0, a.red, &mut face_children);
        // Second hand tail (opposite direction, short)
        let tail_angle = s_angle + pi;
        draw_hand(tail_angle, r * 0.15, 2.0, a.red, &mut face_children);

        // Center cap
        face_children.push(
            div().w(Px(8.0)).h(Px(8.0))
                .bg(a.red).rounded_px(4.0)
                .pos(cx - 4.0, cy - 4.0)
        );

        let analog_card = div().w(Px(face_size)).h(Px(face_size)).shrink(0.0)
            .bg(t.surface_elevated)
            .rounded_px(face_size / 2.0)
            .border(2.0, t.border)
            .overflow_hidden()
            .children(face_children);

        div().w_full().flex_col().gap(8.0).children([
            if modern {
                text("Clock / Timer").font_size(16.0).font_weight(700).letter_spacing(0.2).color(t.primary).shrink(0.0)
            } else {
                text("Clock / Timer").mono().bold().font_size(14.0).color(t.primary).shrink(0.0)
            },
            div().w_full().flex_row().gap(8.0).children([
                analog_card,
                div().flex_1().flex_col().gap(6.0).children([
                    div().flex_row().gap(6.0).children([clock_card, pomo_card]),
                    div().flex_row().gap(6.0).children([sw_card, cd_card]),
                ]),
            ]),
            if modern {
                text("Analog clock — absolute positioning + sin/cos, 5 digital widgets.")
                    .font_size(12.5).color(t.text_disabled).shrink(0.0)
            } else {
                text("Analog clock: absolute positioning + sin/cos. 5 digital widgets.")
                    .mono().font_size(14.0).color(t.text_disabled).shrink(0.0)
            },
        ])
    }

    fn carousel(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();
        let items: &[(&str, Color)] = &[
            ("Warp Dark", a.cyan),
            ("Tokyo Night", a.blue),
            ("Catppuccin", a.magenta),
            ("Dracula", a.bright_magenta),
            ("Nord", a.bright_cyan),
        ];
        let cycle = 3.0;
        let active = ((elapsed / cycle) as usize) % items.len();
        let phase = (elapsed % cycle) / cycle;

        let cards: Vec<Element> = items.iter().enumerate().map(|(i, &(name, color))| {
            let is_active = i == active;
            let opacity = if is_active { 1.0 } else { 0.4 };
            let h = if is_active { 80.0 } else { 60.0 };

            div().w(Px(140.0)).h(Px(h)).shrink(0.0)
                .gradient(color.with_alpha(0.3 * opacity), color.with_alpha(0.1 * opacity), std::f32::consts::PI / 3.0)
                .border(1.0, color.with_alpha(0.3 * opacity))
                .rounded_px(8.0)
                .opacity(opacity)
                .flex_col().items_center().justify_center()
                .children([
                    self.lbl(name, 14.0, 700, color.with_alpha(opacity)).shrink(0.0),
                ])
        }).collect();

        // Progress dots
        let dots: Vec<Element> = (0..items.len()).map(|i| {
            let is_active = i == active;
            let size = if is_active { 8.0 } else { 4.0 };
            let color = if is_active { t.primary } else { t.text_disabled };
            div().w(Px(size)).h(Px(size)).shrink(0.0)
                .bg(color).rounded_px(size / 2.0)
        }).collect();

        div().w_full().flex_col().gap(10.0).children([
            self.lbl("Carousel", 15.0, 700, t.primary).shrink(0.0),
            div().w_full().shrink(0.0)
                .flex_row().items_center().justify_center().gap(8.0)
                .children(cards),
            div().w_full().shrink(0.0)
                .flex_row().items_center().justify_center().gap(6.0)
                .children(dots),
            self.lbl("Auto-cycling cards with opacity + size transitions.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn toggle(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();
        let labels = ["Dark Mode", "Notifications", "Auto-save", "Telemetry"];
        let colors = [a.cyan, a.green, a.magenta, a.yellow];

        let switches: Vec<Element> = (0..4).map(|i| {
            let on = self.toggles[i];
            let color = colors[i];
            let id = format!("toggle-{i}");

            // Animate knob position over 200ms
            let dur = 0.2;
            let anim_t = ((elapsed - self.toggle_changed[i]) / dur).min(1.0);
            let eased = EasingFunction::EaseOutCubic.eval(anim_t);
            let knob_x = if on { 2.0 + 26.0 * eased } else { 28.0 - 26.0 * eased };
            let blend = if on { eased } else { 1.0 - eased };
            let track_bg = color.with_alpha(0.4 * blend);
            let border_c = if blend > 0.5 { color.with_alpha(0.3) } else { t.border };

            div().id(&id).w_full().shrink(0.0).flex_row().items_center().gap(10.0).children([
                div().w(Px(50.0)).h(Px(26.0)).shrink(0.0)
                    .bg(track_bg)
                    .rounded_px(13.0)
                    .border(1.0, border_c)
                    .flex_row().items_center()
                    .children([
                        div().w(Px(knob_x)).shrink(0.0),
                        div().w(Px(20.0)).h(Px(20.0)).shrink(0.0)
                            .bg(Color::new(1.0, 1.0, 1.0, 0.5 + blend * 0.5))
                            .rounded_px(10.0)
                            .shadow_sm(Color::new(0.0, 0.0, 0.0, 0.2 * blend)),
                    ]),
                self.lbl(labels[i], 13.5, 500, if blend > 0.5 { t.text_primary } else { t.text_disabled }).shrink(0.0),
                div().flex_1(),
                self.lbl(if on { "ON" } else { "OFF" }, 12.0, 600, if on { color } else { t.text_disabled }).shrink(0.0),
            ])
        }).collect();

        div().w_full().flex_col().gap(10.0).children([
            self.lbl("Toggle Switch", 15.0, 700, t.primary).shrink(0.0),
            div().w_full().flex_col().gap(8.0)
                .bg(t.surface).rounded_px(8.0).p(Px(12.0))
                .children(switches),
            self.lbl("Click to toggle. State persists.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn terminal(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();

        let lines: &[(&str, f32, Color)] = &[
            ("$ cargo build --release", 0.0, a.green),
            ("   Compiling sabitori v0.1.0", 1.0, t.text_secondary),
            ("   Compiling sabitori-gpu v0.1.0", 1.8, t.text_secondary),
            ("   Compiling sabitori-text v0.1.0", 2.5, t.text_secondary),
            ("    Finished release [optimized] 4.2s", 3.5, a.bright_green),
            ("$ cargo run --example tui_gallery", 4.5, a.green),
            ("    Running target/release/examples/tui_gallery", 5.5, a.cyan),
        ];

        let visible: Vec<Element> = lines.iter().filter_map(|&(line, appear_at, color)| {
            if elapsed < appear_at { return None; }
            let age = elapsed - appear_at;
            let type_progress = (age / 0.5).min(1.0);
            let visible_chars = (type_progress * line.len() as f32) as usize;
            let shown = &line[..visible_chars.min(line.len())];

            Some(
                text(shown).mono().font_size(14.0).color(color).shrink(0.0)
            )
        }).collect();

        // Blinking cursor at end
        let cursor_on = ((elapsed * 2.0) as u32) % 2 == 0;
        let all_done = elapsed > 6.0;

        let mut children = vec![
            text("Terminal").mono().bold().font_size(14.0).color(t.primary).shrink(0.0),
        ];

        let mut term_children = visible;
        if cursor_on && !all_done {
            term_children.push(
                text("█").mono().font_size(14.0).color(a.green).shrink(0.0)
            );
        } else if all_done {
            // Reset cycle
            // Show a waiting cursor
            term_children.push(
                div().flex_row().gap(0.0).children([
                    text("$ ").mono().font_size(14.0).color(a.green).shrink(0.0),
                    if cursor_on {
                        text("█").mono().font_size(14.0).color(a.green).shrink(0.0)
                    } else {
                        div()
                    },
                ])
            );
        }

        children.push(
            div().w_full().shrink(0.0)
                .bg(Color::from_hex("#0c0c14"))
                .rounded_px(8.0)
                .p(Px(10.0))
                .flex_col().gap(2.0)
                .children(term_children)
        );
        children.push(
            text("Typewriter effect on terminal commands with staggered timing.")
                .mono().font_size(14.0).color(t.text_disabled).shrink(0.0)
        );

        div().w_full().flex_col().gap(8.0).children(children)
    }

    // ════════════════════════════════════════
    // Theme gallery
    // ════════════════════════════════════════

    fn theme_gallery(&self, _current: &Theme) -> Element {
        // Use cached presets — no allocation per frame
        let cards: Vec<Element> = self.presets.iter().enumerate().map(|(idx, theme)| {
            let is_active = theme.name == self.theme.name;
            let border_c = if is_active { theme.primary } else { theme.border };

            // Minimal card: name + 4 color dots. No text-heavy content.
            div().id(&format!("theme-{idx}"))
                .w(Px(160.0)).shrink(0.0)
                .bg(theme.surface)
                .border(if is_active { 2.0 } else { 1.0 }, border_c)
                .rounded_px(6.0)
                .p(Px(8.0))
                .flex_col().gap(6.0)
                .children([
                    // Name
                    self.lbl(&theme.name, 13.5, 700, theme.text_primary).shrink(0.0),
                    // Color bar: primary + success + warning + error
                    div().w_full().shrink(0.0).flex_row().gap(3.0).children([
                        div().flex_1().h(Px(6.0)).bg(theme.primary).rounded_px(2.0),
                        div().flex_1().h(Px(6.0)).bg(theme.success).rounded_px(2.0),
                        div().flex_1().h(Px(6.0)).bg(theme.warning).rounded_px(2.0),
                        div().flex_1().h(Px(6.0)).bg(theme.error).rounded_px(2.0),
                    ]),
                    // Surface preview
                    div().w_full().h(Px(20.0)).shrink(0.0)
                        .bg(theme.surface_elevated)
                        .rounded_px(3.0)
                        .flex_row().items_center().px_pad(Px(6.0))
                        .children([
                            self.lbl("Aa", 13.0, 500, theme.text_primary).shrink(0.0),
                        ]),
                ])
        }).collect();

        div().w_full().flex_col().gap(8.0).children([
            self.lbl("Theme Gallery", 15.0, 700, _current.primary).shrink(0.0),
            div().w_full().flex_1()
                .flex_row().flex_wrap(sabitori::element::FlexWrap::Wrap).gap(6.0)
                .children(cards),
        ])
    }

    // ════════════════════════════════════════
    // Splash presets demo
    // ════════════════════════════════════════

    fn splash_presets(&self, t: &Theme, a: &AnsiPalette, ctx: &ViewContext) -> Element {
        let all = SplashPreset::all();
        let preset = all[self.splash_preview_idx % all.len()];
        let elapsed = self.elapsed() - self.splash_preview_start;
        let logo = "sabitori";
        let total = logo.len();
        let char_w = 22.0;
        let logo_w = total as f32 * char_w;

        // Preview area
        let preview_w = 500.0;
        let preview_h = 120.0;
        let logo_left = (preview_w - logo_w) / 2.0;
        let logo_top = preview_h / 2.0 - 15.0;

        let chars: Vec<Element> = logo.chars().enumerate().map(|(i, ch)| {
            let (dx, dy, alpha) = preset.char_state(elapsed, i, total, preview_w);
            let x = logo_left + i as f32 * char_w + dx;
            let y = logo_top + dy;
            text(String::from(ch)).mono().bold().font_size(28.0)
                .color(t.primary.with_alpha(alpha))
                .pos(x, y)
        }).collect();

        let preview = div().w(Px(preview_w)).h(Px(preview_h)).shrink(0.0)
            .bg(t.surface_elevated)
            .rounded_px(8.0)
            .border(1.0, t.border)
            .overflow_hidden()
            .children(chars);

        // Preset buttons
        let buttons: Vec<Element> = all.iter().enumerate().map(|(i, p)| {
            let id = format!("splash-{i}");
            let active = i == self.splash_preview_idx;
            let hovered = ctx.hovered.as_deref() == Some(id.as_str());
            let bg_c = if active { t.primary } else if hovered { t.surface_active } else { t.surface };
            let fg = if active { Color::WHITE } else { t.text_secondary };
            div().id(&id).shrink(0.0)
                .bg(bg_c).rounded_px(4.0)
                .px_pad(Px(8.0)).py(Px(4.0))
                .children([
                    self.lbl(p.name(), 12.5, 500, fg).shrink(0.0),
                ])
        }).collect();

        // Auto-replay indicator
        let dur = preset.duration();
        let progress = (elapsed / dur).min(1.0);
        let replay_text = if elapsed > dur + 0.5 { "Click preset to replay" } else { "" };

        div().w_full().flex_col().gap(10.0).children([
            self.lbl("Splash Presets", 15.0, 700, t.primary).shrink(0.0),
            self.lbl(&format!("{} — {:.1}s", preset.name(), dur), 13.0, 500, t.text_secondary).shrink(0.0),
            preview,
            // Progress bar
            div().w(Px(preview_w)).h(Px(3.0)).shrink(0.0)
                .bg(t.surface_elevated).rounded_px(2.0)
                .overflow_hidden()
                .children([
                    div().w(Px(progress * preview_w)).h_full()
                        .gradient(t.primary, a.magenta, 0.0)
                        .rounded_px(2.0),
                ]),
            // Buttons grid
            div().w_full().flex_row().flex_wrap(sabitori::element::FlexWrap::Wrap).gap(4.0)
                .children(buttons),
            self.lbl(replay_text, 12.5, 400, t.text_disabled).shrink(0.0),
            text("SplashPreset::BounceIn.char_state(elapsed, i, total, w)")
                .mono().font_size(14.0).color(t.text_disabled).shrink(0.0),
        ])
    }

    // ════════════════════════════════════════
    // Form controls demo
    // ════════════════════════════════════════

    fn form_controls(&self, t: &Theme, a: &AnsiPalette, ctx: &ViewContext) -> Element {
        let is_focused = |id: &str| ctx.focused.as_deref() == Some(id);

        // Text input
        let text_display = self.form_text.display_text();
        let is_placeholder = self.form_text.text.is_empty();
        let input = form_text_input(
            "form-input",
            &text_display,
            is_placeholder,
            ((self.elapsed() * 2.0) as u32 % 2 == 0) && is_focused("form-input"),
            0.0,
            is_focused("form-input"),
            t.text_primary,
            t.text_disabled,
            t.surface,
            t.border,
            t.primary,
        );

        // Checkboxes
        let check_labels = ["Enable notifications", "Dark mode", "Auto-save"];
        let checks: Vec<Element> = (0..3).map(|i| {
            checkbox(
                &format!("form-check-{i}"),
                check_labels[i],
                self.form_checks[i],
                t.text_primary,
                a.green,
                t.border,
            )
        }).collect();

        // Radio buttons
        let radio_labels = ["Small", "Medium", "Large"];
        let radios: Vec<Element> = (0..3).map(|i| {
            radio(
                &format!("form-radio-{i}"),
                radio_labels[i],
                self.form_radio == i,
                t.text_primary,
                t.primary,
                t.border,
            )
        }).collect();

        // Slider
        let slider_el = slider(
            "form-slider",
            self.form_slider,
            300.0,
            t.surface_elevated,
            t.primary,
            Color::WHITE,
        );

        // Dropdown
        let dropdown_items = ["Option A", "Option B", "Option C", "Option D"];
        let dropdown_el = dropdown_trigger(
            "form-dropdown",
            dropdown_items[self.form_dropdown_sel],
            self.form_dropdown_open,
            t.text_primary,
            t.surface,
            t.border,
        );

        div().w_full().flex_col().gap(14.0).children([
            self.lbl("Form Controls", 15.0, 700, t.primary).shrink(0.0),

            // Text Input
            self.lbl("Text Input", 12.5, 600, t.text_disabled).shrink(0.0),
            input,

            // Checkboxes
            self.lbl("Checkbox", 12.5, 600, t.text_disabled).shrink(0.0),
            div().flex_col().gap(6.0).children(checks),

            // Radio
            self.lbl("Radio", 12.5, 600, t.text_disabled).shrink(0.0),
            div().flex_row().gap(16.0).children(radios),

            // Slider
            self.lbl(&format!("Slider — {:.0}%", self.form_slider * 100.0), 12.5, 600, t.text_disabled).shrink(0.0),
            slider_el,

            // Dropdown
            self.lbl("Dropdown", 12.5, 600, t.text_disabled).shrink(0.0),
            div().w(Px(200.0)).shrink(0.0).children([dropdown_el]),

            self.lbl("Focus system: click to focus, Tab to cycle, Esc to blur.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    // ════════════════════════════════════════
    // Interaction demos
    // ════════════════════════════════════════

    fn context_menu_demo(&self, t: &Theme) -> Element {
        div().w_full().flex_col().gap(12.0).children([
            self.lbl("Context Menu", 15.0, 700, t.primary).shrink(0.0),
            self.lbl("Right-click anywhere in this area to open a menu.", 13.0, 400, t.text_secondary).shrink(0.0),
            div().id("ctx-area")
                .w_full().h(Px(200.0)).shrink(0.0)
                .bg(t.surface_elevated)
                .border(1.0, t.border)
                .rounded_px(8.0)
                .flex_col().items_center().justify_center()
                .children([
                    self.lbl("Right-click here", 13.0, 500, t.text_disabled).shrink(0.0),
                ]),
            text("context_menu() + context_menu_item() + MenuItem from tui.rs")
                .mono().font_size(14.0).color(t.text_disabled).shrink(0.0),
        ])
    }

    fn view_transition(&self, t: &Theme, a: &AnsiPalette, _ctx: &ViewContext) -> Element {
        let elapsed = self.elapsed();
        let trans_t = ((elapsed - self.transition_start) / 0.4).min(1.0);
        let eased = EasingFunction::EaseOutCubic.eval(trans_t);
        let kind = self.transition_kind % 4;

        let is_even = self.transition_view % 2 == 0;
        let (color, label) = if is_even {
            (a.cyan, "View A")
        } else {
            (a.green, "View B")
        };

        // Simple colored rect — no text inside for perf
        let view_rect = div().w_full().h(Px(160.0)).shrink(0.0)
            .bg(color.with_alpha(0.2))
            .border(1.0, color.with_alpha(0.4))
            .rounded_px(4.0);

        let kind_name = ["slide", "fade", "scale", "wipe"][kind];

        let content = match kind {
            0 => {
                let offset = (1.0 - eased) * 500.0;
                div().w_full().h(Px(160.0)).shrink(0.0)
                    .overflow_hidden().flex_row()
                    .children([
                        div().w(Px(offset)).shrink(0.0),
                        view_rect,
                    ])
            }
            1 => view_rect.opacity(eased),
            2 => {
                let pad = (1.0 - eased) * 80.0;
                div().w_full().h(Px(160.0)).shrink(0.0)
                    .overflow_hidden().p(Px(pad))
                    .children([view_rect])
            }
            _ => {
                let reveal = eased * 600.0;
                div().w(Px(reveal)).h(Px(160.0)).shrink(0.0)
                    .overflow_hidden()
                    .children([view_rect])
            }
        };

        div().w_full().flex_col().gap(8.0).children([
            self.lbl("View Transition", 15.0, 700, t.primary).shrink(0.0),
            div().flex_row().gap(6.0).children(
                ["slide", "fade", "scale", "wipe"].iter().enumerate().map(|(i, name)| {
                    let id = format!("trans-{i}");
                    let active = kind == i;
                    div().id(&id).shrink(0.0)
                        .bg(if active { t.primary.with_alpha(0.2) } else { t.surface })
                        .rounded_px(3.0)
                        .border(1.0, if active { t.primary } else { t.border })
                        .px_pad(Px(8.0)).py(Px(3.0))
                        .children([
                            self.lbl(*name, 12.5, 500, if active { t.primary } else { t.text_secondary }).shrink(0.0),
                        ])
                }).collect::<Vec<_>>()
            ),
            content,
            // Label + trigger
            div().flex_row().gap(8.0).items_center().children([
                self.lbl(label, 14.0, 700, color).shrink(0.0),
                div().flex_1(),
                div().id("trans-trigger").shrink(0.0)
                    .bg(t.primary).rounded_px(3.0)
                    .px_pad(Px(10.0)).py(Px(4.0))
                    .children([
                        self.lbl("Switch", 13.0, 600, Color::WHITE).shrink(0.0),
                    ]),
                self.lbl(kind_name, 12.5, 400, t.text_disabled).shrink(0.0),
            ]),
        ])
    }

    fn modal(&self, t: &Theme, _a: &AnsiPalette) -> Element {
        div().w_full().flex_col().gap(12.0).children([
            self.lbl("Modal Dialog", 15.0, 700, t.primary).shrink(0.0),
            self.lbl("Full-screen overlay with backdrop dim + scale animation.", 12.5, 400, t.text_disabled).shrink(0.0),
            self.lbl("Press [m] or click the button below.", 12.5, 400, t.text_disabled).shrink(0.0),
            div().id("modal-toggle").shrink(0.0)
                .bg(t.primary).rounded_px(4.0)
                .px_pad(Px(14.0)).py(Px(6.0))
                .children([
                    self.lbl("Open Modal", 13.0, 600, Color::WHITE).shrink(0.0),
                ]),
        ])
    }

    fn toast(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();

        let toast_widget = |msg: &str, color: Color, spawn_offset: f32| -> Element {
            let age = elapsed - spawn_offset;
            let enter_dur = 0.3;
            let hold_dur = 2.0;
            let exit_dur = 0.3;
            let total = enter_dur + hold_dur + exit_dur;

            if age < 0.0 || age > total {
                return div();
            }

            let (offset_x, opacity) = if age < enter_dur {
                // Slide in from right
                let t = EasingFunction::EaseOutCubic.eval(age / enter_dur);
                ((1.0 - t) * 300.0, t)
            } else if age < enter_dur + hold_dur {
                (0.0, 1.0)
            } else {
                // Slide out to right
                let t = EasingFunction::EaseInQuad.eval((age - enter_dur - hold_dur) / exit_dur);
                (t * 300.0, 1.0 - t)
            };

            div().w_full().shrink(0.0).flex_row().justify_end().children([
                div().w(Px(offset_x)).shrink(0.0),
                div().shrink(0.0)
                    .bg(t.surface_elevated)
                    .border(1.0, color.with_alpha(0.5))
                    .rounded_px(4.0)
                    .px_pad(Px(10.0)).py(Px(6.0))
                    .opacity(opacity)
                    .glow(color, 4.0 * opacity)
                    .children([
                        self.lbl(msg, 13.0, 500, color).shrink(0.0),
                    ]),
            ])
        };

        let toasts_config: &[(&str, Color)] = &[
            ("Deploy successful", a.green),
            ("Warning: high CPU", a.yellow),
            ("Error: disk full", a.bright_red),
            ("Sync complete", a.cyan),
        ];

        let toast_elements: Vec<Element> = self.toasts.iter().map(|&(spawn, style_idx)| {
            let (msg, color) = toasts_config[style_idx % toasts_config.len()];
            toast_widget(msg, color, spawn)
        }).collect();

        div().w_full().flex_col().gap(8.0).children([
            self.lbl("Toast Notifications", 15.0, 700, t.primary).shrink(0.0),
            self.lbl("Slide in from right, hold, slide out. Click to spawn.", 12.5, 400, t.text_disabled).shrink(0.0),
            div().id("toast-spawn").shrink(0.0)
                .bg(t.primary).rounded_px(3.0)
                .px_pad(Px(10.0)).py(Px(4.0))
                .children([
                    self.lbl("Spawn Toast", 13.0, 600, Color::WHITE).shrink(0.0),
                ]),
            // Toast area
            div().w_full().flex_1().flex_col().gap(4.0).justify_end()
                .children(toast_elements),
        ])
    }
}

impl DeclarativeApp for Gallery {
    fn title(&self) -> &str { "Sabitori TUI Gallery" }
    fn size(&self) -> (f32, f32) { (1000.0, 660.0) }
    fn transparent(&self) -> bool { true }

    fn fonts(&self) -> Vec<Vec<u8>> {
        vec![
            include_bytes!("../assets/fonts/Hack-Regular.ttf").to_vec(),
            include_bytes!("../assets/fonts/Hack-Bold.ttf").to_vec(),
        ]
    }

    fn tick(&mut self, dt: f32) {
        if !self.splash_done && self.elapsed() >= SPLASH_DURATION {
            self.splash_done = true;
        }
        if !self.splash_done { return; }
        self.typewriter_st.tick(dt);
        for s in &mut self.spinners_st { s.tick(dt); }
        for p in &mut self.progress_st { p.tick(dt); }
        self.gradient_st.tick(dt);
        self.wave_st.tick(dt);
        self.pulse_st.tick(dt);
        self.color_cycle_st.tick(dt);
        // form_text doesn't need tick — cursor blink is done with elapsed time
        // Smooth scroll — fast lerp toward target
        let speed = 25.0;
        self.scroll_y += (self.scroll_target - self.scroll_y) * (speed * dt).min(1.0);
        if (self.scroll_target - self.scroll_y).abs() < 0.5 {
            self.scroll_y = self.scroll_target;
        }
    }

    fn overlay_view(&self, ctx: &ViewContext) -> Option<Element> {
        let t = &self.theme;
        let a = &t.ansi;

        // Dropdown overlay
        if self.form_dropdown_open {
            let items = ["Option A", "Option B", "Option C", "Option D"];
            let dd_items: Vec<Element> = items.iter().enumerate().map(|(i, label)| {
                let hovered = ctx.hovered.as_deref() == Some(&format!("form-dd-item-{i}"));
                let item = MenuItem::new(format!("form-dd-item-{i}"), *label);
                context_menu_item(&item, hovered, t.text_primary, t.text_secondary, t.surface_hover)
            }).collect();

            return Some(context_menu(
                ctx.width, ctx.height,
                ctx.mouse_x.min(400.0), ctx.mouse_y.min(400.0), // approximate position
                dd_items, 200.0,
                "dd-backdrop",
                t.surface_elevated.with_alpha(t.opacity), t.border,
            ));
        }

        // Context menu overlay
        if let Some((mx, my)) = self.ctx_menu {
            let items = vec![
                MenuItem::new("ctx-copy", "Copy").shortcut("\u{2318}C"),
                MenuItem::new("ctx-paste", "Paste").shortcut("\u{2318}V"),
                MenuItem::new("ctx-delete", "Delete").shortcut("\u{232b}"),
            ];
            let menu_items: Vec<Element> = items.iter().map(|item| {
                let hovered = ctx.hovered.as_deref() == Some(item.id.as_str());
                context_menu_item(item, hovered, t.text_primary, t.text_secondary, t.surface_hover)
            }).chain(std::iter::once(menu_separator(t.border)))
            .chain(std::iter::once({
                let item = MenuItem::new("ctx-select-all", "Select All").shortcut("\u{2318}A");
                let hovered = ctx.hovered.as_deref() == Some("ctx-select-all");
                context_menu_item(&item, hovered, t.text_primary, t.text_secondary, t.surface_hover)
            }))
            .collect();

            return Some(context_menu(
                ctx.width, ctx.height, mx, my,
                menu_items, 200.0,
                "ctx-backdrop",
                t.surface_elevated.with_alpha(t.opacity), t.border.with_alpha(t.opacity * 0.6),
            ));
        }

        // Modal overlay
        let elapsed = self.elapsed();
        let dur = 0.5;
        let anim_t = ((elapsed - self.modal_start) / dur).min(1.0);
        let show = if self.modal_open { true } else { anim_t < 1.0 };
        if !show { return None; }

        // Both open and close: opacity only. No size change = no text jitter.
        let alpha = if self.modal_open { anim_t } else { 1.0 - anim_t };
        let modal_alpha = alpha;
        let backdrop_alpha = alpha * 0.4;
        let (w, h) = (360.0, 200.0);

        Some(div()
            .id("modal-backdrop")
            .w(Px(ctx.width)).h(Px(ctx.height))
            .bg(Color::new(0.0, 0.0, 0.0, backdrop_alpha))
            .flex_col().items_center().justify_center()
            .children([
                div()
                    .w(Px(w)).h(Px(h))
                    .bg(t.surface_elevated)
                    .border(1.0, t.border.with_alpha(modal_alpha))
                    .rounded_px(10.0)
                    .opacity(modal_alpha)
                    .shadow_md(Color::new(0.0, 0.0, 0.0, 0.5 * modal_alpha))
                    .flex_col().items_center().justify_center().gap(12.0)
                    .children([
                        text("Confirm Action?").mono().bold().font_size(16.0)
                            .color(t.text_primary).shrink(0.0),
                        text("This cannot be undone.").mono().font_size(14.0)
                            .color(t.text_secondary).shrink(0.0),
                        div().flex_row().gap(10.0).children([
                            div().id("modal-toggle").shrink(0.0)
                                .bg(t.surface_hover).rounded_px(4.0)
                                .border(1.0, t.border)
                                .px_pad(Px(16.0)).py(Px(6.0))
                                .children([
                                    text("Cancel").mono().font_size(14.0)
                                        .color(t.text_primary).shrink(0.0),
                                ]),
                            div().id("modal-confirm").shrink(0.0)
                                .bg(a.green.with_alpha(0.8)).rounded_px(4.0)
                                .px_pad(Px(16.0)).py(Px(6.0))
                                .children([
                                    text("Confirm").mono().font_size(14.0)
                                        .color(Color::WHITE).shrink(0.0),
                                ]),
                        ]),
                    ]),
            ]))
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        let t = &self.theme;
        let a = &t.ansi;
        let bg = t.surface.with_alpha(t.opacity);
        let elapsed = self.elapsed();

        // ── Splash screen — 金ロー style drop-in using MotionState ──
        if !self.splash_done {
            let logo = "sabitori";
            let char_w = 24.0;
            let logo_w = logo.len() as f32 * char_w;
            let logo_left = (ctx.width - logo_w) / 2.0;
            let logo_top = ctx.height / 2.0 - 30.0;

            let logo_chars: Vec<Element> = logo.chars().enumerate().map(|(i, ch)| {
                let motion = MotionState::new(0.8)
                    .from_right(ctx.width * 2.0)
                    .bounce(40.0)
                    .delay(i as f32 * 0.15)
                    .easing(EasingFunction::EaseOutCubic);

                if !motion.started(elapsed) {
                    return div();
                }

                let (dx, dy) = motion.offset(elapsed);
                let final_x = logo_left + i as f32 * char_w;
                let x = final_x + dx;
                let y = logo_top + dy;

                text(String::from(ch)).mono().bold().font_size(40.0)
                    .color(t.primary.with_alpha(motion.alpha(elapsed)))
                    .pos(x.round(), y.round())
            }).collect();

            // Subtitle + bar appear after logo lands
            let logo_done_at = logo.len() as f32 * 0.15 + 0.8;
            let sub_t = ((elapsed - logo_done_at).max(0.0) / 0.5).min(1.0);
            let sub_alpha = EasingFunction::EaseOutCubic.eval(sub_t);
            let bar_w = sub_alpha * 240.0;
            let bar_left = (ctx.width - 240.0) / 2.0;
            let sub_y = logo_top + 50.0;

            // Fade out
            let fade_start = SPLASH_DURATION - 0.5;
            let fade_t = ((elapsed - fade_start).max(0.0) / 0.5).min(1.0);
            let fade_out = 1.0 - fade_t;

            let mut children = logo_chars;
            // Gradient bar (absolute)
            children.push(
                div().w(Px(bar_w)).h(Px(2.0))
                    .gradient(t.primary, a.magenta, 0.0)
                    .rounded_px(1.0)
                    .opacity(sub_alpha)
                    .pos(bar_left + (240.0 - bar_w) / 2.0, sub_y)
            );
            // Subtitle (absolute)
            children.push(
                text("GPU-accelerated UI framework").mono().font_size(14.0)
                    .color(t.text_secondary.with_alpha(sub_alpha))
                    .pos(bar_left - 40.0, sub_y + 12.0)
            );

            return div()
                .w(Px(ctx.width)).h(Px(ctx.height))
                .bg(bg)
                .opacity(fade_out)
                .children(children);
        }

        // ── Skin (TUI ↔ Modern) ──────────────────────────────────────────
        // TUI = crisp mono terminal look. Modern = proportional type, soft
        // palette, rounded accents — the landing-page aesthetic. Only the
        // *chrome* reskins; the 32 demos are the exhibits and stay as-is.
        let modern = self.modern;
        let hx = |s: &str| Color::from_hex(s);
        let (s_bg0, s_bg1, s_chrome, s_surface, s_surface_hi, s_border,
             s_text_hi, s_text_mid, s_text_dim, s_accent, s_accent2) = if modern {
            (hx("#0a0c16"), hx("#12162c"), hx("#0d1020"), hx("#171b2e"), hx("#20263f"), hx("#2a3152"),
             hx("#eef1ff"), hx("#a7afd6"), hx("#6b74a0"), hx("#7aa2f7"), hx("#bb9af7"))
        } else {
            (t.surface.with_alpha(t.opacity), t.surface.with_alpha(t.opacity), t.surface_elevated,
             t.surface_elevated, t.surface_active, t.border,
             t.text_primary, t.text_secondary, t.text_disabled, t.primary, a.magenta)
        };

        // ── Sidebar ──
        let mut menu_items: Vec<Element> = Vec::new();
        for (i, name) in ITEMS.iter().enumerate() {
            // Section headers
            let section_label = |label: &str, color: Color| -> Element {
                let base = if modern {
                    text(label).font_size(10.5).font_weight(700).letter_spacing(1.4).color(color)
                } else {
                    text(label).mono().font_size(11.0).color(color)
                };
                base.shrink(0.0).px_pad(Px(8.0)).pt(Px(if modern { 12.0 } else { 6.0 })).pb(Px(3.0))
            };

            match i {
                0 => { menu_items.push(section_label("TEXT", s_text_dim)); }
                5 => { menu_items.push(hsep(s_border)); menu_items.push(section_label("WIDGETS", s_text_dim)); }
                13 => { menu_items.push(hsep(s_border)); menu_items.push(section_label("VISUAL", a.cyan.with_alpha(0.7))); }
                20 => { menu_items.push(hsep(s_border)); menu_items.push(section_label("GPU", s_accent.with_alpha(0.75))); }
                26 => { menu_items.push(hsep(s_border)); menu_items.push(section_label("THEMES", a.magenta.with_alpha(0.7))); }
                27 => { menu_items.push(hsep(s_border)); menu_items.push(section_label("SPLASH", a.bright_cyan.with_alpha(0.7))); }
                28 => { menu_items.push(hsep(s_border)); menu_items.push(section_label("FORMS", a.green.with_alpha(0.7))); }
                29 => { menu_items.push(hsep(s_border)); menu_items.push(section_label("INTERACTION", a.yellow.with_alpha(0.7))); }
                _ => {}
            }

            let id = format!("item-{i}");
            let is_active = i == self.selected;
            let is_hovered = ctx.hovered.as_deref() == Some(id.as_str());
            let fg = if is_active {
                if modern { s_text_hi } else { Color::WHITE }
            } else if is_hovered { s_text_hi } else { s_text_mid };
            let row_bg = if is_active {
                if modern { s_accent.with_alpha(0.16) } else { t.primary }
            } else if is_hovered {
                s_surface_hi
            } else {
                Color::TRANSPARENT
            };

            let item_label = if modern {
                text(*name).font_size(13.5).font_weight(if is_active { 600 } else { 500 }).letter_spacing(0.1).color(fg)
            } else {
                text(*name).mono().font_size(14.0).color(fg)
            };

            menu_items.push(
                div().id(&id)
                    .w_full().h(Px(if modern { 27.0 } else { 24.0 })).shrink(0.0)
                    .bg(row_bg)
                    .rounded_px(if modern { 7.0 } else { 4.0 })
                    .flex_row().items_center().px_pad(Px(8.0)).gap(if modern { 8.0 } else { 4.0 })
                    .children([
                        // Active indicator bar
                        div().w(Px(3.0)).h(Px(14.0)).shrink(0.0)
                            .bg(if is_active { s_accent } else { Color::TRANSPARENT })
                            .rounded_px(2.0),
                        item_label.shrink(0.0),
                    ]),
            );
        }

        let sidebar_title = if modern {
            text("COMPONENTS").font_size(10.5).font_weight(700).letter_spacing(1.6).color(s_text_dim)
        } else {
            text("COMPONENTS").mono().font_size(14.0).color(t.text_disabled)
        };
        let sidebar = div().w(Px(if modern { 198.0 } else { 180.0 })).shrink(0.0).h_full()
            .flex_col()
            .children([
                sidebar_title.shrink(0.0).p_px(if modern { 10.0 } else { 6.0 }).pb(Px(4.0)),
                hsep(s_border),
                div().flex_1()
                    .scroll("sidebar")
                    .flex_col().py(Px(4.0))
                    .children(menu_items),
            ]);

        // ── Preview ──
        let preview = match self.selected {
            // Text
            0 => self.typewriter(t),
            1 => self.gradient_text(t),
            2 => self.wave_text(t),
            3 => self.terminal(t, a),
            4 => self.matrix_rain(t, a),
            // Widgets
            5 => self.progress_bars(t, a),
            6 => self.spinners(t),
            7 => self.toggle(t, a),
            8 => self.tabs_demo(t, a),
            9 => self.counter(t, a),
            10 => self.skeleton(t),
            11 => self.sparkline(t, a),
            12 => self.clock(t, a),
            // Visual
            13 => self.easing_curves(t),
            14 => self.pulse_border(t, a),
            15 => self.color_tween(t),
            16 => self.gradient_demo(t, a),
            17 => self.glassmorphism(t, a),
            18 => self.bento_grid(t, a),
            19 => self.heatmap(t, a),
            // GPU
            20 => self.orbit(t, a),
            21 => self.glow_pulse(t, a),
            22 => self.morph(t, a),
            23 => self.particles(t),
            24 => self.smooth_motion(t, a),
            25 => self.carousel(t, a),
            // Themes
            26 => self.theme_gallery(t),
            // Interaction
            27 => self.splash_presets(t, a, ctx),
            28 => self.form_controls(t, a, ctx),
            29 => self.context_menu_demo(t),
            30 => self.view_transition(t, a, ctx),
            31 => self.modal(t, a),
            32 => self.toast(t, a),
            _ => div(),
        };

        // Fade the preview in whenever the selected component changes.
        let fade = {
            let x = ((elapsed - self.sel_at) / 0.2).clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        };
        let preview = preview.opacity(fade);

        let viewport_h = ctx.height - if modern { 94.0 } else { 66.0 };
        let content_h = self.content_h();
        let needs_scroll = content_h > viewport_h;

        // Framed "stage" — components sit on an elevated, bordered, shadowed
        // surface instead of floating in a bare void. No recentering, so
        // components that use absolute .pos() keep a stable top-left origin.
        let stage_inner = if needs_scroll {
            scroll_container(
                viewport_h,
                content_h,
                self.scroll_y,
                s_surface,
                s_text_dim.with_alpha(0.4),
                vec![preview],
            )
        } else {
            div().flex_1().w_full().p(Px(20.0)).flex_col().children([preview])
        };

        let stage = div()
            .flex_1()
            .w_full()
            .bg(s_surface)
            .rounded_px(if modern { 14.0 } else { 10.0 })
            .border(1.0, s_border)
            .shadow_md(Color::new(0.0, 0.0, 0.0, if modern { 0.45 } else { 0.35 }))
            .overflow_hidden()
            .flex_col()
            .children([stage_inner]);

        let preview_area = div()
            .flex_1()
            .h_full()
            .p(Px(12.0))
            .flex_col()
            .children([stage]);

        // ── Skin toggle (click the pill, or press 'M') ──
        let skin_toggle = if modern {
            div().id("skin-toggle").cursor(Cursor::Pointer)
                .flex_row().items_center().gap(6.0)
                .px_pad(Px(10.0)).py(Px(4.0)).rounded_px(999.0)
                .bg(s_surface_hi).border(1.0, s_border)
                .children([
                    div().w(Px(6.0)).h(Px(6.0)).rounded_px(999.0).bg(s_accent),
                    text("MODERN").font_size(10.0).font_weight(600).letter_spacing(1.0).color(s_text_mid).shrink(0.0),
                ])
        } else {
            div().id("skin-toggle").cursor(Cursor::Pointer)
                .flex_row().items_center()
                .children([text("[TUI]").mono().font_size(14.0).color(t.primary).shrink(0.0)])
        };

        // ── Header ──
        let header = if modern {
            div().w_full().h(Px(46.0)).shrink(0.0)
                .bg(s_chrome)
                .flex_row().items_center().overflow_hidden()
                .px_pad(Px(16.0)).gap(10.0)
                .children([
                    div().w(Px(20.0)).h(Px(20.0)).rounded_px(6.0)
                        .gradient(s_accent, s_accent2, 45.0).glow_sm(s_accent),
                    text("sabitori").font_size(16.0).font_weight(600).letter_spacing(0.2).color(s_text_hi).shrink(0.0),
                    text("/").font_size(15.0).color(s_text_dim).shrink(0.0),
                    text(ITEMS[self.selected]).font_size(15.0).font_weight(500).color(s_text_mid).shrink(0.0),
                    div().flex_1(),
                    skin_toggle,
                    text(&format!("{}/{}", self.selected + 1, ITEMS.len()))
                        .font_size(13.0).font_weight(500).color(s_text_dim).shrink(0.0),
                ])
        } else {
            div().w_full().h(Px(24.0)).shrink(0.0)
                .bg(t.surface_elevated)
                .flex_row().items_center().overflow_hidden()
                .px_pad(Px(10.0)).gap(8.0)
                .children([
                    text("sabitori").mono().bold().font_size(14.0).color(t.primary).shrink(0.0),
                    text("/").mono().font_size(14.0).color(t.text_disabled).shrink(0.0),
                    text(ITEMS[self.selected]).mono().font_size(14.0).color(t.text_primary).shrink(0.0),
                    div().flex_1(),
                    skin_toggle,
                    text(&format!("{}/{}", self.selected + 1, ITEMS.len()))
                        .mono().font_size(14.0).color(t.text_disabled).shrink(0.0),
                    text("j/k").mono().font_size(14.0).color(t.text_disabled).shrink(0.0),
                ])
        };

        // ── Footer ──
        let footer = if modern {
            div().w_full().h(Px(26.0)).shrink(0.0)
                .bg(s_chrome)
                .flex_row().items_center().px_pad(Px(16.0))
                .children([
                    text(&format!("sabitori v0.2.8 · {} · {} demos", t.name, ITEMS.len()))
                        .font_size(11.0).font_weight(500).letter_spacing(0.5).color(s_text_dim).shrink(0.0),
                ])
        } else {
            div().w_full().h(Px(20.0)).shrink(0.0)
                .bg(t.surface_elevated)
                .flex_row().items_center().px_pad(Px(10.0))
                .children([
                    text(&format!("sabitori v0.2.8  |  {}  |  {} demos", t.name, ITEMS.len()))
                        .mono().font_size(14.0).color(t.text_disabled).shrink(0.0),
                ])
        };

        // ── Layout ──
        let mut root_kids: Vec<Element> = Vec::new();
        // Modern skin: soft aurora glows behind the (transparent) sidebar.
        if modern {
            let blob = |cx: f32, cy: f32, r: f32, col: Color, op: f32, gr: f32| {
                div().absolute().pos(cx - r, cy - r).w(Px(r * 2.0)).h(Px(r * 2.0))
                    .rounded_px(r).bg(col).opacity(op).glow(col, gr)
            };
            root_kids.push(blob(ctx.width * 0.12, ctx.height * 0.30 + 22.0 * (elapsed * 0.10).sin(), 84.0, s_accent2, 0.13, 200.0));
            root_kids.push(blob(ctx.width * 0.22, ctx.height * 0.78 + 18.0 * (elapsed * 0.12).cos(), 72.0, s_accent, 0.11, 190.0));
        }
        root_kids.push(header);
        root_kids.push(hsep(s_border));
        root_kids.push(div().flex_1().flex_row().children([sidebar, vsep(s_border), preview_area]));
        root_kids.push(hsep(s_border));
        root_kids.push(footer);

        let root = div().w(Px(ctx.width)).h(Px(ctx.height)).flex_col();
        let root = if modern { root.gradient(s_bg0, s_bg1, 90.0) } else { root.bg(bg) };
        root.children(root_kids)
    }

    fn scroll_intents(&mut self) -> Vec<(String, f32)> {
        self.sidebar_scroll_intent
            .take()
            .map(|y| vec![("sidebar".to_string(), y)])
            .unwrap_or_default()
    }

    fn on_scroll(&mut self, delta_y: f32) {
        let max = self.content_h();
        if max <= 0.0 { return; } // no scrollable content
        self.scroll_target = (self.scroll_target - delta_y).max(0.0).min(max);
    }

    fn on_pointer_move(&mut self, x: f32, _y: f32) {
        if self.slider_dragging {
            // Slider track is ~300px wide, positioned in the preview area
            // Approximate: map mouse x to 0.0-1.0
            let track_left = 200.0; // rough offset
            let track_w = 300.0;
            self.form_slider = ((x - track_left) / track_w).clamp(0.0, 1.0);
        }
    }

    fn on_pointer_up(&mut self) {
        self.slider_dragging = false;
    }

    fn on_right_click(&mut self, _id: &str, x: f32, y: f32) {
        if self.selected == 27 {
            self.ctx_menu = Some((x, y));
        }
    }

    fn on_click(&mut self, id: &str) {
        if !self.splash_done { self.splash_done = true; return; }
        // Track focus for form controls
        self.focused = if id == "form-input" { Some(id.to_string()) } else { None };
        // Close dropdown when clicking elsewhere
        if id != "form-dropdown" { self.form_dropdown_open = false; }
        // Close dropdown backdrop
        if id == "dd-backdrop" {
            self.form_dropdown_open = false;
            return;
        }
        // Close context menu on any click
        if self.ctx_menu.is_some() {
            self.ctx_menu = None;
            return;
        }
        if id == "skin-toggle" {
            self.modern = !self.modern;
            return;
        }
        if let Some(rest) = id.strip_prefix("item-") {
            if let Ok(idx) = rest.parse::<usize>() {
                self.selected = idx;
                self.sel_at = self.elapsed();
                self.scroll_y = 0.0; self.scroll_target = 0.0; // reset scroll on nav
            }
        }
        // Theme selection
        if let Some(rest) = id.strip_prefix("theme-") {
            if let Ok(idx) = rest.parse::<usize>() {
                if let Some(theme) = self.presets.get(idx) {
                    self.theme = theme.clone();
                }
            }
        }
        // View Transition
        if id == "trans-trigger" {
            self.transition_view += 1;
            self.transition_start = self.elapsed();
        }
        if let Some(rest) = id.strip_prefix("trans-") {
            if let Ok(idx) = rest.parse::<usize>() {
                if idx < 4 {
                    self.transition_kind = idx;
                    self.transition_view += 1;
                    self.transition_start = self.elapsed();
                }
            }
        }
        // Splash preset selection
        if let Some(rest) = id.strip_prefix("splash-") {
            if let Ok(idx) = rest.parse::<usize>() {
                self.splash_preview_idx = idx;
                self.splash_preview_start = self.elapsed();
            }
        }
        // Slider click
        if id == "form-slider" {
            self.slider_dragging = true;
        }
        // Dropdown items
        if let Some(rest) = id.strip_prefix("form-dd-item-") {
            if let Ok(idx) = rest.parse::<usize>() {
                self.form_dropdown_sel = idx;
                self.form_dropdown_open = false;
            }
        }
        // Form controls
        if let Some(rest) = id.strip_prefix("form-check-") {
            if let Ok(idx) = rest.parse::<usize>() {
                if idx < 3 { self.form_checks[idx] = !self.form_checks[idx]; }
            }
        }
        if let Some(rest) = id.strip_prefix("form-radio-") {
            if let Ok(idx) = rest.parse::<usize>() {
                self.form_radio = idx;
            }
        }
        if id == "form-dropdown" {
            self.form_dropdown_open = !self.form_dropdown_open;
        }
        // Toggle
        if let Some(rest) = id.strip_prefix("toggle-") {
            if let Ok(idx) = rest.parse::<usize>() {
                if idx < 4 {
                    self.toggles[idx] = !self.toggles[idx];
                    self.toggle_changed[idx] = self.elapsed();
                }
            }
        }
        // Modal
        if id == "modal-backdrop" || id == "modal-toggle" || id == "modal-confirm" {
            self.modal_open = !self.modal_open;
            self.modal_start = self.elapsed();
        }
        // Toast
        if id == "toast-spawn" {
            let spawn_time = self.elapsed();
            let style = self.next_toast_id % 4;
            self.toasts.push((spawn_time, style));
            self.next_toast_id += 1;
            // Clean old toasts
            let now = self.elapsed();
            self.toasts.retain(|&(t, _)| now - t < 3.0);
        }
    }

    fn on_input(&mut self, event: &InputEvent) -> bool {
        if !self.splash_done {
            self.splash_done = true;
            return true;
        }
        // Route input to focused text input
        if self.focused.as_deref() == Some("form-input") {
            match event {
                InputEvent::CharInput(ch) => { self.form_text.on_char(*ch); return true; }
                InputEvent::KeyInput { key, pressed: true, modifiers, .. } => {
                    if self.form_text.on_key(*key, *modifiers) { return true; }
                }
                InputEvent::ImePreedit { text: t, cursor } => {
                    self.form_text.on_ime_preedit(t.clone(), *cursor); return true;
                }
                InputEvent::ImeCommit { text: t } => {
                    self.form_text.on_ime_commit(t); return true;
                }
                _ => {}
            }
        }
        if let InputEvent::CharInput(c) = event {
            match c {
                'j' => {
                    self.selected = (self.selected + 1).min(ITEMS.len() - 1);
                    self.sel_at = self.elapsed();
                    self.scroll_y = 0.0; self.scroll_target = 0.0;
                    self.ensure_sidebar_visible();
                    return true;
                }
                'k' => {
                    if self.selected > 0 { self.selected -= 1; }
                    self.sel_at = self.elapsed();
                    self.scroll_y = 0.0; self.scroll_target = 0.0;
                    self.ensure_sidebar_visible();
                    return true;
                }
                'M' => {
                    self.modern = !self.modern;
                    return true;
                }
                ' ' if self.selected == 30 => {
                    self.transition_view += 1;
                    self.transition_start = self.elapsed();
                    return true;
                }
                'm' if self.selected == 31 => {
                    self.modal_open = !self.modal_open;
                    self.modal_start = self.elapsed();
                    return true;
                }
                't' if self.selected == 32 => {
                    let spawn_time = self.elapsed();
                    let style = self.next_toast_id % 4;
                    self.toasts.push((spawn_time, style));
                    self.next_toast_id += 1;
                    return true;
                }
                _ => {}
            }
        }
        false
    }
}

fn main() {
    sabitori::run_declarative(Gallery {
        theme: Theme::warp_dark().with_opacity(0.88),
        start: Instant::now(),
        selected: 0,
        sel_at: 0.0,
        modern: true,
        splash_done: false,
        typewriter_st: TypewriterState::new("The quick brown fox jumps over the lazy dog.", 4.0),
        spinners_st: vec![
            SpinnerState::braille(),
            SpinnerState::line(),
            SpinnerState::blocks(),
            SpinnerState::bounce(),
            SpinnerState::growing(),
        ],
        progress_st: vec![
            ProgressBarState::new(1.0, 1.2),
            ProgressBarState::new(1.0, 0.8).with_delay(0.3),
            ProgressBarState::new(1.0, 1.5).with_delay(0.8),
            ProgressBarState::new(1.0, 2.0).with_delay(1.5),
        ],
        gradient_st: GradientState::new(vec![
            Color::from_hex("#ff6b6b"), Color::from_hex("#ffa500"),
            Color::from_hex("#4ade80"), Color::from_hex("#22d3ee"),
            Color::from_hex("#c084fc"),
        ], 2.0),
        wave_st: WaveState::new(3.0, 8.0, 4.0),
        pulse_st: PulseState::new(1.5, 0.3, 1.0),
        color_cycle_st: ColorCycleState::new(vec![
            Color::from_hex("#ff6b6b"), Color::from_hex("#fbbf24"),
            Color::from_hex("#4ade80"), Color::from_hex("#22d3ee"),
            Color::from_hex("#c084fc"), Color::from_hex("#f472b6"),
        ], 2.0),
        transition_view: 0,
        transition_start: 0.0,
        transition_kind: 0,
        modal_open: false,
        modal_start: 0.0,
        toasts: Vec::new(),
        next_toast_id: 0,
        scroll_y: 0.0,
        scroll_target: 0.0,
        sidebar_scroll_intent: None,
        presets: Theme::all_presets(),
        toggles: [true, false, true, false],
        toggle_changed: [0.0; 4],
        ctx_menu: None,
        focused: None,
        form_text: sabitori_widgets::TextInputState::new("Type here..."),
        form_checks: [true, false, true],
        form_radio: 0,
        form_slider: 0.5,
        form_dropdown_open: false,
        form_dropdown_sel: 0,
        slider_dragging: false,
        splash_preview_idx: 0,
        splash_preview_start: 0.0,
    });
}
