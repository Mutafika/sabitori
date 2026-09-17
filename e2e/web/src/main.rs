//! web の実機確認に使う最小のアプリ。README を参照。
//!
//! **回帰が起きたら絵で分かるもの**だけを置く。ここに機能を足すときは、
//! 「消えたらスクショで気付けるか」を基準にすること。

use sabitori::*;
use sabitori_core::element::polyline;
use sabitori_widgets::{text_input, TextInputState, TextInputStyle};

struct App {
    name: TextInputState,
    /// #74: URL と戻るボタン。`#/detail/<n>` を出し入れする。
    detail: Option<u32>,
}

impl Default for App {
    fn default() -> Self {
        Self { name: TextInputState::new("お名前"), detail: None }
    }
}

impl DeclarativeApp for App {
    fn url_fragment(&self) -> Option<String> {
        Some(self.fragment())
    }

    fn on_url_changed(&mut self, fragment: &str) {
        self.detail = fragment
            .strip_prefix("#/detail/")
            .and_then(|n| n.parse().ok());
    }

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
                // #74: 画面を変えると URL が変わり、戻るボタンで戻れるか
                div().flex_row().gap(8.0).p(Px(8.0)).children([
                    div()
                        .id("save-csv")
                        .px_pad(Px(10.0))
                        .py(Px(6.0))
                        .bg(Color::from_hex("#3ddc84"))
                        .click(ctx, "save-csv", |_app: &mut App| {
                            // #77: ダウンロードが始まるか
                            sabitori::files::save("vehicles.csv", "車両,状態\nR-0042,貸出可能\n".as_bytes());
                        })
                        .child(text("CSV を保存").color(Color::BLACK)),
                    div()
                        .id("open-detail")
                        .px_pad(Px(10.0))
                        .py(Px(6.0))
                        .bg(Color::from_hex("#2f6fed"))
                        .click(ctx, "open-detail", |app: &mut App| {
                            app.detail = Some(app.detail.unwrap_or(0) + 1)
                        })
                        .child(text("詳細をひらく").color(Color::WHITE)),
                    text(match self.detail {
                        Some(n) => format!("詳細 #{n}"),
                        None => "一覧".to_string(),
                    })
                    .color(Color::WHITE),
                ]),
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

impl App {
    fn fragment(&self) -> String {
        match self.detail {
            Some(n) => format!("#/detail/{n}"),
            None => "#/list".to_string(),
        }
    }
}

fn main() {
    run_declarative(App::default());
}
