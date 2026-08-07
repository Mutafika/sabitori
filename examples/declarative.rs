/// Declarative API demo — build UI with just div()/text()/button() builders.
/// Now with hover detection and ID-based click events.

use sabitori::*;
use sabitori::element::*;

struct MyApp {
    clicks: u32,
}

impl DeclarativeApp for MyApp {
    fn title(&self) -> &str { "Sabitori — Declarative API" }
    fn size(&self) -> (f32, f32) { (900.0, 600.0) }

    fn view(&self, ctx: &ViewContext) -> Element {
        let bg = Color::from_hex("#1a1b26");
        let surface = Color::from_hex("#24283b");
        let surface_hover = Color::from_hex("#2f3549");
        let border_c = Color::from_hex("#414868");
        let primary = Color::from_hex("#7aa2f7");
        let text_c = Color::from_hex("#c0caf5");
        let text2 = Color::from_hex("#9aa5ce");
        let success = Color::from_hex("#9ece6a");
        let warning = Color::from_hex("#e0af68");

        let card = |id: &str, title_text: &str, title_color: Color, sub1: &str, sub2: &str| {
            let is_hovered = ctx.hovered.as_deref() == Some(id);
            let card_bg = if is_hovered { surface_hover } else { surface };

            div()
                .id(id)
                .w(Px(240.0)).h(Px(160.0))
                .bg(card_bg)
                .rounded_px(12.0)
                .border(1.0, if is_hovered { primary.with_alpha(0.5) } else { border_c })
                .shadow_md(Color::from_hex("#00000040"))
                .flex_col()
                .p(Px(20.0))
                .gap(8.0)
                .children([
                    text(title_text)
                        .font_size(16.0)
                        .color(title_color)
                        .bold(),
                    text(sub1)
                        .font_size(13.0)
                        .color(text2),
                    text(sub2)
                        .font_size(12.0)
                        .color(text2),
                ])
        };

        div()
            .w(Px(ctx.width)).h(Px(ctx.height))
            .bg(bg)
            .flex_col()
            .items_center()
            .justify_center()
            .gap(24.0)
            .children([
                // Title
                text("さびとり Declarative API")
                    .font_size(32.0)
                    .color(text_c),

                text("div() / text() / button() だけでUIを構築")
                    .font_size(14.0)
                    .color(text2),

                // Card row
                div()
                    .flex_row()
                    .gap(16.0)
                    .children([
                        card("card-layout", "Layout Engine", primary, "Taffy Flexbox/Grid", "自動レイアウト計算"),
                        card("card-gpu", "GPU Rendering", success, "wgpu + SDF Shaders", "120fps描画"),
                        card("card-anim", "Animation", warning, "Spring / Easing / Keyframe", "物理ベースアニメーション"),
                    ]),

                // Click counter
                {
                    let btn_hovered = ctx.hovered.as_deref() == Some("click-btn");
                    let btn_bg = if btn_hovered { Color::from_hex("#8ab4f8") } else { primary };

                    div()
                        .w(Px(300.0)).h(Px(60.0))
                        .bg(surface)
                        .rounded_px(10.0)
                        .border(1.0, primary.with_alpha(0.3))
                        .flex_row()
                        .items_center()
                        .justify_center()
                        .gap(12.0)
                        .children([
                            text(&format!("Clicks: {}", self.clicks))
                                .font_size(18.0)
                                .color(text_c),
                            button("Click Me")
                                .id("click-btn")
                                .accent(btn_bg),
                        ])
                },

                // Footer
                text("Element → build_tree() → RenderList → GPU  |  Hover + Click via .id()")
                    .font_size(11.0)
                    .color(Color::from_hex("#555568")),
            ])
    }

    fn on_click(&mut self, id: &str) {
        match id {
            "click-btn" => self.clicks += 1,
            id => tracing::info!("Clicked: {id}"),
        }
    }
}

fn main() {
    sabitori::run_declarative(MyApp { clicks: 0 });
}
