# Sabitori

**言語**: [English](README.md) · 日本語

> Rust 製の GPU UI フレームワーク — wgpu + Taffy + cosmic-text を組み合わせた、宣言的 API でデスクトップ / WASM の両方をターゲットにできる軽量フレームワーク。

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024-orange.svg)](Cargo.toml)

**ステータス**: pre-release (`0.6.0`)。コア機能は実装済み、`templates/wasm/` の手順で WASM ビルド可能。`0.5.0` は CSS にあって無かった穴 — grid / `align-self` / `aspect-ratio` / `z-index` / `text-align` — を埋め、折り返す複数行テキスト欄を足した版、`0.6.0` は WASM ビルドに日本語込みのフォールバックフォントを積んで、`fonts()` を 1 行も書かずに日本語 UI が web で出るようにした版です — [CHANGELOG.md](CHANGELOG.md) を参照。

## 特徴

- **宣言的ビルダー API** — `div() / text() / button() / image()` でツリーを組み立てる
- **CSS の形のレイアウト** — Taffy による flex と grid、`align-self` / `align-content` / `aspect-ratio` / `z-index` / `text-align`
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

**操作したいものには id を付けること。** クリック・ホバー・フォーカス・スクロールは全部 id を鍵にしています。

**押したら何が起きるかは、押される要素のところに書きます：**

```rust
div().click(ctx, "save", |app: &mut App| app.saved = true)
```

`click` は id の割り当てとハンドラの登録を 1 回でやるので、**同期を取るべき 2 つ目の場所が存在しません**。一覧なら、id から数字を切り出す代わりに添字を捕まえます：

```rust
div().click(ctx, format!("row-{i}"), move |app: &mut App| app.selected = Some(i))
```

古い書き方 —`.id("save")` と `fn on_click(&mut self, id: &str)` の文字列マッチ— も動きますし、動的に振り分けたいときのために残してあります。ただし**どちらの文字列を打ち間違えてもコンパイルは通り、黙って何も起きません**。`click` を使ってください。

## よく間違える 5 つ

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

ホイールについて 2 点。`.scroll(id)` のコンテナがホイールを消費するのは**その向きにまだ動けるあいだ**だけで、端に達すると外側のコンテナへ、最後は `on_scroll_xy` へ落ちます。そして生のホイールは**先に** `on_input` へ `InputEvent::Wheel` として届きます — カーソル位置・修飾キー・トラックパッドの位相つきなので、⌘+ホイールのズームはそこで書きます。`true` を返せば何もスクロールしません:

```rust
fn on_input(&mut self, ev: &InputEvent) -> bool {
    match ev {
        InputEvent::Wheel { position, delta_y, modifiers, .. } if modifiers.meta => {
            self.zoom_at(*position, *delta_y);
            true // 消費: 何もスクロールしない
        }
        _ => false,
    }
}
```

### 2. テキスト入力と IME

**`view()` に `text_input` を置く。配線はこれで全部です。**

```rust
struct App { name: TextInputState, saved: Option<String> }

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


折り返す複数行の欄は `text_area` です。状態の型も配線の要らなさも同じです。

```rust
text_area(ctx, "memo", &self.memo, &TextInputStyle::default_dark(), 6)  // 6 行ぶんの高さ
```

| | `text_input` | `text_area` |
|---|---|---|
| Enter | アプリへ流す (フォーム送信) | 改行を入れる |
| 貼り付け | 改行を空白に潰す | 改行を保つ |
| ↑ ↓ | アプリへ流す | **視覚行**を 1 つ移動 |
| Home / End | 文字列の先頭 / 末尾 | **視覚行**の先頭 / 末尾 |

「視覚行」であることが要点です。論理行 (`\n` 区切り) で動かすと、折り返した長い段落の中で 1 回押しただけで段落ごと飛びます。`Cmd+Enter` はアプリへ流れるので、「送信」を割り当てられます。

### 3. フォーカスとキーボード

`.focusable` な要素はクリックと Tab でフォーカスが入ります。キーはまず `on_focused_input(id, event)` に行き、処理されなかったぶんが `on_input(event)` に落ちます。

**`on_input` が `true` を返すと、そのキーの既定動作（コピー・ペースト・Escape・Tab）が抑止されます。** 消費していないなら `false` を返すこと。返し間違えると既定動作が黙って死にます。

### 4. テスト

窓も GPU も無しでアプリを動かせます：

```rust
use sabitori::testing::Harness;

