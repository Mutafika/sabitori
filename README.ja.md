# Sabitori

**言語**: [English](README.md) · 日本語

> Rust 製の GPU UI フレームワーク — wgpu + Taffy + cosmic-text を組み合わせた、宣言的 API でデスクトップ / WASM の両方をターゲットにできる軽量フレームワーク。

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](Cargo.toml)

**ステータス**: pre-release (`0.4.0`)。コア機能は実装済み、`templates/wasm/` の手順で WASM ビルド可能。`0.4.0` は「コンパイルは通るのに黙って効かない API」を消すための破壊的変更ラウンドです — [CHANGELOG.md](CHANGELOG.md) を参照。

## 特徴

- **宣言的ビルダー API** — `div() / text() / button() / image()` でツリーを組み立てる
- **GPU レンダリング** — wgpu + SDF シェーダーで角丸 / ボーダー / 影 / グラデーションを 1 パスで描画
- **WASM ファースト** — WebGPU 優先、WebGL2 フォールバック、winit web extension で canvas 自動バインド
- **Spring 物理アニメーション** — snappy / gentle / bouncy プリセット + 11 種の easing + keyframe
- **入力統合** — マウス / タッチ / ペンを Pointer 抽象で統一。日本語 IME は変換中の文字列をその場に出し、キャレットも正しい位置に立つ
- **ヘッドレステスト** — `sabitori::testing::Harness` で、窓も GPU も無しにアプリ全体を動かせる
- **アクセシビリティの土台** — `.role()` / `.label()` / `.heading(n)` でツリーの意味を書ける
- **Markdown レンダラー** — CommonMark + GFM（テーブル / 取り消し線 / フットノート）+ TOC
- **TUI コンポーネント** — Block / StatusBar / Spinner / Typewriter
- **MIT 純度** — `cargo deny` が AGPL/GPL 依存を自動で弾く

## クイックスタート

```rust
use sabitori::*;

struct App { clicks: u32 }

impl DeclarativeApp for App {
    fn title(&self) -> &str { "Hello Sabitori" }
    fn size(&self) -> (f32, f32) { (800.0, 600.0) }

    fn view(&self, ctx: &ViewContext) -> Element {
        div()
            .w(Px(ctx.width)).h(Px(ctx.height))
            .bg(Color::from_hex("#1a1b26"))
            .flex_col()
            .items_center()
            .justify_center()
            .gap(16.0)
            .children([
                text(&format!("Clicks: {}", self.clicks))
                    .font_size(24.0)
                    .color(Color::from_hex("#c0caf5")),
                button("Click Me")
                    .id("btn")
                    .accent(Color::from_hex("#7aa2f7")),
            ])
    }

    fn on_click(&mut self, id: &str) {
        if id == "btn" { self.clicks += 1; }
    }
}

fn main() {
    sabitori::run_declarative(App { clicks: 0 });
}
```

**操作したいものには `.id()` を付けること。** クリック・ホバー・フォーカス・スクロールは全部 id を鍵にしています。

## よく間違える 4 つ

この 4 つが他の全部を合わせたより多いです。それぞれ正解は 1 つだけ。

### 1. スクロール

**コンテナに `.scroll(id)` を付けて、位置はランタイムに持たせる。**

```rust
div().scroll("file-list").flex_1().flex_col().children(rows)
```

配線はこれで全部です。ホイールの配送も慣性のばねもフレーム跨ぎの位置保持もランタイムがやります。**`on_scroll` を実装してはいけません** — ホイールは既に届いているので、自分でも受けると二重に動きます。

位置を読むのは `ctx.scroll_info("file-list")`、プログラムから動かすのは `scroll_intents()` の戻り値：

```rust
fn scroll_intents(&mut self) -> Vec<(String, f32)> {
    self.pending.take().map(|y| ("file-list".into(), y)).into_iter().collect()
}
```

長いリストは、見えている範囲をランタイムに聞いて、残りは spacer で高さだけ確保します（そうしないとスクロールバーの長さが実データと合いません）：

