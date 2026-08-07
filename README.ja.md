# Sabitori

**言語**: [English](README.md) · 日本語

> Rust 製の GPU UI フレームワーク — wgpu + Taffy + cosmic-text を組み合わせた、宣言的 API でデスクトップ / WASM の両方をターゲットにできる軽量フレームワーク。

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](Cargo.toml)

**ステータス**: pre-release (`0.1.0`)。コア機能は実装済み、`templates/wasm/` の手順で WASM ビルド可能。API はまだ流動的。

## 特徴

- **宣言的ビルダー API** — `div() / text() / button() / image()` でツリーを組み立てる
- **GPU レンダリング** — wgpu + SDF シェーダーで角丸 / ボーダー / 影 / グラデーションを 1 パスで描画
- **WASM ファースト** — WebGPU 優先、WebGL2 フォールバック、winit web extension で canvas 自動バインド
- **Spring 物理アニメーション** — snappy / gentle / bouncy プリセット + 11 種の easing + keyframe
- **入力統合** — マウス / タッチ / ペンを Pointer 抽象で統一、日本語 IME + preedit 対応
- **20 種のウィジェット** — Modal / Table（仮想スクロール）/ TreeView / SplitPane / ContextMenu など
- **Markdown レンダラー** — CommonMark + GFM（テーブル / 取り消し線 / フットノート）+ TOC
- **TUI コンポーネント** — Block / StatusBar / Spinner / Typewriter で terminal 風 UI
- **MIT 純度保証** — `cargo deny` で AGPL/GPL 系を自動排除

## クイックスタート

```rust
use sabitori::*;
use sabitori::element::*;

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

## サンプル

```bash
cargo run --example declarative   # 宣言的 API + hover + click
cargo run --example anim          # Spring アニメーション + マウス追従
cargo run --example effects       # GPU エフェクト 4 種（spotlight / magnetic / gravity / fluid）
cargo run --example gpu_flex      # 1 万パーティクル物理シミュレーション
cargo run --example showcase      # 30 デモのグリッド + Modal 拡大
cargo run --example layout        # Taffy Flexbox レイアウト
cargo run --example text          # cosmic-text 統合
cargo run --example tui_demo      # ANSI ベース TUI ダッシュボード
cargo run --example tui_gallery   # アニメーションギャラリー
cargo run --example filer         # ファイルマネージャー（Table / ContextMenu / リネーム）
cargo run --example hello         # low-level API (`SabitoriApp` trait)
```

## アーキテクチャ

13 crates のワークスペース構成:

```
sabitori (umbrella)
├── sabitori-core      Element ビルダー / 基本型 / TUI コンポーネント
├── sabitori-gpu       wgpu SDF レンダラー / OrbitCamera / 画像テクスチャ
├── sabitori-style     CSS 風 StyleProps / Theme / ANSI パレット
├── sabitori-layout    Taffy ラッパー (Flexbox + Grid)
├── sabitori-scene     NodeTree / hit test / 状態管理
├── sabitori-input     Pointer 抽象 / IME / フォーカス
├── sabitori-anim      Spring / Easing / Keyframe / 特化ステート
├── sabitori-text      cosmic-text 統合 / グリフアトラス
├── sabitori-widgets   高レベルウィジェット 21 種
├── sabitori-window    winit ランタイム / EmbeddedRunner
├── sabitori-markdown  Markdown → Element 変換
└── sabitori-net       HTTP fetch (reqwest / wasm fetch)
```

## WASM ターゲット

`templates/wasm/` に `Trunk.toml` + `index.html` のテンプレートと、ハマりどころをまとめた README あり。

```bash
# 初回のみ
rustup target add wasm32-unknown-unknown
cargo install trunk

# テンプレートを自分のアプリにコピー
cp templates/wasm/{Trunk.toml,index.html} /path/to/your-app/

# 開発サーバー / 本番ビルド
trunk serve            # localhost:8080 でホットリロード
trunk build --release  # dist/ に最適化済みアセット
```

WASM 固有の必須設定（`wgpu` の `webgl` feature、フォント同梱、WebGL2 の varying 上限など）は [`templates/wasm/README.md`](templates/wasm/README.md) を参照。

## ロードマップ

実装済み機能と未着手領域は [ROADMAP.ja.md](ROADMAP.ja.md) を参照。

主な未着手:
- macOS ネイティブ統合（NSStatusItem / 透明 NSWindow / 通知）
- 物理単位（`Mm` / `Pt`）と高精度 PPI 検出
- crates.io 公開準備（metadata 整備 + `release-plz` 自動化）

## ライセンス

[MIT](LICENSE)。依存ライブラリのライセンスや参考にした標準技法については [NOTICE.md](NOTICE.md) を参照。

## 動作要件

- Rust 1.85+ (edition 2024)
- wgpu 24 互換の GPU バックエンド (Vulkan / Metal / DX12 / WebGPU / WebGL2)
