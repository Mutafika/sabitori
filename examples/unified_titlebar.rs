//! 中身と一体のタイトルバー (`Titlebar::Unified`) — Warp・Safari の形。
//!
//! タブの帯が窓の上端に来て、信号ボタンはその縦の真ん中に重なる。帯の空いた所を
//! つかむと窓が動き、ダブルクリックで拡大する (`.window_drag()`)。タブは普通に押せる。
//!
//! ```sh
//! cargo run -p sabitori --example unified_titlebar
//! ```

use sabitori::element::*;
use sabitori::*;

const BAR_H: f32 = 38.0;

struct App {
    tabs: Vec<&'static str>,
    active: usize,
}

impl DeclarativeApp for App {
    fn title(&self) -> &str { "Sabitori — Unified titlebar" }
    fn size(&self) -> (f32, f32) { (760.0, 440.0) }
    fn titlebar(&self) -> Titlebar { Titlebar::Unified { height: BAR_H } }

    fn view(&self, ctx: &ViewContext) -> Element {
        let bar_bg = Color::from_hex("#16161e");
        let body_bg = Color::from_hex("#1a1b26");
        let tab_on = Color::from_hex("#24283b");
        let fg = Color::from_hex("#c0caf5");
        let dim = Color::from_hex("#565f89");

        // 信号ボタンの右から 12px 空けて並べる。フルスクリーン中・macOS 以外は左端から。
        let lead = ctx.window_controls.map_or(12.0, |r| r.origin.x + r.size.width + 12.0);

        let tabs = self.tabs.iter().enumerate().map(|(i, name)| {
            let on = i == self.active;
            div()
                .h(Px(BAR_H - 8.0))
                .px_pad(Px(14.0))
                .flex_row()
                .items_center()
                .rounded_px(6.0)
                .bg(if on { tab_on } else { Color::TRANSPARENT })
                .child(text(*name).font_size(13.0).color(if on { fg } else { dim }))
                .click(ctx, format!("tab-{i}"), move |app: &mut App| app.active = i)
        });

        // 帯: 空いた所は窓のつかみどころ。中のタブ (id 付き) は押せば押せる。
        let bar = div()
            .window_drag()
            .no_select()
            .w_full()
            .h(Px(BAR_H))
            .bg(bar_bg)
            .flex_row()
            .items_center()
            .gap(4.0)
            .pl(Px(lead))
            .children(tabs);

        let body = div()
            .flex_1()
            .w_full()
            .bg(body_bg)
            .flex_col()
            .p(Px(24.0))
            .gap(8.0)
            .children([
                text(self.tabs[self.active]).font_size(18.0).color(fg).bold(),
                text("帯の空いた所をつかむと窓が動き、ダブルクリックで拡大します。")
                    .font_size(13.0)
                    .color(dim),
                text("タブは押せます。窓は動きません。").font_size(13.0).color(dim),
            ]);

        div().w(Px(ctx.width)).h(Px(ctx.height)).flex_col().children([bar, body])
    }
}

fn main() {
    run_declarative(App { tabs: vec!["main", "build", "logs"], active: 0 });
}
