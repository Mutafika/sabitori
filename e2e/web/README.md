# web の実機確認 (headless Chromium)

wasm は「native のテストが 1 つも落ちないのに web だけ壊れている」が起きる。
実際に起きたもの:

- ランタイムの初期化が native / wasm で 2 本あり、wasm 側に `LineRenderer` が
  無かった → **web でだけ `polyline()` が 1 本も描かれない**
  ([#66](https://github.com/Mutafika/sabitori/issues/66))
- GPU の limits に WebGL2 の既定をそのまま要求していた → **上限の低い環境
  (SwiftShader) で起動時に panic**
  ([#72](https://github.com/Mutafika/sabitori/issues/72))
- `view()` を組むだけでスタックを食い潰す (#56)

どれも `cargo test` では見えない。ここは **本物のブラウザに描かせて PNG を
見る**ための最小の足場で、依存は Chrome と Node だけ (npm install は要らない)。

## 使い方

```sh
cd e2e/web
trunk build                                   # 初回は数分 (フォント 10MB を積む)
(cd dist && python3 -m http.server 8099 &)

"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --headless=new --remote-debugging-port=9222 \
  --user-data-dir=/tmp/sabitori-e2e-profile \
  --use-angle=swiftshader --use-gl=angle --window-size=800,600 \
  --no-first-run about:blank &

node drive.mjs http://127.0.0.1:8099/index.html /tmp/shot.png
```

`drive.mjs` は CDP (Chrome DevTools Protocol) を Node の組み込み `WebSocket`
だけで叩く。出力は console のログ・例外・PNG。**`--use-angle=swiftshader` は
わざと**指定している — 実 GPU では通って弱い環境で落ちる、が #72 の形なので。

第 3 引数に ES モジュールを渡すと、スクショの前に差し込める
(`export default async ({ send, sleep, logs }) => { ... }`)。`send` は CDP の
生の呼び出しで、`Input.dispatchKeyEvent` や `Input.imeSetComposition` で
キー入力・IME の変換まで送れる。

```sh
node drive.mjs http://127.0.0.1:8099/index.html /tmp/last.png probes/ime.mjs
```

`probes/history.mjs` は **URL と戻るボタン (#74)** を見る: 押すと
`location.hash` が変わり、戻ると画面も戻る。**「URL は戻ったのに画面が戻らない」**
がここで見つかった — `lazy_render` が既定 true なので、DOM 起点の出来事では
ランタイムが起きず、積んだものが誰にも汲まれない (`web_wake`)。

`probes/clipboard.mjs` は **web のクリップボード (#76)** を通しで見る:
打つ → ⌘A → ⌘X で欄が空になる → ⌘V で戻る。クリップボードの中身は CDP から
直接読めないので、**貼り戻して**確かめている。

> プローブが `Input.dispatchKeyEvent` に `commands: ["cut"]` を渡しているのは
> **CDP の都合**。合成したキーイベントはブラウザの編集コマンドを起こさない
> (実際の打鍵だけが起こす)。渡さないと `cut` / `copy` が飛ばず、橋渡しが
> 壊れているように見える。実機では要らない。

`probes/ime.mjs` は **web の日本語入力 (#73)** を通しで見る: 欄を押す →
`にほんご` を変換中にする → `日本語` を確定 → `abc` を足す → Backspace →
⇧← で選択。各段で PNG を吐くので、**変換中の文字が欄に見えているか**まで
目で確かめられる (assert では書けない部分)。

## 何を見ているか

`src/main.rs` は**回帰が起きたら絵で分かる**ものだけを置く:

| 置いてあるもの | これが消えたら |
|---|---|
| `polyline()` の折れ線 | wasm の初期化から `LineRenderer` が落ちた (#66) |
| `rounded(Px(999.0))` のピル | 角丸の半径の丸めが外れた (#71) |
| `text_input` | 隠し textarea の橋渡しが切れた = IME が届かない (#73) |
| 「詳細をひらく」ボタン | URL / 戻るボタンの橋渡しが切れた (#74) |
| そもそも画面が出る | GPU の limits を要求しすぎている (#72) |

CI には載せていない。wasm の成果物が 35MB あり、毎 PR で焼くには重すぎる。
**タグを打つ前と、wasm の初期化・GPU 周りを触ったときに手で回す。**
