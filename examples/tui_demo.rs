/// TUI aesthetic demo — dense, sharp, terminal-native feel rendered with GPU.

use sabitori::*;
use sabitori_style::Theme;

struct Dashboard {
    theme: Theme,
    cmd_input: String,
}

impl DeclarativeApp for Dashboard {
    fn title(&self) -> &str { "Sabitori TUI" }
    fn size(&self) -> (f32, f32) { (900.0, 580.0) }

    /// この example が描く文字は Latin と罫線素片だけなので Hack で足りる。
    /// 日本語を足すなら `tui_gallery.rs` と同じく HackGen へ差し替えること —
    /// Hack のままだと wasm で日本語が豆腐になる (ブラウザにシステムフォントは
    /// 無く、 native では OS が拾ってしまうので気づけない)。
    /// `crates/sabitori-text/tests/example_fonts.rs` がそれを見張っている。
    fn fonts(&self) -> Vec<Vec<u8>> {
        vec![
            include_bytes!("../assets/fonts/Hack-Regular.ttf").to_vec(),
            include_bytes!("../assets/fonts/Hack-Bold.ttf").to_vec(),
        ]
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        let t = &self.theme;
        let a = &t.ansi;
        let bg = Color::from_hex("#08080c");

        // ── Processes ──
        let procs: &[(&str, &str, &str, &str, Color)] = &[
            ("node server.js",  "12.3%", "184M", "████████▓░░░░░░", a.green),
            ("postgres",        " 8.1%", "512M", "█████▒░░░░░░░░░", a.blue),
            ("redis-server",    " 0.4%", " 24M", "█░░░░░░░░░░░░░░", a.cyan),
            ("nginx",           " 1.2%", " 48M", "██░░░░░░░░░░░░░", a.yellow),
            ("sabitori",        " 0.1%", " 16M", "░░░░░░░░░░░░░░░", a.magenta),
        ];

        let mut proc_rows: Vec<Element> = Vec::new();

        // Column header
        proc_rows.push(
            div().w_full().h(Px(18.0)).shrink(0.0)
                .flex_row().items_center()
                .children([
                    text("PROCESS").mono().font_size(10.0).color(t.text_disabled).w(Px(140.0)).shrink(0.0),
                    text("CPU").mono().font_size(10.0).color(t.text_disabled).w(Px(52.0)).shrink(0.0),
                    text("MEM").mono().font_size(10.0).color(t.text_disabled).w(Px(48.0)).shrink(0.0),
                    text("LOAD").mono().font_size(10.0).color(t.text_disabled),
                ]),
        );
        proc_rows.push(hsep(t.border));

        for (i, &(name, cpu, mem, bar, color)) in procs.iter().enumerate() {
            let id = format!("p-{i}");
            let is_hovered = ctx.hovered.as_deref() == Some(id.as_str());
            let row_bg = if is_hovered { t.surface_hover } else { Color::TRANSPARENT };

            proc_rows.push(
                div().id(&id)
                    .w_full().h(Px(20.0)).shrink(0.0)
                    .flex_row().items_center()
                    .bg(row_bg)
                    .children([
                        text(name).mono().font_size(11.0).color(color).w(Px(140.0)).shrink(0.0),
                        text(cpu).mono().font_size(11.0).color(t.text_primary).w(Px(52.0)).shrink(0.0),
                        text(mem).mono().font_size(11.0).color(t.text_secondary).w(Px(48.0)).shrink(0.0),
                        text(bar).mono().font_size(11.0).color(color.with_alpha(0.6)),
                    ]),
            );
        }

        // ── Logs ──
        let logs: &[(&str, &str, &str, Color)] = &[
            ("12:01:14", "INFO ", "Server listening on :3000",   a.green),
            ("12:02:03", "INFO ", "GET /api/health 200 12ms",    a.green),
            ("12:03:47", "WARN ", "Slow query: 850ms",           a.yellow),
            ("12:04:12", "ERROR", "Connection timeout",          a.bright_red),
            ("12:05:00", "INFO ", "Cache cleared (1024 keys)",   a.green),
            ("12:06:31", "INFO ", "Deploy v2.4.1 complete",      a.cyan),
        ];

        let mut log_rows: Vec<Element> = Vec::new();
        for &(time, level, msg, color) in logs {
            log_rows.push(
                div().w_full().h(Px(18.0)).shrink(0.0)
                    .flex_row().items_center()
                    .children([
                        text(time).mono().font_size(10.0).color(t.text_disabled).w(Px(72.0)).shrink(0.0),
                        text(level).mono().font_size(10.0).color(color).w(Px(44.0)).shrink(0.0),
                        text(msg).mono().font_size(10.0).color(t.text_primary),
                    ]),
            );
        }

        // ── Stats (inline, dense) ──
        let stat = |label: &str, value: &str, color: Color| -> Element {
            div().shrink(0.0).flex_row().gap(4.0).items_center().children([
                text(label).mono().font_size(10.0).color(t.text_disabled).shrink(0.0),
                text(value).mono().font_size(10.0).color(color).bold().shrink(0.0),
            ])
        };

        // ── Layout ──
        div()
            .w(Px(ctx.width)).h(Px(ctx.height))
            .bg(bg)
            .flex_col()
            .children([
                // ─ Header bar ─
                div().w_full().h(Px(24.0)).shrink(0.0)
                    .bg(t.surface_elevated)
                    .flex_row().items_center()
                    .overflow_hidden()
                    .px_pad(Px(8.0))
                    .children([
                        text("sabitori").mono().bold().font_size(11.0).color(t.primary).shrink(0.0),
                        text(" / ").mono().font_size(11.0).color(t.text_disabled).shrink(0.0),
                        text("dashboard").mono().font_size(11.0).color(t.text_secondary).shrink(0.0),
                        div().flex_1(),
                        // Stats inline in header
                        div().shrink(0.0).flex_row().gap(12.0).children([
                            stat("up", "3d14h", a.green),
                            stat("req", "1.28M", a.bright_cyan),
                            stat("err", "7", a.bright_red),
                            stat("p99", "142ms", a.bright_yellow),
                        ]),
                        div().w(Px(12.0)).shrink(0.0),
                        // Status
                        text("● ok").mono().font_size(10.0).color(a.green).shrink(0.0),
                    ]),
                hsep(t.border),

                // ─ Processes section ─
                div().w_full().shrink(0.0)
                    .flex_col()
                    .p_px(6.0)
                    .children([
                        text("─ processes").mono().font_size(10.0).color(t.text_disabled).pb(Px(2.0)),
                    ]),
                div().w_full().shrink(0.0)
                    .flex_col()
                    .px_pad(Px(6.0))
                    .children(proc_rows),

                div().h(Px(4.0)).shrink(0.0),
                hsep(t.border),

                // ─ Logs section ─
                div().w_full().shrink(0.0)
                    .flex_col()
                    .p_px(6.0)
                    .children([
                        text("─ activity log").mono().font_size(10.0).color(t.text_disabled).pb(Px(2.0)),
                    ]),
                div().w_full().flex_1()
                    .flex_col()
                    .px_pad(Px(6.0))
                    .children(log_rows),

                hsep(t.border),

                // ─ Command prompt ─
                div().w_full().h(Px(24.0)).shrink(0.0)
                    .bg(t.surface)
                    .flex_row().items_center()
                    .px_pad(Px(8.0))
                    .children([
                        text(">").mono().font_size(12.0).color(t.primary).bold().shrink(0.0),
                        text(" ").mono().font_size(12.0).color(t.text_primary).shrink(0.0),
                        text(if self.cmd_input.is_empty() { "type a command..." } else { &self.cmd_input })
                            .mono().font_size(12.0)
                            .color(if self.cmd_input.is_empty() { t.text_disabled } else { t.text_primary }),
                    ]),

                // ─ Status line ─
                div().w_full().h(Px(18.0)).shrink(0.0)
                    .bg(t.surface_elevated)
                    .flex_row().items_center()
                    .overflow_hidden()
                    .px_pad(Px(8.0))
                    .children([
                        text("5 processes").mono().font_size(9.0).color(t.text_disabled).shrink(0.0),
                        text("  |  ").mono().font_size(9.0).color(t.border).shrink(0.0),
                        text("6 log entries").mono().font_size(9.0).color(t.text_disabled).shrink(0.0),
                        div().flex_1(),
                        text("[r]efresh  [q]uit").mono().font_size(9.0).color(t.text_disabled).shrink(0.0),
                    ]),
            ])
    }

    fn on_click(&mut self, _id: &str) {}

    fn on_input(&mut self, event: &InputEvent) -> bool {
        match event {
            InputEvent::CharInput(c) => {
                self.cmd_input.push(*c);
                true
            }
            InputEvent::KeyInput { key: Key::Backspace, pressed: true, .. } => {
                self.cmd_input.pop();
                true
            }
            InputEvent::KeyInput { key: Key::Enter, pressed: true, .. } => {
                self.cmd_input.clear();
                true
            }
            _ => false,
        }
    }
}

fn main() {
    sabitori::run_declarative(Dashboard {
        theme: Theme::warp_dark(),
        cmd_input: String::new(),
    });
}