let mut h = Harness::new(App::default(), 800.0, 600.0);
h.frame();                  // 組み立て + レイアウト
h.click("name");            // id で欄にフォーカスを入れる
h.text("hello");            // 打鍵はフォーカス中の要素へ行く
h.click("save");            // ここで初めてハンドラが打った値を見る
h.scroll("file-list", 400.0);
h.settle();                 // ばねを終わらせる (scroll_intents に必要)
assert_eq!(h.app().saved.as_deref(), Some("hello"));
```

**`frame()` は時間を進めません。** 慣性スクロール・`scroll_intents`・style アニメーションなど、ばねで動くものは `tick(dt)` か `settle()` が要ります。

### 5. `tick` で絵を動かしているのに名乗らない

**`tick(dt)` が画面を動かすなら `is_animating` を上書きします。**

```rust
fn is_animating(&self) -> bool { true }   // 粒子・スピナー・時計
fn tick(&mut self, dt: f32) { self.t += dt; }
```

ランタイムは、誰も要求していないフレームを描きません。自分が持っている
アニメーター — スクロールのばね、style、presence、ドラッグ、tooltip の遅延、
そして組み込み `text_input` のキャレット — は見えますが、**アプリの状態は
見えません**。名乗らずに `tick` で粒子を動かすと、**次の入力が来るまで画面が
止まります**。

一度きりの変化 (ワーカースレッドが終わった、toast の時間が切れた) は
`poll_dirty` の方です。毎 tick に 1 回問われ、読むと下ります。

`lazy_render` に `false` を返すと窓ごと降りて、無条件に描き続けます。0.8.0 まで
の既定がこれで、idle の窓で 1 コアと GPU の一部を焼きます。まず `is_animating`
を検討してください。

`run_scene` (`SceneApp`) も同じ規則です。**`render_scene` が自前の時計で絵を
動かすなら名乗ってください** — 回り続けるカメラや粒子は UI 側のイベントを
伴わないので、名乗らないと次のクリックまで scene ごと止まります。

## レイアウト

flex と grid の両方が使えます。土台は Taffy で、名前は CSS に揃えてあります。

```rust
// flex
let toolbar = div().flex_row().items_center().justify_between().gap(8.0);

// grid — サイドバー固定 + 本文が余りを取る
let shell = grid()
    .grid_cols([Track::px(240.0), Track::fr(1.0)])
    .gap(12.0)
    .children([sidebar, body]);

// 全列にまたがる見出し行
let sheet = grid()
    .grid_cols(Track::repeat(3, Track::fr(1.0)))
    .children([header.col_span(3), a, b, c]);
```

`Track` は CSS の `minmax(min, max)` そのものです。`Track::px / pct / fr / auto / min_content / max_content / minmax` で作り、`Track::repeat(n, track)` で並べます。`auto-fill` / `auto-fit` は未対応で、本数は呼び出し側が決めます。

| | |
|---|---|
| 子 1 個だけ親の揃えから外す | `.self_start()` `.self_center()` `.self_end()` `.self_stretch()` |
| 折り返した**行**の配り方 | `.wrap()` した入れ物に `.align_content(..)` |
| 縦横比を固定する | `.aspect(16.0 / 9.0)` |
| 兄弟の中での重なり順 | `.z(5)` |
| 折り返したテキストの揃え | `.text_center()` `.text_right()` |

このうち 3 つには前提があります。「効かない」と思う前にここを見てください。

- **`.aspect()` は stretch に負けます。** `flex_col` の既定 (`align_items: stretch`) は子の幅を先に決めてしまうので、辺が 2 つ決まった時点で比の出番がありません。高さから幅を出したいなら `.self_start()` を併記します。
- **`.text_center()` には幅が要ります。** テキスト要素は中身なりの大きさなので、揃える余白がそもそもありません。`flex_col` の中では親幅まで伸びるのでそのまま効きますが、`flex_row` の中では効きません。
- **`.z()` は親を飛び越えません。** CSS の重なり文脈と同じです。効くのは兄弟の中だけで、描画順とクリック順が一緒に動きます。木を飛び越えて最前面に出したいなら (ポップアップ、コンテキストメニュー) `.overlay()` を使ってください。

`display: none` は意図的に入れていません。要素を出さなければいいので、そちらの方がレイアウト計算ごと消えます。

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
let close = div().id("close").role(Role::Button).label("閉じる");   // アイコンだけのボタン
let heading = text("設定").role(Role::Heading).heading(2);
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
cargo run --example hot_reload    # ホットリロードのデモ（下記）
```

## ホットリロード（実験的）

`view()` を書き換えて保存すると、**状態を保ったまま**画面だけが更新される。
走行中のプロセスに機械語パッチを当てる [subsecond](https://crates.io/crates/subsecond)
を使う。パッチを作るのは Dioxus CLI なので、起動が `cargo run` ではなく `dx serve`
になる。

有効にするのは feature ひとつで、コードの変更は要らない。

```toml
[dependencies]
sabitori = { version = "0.6", features = ["hot-reload"] }
```

```bash
cargo install dioxus-cli   # 初回だけ
dx serve --hot-patch       # アプリのクレート直下で、`cargo run` の代わりに
```

`src/main.rs` を書き換えて保存する。パッチが当たるたびにアプリが
`hot-reload: パッチ適用` を出す。M2 Max で 1 回あたり実測 0.7〜1.0 秒
（起動直後の 1 回目はパッチキャッシュを作るぶん遅い）。

**`dx` が見ているのは各クレートの `src` / `tests` / `Cargo.toml` だけ。**
そこから外れたコードは監視されないので、アプリはクレートの `src/` に置く必要がある。
同じ理由で、同梱の `examples/hot_reload.rs` は — このリポジトリが example を
ワークスペース直下（どのクレートにも属さない場所）に置いているため —
**ここからはホットリロードできない**。雛形として読み、自分のクレートの
`src/main.rs` に写してから `dx serve` すること。

- **効く**: `view()` / `overlay_view()` / `view_for()` の中身と、そこから呼ばれる全て。
  レイアウト・色・文言・分岐
- **効かない**: 状態を持つ struct のフィールド追加・削除・型変更。メモリレイアウトが
  変わるので `dx` がフル再起動に落とす
- subsecond は `debug_assertions` が有効なときだけ働く。release では境界が素の呼び出しに
  畳まれるので、feature を立てたまま出荷しても実行時コストは無い
- devserver が居なければ黙って無効になる。`cargo run` は今までどおり
- ネイティブのみ。WASM は `trunk serve` のライブリロードを使う

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
