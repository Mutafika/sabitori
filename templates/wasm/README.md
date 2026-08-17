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

### 2. フォント — 既定では**何もしなくていい**

WASM 環境にはシステムフォントが無い。`0.5.2` から、sabitori は
**wasm32 ビルドにだけフォントを自動で埋め込む**ので、`fonts()` を 1 行も
書かなくても日本語 UI がそのまま出る。まっさらな状態でパニックすることは無くなった。

既定は **HackGen（白源）**。半角 2 文字が全角 1 文字にちょうど乗るので、
罫線・日本語・英数字が同じ桁に揃う。

| feature | フォント | raw | gzip | 日本語 |
|---|---|---|---|---|
| `builtin-font-jp`（既定） | HackGen | 10.2MB | **4.9MB** | 出る |
| `builtin-font-latin` | Hack | 302KB | **144KB** | 豆腐 |

```toml
sabitori = "0.5"                                     # HackGen（日本語込み）

sabitori = { version = "0.5", default-features = false,
             features = ["builtin-font-latin"] }     # Hack（軽い・日本語は豆腐）

sabitori = { version = "0.5", default-features = false }  # 組み込み無し
```

`-latin` から `-jp` へ切り替えると **英数字は約 12% 細くなる**。HackGen の字形は
Hack そのものだが、全角グリッドに乗せるため advance を詰めてあるため
（0.602em → 0.527em）。字形が同じでもレイアウトは同じにならない。

組み込み無しでフォントを 1 つも渡さないと、シェープに入る手前で
「`fonts()` に何を書けばいいか」を書いたメッセージで停止する。

<details>
<summary>自前のフォントを使う</summary>

```rust
fn fonts(&self) -> Vec<Vec<u8>> {
    vec![
        include_bytes!("path/to/YourFont.otf").to_vec(),
    ]
}
```

ここで渡したフォントは組み込みより**先**に当たるので、上書きの心配は要らない。
組み込みは穴埋めにしか使われない。

native と wasm で見た目を揃えたいなら、組み込みに寄りかからず必ず `fonts()` で
渡すこと — 組み込みは wasm にしか積まれないので、native ではシステムフォントが
埋めてしまう。
</details>

### 3. シェーダーの inter-stage component 上限

WebGL2 の varying 上限は **31 コンポーネント**（WebGPU より低い）。
VertexOutput の `@location` フィールドの合計コンポーネント数が 31 以下であること。
`vec4` = 4, `vec2` = 2, `f32` = 1 でカウント。

## その他の注意

- `textureSample` は分岐の外で呼ぶこと（WebGPU制約）
- HDR（Rgba16Float）はブラウザ非対応の場合がある → `ctx.surface_format` を使う
- `getrandom` クレートを使う場合は `features = ["js"]` が必要