```rust
let (first, count) = ctx.visible_range("file-list", ROW_H);
```

`virtual_list(ctx, id, &items, row_h, render)` がこれをやってくれます。

もう 1 つのモデルが `.scroll_manual(x, y)` で、こちらは**アプリが**位置を持ち、ランタイムは一切触りません。どちらかを選ぶ — 型がどちらかを示します。

### 2. テキスト入力と IME

**`view()` に `text_input` を置く。配線はこれで全部です。**

```rust
struct App { name: TextInputState }

impl DeclarativeApp for App {
    fn view(&self, ctx: &ViewContext) -> Element {
        text_input(ctx, "name", &self.name, &TextInputStyle::default_dark())
    }
    fn on_click(&mut self, id: &str) {
        if id == "save" { self.saved = Some(self.name.text()); }
    }
}
```

他に書くことはありません。打鍵も IME の変換も貼り付けもキャレットの点滅もフォーカス状態も、OS の変換候補ウィンドウの位置も、全部ランタイムがやります。ウィジェットが組み立てのときに自分を `ViewContext` へ登録するからです。**`on_focused_input` も `tick` も `ime_cursor_area` も、忘れる余地がありません。**

日本語変換は文字列がその場に出て、キャレットは preedit の**中**に立ちます。いま何を変換しているのかが分かるのはこれのおかげです。

値の読み書きはアクセサ経由です — `text()` / `set_text()` / `clear()` / `is_focused()` / `is_composing()`。状態は複製の軽い共有ハンドルで、これがあるから `view(&self)` からランタイムへ渡せます。

自分でテキスト欄を作る場合（`Role::TextInput` を名乗る要素を自作する場合）は配線が自分の責任に戻ります。打った文字がどこにも行かないと、ランタイムが 1 度だけ警告します：

```rust
assert!(h.unrouted_text_inputs().is_empty());
```

### 3. フォーカスとキーボード

`.focusable` な要素はクリックと Tab でフォーカスが入ります。キーはまず `on_focused_input(id, event)` に行き、処理されなかったぶんが `on_input(event)` に落ちます。

**`on_input` が `true` を返すと、そのキーの既定動作（コピー・ペースト・Escape・Tab）が抑止されます。** 消費していないなら `false` を返すこと。返し間違えると既定動作が黙って死にます。

### 4. テスト

窓も GPU も無しでアプリを動かせます：

```rust
use sabitori::testing::Harness;

let mut h = Harness::new(App::default(), 800.0, 600.0);
h.frame();                  // 組み立て + レイアウト
h.click("save");            // id で押す
h.text("hello");            // フォーカス中の要素へ打鍵
h.scroll("file-list", 400.0);
h.settle();                 // ばねを終わらせる (scroll_intents に必要)
assert_eq!(h.app().saved.as_deref(), Some("hello"));
```

**`frame()` は時間を進めません。** 慣性スクロール・`scroll_intents`・style アニメーションなど、ばねで動くものは `tick(dt)` か `settle()` が要ります。

## ウィジェット

2 種類あり、その区別がそのまま API になっています。

- **状態** はアプリに持たせる struct： `TextInputState` / `TableState` / `DropdownState` / `SplitPaneState`
- **見た目** は `view()` から呼ぶ自由関数： `text_input(ctx, id, &state, &style) -> Element`

Element を返す入口は**すべて `snake_case` の自由関数で、第 1 引数が `&ViewContext`、第 2 引数が `id`** です。`sabitori_core::forms`（`checkbox` / `radio` / `slider` / `segment_control` / `progress_bar` / `numeric_input` / `collapsing_header` / `dropdown_trigger`）も同じ形なので、ウィジェットごとに調べ直す必要はありません。

```rust
div().flex_col().children([
    text_input(ctx, "name", &self.name, &TextInputStyle::default_dark()),
    table(ctx, "files", &self.files, &TableStyle::default_dark()),
    tree_view(ctx, "tree", &self.tree, &TreeViewStyle::default_dark()),
])
```

