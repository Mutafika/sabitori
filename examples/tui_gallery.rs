//! TUI Animation Gallery — wabi-inspired animated components on GPU.
//!
//! `cargo run -p sabitori --example tui_gallery`
//!
//! 左の一覧から部品を選ぶ (`j` / `k` でも動く)。右上の丸ボタンか `M` で、
//! 外枠の見た目を TUI (等幅・角の立った端末風) と Modern (プロポーショナル・
//! 柔らかい角) で切り替える。
//!
//! 撮影・確認用に、起動時の状態を env で決められる:
//!
//! ```sh
//! # 起動画面を飛ばして "Drag & Drop" を TUI の見た目で開く
//! TUI_GALLERY_ITEM="Drag & Drop" TUI_GALLERY_SKIN=tui \
//!     SABITORI_BACKGROUND=1 SABITORI_SCREENSHOT=out.png \
//!     cargo run -p sabitori --example tui_gallery
//! ```

use sabitori::*;
use sabitori_style::{AnsiPalette, Theme};
use std::ops::Range;
use std::path::PathBuf;
use std::time::Instant;

// ── Gallery state ──

const SPLASH_DURATION: f32 = 4.5;

struct Gallery {
    theme: Theme,
    start: Instant,
    /// `ITEMS` の添字。
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
    // Interaction
    transition_view: usize,
    transition_start: f32,
    transition_kind: usize,
    modal_open: bool,
    modal_start: f32,
    toasts: Vec<(f32, usize)>,
    next_toast_id: usize,
    /// 一覧と本文のスクロール要求。 `scroll_intents` が 1 度だけ吸い出す。
    ///
    /// 位置そのものはランタイムが持つ (`.scroll(id)`)。 プログラムから動かす口は
    /// `scroll_intents` だけ。
    scroll_requests: Vec<ScrollIntent>,
    // Cached presets
    presets: Vec<Theme>,
    // Toggle states
    toggles: [bool; 4],
    toggle_changed: [f32; 4],
    // Context menu
    ctx_menu: Option<(f32, f32)>,
    /// 最後に選ばれたメニューの項目。
    ctx_last: Option<&'static str>,
    // Form controls
    form_text: TextInputState,
    form_checks: [bool; 3],
    form_radio: usize,
    form_slider: f32,
    form_dropdown: DropdownState,
    slider_dragging: bool,
    /// 直近のフレームで組んだスライダーの溝の矩形 (`on_build` で拾う)。
    ///
    /// 以前は溝の左端を `200.0` と決め打ちしていて、 窓の幅や一覧の幅が
    /// 変わるとつまみが指からずれていた。
    slider_track: Option<Rect>,
    pointer_x: f32,
    // Splash preview
    splash_preview_idx: usize,
    splash_preview_start: f32,
    // Text Selection
    /// いま選ばれている範囲 (段落の id, バイト範囲)。 `on_selection_changed` で受ける。
    selection: Vec<(String, Range<usize>)>,
    /// 「Mark」で残した範囲。
    marks: Vec<(String, Range<usize>)>,
    // Goo
    goo_open: bool,
    goo_p: f32,
    goo_v: f32,
    // Drag & Drop
    /// カード i が入っている列。
    dd_cards: [usize; 5],
    /// 外から落とされたファイルの名前 (新しい順)。
    dd_files: Vec<String>,
}

/// 一覧の見出し。 項目の並びの中で区分が変わった所に差す。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section { Text, Widgets, Visual, Gpu, Themes, Splash, Forms, Interaction }

impl Section {
    fn label(self) -> &'static str {
        match self {
            Section::Text => "TEXT",
            Section::Widgets => "WIDGETS",
            Section::Visual => "VISUAL",
            Section::Gpu => "GPU",
            Section::Themes => "THEMES",
            Section::Splash => "SPLASH",
            Section::Forms => "FORMS",
            Section::Interaction => "INTERACTION",
        }
    }
}

/// 部品。 一覧の順は `ITEMS` が決め、 処理はこの名前で分ける。
///
/// 以前は添字の数字で分けていて、 項目を足すたびに `selected == 27` のような
/// 比較が黙ってずれた (Context Menu の右クリックが Splash Presets を見ていて、
/// メニューが開かなくなっていた)。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Demo {
    Typewriter, GradientText, WaveText, Terminal, MatrixRain, TextSelection,
    ProgressBars, Spinners, Toggle, Tabs, Counter, Skeleton, Sparkline, Clock,
    EasingCurves, PulseBorder, ColorTween, Gradient, Glassmorphism, BentoGrid, Heatmap,
    Orbit, GlowPulse, Morph, Particles, SmoothMotion, Carousel, Goo, Layers,
    ThemeGallery,
    SplashPresets,
    FormControls,
    ContextMenu, ViewTransition, Modal, Toast, DragDrop,
}

const ITEMS: &[(Demo, &str, Section)] = &[
    (Demo::Typewriter, "Typewriter", Section::Text),
    (Demo::GradientText, "Gradient Text", Section::Text),
    (Demo::WaveText, "Wave Text", Section::Text),
    (Demo::Terminal, "Terminal", Section::Text),
    (Demo::MatrixRain, "Matrix Rain", Section::Text),
    (Demo::TextSelection, "Text Selection", Section::Text),
    (Demo::ProgressBars, "Progress Bars", Section::Widgets),
    (Demo::Spinners, "Spinners", Section::Widgets),
    (Demo::Toggle, "Toggle", Section::Widgets),
    (Demo::Tabs, "Tabs", Section::Widgets),
    (Demo::Counter, "Counter", Section::Widgets),
    (Demo::Skeleton, "Skeleton", Section::Widgets),
    (Demo::Sparkline, "Sparkline", Section::Widgets),
    (Demo::Clock, "Clock", Section::Widgets),
    (Demo::EasingCurves, "Easing Curves", Section::Visual),
    (Demo::PulseBorder, "Pulse Border", Section::Visual),
    (Demo::ColorTween, "Color Tween", Section::Visual),
    (Demo::Gradient, "Gradient", Section::Visual),
    (Demo::Glassmorphism, "Glassmorphism", Section::Visual),
    (Demo::BentoGrid, "Bento Grid", Section::Visual),
    (Demo::Heatmap, "Heatmap", Section::Visual),
    (Demo::Orbit, "Orbit", Section::Gpu),
    (Demo::GlowPulse, "Glow Pulse", Section::Gpu),
    (Demo::Morph, "Morph", Section::Gpu),
    (Demo::Particles, "Particles", Section::Gpu),
    (Demo::SmoothMotion, "Smooth Motion", Section::Gpu),
    (Demo::Carousel, "Carousel", Section::Gpu),
    (Demo::Goo, "Goo", Section::Gpu),
    (Demo::Layers, "Overlap", Section::Gpu),
    (Demo::ThemeGallery, "Theme Gallery", Section::Themes),
    (Demo::SplashPresets, "Splash Presets", Section::Splash),
    (Demo::FormControls, "Form Controls", Section::Forms),
    (Demo::ContextMenu, "Context Menu", Section::Interaction),
    (Demo::ViewTransition, "View Transition", Section::Interaction),
    (Demo::Modal, "Modal", Section::Interaction),
    (Demo::Toast, "Toast", Section::Interaction),
    (Demo::DragDrop, "Drag & Drop", Section::Interaction),
];

/// Drag & Drop の列とカード。
const DD_COLUMNS: [&str; 3] = ["Backlog", "Doing", "Done"];
const DD_CARDS: [&str; 5] = ["Parse config", "Spring tuning", "IME preedit", "Glyph atlas", "Release notes"];

/// Text Selection の段落 (id, 本文)。 id は選択の `owner` として返ってくる。
const SEL_PARAS: [(&str, &str); 2] = [
    ("para-en", "Drag across this sentence to select part of it. The app reads the range back with ctx.text_selection() and can keep it as a marker."),
    ("para-ja", "日本語も同じです。範囲は本文の中のバイト位置で届くので、段落の id と組にして保存できます。"),
];

