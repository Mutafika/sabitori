//! ホットリロードの雛形。
//!
//! **このファイルを直接 `dx serve` しても、ホットリロードは効かない。**
//! `dx` が監視するのは各クレートの `src` / `tests` / `Cargo.toml` だけで、
//! このリポジトリは example をワークスペース直下 (どのクレートにも属さない場所)
//! に置いているため、編集しても `dx` の視界に入らない。
//!
//! 動かすには、自分のクレートの `src/main.rs` にこの中身を写して:
//!
//! ```sh
//! cargo install dioxus-cli   # 初回だけ
//! dx serve --hot-patch       # そのクレート直下で
//! ```
//!
//! 起動したらボタンを何回か押してカウンタを上げ、そのまま下の `view` の中身
//! (色・文言・サイズ・レイアウト) を書き換えて保存する。**カウンタの値を保ったまま**
//! 画面だけが変われば成功。パッチが当たるたびに `hot-reload: パッチ適用` が出る。
//!
//! 逆に `HotReloadDemo` のフィールドを足し引きすると状態のメモリレイアウトが
//! 変わるので、そこは dx がフル再起動に落とす (＝カウンタは 0 に戻る)。
//! これは制約であって不具合ではない。
//!
//! 素の確認だけなら `cargo run --example hot_reload` でも動く
//! (devserver が居ないので、ホットリロードは黙って無効になる)。

use sabitori::element::*;
use sabitori::*;

/// ここを触るとフル再起動。`view` の中だけならホットリロードで済む。
struct HotReloadDemo {
    clicks: u32,
}

// ── ここから下を書き換えて保存すると、走ったまま画面が変わる ──────────────

const BG: &str = "#1a1b26";
const FG: &str = "#c0caf5";
const ACCENT: &str = "#7aa2f7";
const HEADLINE: &str = "Edit me while I'm running";

impl DeclarativeApp for HotReloadDemo {
    fn title(&self) -> &str {
        "Sabitori — hot reload"
    }

    fn size(&self) -> (f32, f32) {
        (720.0, 480.0)
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        div()
            .w(Px(ctx.width))
            .h(Px(ctx.height))
            .bg(Color::from_hex(BG))
            .flex_col()
            .items_center()
            .justify_center()
            .gap(20.0)
            .children([
                text(HEADLINE)
                    .font_size(28.0)
                    .color(Color::from_hex(FG)),
                text(&format!("clicks: {}", self.clicks))
                    .font_size(48.0)
                    .color(Color::from_hex(ACCENT)),
                button("Click me").id("bump").accent(Color::from_hex(ACCENT)),
                text("この値を残したまま、上の const や view を書き換えて保存")
                    .font_size(14.0)
                    .color(Color::from_hex(FG).with_alpha(0.5)),
            ])
    }

    fn on_click(&mut self, id: &str) {
        if id == "bump" {
            self.clicks += 1;
        }
    }
}

// ── ここまで ────────────────────────────────────────────────────────

fn main() {
    sabitori::run_declarative(HotReloadDemo { clicks: 0 });
}
