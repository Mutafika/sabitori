//! web の実機確認に使う最小のアプリ。README を参照。
//!
//! **回帰が起きたら絵で分かるもの**だけを置く。ここに機能を足すときは、
//! 「消えたらスクショで気付けるか」を基準にすること。

use sabitori::*;
use sabitori_core::element::polyline;
use sabitori_widgets::{text_input, TextInputState, TextInputStyle};

struct App {
    name: TextInputState,
}

impl Default for App {
    fn default() -> Self {
        Self { name: TextInputState::new("お名前") }
    }
}

impl DeclarativeApp for App {
    fn view(&self, ctx: &ViewContext) -> Element {
        div()
            .w_full()
            .h_full()
            .flex_col()
            .bg(Color::from_hex("#101018"))
            .children([
                // #66: wasm で polyline が描かれるか
                div().id("chart").w(Px(300.0)).h(Px(120.0)).child(
                    polyline()
                        .points(vec![(0.0, 100.0), (80.0, 20.0), (160.0, 90.0), (280.0, 10.0)])
                        .stroke_width(3.0)
                        .stroke_color(Color::from_hex("#7dcfff")),
                ),
                // #73: 日本語 IME が届くか (隠し textarea の橋渡し)
                div().w(Px(320.0)).p(Px(8.0)).child(text_input(
                    ctx,
                    "name",
                    &self.name,
                    &TextInputStyle::default_dark(),
                )),
                // #71: ピルが描かれるか
                div()
                    .id("pill")
                    .px_pad(Px(10.0))
                    .py(Px(3.0))
                    .bg(Color::from_hex("#3ddc84"))
                    .rounded(Px(999.0))
                    .child(text("貸出可能").color(Color::BLACK)),
            ])
    }
}

fn main() {
    run_declarative(App::default());
}