/// Goo の寸法 (舞台の中の論理 px)。
const GOO_PILL: (f32, f32, f32, f32) = (84.0, 24.0, 132.0, 34.0);
const GOO_PANEL: (f32, f32) = (240.0, 150.0);

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

    fn demo(&self) -> Demo {
        ITEMS[self.selected].0
    }

    /// 角の丸み。 Modern は柔らかく、 TUI は端末らしく立てる。
    fn round(&self, modern: f32, tui: f32) -> f32 {
        if self.modern { modern } else { tui }
    }

    /// 項目を選ぶ。 本文は頭へ戻し、 `scroll_sidebar` なら一覧も選んだ所へ送る。
    fn select(&mut self, idx: usize, scroll_sidebar: bool) {
        self.selected = idx.min(ITEMS.len() - 1);
        self.sel_at = self.elapsed();
        self.scroll_requests.push(ScrollIntent::y("stage", 0.0));
        if scroll_sidebar {
            // 現在の位置はランタイムが持っているので「足りない分だけ動かす」は
            // 書けない。 選んだ項目が中央に来る位置を頼み、 端のクランプは
            // ランタイムに任せる。
            let item_h = 26.0;
            let viewport = 500.0; // サイドバーのおおよその可視高さ
            let y = self.selected as f32 * item_h;
            self.scroll_requests.push(ScrollIntent::y("sidebar", (y - viewport * 0.5).max(0.0)));
        }
    }

    fn spawn_toast(&mut self) {
        let now = self.elapsed();
        self.toasts.retain(|&(t, _)| now - t < 3.0);
        self.toasts.push((now, self.next_toast_id % 4));
        self.next_toast_id += 1;
    }

    fn toggle_modal(&mut self) {
        self.modal_open = !self.modal_open;
        self.modal_start = self.elapsed();
    }

    fn switch_view(&mut self) {
        self.transition_view += 1;
        self.transition_start = self.elapsed();
    }

    /// スライダーの値を、 ポインタの x から決める (溝の矩形は前のフレームのもの)。
    fn drag_slider_to(&mut self, x: f32) {
        if let Some(track) = self.slider_track {
            // つまみ (16px) の中心が指に来るように、 端から半分ずつ削る。
            let usable = (track.size.width - 16.0).max(1.0);
            self.form_slider = ((x - track.origin.x - 8.0) / usable).clamp(0.0, 1.0);
        }
    }

    /// 選んでいる範囲をマーカーとして残す。
    fn mark_selection(&mut self) {
        for piece in self.selection.drain(..) {
            if !self.marks.contains(&piece) {
                self.marks.push(piece);
            }
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
            self.lbl("Typewriter", 15.0, 700, t.primary).shrink(0.0),
            text(&display).mono().font_size(14.0).color(t.text_primary).shrink(0.0),
            self.lbl("Characters reveal one-by-one with blinking cursor.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn gradient_text(&self, t: &Theme) -> Element {
        let content = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

        // 1 文字ごとの色は `color_spans` で 1 要素のまま付ける。 文字を 1 つずつ
        // `text()` に分けると、 要素が字数だけ増え、 字送りも要素の境で丸まる。
        let spans: Vec<(Range<usize>, Color)> = content.char_indices()
            .enumerate()
            .map(|(i, (at, c))| (at..at + c.len_utf8(), self.gradient_st.color_at(i)))
            .collect();

        div().w_full().flex_col().gap(8.0).children([
            self.lbl("Gradient Text", 15.0, 700, t.primary).shrink(0.0),
            text(content).mono().font_size(18.0).bold().color(t.text_primary).color_spans(spans).shrink(0.0),
            self.lbl("Per-character color interpolation across gradient stops — one text element with color_spans.", 12.5, 400, t.text_disabled).shrink(0.0),
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
                self.lbl(names[i], 13.0, 500, t.text_secondary).shrink(0.0),
            ])
        }).collect();

        let mut children = vec![
            self.lbl("Spinners", 15.0, 700, t.primary).shrink(0.0),
        ];
        children.extend(spinners);
        children.push(
            self.lbl("Frame-based character cycling at configurable intervals.", 12.5, 400, t.text_disabled).shrink(0.0),
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
                    .w(Px(130.0)).shrink(0.0),
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
        let (area_w, area_h) = (460.0, 300.0);
        let cx = area_w / 2.0;
        let cy = area_h / 2.0;

        // (半径, 速さ, 大きさ, 色)。 奇数番は 1 つ前の玉を 0.15 秒遅れで追う尾。
        let orbs: &[(f32, f32, f32, Color)] = &[
            (60.0, 1.0,  6.0, a.cyan),
            (60.0, 1.0,  4.0, a.cyan.with_alpha(0.4)),
            (45.0, 1.7,  5.0, a.green),
            (45.0, 1.7,  3.0, a.green.with_alpha(0.4)),
            (80.0, 0.6,  7.0, a.magenta),
            (80.0, 0.6,  5.0, a.magenta.with_alpha(0.3)),
            (30.0, 2.5,  4.0, a.yellow),
            (30.0, 2.5,  2.5, a.yellow.with_alpha(0.3)),
        ];

        let mut kids: Vec<Element> = Vec::new();
        // 軌道の輪 (玉ごとに 1 本)。
        for &(radius, _, _, color) in orbs.iter().step_by(2) {
            kids.push(
                div().pos(cx - radius, cy - radius).w(Px(radius * 2.0)).h(Px(radius * 2.0))
                    .border(1.0, color.with_alpha(0.12)).rounded_px(radius),
            );
        }
        kids.push(div().pos(cx - 4.0, cy - 4.0).w(Px(8.0)).h(Px(8.0)).bg(t.text_primary).rounded_px(4.0));
        // `.pos()` で置くので、 位置は小数のまま (流し込みの行に分けて近似する必要は無い)。
        for (i, &(radius, speed, size, color)) in orbs.iter().enumerate() {
            let delay = if i % 2 == 1 { -0.15 } else { 0.0 };
            let angle = (elapsed + delay) * speed * pi2;
            let ox = cx + angle.cos() * radius - size / 2.0;
            let oy = cy + angle.sin() * radius - size / 2.0;
            kids.push(
                div().pos(ox, oy).w(Px(size)).h(Px(size))
                    .bg(color).rounded_px(size / 2.0).glow(color, 6.0),
            );
        }

        div().w_full().flex_col().gap(0.0).children([
            self.lbl("Orbit — GPU Only", 15.0, 700, t.primary).shrink(0.0).pb(Px(4.0)),
            div().w(Px(area_w)).h(Px(area_h)).shrink(0.0)
                .bg(t.surface)
                .rounded_px(self.round(12.0, 4.0))
                .overflow_hidden()
                .children(kids),
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
        let eased = EasingFunction::EaseOutBack.eval(ping.clamp(0.0, 1.0));

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
            div().w_full().h(Px(140.0)).shrink(0.0)
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
            let phase_x = seed * std::f32::consts::PI;
            let phase_y = seed * 1.57;

            let x = ((elapsed * speed_x * 0.02 + phase_x).sin() * 0.5 + 0.5) * (area_w - 6.0);
            let y = ((elapsed * speed_y * 0.02 + phase_y).cos() * 0.5 + 0.5) * (area_h - 6.0);
            let size = 2.0 + ((seed * 5.0).sin() * 0.5 + 0.5) * 4.0;

            let hue = (i as f32 / count as f32 + elapsed * 0.1) % 1.0;
            let color = Color::new(
                (hue * std::f32::consts::TAU).sin() * 0.5 + 0.5,
                ((hue + 0.33) * std::f32::consts::TAU).sin() * 0.5 + 0.5,
                ((hue + 0.66) * std::f32::consts::TAU).sin() * 0.5 + 0.5,
                0.7,
            );
            dots.push((x, y, size, color));
        }

        let kids: Vec<Element> = dots.iter().map(|&(x, y, size, color)| {
            div().pos(x, y).w(Px(size)).h(Px(size)).bg(color).rounded_px(size / 2.0)
        }).collect();

        div().w_full().flex_col().gap(0.0).children([
            self.lbl("Particles — GPU Only", 15.0, 700, t.primary).shrink(0.0).pb(Px(4.0)),
            div().w(Px(area_w)).h(Px(area_h)).shrink(0.0)
                .bg(t.surface)
                .rounded_px(self.round(12.0, 4.0))
                .overflow_hidden()
                .children(kids),
            self.lbl(&format!("{count} independent dots placed with .pos() — sub-pixel positions, per-dot color."), 12.5, 400, t.text_disabled).shrink(0.0).pt(Px(4.0)),
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

            div().w_full().shrink(0.0).flex_col().gap(2.0).children([
                self.lbl(label, 12.5, 500, t.text_disabled).shrink(0.0),
                div().w(Px(track_w)).h(Px(12.0)).shrink(0.0)
                    .bg(t.surface)
                    .rounded_px(2.0)
                    .flex_row().items_center()
                    .children([
                        div().w(Px(x)).shrink(0.0).allow_overflow(),
                        div().w(Px(16.0)).h(Px(10.0)).shrink(0.0).allow_overflow()
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
                text(format!("{:.0}%", pct)).mono().font_size(14.0).color(t.text_disabled).shrink(0.0),
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
                                    .ml(Px(phase.clamp(-32.0, 32.0))),
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
                    text(format!("{}", value)).mono().bold().font_size(24.0).color(color).shrink(0.0),
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
                let v = (i as f32 * 0.7 + seed + elapsed * 0.3).sin() * 0.5 + 0.5;
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
            line("Network I/O", a.green, std::f32::consts::PI, 40),
            line("Disk IOPS", a.yellow, 1.57, 40),
            self.lbl("Per-bar height animated with sine wave + per-bar alpha.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    fn matrix_rain(&self, t: &Theme, a: &AnsiPalette, ctx: &ViewContext) -> Element {
        let elapsed = self.elapsed();
        let cols = 64;
        let rows = 16;
        let font_size = 13.0;

        // 半角カナ + 数字。 格子は 1 セル 1 字なので、 全角の字は 2 セルを取る。
        let char_vec: Vec<char> = "ｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎ0123456789".chars().collect();

        // 格子は要素 1 つ。 1 字ずつ `text()` を並べると 1024 要素になる。
        let mut grid = CellGrid::new(cols, rows, a.green);
        for row in 0..rows {
            for col in 0..cols {
                let seed = (col as f32 * 7.31 + row as f32 * std::f32::consts::PI) as u32;
                let speed = 1.5 + (seed % 10) as f32 * 0.3;
                let phase = (elapsed * speed + col as f32 * 0.5) % (rows as f32 * 1.5);
                let dist = (phase - row as f32).abs();

                let alpha = if dist < 1.0 { 1.0 } else if dist < 3.0 { 0.6 / dist } else { 0.05 };
                let is_head = dist < 0.5;

                let char_idx = ((seed as f32 + elapsed * 8.0) as usize) % char_vec.len();
                let ch = char_vec[(char_idx + col + row * 7) % char_vec.len()];

                let fg = if is_head {
                    Color::new(0.8, 1.0, 0.8, alpha)
                } else {
                    a.green.with_alpha(alpha * 0.8)
                };
                let flags = if is_head { CellFlags::BOLD } else { CellFlags::NONE };
                grid.set(col, row, GridCell { ch, fg, bg: None, flags });
            }
        }
        let cell_w = ctx.mono_advance * font_size;
        let cell_h = 17.0;

        div().w_full().flex_col().gap(6.0).children([
            self.lbl("Matrix Rain", 15.0, 700, t.primary).shrink(0.0),
            div().shrink(0.0)
                .bg(Color::from_hex("#010a01"))
                .rounded_px(self.round(10.0, 4.0))
                .p(Px(8.0))
                .overflow_hidden()
                .children([
                    cell_grid(std::sync::Arc::new(grid), cell_w, cell_h)
                        .id("matrix-grid")
                        .font_size(font_size),
                ]),
            self.lbl(&format!("{} cells, one element: cell_grid caches glyphs per character and pins them to col × cell_w.", cols * rows), 12.5, 400, t.text_disabled).shrink(0.0),
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
                    self.lbl(label, 13.5, 700, color)
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
        for (row, day) in days.iter().enumerate().take(rows) {
            let mut cells: Vec<Element> = vec![
                self.lbl(day, 12.0, 500, t.text_disabled)
                    .w(Px(30.0)).shrink(0.0),
            ];
            for col in 0..cols {
                let seed = (row * cols + col) as f32 * 2.71;
                let base = (seed * 1.3).sin() * 0.5 + 0.5;
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
            let alpha = (elapsed * 2.0).sin() * 0.5 + 0.5;
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

        let cd_card = div().flex_1().shrink(0.0)
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
        for (i, label) in hour_labels.iter().enumerate() {
            let ang = (i as f32 / 12.0) * pi * 2.0 - pi / 2.0;
            let nr = r - 16.0;
            let x = cx + ang.cos() * nr - 8.0;
            let y = cy + ang.sin() * nr - 7.0;
            face_children.push(
                text(*label).mono().font_size(12.0)
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
        let draw_hand = |angle: f32, length: f32, width: f32, color: Color, children: &mut Vec<Element>| {
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

    fn toggle(&self, t: &Theme, a: &AnsiPalette, ctx: &ViewContext) -> Element {
        let elapsed = self.elapsed();
        let labels = ["Dark Mode", "Notifications", "Auto-save", "Telemetry"];
        let colors = [a.cyan, a.green, a.magenta, a.yellow];

        let switches: Vec<Element> = (0..4).map(|i| {
            let on = self.toggles[i];
            let color = colors[i];

            // Animate knob position over 200ms
            let dur = 0.2;
            let anim_t = ((elapsed - self.toggle_changed[i]) / dur).min(1.0);
            let eased = EasingFunction::EaseOutCubic.eval(anim_t);
            let knob_x = if on { 2.0 + 26.0 * eased } else { 28.0 - 26.0 * eased };
            let blend = if on { eased } else { 1.0 - eased };
            let track_bg = color.with_alpha(0.4 * blend);
            let border_c = if blend > 0.5 { color.with_alpha(0.3) } else { t.border };

            div()
                .click(ctx, format!("toggle-{i}"), move |app: &mut Gallery| {
                    app.toggles[i] = !app.toggles[i];
                    app.toggle_changed[i] = app.elapsed();
                })
                .cursor(Cursor::Pointer)
                .w_full().shrink(0.0).flex_row().items_center().gap(10.0).children([
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
            (concat!("   Compiling sabitori-core v", env!("CARGO_PKG_VERSION")), 1.0, t.text_secondary),
            (concat!("   Compiling sabitori-gpu v", env!("CARGO_PKG_VERSION")), 1.8, t.text_secondary),
            (concat!("   Compiling sabitori v", env!("CARGO_PKG_VERSION")), 2.5, t.text_secondary),
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
            self.lbl("Terminal", 15.0, 700, t.primary).shrink(0.0),
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
            self.lbl("Typewriter effect on terminal commands with staggered timing.", 12.5, 400, t.text_disabled).shrink(0.0)
        );

        div().w_full().flex_col().gap(8.0).children(children)
    }

    // ════════════════════════════════════════
    // Theme gallery
    // ════════════════════════════════════════

    fn theme_gallery(&self, _current: &Theme, ctx: &ViewContext) -> Element {
        // Use cached presets — no allocation per frame
        let cards: Vec<Element> = self.presets.iter().enumerate().map(|(idx, theme)| {
            let is_active = theme.name == self.theme.name;
            let border_c = if is_active { theme.primary } else { theme.border };

            // Minimal card: name + 4 color dots. No text-heavy content.
            div()
                .click(ctx, format!("theme-{idx}"), move |app: &mut Gallery| {
                    app.theme = app.presets[idx].clone();
                })
                .cursor(Cursor::Pointer)
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
            div().w_full()
                .flex_row().flex_wrap(FlexWrap::Wrap).gap(6.0)
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
            div()
                .click(ctx, id, move |app: &mut Gallery| {
                    app.splash_preview_idx = i;
                    app.splash_preview_start = app.elapsed();
                })
                .cursor(Cursor::Pointer)
                .shrink(0.0)
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
            div().w_full().flex_row().flex_wrap(FlexWrap::Wrap).gap(4.0)
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
        // Text input: `view()` に置くだけ。 打鍵・IME・キャレットの点滅・焦点は
        // ランタイムが配る (`on_input` に配線を書くと二重になる)。
        let input_style = TextInputStyle {
            bg: t.surface,
            border: t.border,
            text: t.text_primary,
            placeholder: t.text_disabled,
            font_size: 14.0,
            radius: self.round(8.0, 3.0),
            padding: 10.0,
            focus_border: Some(t.primary),
            caret: Some(t.text_primary),
            preedit: Some(t.primary.with_alpha(0.25)),
            ..TextInputStyle::default_dark()
        };
        let input = text_input(ctx, "form-input", &self.form_text, &input_style);

        // Checkboxes
        let check_labels = ["Enable notifications", "Dark mode", "Auto-save"];
        let checks: Vec<Element> = (0..3).map(|i| {
            checkbox(&format!("form-check-{i}"), check_labels[i], self.form_checks[i], t.text_primary, a.green, t.border)
                .click(ctx, format!("form-check-{i}"), move |app: &mut Gallery| {
                    app.form_checks[i] = !app.form_checks[i];
                })
        }).collect();

        // Radio buttons
        let radio_labels = ["Small", "Medium", "Large"];
        let radios: Vec<Element> = (0..3).map(|i| {
            radio(&format!("form-radio-{i}"), radio_labels[i], self.form_radio == i, t.text_primary, t.primary, t.border)
                .click(ctx, format!("form-radio-{i}"), move |app: &mut Gallery| app.form_radio = i)
        }).collect();

        // Slider: 押した所へ飛び、 離すまで指に付いてくる (`on_pointer_move`)。
        let slider_el = slider("form-slider", self.form_slider, 300.0, t.surface_elevated, t.primary, Color::WHITE)
            .click(ctx, "form-slider", |app: &mut Gallery| {
                app.slider_dragging = true;
                app.drag_slider_to(app.pointer_x);
            });

        // Dropdown: `DropdownState` の trigger + menu。 開閉と選択は `on_click` の
        // `handle_click` が解釈する。 menu は trigger に `anchor_to` で貼り付くので、
        // 位置を自分で計らない。
        let dd_style = DropdownStyle {
            bg: t.surface,
            bg_hover: t.surface_hover,
            fg: t.text_primary,
            fg_selected: t.primary,
            border_color: t.border,
            border_active: t.primary,
            menu_bg: t.surface_elevated,
            menu_item_hover: t.surface_hover,
            corner_radius: self.round(8.0, 3.0),
            ..DropdownStyle::default_dark()
        };
        let dropdown_el = div().w_full().flex_col().children(
            std::iter::once(self.form_dropdown.trigger(&dd_style, ctx.hovered.as_deref()))
                .chain(self.form_dropdown.menu(ctx.hovered.as_deref(), &dd_style)),
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
            self.lbl("Right-click anywhere in this area to open a menu. Right-click again while it is open to reopen it in place.", 13.0, 400, t.text_secondary).shrink(0.0),
            div().id("ctx-area")
                .w_full().h(Px(200.0)).shrink(0.0)
                .bg(t.surface_elevated)
                .border(1.0, t.border)
                .rounded_px(self.round(12.0, 4.0))
                .flex_col().items_center().justify_center().gap(6.0)
                .children([
                    self.lbl("Right-click here", 13.0, 500, t.text_disabled).shrink(0.0),
                    match self.ctx_last {
                        Some(action) => self.lbl(&format!("last action: {action}"), 12.5, 500, t.primary).shrink(0.0),
                        None => div(),
                    },
                ]),
            text("context_menu() + .right_click_through() on the backdrop")
                .mono().font_size(13.0).color(t.text_disabled).shrink(0.0),
        ])
    }

    fn view_transition(&self, t: &Theme, a: &AnsiPalette, ctx: &ViewContext) -> Element {
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
                    let active = kind == i;
                    div()
                        .click(ctx, format!("trans-{i}"), move |app: &mut Gallery| {
                            app.transition_kind = i;
                            app.switch_view();
                        })
                        .cursor(Cursor::Pointer)
                        .shrink(0.0)
                        .bg(if active { t.primary.with_alpha(0.2) } else { t.surface })
                        .rounded_px(3.0)
                        .border(1.0, if active { t.primary } else { t.border })
                        .px_pad(Px(8.0)).py(Px(3.0))
                        .children([
                            self.lbl(name, 12.5, 500, if active { t.primary } else { t.text_secondary }).shrink(0.0),
                        ])
                }).collect::<Vec<_>>()
            ),
            content,
            // Label + trigger
            div().flex_row().gap(8.0).items_center().children([
                self.lbl(label, 14.0, 700, color).shrink(0.0),
                div().flex_1(),
                div().click(ctx, "trans-trigger", |app: &mut Gallery| app.switch_view())
                    .cursor(Cursor::Pointer).shrink(0.0)
                    .bg(t.primary).rounded_px(self.round(6.0, 3.0))
                    .px_pad(Px(10.0)).py(Px(4.0))
                    .children([
                        self.lbl("Switch", 13.0, 600, Color::WHITE).shrink(0.0),
                    ]),
                self.lbl(kind_name, 12.5, 400, t.text_disabled).shrink(0.0),
            ]),
        ])
    }

    fn modal(&self, t: &Theme, _a: &AnsiPalette, ctx: &ViewContext) -> Element {
        div().w_full().flex_col().gap(12.0).children([
            self.lbl("Modal Dialog", 15.0, 700, t.primary).shrink(0.0),
            self.lbl("Full-screen overlay with backdrop dim + fade.", 12.5, 400, t.text_disabled).shrink(0.0),
            self.lbl("Press [m] or click the button below.", 12.5, 400, t.text_disabled).shrink(0.0),
            div().click(ctx, "modal-open", |app: &mut Gallery| app.toggle_modal())
                .cursor(Cursor::Pointer).shrink(0.0).self_start()
                .bg(t.primary).rounded_px(self.round(6.0, 3.0))
                .px_pad(Px(14.0)).py(Px(6.0))
                .children([
                    self.lbl("Open Modal", 13.0, 600, Color::WHITE).shrink(0.0),
                ]),
        ])
    }

    fn toast(&self, t: &Theme, a: &AnsiPalette, ctx: &ViewContext) -> Element {
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
            div().click(ctx, "toast-spawn", |app: &mut Gallery| app.spawn_toast())
                .cursor(Cursor::Pointer).shrink(0.0).self_start()
                .bg(t.primary).rounded_px(self.round(6.0, 3.0))
                .px_pad(Px(10.0)).py(Px(4.0))
                .children([
                    self.lbl("Spawn Toast", 13.0, 600, Color::WHITE).shrink(0.0),
                ]),
            // Toast area
            div().w_full().min_h(Px(180.0)).flex_col().gap(4.0).justify_end()
                .children(toast_elements),
        ])
    }

    // ════════════════════════════════════════
    // v0.13 より後に入った物
    // ════════════════════════════════════════

    /// 文字の選択を読む (`ctx.text_selection()` / `on_selection_changed`)。
    fn text_selection(&self, t: &Theme, a: &AnsiPalette, ctx: &ViewContext) -> Element {
        let mark_color = a.yellow.with_alpha(0.35);
        let paras: Vec<Element> = SEL_PARAS.iter().map(|&(id, body)| {
            let ranges: Vec<(usize, usize)> = self.marks.iter()
                .filter(|(owner, _)| owner == id)
                .map(|(_, r)| (r.start, r.end))
                .collect();
            let mut para = text(body).font_size(15.0).color(t.text_primary).line_height(1.6).min_w(Px(0.0));
            if self.modern {
                para = para.letter_spacing(0.1);
            } else {
                para = para.mono();
            }
            if !ranges.is_empty() {
                para = para.highlight(HighlightSpec { ranges, color: mark_color, current: None, current_color: mark_color });
            }
            // id は段落の入れ物に付ける。 選択の `owner` は「id の付いたいちばん近い祖先」。
            div().id(id).w_full().child(para)
        }).collect();

        let readout: Vec<Element> = match ctx.text_selection() {
            Some(sel) => sel.pieces.iter().map(|p| {
                let owner = p.owner.as_deref().unwrap_or("-");
                text(format!("#{owner}  {}..{}  \"{}\"", p.range.start, p.range.end, p.text()))
                    .mono().font_size(12.5).color(t.text_secondary).max_lines(1)
            }).collect(),
            None => vec![self.lbl("Nothing selected — drag across the text above.", 12.5, 400, t.text_disabled)],
        };

        let button = |id: &'static str, label: &str, enabled: bool| {
            div().id(id).cursor(Cursor::Pointer).no_select().shrink(0.0)
                .bg(if enabled { t.primary } else { t.surface_elevated })
                .rounded_px(self.round(6.0, 3.0))
                .px_pad(Px(12.0)).py(Px(5.0))
                .children([self.lbl(label, 13.0, 600, if enabled { Color::WHITE } else { t.text_disabled })])
        };

        div().w_full().flex_col().gap(12.0).children([
            self.lbl("Text Selection", 15.0, 700, t.primary).shrink(0.0),
            div().w_full().flex_col().gap(10.0)
                .bg(t.surface_elevated).border(1.0, t.border)
                .rounded_px(self.round(12.0, 4.0)).p(Px(16.0))
                .children(paras),
            div().flex_row().gap(8.0).items_center().children([
                button("sel-mark", "Mark selection", !self.selection.is_empty())
                    .click(ctx, "sel-mark", |app: &mut Gallery| app.mark_selection()),
                button("sel-clear", "Clear marks", !self.marks.is_empty())
                    .click(ctx, "sel-clear", |app: &mut Gallery| app.marks.clear()),
                self.lbl(&format!("{} marker(s)", self.marks.len()), 12.5, 500, t.text_disabled).shrink(0.0),
            ]),
            div().w_full().flex_col().gap(4.0)
                .bg(t.surface).rounded_px(self.round(8.0, 3.0)).p(Px(10.0))
                .children(readout),
            self.lbl("Each piece comes back as (owner id, byte range, rect). Marks are drawn with HighlightSpec.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    /// 角丸矩形 2 つを 1 つの形として塗る `goo` (首でつながる)。
    fn goo_demo(&self, t: &Theme, a: &AnsiPalette, ctx: &ViewContext) -> Element {
        let elapsed = self.elapsed();
        let (stage_w, stage_h) = (560.0, 250.0);
        let fill = t.surface_elevated;
        let rim = t.primary.with_alpha(0.55);
        let shadow = BoxShadow { color: Color::new(0.0, 0.0, 0.0, 0.45), offset: Point::new(0.0, 6.0), blur: 18.0, spread: 0.0 };

        // 左: ピルから垂れて出るメニュー。 押すと開閉する (ばね)。
        let (px, py, pw, ph) = GOO_PILL;
        let pill = Rect::new(px, py, pw, ph);
        let p = self.goo_p.max(0.0);
        let lerp = |a: f32, b: f32| a + (b - a) * p;
        let open = Rect::new(px - (GOO_PANEL.0 - pw) * 0.5, py + ph + 14.0, GOO_PANEL.0, GOO_PANEL.1);
        let panel = Rect::new(
            lerp(px + 8.0, open.origin.x),
            lerp(py + 4.0, open.origin.y),
            lerp(pw - 16.0, open.size.width),
            lerp(ph - 8.0, open.size.height),
        );
        // 出ていく間は首を太く、 落ち着いたら消す。
        let k = 48.0 * (1.0 - self.goo_p.clamp(0.0, 1.0)).powf(1.5);
        let mut kids: Vec<Element> = vec![
            goo(pill, panel).goo_radii(ph / 2.0, self.round(14.0, 4.0)).goo_smooth(k)
                .goo_gradient(t.surface_hover, fill).goo_border(1.0, rim).goo_shadow(shadow),
            // 押せる所はピルの上に重ねた透明な箱。 goo 自体は当たりを持たない。
            div().pos(px, py).w(Px(pw)).h(Px(ph))
                .click(ctx, "goo-pill", |app: &mut Gallery| app.goo_open = !app.goo_open)
                .cursor(Cursor::Pointer)
                .flex_row().items_center().justify_center()
                .children([self.lbl(if self.goo_open { "Close ▴" } else { "Menu ▾" }, 13.0, 600, t.text_primary)]),
        ];
        // 行は goo の後に置く — goo は木の順で背景として描かれ、 後ろの兄弟が上に乗る。
        if self.goo_p > 0.6 {
            let rows = ["New window", "Duplicate", "Rename…"];
            for (i, label) in rows.iter().enumerate() {
                let y = panel.origin.y + 16.0 + i as f32 * 40.0;
                if y + 30.0 > panel.origin.y + panel.size.height {
                    break;
                }
                kids.push(
                    div().pos(panel.origin.x + 12.0, y).w(Px(panel.size.width - 24.0)).h(Px(30.0))
                        .bg(t.surface_hover.with_alpha(0.6)).rounded_px(self.round(8.0, 2.0))
                        .opacity(((self.goo_p - 0.6) / 0.4).clamp(0.0, 1.0))
                        .flex_row().items_center().px_pad(Px(10.0))
                        .children([self.lbl(label, 13.0, 500, t.text_secondary)]),
                );
            }
        }

        // 右: 帯から雫が落ちては戻る。 隙間が smooth の半分を切ると首でつながる。
        let bar = Rect::new(330.0, 30.0, 200.0, 26.0);
        // 二乗して、 帯の近く (首でつながっている所) に長く居させる。
        let phase = ((elapsed * 0.8).sin() * 0.5 + 0.5).powi(2);
        let drop_size = 34.0 + 10.0 * (1.0 - phase);
        let drop = Rect::new(430.0 - drop_size / 2.0, 62.0 + phase * 120.0, drop_size, drop_size);
        kids.push(
            goo(bar, drop).goo_radii(self.round(13.0, 3.0), drop_size / 2.0).goo_smooth(64.0)
                .goo_gradient(a.cyan.with_alpha(0.85), a.blue.with_alpha(0.85)),
        );

        div().w_full().flex_col().gap(8.0).children([
            self.lbl("Goo — SDF smooth union", 15.0, 700, t.primary).shrink(0.0),
            div().w(Px(stage_w)).h(Px(stage_h)).shrink(0.0)
                .bg(t.surface).rounded_px(self.round(12.0, 4.0)).overflow_hidden()
                .children(kids),
            self.lbl("Click the pill: the panel drips out with a liquid neck. goo_gradient / goo_border / goo_shadow follow the fused outline.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    /// 重なった箱。重なりは**書いた順**で決まる (主窓は木の順に描く)。先に書いた物を
    /// 手前に出したいときだけ `.layer(n)` で上げる。
    fn layers_demo(&self, t: &Theme, a: &AnsiPalette) -> Element {
        let elapsed = self.elapsed();
        let slide = (elapsed * 0.9).sin() * 18.0;

        // 文字を持つカードと、 半分かぶさる不透明なカード。 `card_first` なら文字のカードを
        // 後に書く。 `raise` なら先に書いた方を `.layer(1)` で上げる。
        let scene = |card_first: bool, raise: bool, caption: &str| -> Element {
            let mut card = div().pos(16.0, 18.0).w(Px(200.0)).h(Px(96.0))
                .bg(t.surface_elevated).border(1.0, t.border).rounded_px(self.round(10.0, 3.0))
                .p(Px(12.0)).flex_col().gap(4.0)
                .children([
                    self.lbl("Card", 13.5, 700, t.text_primary),
                    self.lbl("This text belongs to the card.", 12.5, 400, t.text_secondary),
                ]);
            let mut over = div().pos(90.0 + slide, 64.0).w(Px(120.0)).h(Px(80.0))
                .bg(a.magenta.lerp(t.surface, 0.55)).border(1.0, a.magenta).rounded_px(self.round(10.0, 3.0))
                .shadow_md(Color::new(0.0, 0.0, 0.0, 0.45))
                .p(Px(12.0))
                .children([self.lbl("Box", 13.5, 700, Color::WHITE)]);
            let kids = if card_first {
                if raise {
                    over = over.layer(1);
                }
                [over, card]
            } else {
                if raise {
                    card = card.layer(1);
                }
                [card, over]
            };
            div().flex_1().flex_col().gap(6.0).children([
                div().w_full().h(Px(170.0))
                    .bg(t.surface).rounded_px(self.round(12.0, 4.0)).overflow_hidden()
                    .children(kids),
                text(caption).mono().font_size(12.5).color(if raise { a.green } else { a.yellow }),
            ])
        };

        div().w_full().flex_col().gap(10.0).children([
            self.lbl("Overlap — written order", 15.0, 700, t.primary).shrink(0.0),
            div().w_full().flex_row().gap(12.0).children([
                scene(false, false, "card, box: the box covers the card"),
                scene(true, false, "box, card: the card covers the box"),
                scene(true, true, "box.layer(1), card: the box comes back on top"),
            ]),
            self.lbl("Later siblings draw on top, text and images included.", 12.5, 400, t.text_disabled).shrink(0.0),
            self.lbl(".layer(n) lifts something above siblings written after it.", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }

    /// 窓の中のドラッグ (`draggable` / `droppable` / `on_drop`) と、 外から持ち込む
    /// ファイル (`FILE_DRAG` / `on_file_drop_at`)。
    fn drag_drop(&self, t: &Theme, a: &AnsiPalette, ctx: &ViewContext) -> Element {
        let over = ctx.drag_over();
        let dragging_card = ctx.drag.as_ref().filter(|d| d.data != FILE_DRAG).and_then(|d| d.source_id.clone());
        let col_colors = [a.cyan, a.yellow, a.green];

        let columns: Vec<Element> = DD_COLUMNS.iter().enumerate().map(|(c, name)| {
            let col_id = format!("dd-col-{c}");
            let hot = over == Some(col_id.as_str()) && dragging_card.is_some();
            let cards: Vec<Element> = (0..DD_CARDS.len()).filter(|&i| self.dd_cards[i] == c).map(|i| {
                let id = format!("dd-card-{i}");
                let lifted = dragging_card.as_deref() == Some(id.as_str());
                div().id(id).draggable(i.to_string()).cursor(Cursor::Pointer).no_select()
                    .w_full().shrink(0.0)
                    .bg(t.surface_elevated).border(1.0, if lifted { col_colors[c] } else { t.border })
                    .rounded_px(self.round(8.0, 2.0))
                    .opacity(if lifted { 0.35 } else { 1.0 })
                    .px_pad(Px(10.0)).py(Px(8.0))
                    .flex_row().items_center().gap(8.0)
                    .children([
                        div().w(Px(6.0)).h(Px(6.0)).rounded_px(3.0).bg(col_colors[c]).shrink(0.0),
                        self.lbl(DD_CARDS[i], 13.0, 500, t.text_primary),
                    ])
            }).collect();
            let count = cards.len();
            div().id(col_id).droppable()
                .flex_1().min_h(Px(220.0)).flex_col().gap(6.0).p(Px(8.0))
                .bg(if hot { col_colors[c].with_alpha(0.12) } else { t.surface })
                .border(1.0, if hot { col_colors[c] } else { t.border })
                .rounded_px(self.round(12.0, 4.0))
                .children(
                    std::iter::once(
                        div().flex_row().items_center().gap(6.0).pb(Px(4.0)).children([
                            self.lbl(name, 12.5, 700, col_colors[c]),
                            self.lbl(&count.to_string(), 12.0, 500, t.text_disabled),
                        ]),
                    ).chain(cards),
                )
        }).collect();

        // 外 (Finder など) からのファイル。 落とす前から `ctx.drag` に FILE_DRAG で出る。
        let files_incoming = ctx.drag_data() == Some(FILE_DRAG);
        let files_hot = files_incoming && over == Some("dd-files");
        let file_lines: Vec<Element> = if self.dd_files.is_empty() {
            vec![self.lbl(if files_incoming { "Drop here" } else { "Drag files in from Finder" }, 13.0, 500,
                if files_incoming { a.green } else { t.text_disabled })]
        } else {
            self.dd_files.iter().take(4)
                .map(|n| text(n).mono().font_size(12.5).color(t.text_secondary).max_lines(1))
                .collect()
        };
        let file_zone = div().id("dd-files").droppable()
            .w_full().min_h(Px(64.0)).p(Px(10.0)).flex_col().gap(3.0).justify_center()
            .bg(if files_hot { a.green.with_alpha(0.12) } else { t.surface })
            .border(1.0, if files_hot { a.green } else if files_incoming { a.green.with_alpha(0.5) } else { t.border })
            .rounded_px(self.round(12.0, 4.0))
            .children(file_lines);

        div().w_full().flex_col().gap(10.0).children([
            self.lbl("Drag & Drop", 15.0, 700, t.primary).shrink(0.0),
            div().w_full().flex_row().gap(8.0).items_start().children(columns),
            file_zone,
            self.lbl("Cards: .draggable() → .droppable() → on_drop. Files from other apps light the same drop zones and arrive in on_file_drop_at (macOS).", 12.5, 400, t.text_disabled).shrink(0.0),
        ])
    }
}

impl DeclarativeApp for Gallery {
    fn title(&self) -> &str { "Sabitori TUI Gallery" }
    fn size(&self) -> (f32, f32) { (1000.0, 660.0) }
    fn transparent(&self) -> bool { true }

    /// HackGen (白源)。 Hack だけだと wasm で日本語が豆腐になる。
    ///
    /// 字形は Hack のままだが **advance は詰まっている** (0.602em → 0.527em)
    /// ので、 英数字は Hack より約 12% 細く出る。 その代わり半角 2 文字が
    /// 全角 1 文字にちょうど乗るので、 罫線・日本語・英数字が同じ桁に揃う ──
    /// TUI を名乗る以上こちらが正しい。
    fn fonts(&self) -> Vec<Vec<u8>> {
        vec![
            include_bytes!("../assets/fonts/HackGen-Regular.ttf").to_vec(),
            include_bytes!("../assets/fonts/HackGen-Bold.ttf").to_vec(),
        ]
    }

    /// スピナー・グラデーション・波・パルス・タイプライタが常時回っていて、
    /// 入力が 1 つも無くても絵が変わり続ける。 既定の `lazy_render` に対して
    /// 「自分は動いている」と名乗る側。
    fn is_animating(&self) -> bool {
        true
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
        // Goo のピル: 少しだけ行き過ぎるばね。
        let target = if self.goo_open { 1.0 } else { 0.0 };
        let dt = dt.min(1.0 / 30.0);
        self.goo_v += ((target - self.goo_p) * 180.0 - self.goo_v * 20.0) * dt;
        self.goo_p += self.goo_v * dt;
    }

    fn overlay_view(&self, ctx: &ViewContext) -> Option<Element> {
        let t = &self.theme;
        let a = &t.ansi;

        // Context menu overlay
        if let Some((mx, my)) = self.ctx_menu {
            let entry = |id: &'static str, label: &'static str, shortcut: &str| {
                let item = MenuItem::new(id, label).shortcut(shortcut);
                let hovered = ctx.hovered.as_deref() == Some(id);
                context_menu_item(&item, hovered, t.text_primary, t.text_secondary, t.surface_hover)
                    .click(ctx, id, move |app: &mut Gallery| app.ctx_last = Some(label))
            };
            let menu_items = vec![
                entry("ctx-copy", "Copy", "\u{2318}C"),
                entry("ctx-paste", "Paste", "\u{2318}V"),
                entry("ctx-delete", "Delete", "\u{232b}"),
                menu_separator(t.border),
                entry("ctx-select-all", "Select All", "\u{2318}A"),
            ];

            // 幕は右クリックを下へ通す (v0.38)。 開いたまま別の所を右クリックすると、
            // 幕ではなくその物に `on_right_click` が届き、 その場で開き直せる。
            return Some(context_menu(
                ctx.width, ctx.height, mx, my,
                menu_items, 200.0,
                "ctx-backdrop",
                t.surface_elevated.with_alpha(t.opacity), t.border.with_alpha(t.opacity * 0.6),
            ).right_click_through());
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

        let rad = self.round(14.0, 4.0);
        // 押せるのは開いている間だけ (閉じかけの幕が次のクリックを吸わないように)。
        let closable = |e: Element, id: &'static str| {
            if self.modal_open { e.click(ctx, id, |app: &mut Gallery| app.toggle_modal()) } else { e }
        };
        Some(closable(div(), "modal-backdrop")
            .w(Px(ctx.width)).h(Px(ctx.height))
            .bg(Color::new(0.0, 0.0, 0.0, backdrop_alpha))
            .flex_col().items_center().justify_center()
            .children([
                // ダイアログ自身はクリックを吸う (余白を押しても閉じない)。
                (if self.modal_open { div().id("modal-dialog") } else { div() })
                    .w(Px(w)).h(Px(h))
                    .bg(t.surface_elevated)
                    .border(1.0, t.border.with_alpha(modal_alpha))
                    .rounded_px(rad)
                    .opacity(modal_alpha)
                    .shadow_md(Color::new(0.0, 0.0, 0.0, 0.5 * modal_alpha))
                    .flex_col().items_center().justify_center().gap(12.0)
                    .children([
                        self.lbl("Confirm Action?", 16.0, 700, t.text_primary).shrink(0.0),
                        self.lbl("This cannot be undone.", 13.5, 400, t.text_secondary).shrink(0.0),
                        div().flex_row().gap(10.0).children([
                            // 開くボタン (`modal-open`) と id を分ける。 同じ id が 2 つあると、
                            // 片方のホバーの見た目がもう片方に移る。
                            closable(div(), "modal-cancel").cursor(Cursor::Pointer).shrink(0.0)
                                .bg(t.surface_hover).rounded_px(self.round(7.0, 3.0))
                                .border(1.0, t.border)
                                .px_pad(Px(16.0)).py(Px(6.0))
                                .children([self.lbl("Cancel", 13.5, 500, t.text_primary).shrink(0.0)]),
                            closable(div(), "modal-confirm").cursor(Cursor::Pointer).shrink(0.0)
                                .bg(a.green.with_alpha(0.8)).rounded_px(self.round(7.0, 3.0))
                                .px_pad(Px(16.0)).py(Px(6.0))
                                .children([self.lbl("Confirm", 13.5, 600, Color::WHITE).shrink(0.0)]),
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
        let mut prev_section: Option<Section> = None;
        for (i, &(_, name, section)) in ITEMS.iter().enumerate() {
            // Section headers
            let section_label = |label: &str, color: Color| -> Element {
                let base = if modern {
                    text(label).font_size(10.5).font_weight(700).letter_spacing(1.4).color(color)
                } else {
                    text(label).mono().font_size(11.0).color(color)
                };
                base.shrink(0.0).px_pad(Px(8.0)).pt(Px(if modern { 12.0 } else { 6.0 })).pb(Px(3.0))
            };

            if prev_section != Some(section) {
                if prev_section.is_some() {
                    menu_items.push(hsep(s_border));
                }
                let color = match section {
                    Section::Text | Section::Widgets => s_text_dim,
                    Section::Visual => a.cyan.with_alpha(0.7),
                    Section::Gpu => s_accent.with_alpha(0.75),
                    Section::Themes => a.magenta.with_alpha(0.7),
                    Section::Splash => a.bright_cyan.with_alpha(0.7),
                    Section::Forms => a.green.with_alpha(0.7),
                    Section::Interaction => a.yellow.with_alpha(0.7),
                };
                menu_items.push(section_label(section.label(), color));
                prev_section = Some(section);
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
                text(name).font_size(13.5).font_weight(if is_active { 600 } else { 500 }).letter_spacing(0.1).color(fg)
            } else {
                text(name).mono().font_size(14.0).color(fg)
            };

            menu_items.push(
                div().click(ctx, id, move |app: &mut Gallery| app.select(i, false))
                    .cursor(Cursor::Pointer).no_select()
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
        let preview = match self.demo() {
            Demo::Typewriter => self.typewriter(t),
            Demo::GradientText => self.gradient_text(t),
            Demo::WaveText => self.wave_text(t),
            Demo::Terminal => self.terminal(t, a),
            Demo::MatrixRain => self.matrix_rain(t, a, ctx),
            Demo::TextSelection => self.text_selection(t, a, ctx),
            Demo::ProgressBars => self.progress_bars(t, a),
            Demo::Spinners => self.spinners(t),
            Demo::Toggle => self.toggle(t, a, ctx),
            Demo::Tabs => self.tabs_demo(t, a),
            Demo::Counter => self.counter(t, a),
            Demo::Skeleton => self.skeleton(t),
            Demo::Sparkline => self.sparkline(t, a),
            Demo::Clock => self.clock(t, a),
            Demo::EasingCurves => self.easing_curves(t),
            Demo::PulseBorder => self.pulse_border(t, a),
            Demo::ColorTween => self.color_tween(t),
            Demo::Gradient => self.gradient_demo(t, a),
            Demo::Glassmorphism => self.glassmorphism(t, a),
            Demo::BentoGrid => self.bento_grid(t, a),
            Demo::Heatmap => self.heatmap(t, a),
            Demo::Orbit => self.orbit(t, a),
            Demo::GlowPulse => self.glow_pulse(t, a),
            Demo::Morph => self.morph(t, a),
            Demo::Particles => self.particles(t),
            Demo::SmoothMotion => self.smooth_motion(t, a),
            Demo::Carousel => self.carousel(t, a),
            Demo::Goo => self.goo_demo(t, a, ctx),
            Demo::Layers => self.layers_demo(t, a),
            Demo::ThemeGallery => self.theme_gallery(t, ctx),
            Demo::SplashPresets => self.splash_presets(t, a, ctx),
            Demo::FormControls => self.form_controls(t, a, ctx),
            Demo::ContextMenu => self.context_menu_demo(t),
            Demo::ViewTransition => self.view_transition(t, a, ctx),
            Demo::Modal => self.modal(t, a, ctx),
            Demo::Toast => self.toast(t, a, ctx),
            Demo::DragDrop => self.drag_drop(t, a, ctx),
        };

        // Fade the preview in whenever the selected component changes.
        let fade = {
            let x = ((elapsed - self.sel_at) / 0.2).clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        };
        let preview = preview.opacity(fade);

        // Framed "stage" — components sit on an elevated, bordered, shadowed
        // surface instead of floating in a bare void. No recentering, so
        // components that use absolute .pos() keep a stable top-left origin.
        //
        // 本文のスクロールはランタイムに任せる (`.scroll(id)`)。 以前は部品ごとの
        // 高さの見積り表と `on_scroll` で自前で動かしていたので、 見積りに無い
        // 部品は窓が低いと下が切れたまま届かなかった。
        let stage_inner = div().flex_1().w_full()
            .scroll("stage")
            .scrollbar_grab(14.0)
            .scrollbar_lit(s_text_dim.with_alpha(0.6), s_text_mid)
            .scrollbar(s_text_dim.with_alpha(0.4))
            .p(Px(20.0)).flex_col().children([preview]);

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
        // 角の丸みは小さい方の辺の半分まで (999 のような大きな値は小さな箱で
        // 何も描かれないことがある)。
        let skin_toggle = if modern {
            div()
                .flex_row().items_center().gap(6.0)
                .px_pad(Px(10.0)).py(Px(4.0)).rounded_px(10.0)
                .bg(s_surface_hi).border(1.0, s_border)
                .children([
                    div().w(Px(6.0)).h(Px(6.0)).rounded_px(3.0).bg(s_accent),
                    text("MODERN").font_size(10.0).font_weight(600).letter_spacing(1.0).color(s_text_mid).shrink(0.0),
                ])
        } else {
            div()
                .flex_row().items_center()
                .children([text("[TUI]").mono().font_size(14.0).color(t.primary).shrink(0.0)])
        };
        let skin_toggle = skin_toggle
            .click(ctx, "skin-toggle", |app: &mut Gallery| app.modern = !app.modern)
            .cursor(Cursor::Pointer)
            .no_select()
            .tooltip("Switch skin: TUI ↔ Modern (M)");
        let item_name = ITEMS[self.selected].1;
        let version = concat!("sabitori v", env!("CARGO_PKG_VERSION"));

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
                    text(item_name).font_size(15.0).font_weight(500).color(s_text_mid).shrink(0.0),
                    div().flex_1(),
                    skin_toggle,
                    text(format!("{}/{}", self.selected + 1, ITEMS.len()))
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
                    text(item_name).mono().font_size(14.0).color(t.text_primary).shrink(0.0),
                    div().flex_1(),
                    skin_toggle,
                    text(format!("{}/{}", self.selected + 1, ITEMS.len()))
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
                    text(format!("{version} · {} · {} demos", t.name, ITEMS.len()))
                        .font_size(11.0).font_weight(500).letter_spacing(0.5).color(s_text_dim).shrink(0.0),
                ])
        } else {
            div().w_full().h(Px(20.0)).shrink(0.0)
                .bg(t.surface_elevated)
                .flex_row().items_center().px_pad(Px(10.0))
                .children([
                    text(format!("{version}  |  {}  |  {} demos", t.name, ITEMS.len()))
                        .mono().font_size(14.0).color(t.text_disabled).shrink(0.0),
                ])
        };

        // ── Layout ──
        let mut root_kids: Vec<Element> = Vec::new();
        // Modern skin: soft aurora glows behind the (transparent) sidebar.
        if modern {
            // v0.17 で `.opacity()` が二重に掛からなくなり、 同じ値のままだと芯が
            // くっきりした円に見える。 以前の見た目 (= op²) に合わせて二乗で渡す。
            let blob = |cx: f32, cy: f32, r: f32, col: Color, op: f32, gr: f32| {
                div().absolute().pos(cx - r, cy - r).w(Px(r * 2.0)).h(Px(r * 2.0))
                    .rounded_px(r).bg(col).opacity(op * op).glow(col, gr)
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

    fn scroll_intents(&mut self) -> Vec<ScrollIntent> {
        std::mem::take(&mut self.scroll_requests)
    }

    fn on_build(&mut self, build: &sabitori_core::build::BuildResult) {
        self.slider_track = build.region_rect("form-slider");
    }

    fn on_selection_changed(&mut self, selection: Option<&SelectedText>) {
        self.selection = selection
            .map(|sel| {
                sel.pieces.iter()
                    .filter_map(|p| {
                        let owner = p.owner.as_deref()?;
                        SEL_PARAS.iter().any(|(id, _)| *id == owner).then(|| (owner.to_string(), p.range.clone()))
                    })
                    .collect()
            })
            .unwrap_or_default();
    }

    fn on_drop(&mut self, data: &str, target_id: &str) {
        let card = data.parse::<usize>().ok().filter(|&i| i < DD_CARDS.len());
        let column = target_id.strip_prefix("dd-col-").and_then(|c| c.parse::<usize>().ok()).filter(|&c| c < DD_COLUMNS.len());
        if let (Some(card), Some(column)) = (card, column) {
            self.dd_cards[card] = column;
        }
    }

    fn drag_ghost(&self, ctx: &ViewContext) -> Option<Element> {
        let drag = ctx.drag.as_ref()?;
        let card = drag.data.parse::<usize>().ok().filter(|&i| i < DD_CARDS.len())?;
        let t = &self.theme;
        Some(
            div().pos(ctx.mouse_x + 10.0, ctx.mouse_y + 6.0)
                .bg(t.surface_elevated).border(1.0, t.primary)
                .rounded_px(self.round(8.0, 2.0))
                .shadow_md(Color::new(0.0, 0.0, 0.0, 0.5))
                .px_pad(Px(10.0)).py(Px(8.0))
                .children([self.lbl(DD_CARDS[card], 13.0, 600, t.text_primary)]),
        )
    }

    fn on_file_drop_at(&mut self, paths: Vec<PathBuf>, target_id: Option<&str>) {
        if target_id != Some("dd-files") {
            return;
        }
        for p in paths {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string());
            self.dd_files.insert(0, name);
        }
        self.dd_files.truncate(8);
    }

    fn on_pointer_move(&mut self, x: f32, _y: f32) {
        self.pointer_x = x;
        if self.slider_dragging {
            self.drag_slider_to(x);
        }
    }

    fn on_pointer_up(&mut self) {
        self.slider_dragging = false;
    }

    fn on_right_click(&mut self, id: &str, x: f32, y: f32) {
        // 幕は右クリックを通すので、 メニューを開いたまま押しても届く。
        // 枠の上なら開き直し、 それ以外なら閉じる。
        self.ctx_menu = (self.demo() == Demo::ContextMenu && id == "ctx-area").then_some((x, y));
    }

    /// 押された id の解釈が要る物 (浮いている物の後始末と `DropdownState`) だけ。
    /// 押したら何をするかは、 それぞれの要素に `.click(ctx, ..)` で書いてある
    /// (こちらが先に呼ばれる)。
    fn on_click(&mut self, id: &str) {
        // 開いているメニューは、どこを押しても閉じる (項目の処理はこの後に走る)。
        self.ctx_menu = None;
        if self.form_dropdown.handle_click(id) == DropdownEvent::Ignored {
            self.form_dropdown.close();
        }
    }

    fn on_input(&mut self, event: &InputEvent) -> bool {
        if !self.splash_done {
            self.splash_done = true;
            return true;
        }
        // テキスト欄への配線は要らない。 `text_input(..)` を `view()` に置いた
        // 時点でランタイムが配る (焦点のある欄が先に受けるので、 ここの j / k は
        // 欄に打っている間は来ない)。
        if let InputEvent::CharInput(c) = event {
            match (c, self.demo()) {
                ('j', _) => {
                    self.select(self.selected + 1, true);
                    return true;
                }
                ('k', _) => {
                    self.select(self.selected.saturating_sub(1), true);
                    return true;
                }
                ('M', _) => {
                    self.modern = !self.modern;
                    return true;
                }
                (' ', Demo::ViewTransition) => {
                    self.switch_view();
                    return true;
                }
                ('m', Demo::Modal) => {
                    self.toggle_modal();
                    return true;
                }
                ('t', Demo::Toast) => {
                    self.spawn_toast();
                    return true;
                }
                _ => {}
            }
        }
        false
    }
}

impl Gallery {
    fn new() -> Self {
        Gallery {
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
            scroll_requests: Vec::new(),
            presets: Theme::all_presets(),
            toggles: [true, false, true, false],
            toggle_changed: [0.0; 4],
            ctx_menu: None,
            ctx_last: None,
            form_text: TextInputState::new("Type here..."),
            form_checks: [true, false, true],
            form_radio: 0,
            form_slider: 0.5,
            form_dropdown: DropdownState::new(
                "form-dropdown",
                ["Option A", "Option B", "Option C", "Option D"].map(String::from).to_vec(),
            ),
            slider_dragging: false,
            slider_track: None,
            pointer_x: 0.0,
            splash_preview_idx: 0,
            splash_preview_start: 0.0,
            selection: Vec::new(),
            marks: Vec::new(),
            goo_open: false,
            goo_p: 0.0,
            goo_v: 0.0,
            dd_cards: [0, 0, 1, 1, 2],
            dd_files: Vec::new(),
        }
    }

    /// 撮影・確認用: `TUI_GALLERY_ITEM=<一覧の名前>` で起動画面を飛ばしてその部品を開き、
    /// `TUI_GALLERY_SKIN=tui` で TUI の見た目から始める。
    fn apply_env(&mut self) {
        let item = std::env::var("TUI_GALLERY_ITEM").ok()
            .and_then(|name| ITEMS.iter().position(|(_, n, _)| n.eq_ignore_ascii_case(&name)));
        if let Some(idx) = item {
            self.selected = idx;
            self.splash_done = true;
            self.sel_at = -1.0; // 切り替えのフェードを終えた状態で始める
        }
        if let Ok(skin) = std::env::var("TUI_GALLERY_SKIN") {
            self.modern = !skin.eq_ignore_ascii_case("tui");
        }
        if std::env::var("TUI_GALLERY_GOO_OPEN").is_ok_and(|v| v == "1") {
            self.goo_open = true;
        }
    }
}

fn main() {
    let mut gallery = Gallery::new();
    gallery.apply_env();
    sabitori::run_declarative(gallery);
}

#[cfg(test)]
mod tests {
    //! 画面を撮るだけでは確かめられない、 押す・引きずる・選ぶの通し。
    //! `cargo test -p sabitori --example tui_gallery` で走る。

    use super::*;
    use sabitori::testing::Harness;

    fn open(item: &str) -> Harness<Gallery> {
        let mut g = Gallery::new();
        g.splash_done = true;
        g.selected = ITEMS.iter().position(|(_, n, _)| *n == item).expect("一覧に無い");
        let mut h = Harness::new(g, 1000.0, 660.0);
        h.frame();
        h
    }

    #[test]
    fn sidebar_click_selects_the_item() {
        let mut h = open("Typewriter");
        let idx = ITEMS.iter().position(|(d, _, _)| *d == Demo::DragDrop).unwrap();
        h.scroll_into_view("sidebar", &format!("item-{idx}"));
        h.click(&format!("item-{idx}"));
        h.frame();
        assert_eq!(h.app().demo(), Demo::DragDrop);
    }

    #[test]
    fn context_menu_opens_on_the_area_and_runs_the_item() {
        let mut h = open("Context Menu");
        h.right_click("ctx-area");
        h.frame();
        assert!(h.app().ctx_menu.is_some(), "枠の上の右クリックでメニューが開く");
        h.click("ctx-paste");
        h.frame();
        assert!(h.app().ctx_menu.is_none());
        assert_eq!(h.app().ctx_last, Some("Paste"));
    }

    #[test]
    fn dragging_a_card_moves_it_to_the_column() {
        let mut h = open("Drag & Drop");
        assert_eq!(h.app().dd_cards[0], 0);
        let from = h.rect_of("dd-card-0").expect("カード");
        let to = h.rect_of("dd-col-2").expect("列");
        h.press_at(from.origin.x + 10.0, from.origin.y + 10.0);
        h.move_to(from.origin.x + 40.0, from.origin.y + 30.0);
        h.move_to(to.origin.x + 20.0, to.origin.y + to.size.height - 20.0);
        h.frame();
        assert_eq!(h.drag_info(), Some(("0".to_string(), Some("dd-card-0".to_string()))), "カード 0 を運んでいる");
        h.release();
        h.frame();
        assert_eq!(h.app().dd_cards[0], 2, "落とした列に移る");
    }

    #[test]
    fn files_dropped_on_the_zone_are_listed() {
        let mut h = open("Drag & Drop");
        let zone = h.rect_of("dd-files").expect("ファイルの入れ先");
        let (x, y) = (zone.origin.x + 20.0, zone.origin.y + 20.0);
        let files = [PathBuf::from("/tmp/notes.md"), PathBuf::from("/tmp/photo.png")];
        h.hover_files(&files, x, y);
        h.frame();
        h.drop_files(&files, x, y);
        h.frame();
        assert_eq!(h.app().dd_files, vec!["photo.png".to_string(), "notes.md".to_string()]);
    }

    #[test]
    fn form_controls_respond_to_clicks() {
        let mut h = open("Form Controls");
        h.click("form-check-1");
        h.click("form-radio-2");
        h.frame();
        assert!(h.app().form_checks[1]);
        assert_eq!(h.app().form_radio, 2);

        h.click("form-dropdown");
        h.frame();
        assert!(h.app().form_dropdown.open);
        // メニューは trigger のすぐ下に貼り付く (以前はマウスの位置からの目分量)。
        let trigger = h.rect_of("form-dropdown").expect("trigger");
        let first = h.rect_of("form-dropdown::item:0").expect("メニューの行");
        assert!((first.origin.x - trigger.origin.x).abs() < 4.0, "左が揃う: {trigger:?} / {first:?}");
        assert!(first.origin.y >= trigger.origin.y + trigger.size.height, "trigger の下: {trigger:?} / {first:?}");
        assert!(first.origin.y < trigger.origin.y + trigger.size.height + 12.0, "trigger のすぐ下: {trigger:?} / {first:?}");
        h.click("form-dropdown::item:2");
        h.frame();
        assert_eq!(h.app().form_dropdown.selected, 2);
        assert!(!h.app().form_dropdown.open);

        // スライダーは前のフレームの溝の矩形から値を出す (決め打ちの座標ではない)。
        let track = h.rect_of("form-slider").expect("スライダー");
        h.move_to(track.origin.x + track.size.width - 2.0, track.origin.y + 10.0);
        h.press_at(track.origin.x + track.size.width - 2.0, track.origin.y + 10.0);
        h.frame();
        assert!(h.app().form_slider > 0.95, "右端を押せば最大近く: {}", h.app().form_slider);
        h.move_to(track.origin.x + 8.0, track.origin.y + 10.0);
        h.release();
        h.frame();
        assert!(h.app().form_slider < 0.05, "左端まで引けば 0 近く: {}", h.app().form_slider);
    }

    #[test]
    fn text_input_needs_no_wiring() {
        let mut h = open("Form Controls");
        h.click("form-input");
        h.text("hello");
        h.frame();
        assert_eq!(h.app().form_text.text(), "hello");
        assert!(h.unrouted_text_inputs().is_empty());
    }

    #[test]
    fn modal_opens_and_closes_with_distinct_ids() {
        let mut h = open("Modal");
        h.click("modal-open");
        h.frame();
        assert!(h.app().modal_open);
        h.click("modal-cancel");
        h.frame();
        assert!(!h.app().modal_open);
    }

    #[test]
    fn marking_a_selection_keeps_the_range() {
        let mut h = open("Text Selection");
        // 選択そのものはランタイムが作る。 文字の当たり (text_layouts) は GPU で描く
        // 経路でしか組まれないので、 Harness ではドラッグで選べない。 ここでは
        // アプリに届いた形から先を見る。
        let piece = ("para-en".to_string(), 0..4);
        h.app_mut().selection = vec![piece.clone()];
        h.frame();
        h.click("sel-mark");
        h.frame();
        assert_eq!(h.app().marks, vec![piece]);
        h.click("sel-clear");
        h.frame();
        assert!(h.app().marks.is_empty());
    }

    /// 窓の既定の大きさで、 どの部品も親からはみ出さない (実物の書体で測る)。
    /// 撮った画面では debug ビルドの縞として出る崩れを、 ここで止める。
    #[test]
    fn no_item_overflows_its_parent() {
        let mut found = Vec::new();
        for (i, (_, name, _)) in ITEMS.iter().enumerate() {
            for modern in [true, false] {
                let mut g = Gallery::new();
                g.splash_done = true;
                g.selected = i;
                g.modern = modern;
                let mut h = Harness::with_real_text(g, 1000.0, 660.0);
                h.frame();
                h.frame();
                for o in h.overflows() {
                    found.push(format!("{name} (modern={modern}): {} {:?}", o.path, o.rect));
                }
            }
        }
        assert!(found.is_empty(), "はみ出し:\n{}", found.join("\n"));
    }

    #[test]
    fn every_item_builds_in_both_skins() {
        for (i, (_, name, _)) in ITEMS.iter().enumerate() {
            for modern in [true, false] {
                let mut g = Gallery::new();
                g.splash_done = true;
                g.selected = i;
                g.modern = modern;
                let mut h = Harness::new(g, 1000.0, 660.0);
                h.frame();
                // 同じ id が 2 つあると、 片方のホバーやばねの見た目がもう片方に移る。
                let mut seen = std::collections::HashSet::new();
                for region in &h.build().hit_regions {
                    if let Some(id) = &region.id {
                        assert!(seen.insert(id.clone()), "{name} (modern={modern}): id `{id}` が 2 つある");
                    }
                }
            }
        }
    }
}
