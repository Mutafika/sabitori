//! 自前で描くタイトルバー (`Titlebar::Custom`) と、窓のボタンの並び
//! (`titlebar_buttons`)。
//!
//! 信号ボタンを消し、窓の操作ボタンはアプリの帯に並べる。並びは 3 つから選べる:
//!
//! * **windows** — 右端に − □ ✕ (プリセットそのまま)
//! * **lights** — 左端に赤・黄・緑の丸 (プリセットそのまま)
//! * **custom** — − □ ✕ に「ピン留め」(アプリの操作) を足し、□ をフルスクリーンに
//!   付け替え、✕ の絵を差し替えたもの
//!
//! ボタンは押すだけで窓が動く。アプリは窓のハンドルもクリックの振り分けも持たない。
//!
//! ```sh
//! cargo run -p sabitori --example custom_titlebar
//! PACK=lights cargo run -p sabitori --example custom_titlebar   # 最初の並び
//! ```

use sabitori::element::*;
use sabitori::*;

const BAR_H: f32 = 38.0;

#[derive(Clone, Copy, PartialEq)]
enum Pack {
    Windows,
    Lights,
    Custom,
}

impl Pack {
    const ALL: [(Pack, &'static str); 3] =
        [(Pack::Windows, "windows"), (Pack::Lights, "lights"), (Pack::Custom, "custom")];

    fn buttons(self, theme: &AppTheme) -> TitlebarButtons {
        match self {
            Pack::Windows => TitlebarButtons::windows(theme, BAR_H),
            Pack::Lights => TitlebarButtons::traffic_lights(),
            Pack::Custom => TitlebarButtons::windows(theme, BAR_H)
                // アプリの操作: 押すと on_click("win:pin")。窓には何もしない
                .insert(0, TitlebarButton::new("pin", TitlebarIcon::svg(PIN, 24.0)).label("ピン留め"))
                // □ をフルスクリーンに付け替える (絵も四隅に)
                .edit("maximize", |b| {
                    b.action(WindowAction::ToggleFullscreen).icon(TitlebarIcon::Fullscreen)
                })
                // ✕ の絵だけ差し替える。押すと閉じるのはそのまま
                .edit("close", |b| b.icon(TitlebarIcon::svg("M6 6 L18 18 M18 6 L6 18", 24.0))),
        }
    }
}

/// 画鋲 (24 四方)。
const PIN: &str = "M12 17 V22 M9 3 H15 M10 3 V10 L6 14 H18 L14 10 V3";

struct App {
    pack: Pack,
    pinned: bool,
}

impl DeclarativeApp for App {
    fn title(&self) -> &str { "Sabitori — Custom titlebar" }
    fn size(&self) -> (f32, f32) { (760.0, 440.0) }
    fn min_size(&self) -> (f32, f32) { (480.0, 260.0) }
    fn titlebar(&self) -> Titlebar { Titlebar::Custom { height: BAR_H } }
    fn resize_handles(&self) -> Option<ResizeHandles> { Some(ResizeHandles::default()) }

    fn on_click(&mut self, id: &str) {
        if id == titlebar_button_id("win", "pin") {
            self.pinned = !self.pinned;
        }
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        let theme = &ctx.theme;
        let mut buttons = self.pack.buttons(theme);
        // ピン留め中は絵を主色に (見た目の状態はアプリが持つ)
        if self.pinned {
            buttons.style.icon = theme.primary;
        }

        let picker = div().flex_row().gap(2.0).children(Pack::ALL.map(|(pack, name)| {
            let on = pack == self.pack;
            let id = format!("pack-{name}");
            let hot = ctx.hovered.as_deref() == Some(id.as_str());
            div()
                .h(Px(24.0))
                .px_pad(Px(10.0))
                .rounded_px(6.0)
                .flex_row()
                .items_center()
                .bg(if on { theme.select_bg } else if hot { theme.hover_bg } else { Color::TRANSPARENT })
                .child(text(name).font_size(12.0).color(if on {
                    theme.text_primary
                } else {
                    theme.text_secondary
                }))
                .click(ctx, id, move |app: &mut App| app.pack = pack)
        }));

        let title = text("custom_titlebar").font_size(13.0).color(theme.text_secondary);
        let mut row = vec![];
        if self.pack == Pack::Lights {
            row.push(titlebar_buttons(ctx, "win", &buttons).ml(Px(12.0)));
            row.push(div().w(Px(16.0)));
            row.push(title);
            row.push(div().flex_1());
            row.push(picker.mr(Px(12.0)));
        } else {
            row.push(title.ml(Px(14.0)));
            row.push(div().flex_1());
            row.push(picker.mr(Px(8.0)));
            row.push(titlebar_buttons(ctx, "win", &buttons));
        }
        let bar = div()
            .window_drag()
            .no_select()
            .w_full()
            .h(Px(BAR_H))
            .shrink(0.0)
            .bg(theme.surface)
            .flex_row()
            .items_center()
            .children(row);

        let state = ctx.window_state;
        let line = |s: String| text(s).font_size(13.0).color(theme.text_secondary);
        let body = div()
            .flex_1()
            .w_full()
            .bg(theme.bg)
            .flex_col()
            .p(Px(24.0))
            .gap(8.0)
            .children([
                text("窓のボタンは並べるだけで窓が動きます").font_size(18.0).color(theme.text_primary).bold(),
                line("帯の空いた所をつかむと窓が動き、ダブルクリックで拡大します。".into()),
                line(format!(
                    "拡大: {} / フルスクリーン: {} / ピン留め: {}",
                    state.maximized, state.fullscreen, self.pinned
                )),
            ]);

        div().w(Px(ctx.width)).h(Px(ctx.height)).flex_col().children([bar, body])
    }
}

fn main() {
    let pack = match std::env::var("PACK").as_deref() {
        Ok("lights") => Pack::Lights,
        Ok("custom") => Pack::Custom,
        _ => Pack::Windows,
    };
    run_declarative(App { pack, pinned: false });
}