## アクセシビリティ

窓の中身は GPU で描いたピクセルなので、ツリーが「これは何か」を言わない限りスクリーンリーダーには何も見えません。`button()` は自分で `Role::Button` を名乗ります。それ以外は書いてください：

```rust
div().id("close").role(Role::Button).label("閉じる")   // アイコンだけのボタン
text("設定").role(Role::Heading).heading(2)
```

意味の層は入っていて `hit_regions` まで通っています。OS 側のアダプタ（accesskit）はまだ繋がっていません。

## サンプル

```bash
cargo run --example declarative   # 宣言的 API + hover + click
cargo run --example anim          # Spring アニメーション + マウス追従
cargo run --example effects       # GPU エフェクト 4 種（spotlight / magnetic / gravity / fluid）
cargo run --example gpu_flex      # 1 万パーティクルの物理シミュレーション
cargo run --example showcase      # 30 デモのグリッド + モーダルズーム
cargo run --example layout        # Taffy Flexbox レイアウト
cargo run --example text          # cosmic-text 統合
cargo run --example tui_demo      # ANSI ベースの TUI ダッシュボード
cargo run --example tui_gallery   # アニメーションギャラリー
cargo run --example filer         # ファイラ — ランタイム管理スクロール + 行の仮想化
cargo run --example hello         # 低レベル API（`SabitoriApp` トレイト）
```

## アーキテクチャ

13 クレート構成のワークスペース：

```
sabitori (umbrella)
├── sabitori-core      Element ビルダー / コア型 / フォームコントロール / TUI
├── sabitori-gpu       wgpu SDF レンダラー / OrbitCamera / 画像テクスチャ
├── sabitori-style     Theme / ANSI パレット / StyleProps（レイアウト型は core を再輸出）
├── sabitori-layout    Taffy ラッパー（Flexbox + Grid）
├── sabitori-scene     NodeTree / ヒットテスト / 状態管理
├── sabitori-input     Pointer 抽象 / IME / フォーカス / 配信テーブル
├── sabitori-anim      Spring / Easing / Keyframe / 特化状態
├── sabitori-text      cosmic-text 統合 / グリフアトラス
├── sabitori-widgets   状態を持つウィジェット（状態 struct + Element 関数）
├── sabitori-window    winit ランタイム / EmbeddedRunner
├── sabitori-markdown  Markdown → Element 変換
└── sabitori-net       HTTP fetch（reqwest / wasm fetch）
```

## WASM ターゲット

`templates/wasm/` に `Trunk.toml` + `index.html` のテンプレートと、よくある落とし穴をまとめた README があります。

```bash
# 初回だけ
rustup target add wasm32-unknown-unknown
cargo install trunk

# テンプレートを自分のアプリへコピー
cp templates/wasm/{Trunk.toml,index.html} /path/to/your-app/

# 開発サーバー / 本番ビルド
trunk serve            # localhost:8080 でホットリロード
trunk build --release  # dist/ に最適化済みアセット
```

WASM 固有の要件（`wgpu` の `webgl` feature、フォントの同梱、WebGL2 の varying 上限など）は [`templates/wasm/README.md`](templates/wasm/README.md) を参照。

## ロードマップ

実装済み機能と未着手領域は [ROADMAP.ja.md](ROADMAP.ja.md) にまとめてあります。

主な未着手項目：
- accesskit アダプタ（意味の層を VoiceOver / NVDA / Narrator まで届ける）
- macOS ネイティブ統合（NSStatusItem / 透過 NSWindow / 通知）
- 物理単位（`Mm` / `Pt`）と正確な PPI 検出
- crates.io 公開準備（メタデータ整理 + `release-plz` 自動化）

## ライセンス

[MIT](LICENSE)。サードパーティライセンスと参考にした標準的手法については [NOTICE.md](NOTICE.md) を参照。

## 動作要件

- Rust 1.85+（edition 2024）
- wgpu 24 対応の GPU バックエンド（Vulkan / Metal / DX12 / WebGPU / WebGL2）
