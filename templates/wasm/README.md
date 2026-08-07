# Sabitori WASM Build

Sabitoriアプリをブラウザで動かすためのテンプレート。

## セットアップ

```bash
# 1. このディレクトリのファイルをプロジェクトルートにコピー
cp index.html Trunk.toml /path/to/your-app/

# 2. WASMターゲット追加（初回のみ）
rustup target add wasm32-unknown-unknown

# 3. Trunk インストール（初回のみ）
cargo install trunk
```

## ビルド & 実行

```bash
# 開発サーバー（ホットリロード）
trunk serve

# 本番ビルド
trunk build --release

# dist/ をデプロイ（Vercel / Netlify / GitHub Pages）
```

## 必須設定

### 1. wgpu の `webgl` feature を有効にする

```toml
# Cargo.toml (ワークスペース or 直接依存)
wgpu = { version = "24", features = ["webgl"] }
```

これがないと WebGPU バックエンドしかビルドされず、
`Backends::GL` を指定しても無視される。
localhost では動くがデプロイ先で `canvas context is not a GPUCanvasContext` でパニックする。

### 2. フォントを `fonts()` でバンドルする

WASM 環境にはシステムフォントがないため、
`FontSystem::new()` が `no default font found` でパニックする。
`DeclarativeApp::fonts()` で最低1つのフォントを返すこと。

```rust
fn fonts(&self) -> Vec<Vec<u8>> {
    vec![
        include_bytes!("path/to/font.ttf").to_vec(),
    ]
}
```

### 3. シェーダーの inter-stage component 上限

WebGL2 の varying 上限は **31 コンポーネント**（WebGPU より低い）。
VertexOutput の `@location` フィールドの合計コンポーネント数が 31 以下であること。
`vec4` = 4, `vec2` = 2, `f32` = 1 でカウント。

## その他の注意

- `textureSample` は分岐の外で呼ぶこと（WebGPU制約）
- HDR（Rgba16Float）はブラウザ非対応の場合がある → `ctx.surface_format` を使う
- `getrandom` クレートを使う場合は `features = ["js"]` が必要
