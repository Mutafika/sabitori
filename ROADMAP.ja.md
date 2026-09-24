# Sabitori ロードマップ

**言語**: [English](ROADMAP.md) · 日本語

## 現在地

`0.20.0` (pre-release)。コア機能は一通り揃い、`templates/wasm/` の手順で WASM ビルド可能。
利用側は **git タグ依存**が前提（クレート間が path 依存で、シェーダーをクレート外から
`include_str!` しているため）。

直近の focus は、実際に業務アプリ（予約管理）を sabitori で書いて出てきた穴を埋めること。
下の「業務アプリ / Web で足りていないもの」がその一覧で、どれも issue になっている。
リリースラインは **`0.12.x` の一本**。各版の内容は [CHANGELOG.md](./CHANGELOG.md) を参照。

## 実装済み

### 描画・レイアウト
- ✅ wgpu ベース GPU レンダラー（SDF 角丸矩形 + 影 + グラデーション + ボーダー + 回転）
- ✅ cosmic-text 統合 + グリフアトラス（サブピクセル整列、輝度補正コントラスト）
- ✅ 画像テクスチャ描画（async URL ロード + キャッシュ）
- ✅ 3D シーン描画（`scene3d.wgsl` + `OrbitCamera`）
- ✅ Taffy 経由の Flexbox / Grid レイアウト
- ✅ オーバーフロースクロール（慣性 + バウンス + 2D 対応）

### 宣言的 API
- ✅ `DeclarativeApp` トレイト + `Element` ビルダー (`div() / text() / button() / image()`)
- ✅ `ViewContext`（hovered / focused / scroll_info / image_url ロード）
- ✅ ID ベースの `on_click` ルーティング
- ✅ `EmbeddedRunner`（winit 外で sabitori を埋め込み実行する API）

### 入力
- ✅ Pointer 抽象（マウス / タッチ / ペン unified）
- ✅ 日本語 IME + preedit composition
- ✅ Tab / Shift+Tab フォーカス遷移
- ✅ ピンチジェスチャー（タッチパネル + macOS トラックパッド）、慣性スクロール、バウンス
- ✅ macOS ネイティブ Drag & Drop（ファイルドロップ）

### アニメーション
- ✅ Spring 物理 (`snappy` / `gentle` / `bouncy` プリセット)
- ✅ Easing 11 種 + Cubic Bezier カスタム
- ✅ Keyframe + RepeatMode (Once / Loop / PingPong)
- ✅ 特化ステート: Typewriter / Spinner / ProgressBar / Gradient / Wave / Pulse / ColorCycle
- ✅ Splash プリセット 10 種
- ✅ Presence enter/exit + StyleAnimator（fill/border/text の自動補間）

### ウィジェット (`sabitori-widgets` 20 種)
Button / TextInput / Slider / Dropdown / Modal / Card / Panel /
ScrollView / Table（仮想スクロール + ソート） / Tabs / TreeView / VirtualList /
SplitPane / Tooltip / Toast / ContextMenu / FileBrowser / DragManager /
StyleAnimator / PresenceAnimator

### TUI コンポーネント（`sabitori-core::tui`）
- ✅ Block (枠付き title box) / Separator / StatusBar / KeyHint
- ✅ Gradient text / Wave text
- ✅ ANSI 16 色 + xterm-256 パレット

### スタイル
- ✅ CSS 風 `StyleProps`（margin / padding / flex / position / overflow / z-index）
- ✅ `Fill::LinearGradient` でのグラデーション塗り
- ✅ `BoxShadow`（offset / blur / spread / color）
- ✅ Theme システム（YAML ロード + opacity 対応）

### Markdown
- ✅ `sabitori-markdown`：CommonMark + GFM（テーブル / 取り消し線 / フットノート）
- ✅ TOC 抽出、画像リゾルバー連携

### ネットワーク
- ✅ `sabitori-net::fetch_bytes`：reqwest (native) / fetch API (wasm) の cfg 分岐
- ✅ 画像の async ロード + デコード

### WASM / クロスプラットフォーム
- ✅ wasm-bindgen + WebGL2 fallback（WebGPU 自動検出）
- ✅ Trunk ビルドテンプレート（`templates/wasm/` 参照）
- ✅ winit web extension で canvas 自動バインド
- ✅ Lazy render モード（idle 時の 60fps ループ停止）

## 未着手 / 計画中

### macOS ネイティブ統合
`Cargo.toml` に `objc2-app-kit` 依存はあるが Drag & Drop 以外は未実装。

- ⬜ NSStatusItem（メニューバー常駐アイコン）
- ⬜ 透明 NSWindow + wgpu 描画（オーバーレイ用途）
- ⬜ macOS 通知 (UNUserNotificationCenter)
- ⬜ launchd デーモン化サンプル

### 物理単位レイアウト
- ⬜ `Mm(f32)` / `Pt(f32)` 型の追加
- ⬜ OS API 経由の PPI 検出（winit `scale_factor` 以上の精度）
- ⬜ GPU 性能検出 → 品質自動調整（現状は手動 `QualityPreset`）

### crates.io 公開
- ⬜ 各 crate の `description` / `keywords` / `categories` / `readme` 整備
- ⬜ inter-crate dep に `version = "..."` を併記
- ⬜ `release-plz` セットアップでロックステップ release 自動化
- ⬜ docs.rs 用 `#[doc]` コメント整備

### 業務アプリ / Web で足りていないもの

実際に業務アプリ（予約管理、約 20 画面）を sabitori で書いて出てきた穴。
やる / やらないをここで宣言はしていない（まだ決めていない）が、**どれも issue があり、
消費側の回避策も issue に書いてある**ので「待つか自前で書くか」はそこで判断できる。

- ⬜ Web の日本語 IME・ソフトキーボード（隠し textarea の橋渡し）— [#73](https://github.com/Mutafika/sabitori/issues/73)
- ⬜ Web のクリップボード（`copy` / `paste` イベント経由）— [#76](https://github.com/Mutafika/sabitori/issues/76)
- ⬜ Web の URL / 戻るボタン（History）と、アプリからの横スクロール — [#74](https://github.com/Mutafika/sabitori/issues/74)
- ⬜ ファイルを選ぶ / 保存する口（native のダイアログ / Web の input・ダウンロード）— [#77](https://github.com/Mutafika/sabitori/issues/77)
- ⬜ HTTP クライアント（POST/PUT/DELETE・JSON・Cookie）を native/wasm 共通で — [#63](https://github.com/Mutafika/sabitori/issues/63)
- ⬜ 非同期の結果を UI に戻す標準の形（`Tasks`）— [#64](https://github.com/Mutafika/sabitori/issues/64)
- ⬜ ライトテーマと、ウィジェット既定スタイルの `AppTheme` 追従 — [#65](https://github.com/Mutafika/sabitori/issues/65)
- ⬜ フォーム: パスワード欄 [#61](https://github.com/Mutafika/sabitori/issues/61) / disabled 状態 [#62](https://github.com/Mutafika/sabitori/issues/62)
- ⬜ 小粒の部品（時刻ピッカー・sticky・表のセルに Element など）— [#75](https://github.com/Mutafika/sabitori/issues/75)
- ⬜ グラフ / 印刷・画面外描画 / i18n — [#75](https://github.com/Mutafika/sabitori/issues/75) の 11・12 と個別検討

### その他検討中
- ⬜ WebSocket / SSE クライアント
- ✅ Rust コードのホットリロード（subsecond / `feature = "hot-reload"`）
- ⬜ カスタムシェーダーホットリロード
- ⬜ CSS Grid 専用スタイル props（現状は Taffy にパススルー）
