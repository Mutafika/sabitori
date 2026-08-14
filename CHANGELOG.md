# Changelog

このプロジェクトの注目すべき変更点を記録する。
書式は [Keep a Changelog](https://keepachangelog.com/ja/1.1.0/) に準拠し、
バージョニングは [Semantic Versioning](https://semver.org/lang/ja/) に従う。

> **0.x の方針:** 1.0.0 までは API が動く可能性がある。
> 機能追加・破壊的変更は **minor** を、修正のみは **patch** を上げる。
> リリース手順は [RELEASING.md](./RELEASING.md) を参照。

> **補記 (2026-07-27):** `0.1.4` / `0.2.1` / `0.2.3`〜`0.2.10` / `0.2.14`〜`0.2.20`
> の 17 版は、リリース当時に記載を落としていたものを `git log` から後追いで
> 復元した ([#24](https://github.com/Mutafika/sabitori/issues/24))。commit 件名が
> 元になっているため、当時の背景や設計意図の記述は他の版より薄い。

## [Unreleased]

### Added

CSS にあって sabitori に無かったレイアウト機能のうち、**土台が既に持っているのに
変換層で捨てていたもの**を通した回。`0.4.0` までの「宣言はあるが届かない」潰しとは
逆方向で、こちらは純粋な機能不足。

taffy 0.7 は grid も `align-self` も `aspect-ratio` も実装済みなのに、
`convert_to_taffy_style` が `display: Flex` を決め打ちし、対応するフィールドを
書いていなかった。cosmic-text も `Align` と `Style::Italic` を持っていた。
つまり**依存クレートは出来ていて、繋いでいなかった**だけ。

- **grid レイアウト** — `Display::Grid`、`Track` / `TrackSize` / `GridPlacement` /
  `GridAutoFlow`。

  ```rust
  grid()
      .grid_cols([Track::px(240.0), Track::fr(1.0)])   // サイドバー + 本文
      .gap(12.0)
      .children([sidebar, main])

  grid()
      .grid_cols(Track::repeat(3, Track::fr(1.0)))
      .children([header.col_span(3), a, b, c])
  ```

  `Track` は CSS の `minmax(min, max)` そのままの対。`Track::px` / `pct` / `fr` /
  `auto` / `min_content` / `max_content` / `minmax` / `repeat` で作る。`fr` の下限が
  `Auto` なのは CSS の `1fr` と同じ（中身より小さくならない）。

  `auto-fill` / `auto-fit` は未対応 — 本数は呼び出し側が決める。

  これが無かったので、列を揃えるレイアウトは flex の `grow` に読み替えるしかなく、
  `TableColumn::{flex, fixed}` はその穴埋めとして存在している。

- **`align_self` / `justify_self`** — `.self_start()` / `.self_center()` /
  `.self_end()` / `.self_stretch()` / `.align_self(..)`。

  親の `align_items` から**子 1 個だけ**外れられる。これが無かったので、例外に
  したい子を別の入れ物で包んで逃がすしかなく、木が 1 段深くなっていた。

- **`align_content` / `justify_items`** — 折り返した**行そのもの**の配り方。
  `wrap()` した入れ物と grid で効く。

- **`aspect_ratio`** — `.aspect(16.0 / 9.0)`。片方の辺から比でもう片方が決まる。

  **交差軸が stretch だと効かない**（決まった辺が 2 つあれば比の出番が無い、CSS と
  同じ）。`flex_col` の中で高さから幅を出したいなら `.self_start()` を併記する。

- **`AlignItems::Baseline`** — フォントサイズの違う文字を横に並べたときに、箱の中心
  ではなくベースラインで揃える。

- **`text_align`** — `.text_center()` / `.text_right()` / `.text_align(..)`。
  `TextAlign::{Start, Center, End, Justify}`。

  **折り返しが起きて、かつ要素に幅があるときだけ効く。** `flex_col` の既定
  （`align_items: stretch`）ではテキストが親幅まで伸びるのでそのまま効くが、
  `flex_row` の中では中身なりの幅しか無く、揃える余白が生まれない。

- **`italic`** — `.italic()`。face に斜体が無ければ cosmic-text が傾けて代用する。
  和文フォントは斜体を持たないのが普通なので、日本語はほぼ合成斜体になる。

- **`z_index`** — `.z(5)`。兄弟の中での重なり順で、大きいほど手前。描画順と
  クリック順の**両方**が動く。

  **同じ親を持つ兄弟の中でしか効かない**（CSS の重なり文脈と同じ）。実装は
  「兄弟を安定ソートして部分木ごと入れ替える」で、出来上がったコマンド列を後から
  並べ替えてはいない — `PushClip` / `PopClip` の対応が崩れ、持ち上げた子が親の
  クリップの外に描かれてしまうため。木を飛び越えたいなら従来どおり `.overlay()`。

### Changed

- **`Typography` にフィールドが 2 つ増えた** (`italic` / `align`)。構造体リテラルで
  組んでいる箇所は `..Typography::default()` を足すこと。
- **`AlignItems` に `Baseline` が増えた**。`AlignItems` を網羅マッチしている箇所は
  腕の追加が要る（repo 内では `sabitori-layout` が該当し、コンパイルエラーで
  止まった）。
- **`HitRegion::element_index` は「ペイント順」の深さ優先添字**になった。`z_index` を
  書いた兄弟が居ると元のツリー順とずれる。

### Notes

**足さなかったもの**と理由:

- **`display: none`** — 宣言的に組む以上「隠す」は要素を出さないことで書けて、
  そちらの方がレイアウト計算ごと消える。`if cond { children.push(x) }`。
- **`position: sticky`** — スクロール位置に応じた再配置が要るので、レイアウト層
  だけでは閉じない。ランタイム側の作業。

## [0.4.0] - 2026-08-14

フレームワーク全体を見直して挙げた [#14〜#22](https://github.com/Mutafika/sabitori/issues)
と、その後の 2 巡目レビューで出た構造的な穴を潰す破壊的変更ラウンド。旧 API は
`#[deprecated]` を挟まず削除し、代わりに移行手順をここに残してある。

**2 巡目で出たもの。** 1 巡目でランタイムの契約は直ったが、その上の層——ウィジェット・
example・README——が古いモデルを教えたままだった。機構が正しくても、消費側が読む場所に
それが書いてなければ症状は減らない。widget crate は「`view()` から使える宣言的なもの」と
「画面座標を渡す retained なもの」の 2 系統に割れており、後者は `Element` を返さないので
`view()` から組み込めず、repo 内の使用箇所も 0 だった。旗艦 example の `filer` は
テストされていない手動スクロールを教えていた。README は `0.1.0` のままだった。

**背景。** 立て続けに来た issue を分類したら、純粋な機能不足は 1 件だけで、残りは
すべて「core は持っているのに消費側に届かない」「doc と実装が食い違う」だった。しかも
全部**黙って**落ちる — コンパイルは通り、パニックもせず、ただ何も起きない。個々を
潰すより、**そういう状態を作れなくする**方に投資した版。

象徴的なのは #17 で、`InputEvent` に variant を足すと 3 ランタイムすべてが
コンパイルエラーで止まるようにした。その直後の #20（ペースト実装）で実際に発火し、
配線漏れの余地なく全箇所を通ることになった。

### Added
- **入力イベントの配線漏れをコンパイルエラーで止める仕組み**
  ([#17](https://github.com/Mutafika/sabitori/issues/17))。

  sabitori はイベント処理を共有しない 3 つのランタイム（`DeclarativeApp` /
  `SceneApp` / `SabitoriApp`）を持ち、配線は 1 つずつ手で書かれている。その結果
  「core は持っているのにランタイムが配らない」事故が繰り返し起きていた
  （[#1](https://github.com/Mutafika/sabitori/issues/1) /
  [#3](https://github.com/Mutafika/sabitori/issues/3) /
  [#12](https://github.com/Mutafika/sabitori/issues/12)）。#12 に至っては、その修正作業の
  中で `sabitori-window` が新 variant を `_ => {}` で握り潰す**同型のバグを作っている**。

  - **`InputEventKind`** — `InputEvent` からペイロードを落とした種別。値を持たないので
    `match` の腕として並べられる。`InputEventKind::ALL` で全種別を舐められる。
  - **`InputEvent::kind()`** — 「全 variant を知っている唯一の場所」。`InputEvent` に
    variant を足すとまずここが壊れる。
  - **`Delivery`** — 1 種別をアプリへどう扱うかの宣言。`ToApp` / `FocusedOnly` /
    `Internal(理由)` / `NotProduced(理由)`。
  - **`input_delivery(kind) -> Delivery`** — 3 ランタイムそれぞれに追加。
    `sabitori::declarative` / `sabitori::scene_app` / `sabitori_window` の各モジュール。
    `InputEventKind` に対する**網羅マッチ**なので、種別が増えると 3 つとも
    コンパイルエラーになり、「配る / 内部で消費する / 発行しない」の判断を必ず通る。

  検証済み: `InputEvent` に variant を 1 つ足すと**計 6 箇所**（`kind()`、
  `sabitori-window` の配信表とポインタ match 2 つ、`declarative` と `scene_app` の
  配信表）がコンパイルエラーになる。

  宣言はドキュメントでもある。「このランタイムで `InputEvent::ImeEnabled` は来るのか」
  を表 1 つで確認できる。実際にこの作業でランタイム間の差が 2 件見つかっており、
  事実として宣言に書き出したうえで
  [#22](https://github.com/Mutafika/sabitori/issues/22) に切り出した。

- **`ViewContext` に実フォント計測を追加**
  ([#15](https://github.com/Mutafika/sabitori/issues/15))。

  ```rust
  ctx.caret_x(&self.input.text, self.input.cursor_pos, 14.0, false) // キャレットの x 位置
  ctx.text_width("ラベル", 14.0, false)                             // 1 行の幅
  ctx.measure(text, size, bold, monospace, family)                  // 太字・書体指定つき
  ```

  それまで `view()` の中でアプリが触れる計測手段は `ViewContext::mono_advance`
  （等幅 1 セルぶんの送り）**だけ**だった。等幅なら `index * mono_advance` で
  キャレット位置を出せるが、**プロポーショナル書体では計算する方法が存在しなかった**。
  `sabitori_core::forms::text_input` が `cursor_pos_px` を受け取って無視していたのは、
  呼び出し側が正しい値を作れなかったから。

  `caret_x` は `byte_offset` が文字境界の途中でも panic せず、直前の境界まで戻る
  （日本語のテキスト欄はカーソルが 3 バイト単位で動くので、1 バイトのずれで落ちる
  API では使えない）。

  精度の注記: 実装は `text[..byte_offset]` を単独で整形して幅を取る。合字やカーニングが
  境界をまたぐ場合、全体を整形してクラスタ送りを足すのとは 1px 未満ずれ得る。

### Changed（破壊的）
- **`text_input` を 1 本に統合し、`sabitori_core::forms::text_input`
  (`form_text_input`) を削除**
  ([#16](https://github.com/Mutafika/sabitori/issues/16))。

  テキスト欄の実装が 2 つあり、**どちらも不完全**だった:

  | | preedit（変換中） | キャレット |
  |---|---|---|
  | `sabitori_widgets::text_input` | 出る | **描画コードが無かった** |
  | `form_text_input` | 出ない | 描くが**常に文末** |

  後者が `cursor_pos_px` を受け取って無視していたのは、呼び出し側に幅を測る手段が
  無かったから（#15）。それが通ったので統合した。新しい `text_input` は:

  - 確定済みテキスト + 変換中の文字を合成して表示（`preedit` 色で範囲を示す）
  - **キャレットを正しい位置に描く**。変換中は preedit の**中**を指す
  - 点滅する。ただし**変換中は点滅を止める**（編集位置を見失わないため）

  ```rust
  // 旧（11 引数、キャレット位置は 0 をベタ書きするしかなかった）
  form_text_input(id, &display, is_placeholder, cursor_visible, 0.0, focused,
                  text_color, placeholder_color, bg, border, focus_border)
  // 新
  text_input(ctx, id, &self.name, &style)
  ```

  `TextInputStyle` に `focus_border` / `caret` / `preedit`（いずれも `Option`）が
  増えた。`ime_cursor_area` に渡す矩形は `caret_rect(ctx, origin, state, style)` で
  作れる（返さないと**変換候補が画面左上に出る**）。

- **アクセシビリティの意味層**（`Role` / `label` / `heading`）
  ([#21](https://github.com/Mutafika/sabitori/issues/21))。

  ```rust
  div().role(Role::Button).label("閉じる").on_click(|| {})
  div().heading(2).label("設定")
  ```

  `HitRegion` に `role` / `label` / `heading_level` が載り、**役割やラベルだけを
  持つ要素**（クリックもフォーカスもしない見出しや画像）も `hit_regions` に出る
  ようになった。`button()` と `text_input` は既定で役割を名乗る。

  ⚠️ **これだけでは VoiceOver / NVDA からはまだ空の窓に見える。** `accesskit`
  への接続は入っていない（#25）。残りは `accesskit_winit` のアダプタの
  ライフサイクル依存で、スクリーンリーダの実機確認なしに「入った」と言えないため
  意味層で区切った。素材（id / 矩形 / role / label / focusable）はここで揃っている。

- **ペーストが動くようになった / コピーが全プラットフォーム対応になった**
  ([#20](https://github.com/Mutafika/sabitori/issues/20))。

  **ペーストはどのプラットフォームにも実装が無かった。** `TextInputState` に
  `Key::V if is_cmd` の受け口と「実際のペーストテキストは CharInput か ImeCommit
  で届く」というコメントだけがあり、**クリップボードを読むコードが repo に
  存在しなかった**ので何も届かなかった。コピーも macOS 専用（`pbcopy`
  サブプロセス）で、他は `let _ = text;` と捨てていた。

  - `InputEvent::Paste { text }` を追加。`CharInput` の連打ではなく **1 操作 =
    1 イベント**（undo の単位や IME の状態と噛み合わせられるように）
  - `sabitori::clipboard`（`read_text` / `write_text` / ショートカット判定）を
    追加。`arboard` で macOS / Windows / Linux を 1 本に
  - `TextInputState::on_paste` — 選択を置換し、複数行は空白に均す（単一行の欄なので）
  - `macos_drag::copy_text_to_clipboard` を削除（`clipboard::write_text` に統合）

  wasm は `navigator.clipboard` が非同期なので未対応（`read_text` は `None`）。

  ⚠️ `TextInputState::on_key` は **Cmd/Ctrl+V を消費しなくなった**（`false` を
  返す）。消費するとランタイムの既定動作＝クリップボード読みが #18 の仕組みで
  止まり、ペーストが永久に起きない。自前のフィールド実装で `Key::V` に `true` を
  返しているコードは同じ理由で直すこと。

  この variant 追加で **#17 の仕組みが実際に発火した** — `InputEvent` に
  `Paste` を足した瞬間、3 ランタイムすべてがコンパイルエラーで止まり、配線の
  判断を全箇所で通ることになった。

- **`sabitori::testing` — アプリの回帰テストを窓も GPU も無しで書ける**
  ([#19](https://github.com/Mutafika/sabitori/issues/19))。

  ```rust
  let mut h = Harness::new(MyApp::default(), 800.0, 600.0);
  h.frame();
  h.click("save");
  assert!(h.app().saved);
  ```

  ランタイムはヘッドレスでフレームを回せる作りだったのに、その入口が
  `#[cfg(test)]` の中に閉じていた。実際 #1 / #3 / #12 / #14 は**全部「人間が手で
  動かして気づいた」**で見つかっている。いずれも見た目に異常が出ない
  「黙って効かない」タイプで、手動確認では見落としやすい。

  `frame` / `click` / `click_at` / `press_at` / `release` / `key` / `text` /
  `scroll` / `move_to` と、`app` / `capture` / `focused_id` / `rect_of` /
  `visible_ids` / `scroll_y`。テキスト計測はスタブ（1 文字 = `font_size * 0.5`）
  なので、ピクセル値そのものの assert には向かない。

  そのために winit の match に埋まっていたポインタ処理を
  `press_primary` / `release_primary` に切り出した。

- **`TextInputState` が `Default` を実装した**
  ([#19](https://github.com/Mutafika/sabitori/issues/19))。アプリの state 構造体に
  `#[derive(Default)]` を付けたままテキスト欄を持てなかった。ハーネスの使用感を
  外から確かめている最中に判明。

### Changed（破壊的・続き）
- **レイアウト基本型の二重定義を解消した**
  ([#24](https://github.com/Mutafika/sabitori/issues/24))。

  `sabitori-core::element` と `sabitori-style::props` が、**同じ名前の型を 9 個
  別々に定義**していた（`AlignItems` / `BoxShadow` / `Dimension` /
  `EdgeDimensions` / `FlexDirection` / `FlexWrap` / `JustifyContent` /
  `Overflow` / `Position`）。ファサードは style 側だけを名前付きで出していたので:

  ```rust
  use sabitori::{div, Overflow};
  div().overflow(Overflow::Scroll);
  // error: expected `sabitori::element::Overflow`, found `sabitori::Overflow`
  ```

  **エラーの 2 つの名前がほぼ同じに見える**ので、踏むとコンパイラを疑うレベルで
  混乱する。`sabitori::Px` は core 側、`sabitori::Dimension::Px` は style 側という
  食い違いもあった。

  構造も `Default` も同一で、差は style 側にだけ `Serialize` / `Deserialize` が
  付いていた点だけだった（YAML テーマ用）。core にその derive を足したうえで
  **style 側の定義を削除**し、core の 1 組に統合した（`props.rs` は 227 → 116 行）。

  `StyleProps` を組むコードと `Element` を組むコードが**同じ型を共有する**ように
  なったので、書き分けは不要。`tests/facade.rs` の
  `layout_types_are_shared_between_element_and_style_props` が一本化を固定している。

- **`SceneApp` だけ IME の届き方が違った**
  ([#22](https://github.com/Mutafika/sabitori/issues/22))。`ImeEnabled` を
  組み立てておらず、preedit / commit も `on_focused_input` にしか渡していなかった。
  **フォーカス中の要素が無いと変換中の文字がどこにも届かない**ので、ターミナルの
  ような「フォーカス要素は無いが IME 入力は受ける」アプリが `SceneApp` では
  書けなかった。`DeclarativeApp` と同じ形に揃えた。

  この差は #17 で入れた配信表が炙り出したもので、揃えたことで
  `known_ime_divergence_between_declarative_and_scene_app` が**設計どおり落ちた**
  （差が消えたことに気づくための仕掛け）。2 ランタイムの全種別一致を固定する
  テストに差し替えてある。

- **`on_input` の戻り値が効くようになった（既定動作の抑止）**
  ([#18](https://github.com/Mutafika/sabitori/issues/18))。doc は "Return true if
  handled" と言っていたのに、**呼び出し 15 箇所すべてが戻り値を捨てていた**。
  `true` を返しても Tab のフォーカス移動も Escape のフォーカス解除も走る、という
  状態で、独自キーバインドを持つアプリが書けなかった。

  `true` を返すと止まるもの:
  - Tab / Shift+Tab のフォーカス移動
  - Escape のフォーカス解除
  - Cmd/Ctrl+C による選択テキストのコピー
  - 「コピー以外のキーで選択を解除する」挙動

  ⚠️ **配信順が変わった。** 既定動作を抑止するには、アプリが先に見る必要がある。
  そのため Tab / Escape を `on_input` で受け取った時点では**まだフォーカスが
  動いていない**（以前は動いた後だった）。移動後の状態は直後の `on_ui_capture`
  で届く。フォーカス移動後の状態を `on_input` の中で読んでいたコードは要確認。

- **キャレットの点滅を `TextInputState` に寄せた**
  ([#16](https://github.com/Mutafika/sabitori/issues/16))。点滅を
  `FocusManager` / `TextInputState` / `TextInput` が別々に数えていて、どれを見れば
  いいのか決まっていなかった。`TextInputState::tick(dt)` / `cursor_visible()` /
  `caret_byte_offset()` が正になり、`FocusManager` の同名 API は各フィールドへ
  委譲する（`FocusManager` 利用側の書き換えは不要）。

  単一フィールドのアプリは `DeclarativeApp::tick` で `self.name.tick(dt)` を
  呼ぶこと。呼ばないとキャレットが点滅しない（表示はされる）。

- **`ViewContext` にライフタイム引数が付いた** (`ViewContext<'a>`)
  ([#15](https://github.com/Mutafika/sabitori/issues/15))。計測器を借用で持つため。
  `fn view(&self, ctx: &ViewContext) -> Element` はライフタイム省略で通るので、
  **既存の実装は書き換え不要**。`ViewContext` を構造体フィールドに保持している場合
  だけ `ViewContext<'_>` の記述が要る。

- **`.overflow_scroll()` と `.scroll_offset(x, y)` を削除し、`.scroll(id)` と
  `.scroll_manual(x, y)` に分けた**
  ([#14](https://github.com/Mutafika/sabitori/issues/14))。

  スクロールには最初から 2 つのモデル（位置をランタイムが持つ／アプリが持つ）が
  あったのに、データ上は区別が無かった。`ElementStyle` に `scroll_owner:
  ScrollOwner` を足して明示する。

  ```rust
  // 旧
  div().id("rows").flex_1().overflow_scroll().children(rows)
  // 新 — 位置はランタイムが持つ。id はその状態のキー
  div().scroll("rows").flex_1().children(rows)

  // 旧
  div().overflow_scroll().scroll_offset(0.0, self.y).children(rows)
  // 新 — 位置はアプリが持つ。ランタイムは触らない
  div().scroll_manual(0.0, self.y).children(rows)
  ```

  `.scroll(id)` が **id を引数で要求する**のは、それがスクロール状態のキーだから。
  `id` はこの要素の `.id()` そのもので、`on_click` のルーティングと共用になる。

  `.overflow(Overflow::Scroll)` は生の逃げ道として残るが、**ランタイム管理には
  乗らない**（キーが無いので状態を引けない）。スクロールさせたいなら上の 2 つを使う。

### Fixed
- **役割だけを持つ要素がクリックを飲み込んでいた**
  ([#21](https://github.com/Mutafika/sabitori/issues/21) の副作用)。
  `.role()` / `.label()` を持つ要素も `hit_regions` に出すようにした結果、
  **id もハンドラも持たない意味だけの子が手前に居ると、id を持つ親のクリックが
  一切通らなくなっていた**。0.4.0 の宣言版 `table` でセルに `Role::Cell` を
  書き足した瞬間に行が押せなくなり発覚。コンパイルも通るし警告も出ない。

  `HitRegion::is_interactive()` を足し、ポインタを解決する側が意味だけの領域を
  透過するようにした（`hit_region_at` / `wants_pointer` / declarative と scene の
  マウス押下・タッチ押下、計 6 箇所）。

- **`testing::Harness` に時間が無かった**
  ([#19](https://github.com/Mutafika/sabitori/issues/19) の穴)。すべての tick
  （アプリの `tick`、スクロールのばね・慣性、tooltip の遅延、drag、style /
  presence）が `about_to_wait` にベタ書きで、Harness からは 1 つも回せなかった。
  そのため**ばねで動くものはテストすると必ず「動かない」ように見えた** —
  とくに `scroll_intents()` は `smooth_scroll_to`（目標を置くだけ）なので、
  実機では動くのにテストでは 1px も動かない。`AppState::advance(dt)` /
  `is_animating()` に括り出し、ランタイムと Harness が同じ実装を通るようにした。

- **`VirtualList` が可視範囲をウィンドウ高さから計算していた**。`ctx.height` は
  ウィンドウの高さであって、リストを置いた入れ物の高さではない。サイドパネルに
  入れると実際の 3〜4 倍の行を作った上に、スクロールすると下端で行が尽きた。
  `ctx.visible_range(id, item_height)` に寄せた。

- **`TreeView` の開閉が label 一致だった**。木を舐めて最初に label が一致した
  ノードを開閉していたので、同じ名前のノードが 2 つあると**別のノードが開いた**。
  展開位置の添字で辿る形に直した。

- **`sabitori-window` のポインタ `match` が網羅でなかった**
  ([#17](https://github.com/Mutafika/sabitori/issues/17))。`process_event` /
  `inject_event` の `_ => {}` を廃止し、無視する種別も明示的に並べた。#12 で
  `ModifiersChanged` が app へ一度も届かなかったのがこの穴で、当時はマージ前
  レビューでしか捕まらなかった。

- **`EmbeddedRunner::inject_event` が `PointerCancelled` を処理していなかった**
  ([#17](https://github.com/Mutafika/sabitori/issues/17))。`process_event` 側には
  最初からある腕が注入経路には無く、`_ => {}` が隠していた。ホストが cancel を
  注入すると押下ノードが解除されず、**以後ずっと押されたまま**になる。網羅マッチに
  した結果あらわれた実バグ。

- **手動スクロールが事実上存在しなかった**
  ([#14](https://github.com/Mutafika/sabitori/issues/14))。ランタイムは
  `Overflow::Scroll` の要素を**全部**管理対象にし、アプリが渡したオフセットを
  毎フレーム上書きしていた（初回フレームでは `ScrollView` の初期値 0）。id を
  付けなければ避けられる、ということも無く、**id が無ければツリー上の位置から
  合成されて管理対象になった**。

  同梱コンポーネントが 2 つこれを踏んでいた:
  - `sabitori_core::tui::scroll_container` — 呼び出し側から `scroll_y` を受け取る
    API なのに、その値は一度も効いていなかった
  - `examples/tui_gallery.rs` — `ensure_sidebar_visible()` が書く値が毎フレーム
    捨てられていた（完全な死にコード）。管理モード + `scroll_intents` に直した

- **ツリーの形が変わるとスクロール位置が飛んだ**
  ([#14](https://github.com/Mutafika/sabitori/issues/14))。id を省いたときに合成
  していた `__scroll:0.2.1` は子インデックス由来で、**兄弟が 1 つ増減しただけで
  別 id** になった。条件付きレンダリングでヘッダが出入りすると別の状態を引き、
  位置が 0 に戻る。「エラーは出ないのに、ある条件のときだけスクロールが巻き戻る」
  という掴みにくい症状になっていた。`.scroll(id)` が安定した名前を要求するので、
  位置依存の合成そのものを廃止した。

- **`SceneApp` がスクロール処理を逐語コピーで持っていた**
  ([#14](https://github.com/Mutafika/sabitori/issues/14))。`patch_scroll_offsets`
  と `apply_scroll_measures` の複製を削除し、`declarative` と同じ
  `scroll_sync` の関数を呼ぶようにした。片方だけ直す事故の温床だった。

### Added（クリック処理をその場に書く）

- **`Element::click(ctx, id, handler)`** — 押されたときにアプリをどう変えるかを、
  押される要素のところに書く。

  ```rust
  div().click(ctx, "save", |app: &mut App| app.saved = true)
  ```

  従来は `.id("save")` を置いて `DeclarativeApp::on_click` で文字列を突き合わせて
  いた。 id を書く場所と受ける場所が離れていて、 型が繋いでいない:

  ```rust
  fn view(..) { div().id("save") }
  fn on_click(&mut self, id: &str) {
      if id == "sav" { self.saved = true; }   // ← タイプミス
  }
  ```

  **これはコンパイルが通り、 押しても何も起きない。** このラウンドで潰し続けた
  のとまったく同じ形の失敗が、 いちばん中心の経路に残っていた。 `click` なら
  文字列が 1 回しか出てこないので、 **食い違う場所が存在しない**。

  動的な一覧では添字を**捕まえる**。 id から切り出して `parse` する必要が無い:

  ```rust
  div().click(ctx, format!("row-{i}"), move |app: &mut App| app.selected = Some(i))
  ```

  仕組みはテキスト入力と同じ登録方式。 `ViewContext::register_action` に
  `Rc<dyn Fn(&mut dyn Any)>` を積み、 ランタイムがクリック時に降ろして呼ぶ。
  `downcast` は `click` の中だけで、 アプリ側には出てこない。

  **従来の `on_click(id)` はそのまま動く** (こちらが先に走り、 その後で呼ばれる)。
  既存のコードを書き換える必要は無い。 動的に振り分けたい場合の口としても残る。

- `DeclarativeApp` に `'static` 境界が付いた。 ハンドラが `&mut dyn Any` 経由で
  アプリ本体に降りるため。 アプリはランタイムが所有するので実質的な制約は無いが、
  借用を持つアプリ型はコンパイルエラーになる。

### Changed（破壊的・テキスト入力の配線を廃止）

- **`text_input` を `view()` に置くだけで動くようにした。** それが配線の全部。

  0.4.0 の途中まで、 `text_input(ctx, "name", ..)` を置いても**それだけでは
  動かなかった**。 別途 `on_focused_input` / `tick` / `ime_cursor_area` の 3 つを
  実装して橋渡しする必要があり、 忘れると **フォーカスは入って枠も光るのに
  打った文字がどこにも行かない**。 コンパイルは通り、 パニックもせず、 ただ何も
  起きない。 このラウンドが潰してきたのとまったく同じ形の失敗が、 いちばん
  よく使うウィジェットに残っていた。

  ```rust
  // これで全部。 他に書くことは無い。
  fn view(&self, ctx: &ViewContext) -> Element {
      text_input(ctx, "name", &self.name, &TextInputStyle::default_dark())
  }
  ```

  仕組み: `ViewContext` に登録の口 (`register_managed` /
  [`Managed`](https://docs.rs/sabitori-core)) を足し、 `text_input` が組み立て
  のときに自分を登録する。 ランタイムは登録された欄へ**アプリより先に**入力を
  配り、 毎フレーム tick し、 フォーカス状態を反映し、 IME 変換候補の位置も
  算出する。 **書き忘れる場所が存在しない。**

  型消しは core 側 (`Managed` は `as_any` だけ) — 中身は widgets、 イベント型は
  input にあり、 どちらも core に依存しているので、 core が知ると循環するため。

- **`TextInputState` が共有ハンドルになった** (`Rc<RefCell<..>>`)。
  `view(&self)` は不変借用なので、 ランタイムがそこへ書き込むには内部可変性が
  要る。 公開フィールドはアクセサに変わる:

  | 前 | 後 |
  |---|---|
  | `state.text` | `state.text()` / `state.set_text(..)` |
  | `state.focused` | `state.is_focused()` |
  | `state.cursor_pos` | `state.cursor_pos()` |
  | `state.preedit.is_active()` | `state.is_composing()` |
  | `state.on_focused_input(e)` / `on_char` / `on_key` | 不要 (ランタイムが呼ぶ) |

  標準の操作で足りない場合は `with(..)` / `with_mut(..)` で中身
  (`TextInputInner`) を借りられる。 `Clone` は同じ欄を指す (複製しない)。

- **`Harness` に IME 操作を追加**: `ime_preedit(text, cursor)` / `ime_commit(text)` /
  `ime_enabled()`。 日本語入力はこのフレームワークの主用途なのに、 テストから
  変換を再現する手段が無かった。

- **自作のテキスト欄には検出器が残る**。 `Role::TextInput` を名乗る要素に
  フォーカスがあるのに打鍵を誰も消費しなかったら `log::warn!` を 1 回。
  テストからは `Harness::unrouted_text_inputs()` で見える。 `text_input` を
  使う限りこの状況は起きない。

### Changed（破壊的・ウィジェット層）

- **`view()` から使えない retained ウィジェットを削除した**。`new(x, y, w, h)` や
  `new(bounds: Rect)` に画面座標を渡し、`hit_test(point)` で自分で当たり判定をする
  一群。`Element` を返さないので宣言的ツリーには組み込めず、examples・他 crate
  合わせて**使用箇所は 0** だった（grep のヒットは全部コメントと文字列リテラル）。
  それでいて README は「20 widgets」の一部として数えていた。

  | 消えたもの | 代わり |
  |---|---|
  | `Button` / `ButtonStyle` / `ButtonVariant` | `element::button()` |
  | `Card` / `CardStyle` | `div()` + `.bg()` / `.rounded()` / `.shadow_md()` |
  | `Tabs` / `TabStyle` | `forms::segment_control()` |
  | `Dropdown` | `DropdownState` + `DropdownStyle` |
  | `TextInput`（`bounds: Rect` を持つ方） | `text_input()` + `TextInputState` |

  `DropdownStyle` は残る（宣言版 `select` が使う）。定義は `select.rs` へ移した。

- **`Table` / `SplitPane` を宣言版に作り直した**。宣言的な等価物が無かったので
  削除では穴が開く。列幅は taffy、当たり判定は id、スクロールは `.scroll(id)` が
  持つので、widget 側に幾何演算は 1 行も要らない。

  ```rust
  table(ctx, "files", &self.files, &TableStyle::default_dark())
  split_pane(ctx, "main", &self.split, &style, sidebar, editor)
  tree_view(ctx, "tree", &self.tree, &TreeViewStyle::default_dark())
  ```

  `table` / `tree_view` / `virtual_list` は `ctx.visible_range()` で行を仮想化し、
  上下に spacer を積む。10 万行でも作る Element は viewport ぶんだけで、
  スクロール量は実データと一致する。

- **Element を返す入口の命名を統一した**。`view()` / `bar()` / `build()` /
  `render()` / `trigger()` / `to_element()` と 6 通りあり、ウィジェットごとに
  ソースを読まないと分からなかった。**`snake_case` の自由関数で、第 1 引数が
  `&ViewContext`、第 2 引数が `id`** に統一（`core::forms` と同じ形）。

- **`VirtualList::build` から `scroll_y` 引数を落とした**。出どころが doc に
  書かれておらず、素直に書くと 0 のまま動かなかった。位置はランタイムが持つ。

- **`Role` に表と木の役割を追加**: `Table` / `Row` / `Cell` / `ColumnHeader` /
  `Tree` / `TreeItem`。`Cursor` に `ResizeNs`（上下分割の仕切り）。どちらも
  網羅マッチを壊すので、下流で `match` している箇所はコンパイルエラーになる。

- **`core::forms` の 8 コントロールと宣言的ウィジェットに `.role()` / `.label()`
  を付けた**。#21 で仕組みは入れたのに使っていたのは `text_input` だけで、
  ウィジェットで組んだアプリの意味ツリーは実質空だった。

- **`Harness::tick(dt)` / `settle()` / `settle_for(n)` を追加**。ばねで動くものを
  テストするのに要る（上の Fixed 参照）。

### Changed（example と doc）

- **`examples/filer.rs` を runtime 管理スクロールへ移した**。旗艦 example が
  `ScrollView` を自前で持ち、`first = scroll_y / ROW_H` で自前に仮想化し、
  スクロールバーも `mt(top_offset)` も `on_scroll` 転送も `tick` も手書き
  していた——つまり**#14 で直してテストもある `.scroll(id)` ではない方**を
  教えていた。list / grid 両モードを `.scroll(FILE_SCROLL_ID)` へ。
  `Tab` から `ScrollView` フィールドが消え、content_height の手計算も不要に
  なった（レイアウトが測る）。

- **README を全面的に書き直した**。`0.1.0` と書いてあり、`.scroll()` も
  `testing::Harness` も role もクリップボードも——**0.4.0 で足したものが 1 つも
  書かれていなかった**。「よく間違える 4 つ」（スクロール / テキスト入力と IME /
  フォーカスとキーボード / テスト）を追加し、それぞれ正解を 1 つだけ示す形にした。

- **README のコード例をコンパイル検査で固定した**
  (`crates/sabitori/tests/readme_examples.rs`)。API を壊すと落ちるので、README を
  直し忘れてマージすることが無くなる。

- **ウィジェットをランタイム越しに動かすテストを追加した**
  (`crates/sabitori/tests/widgets_through_runtime.rs`)。依存の向きが
  `sabitori → sabitori-widgets` なので widget crate からは `Harness` に手が
  届かず、それまでの widget テストは全部「関数を直接呼んで戻り値を見る」単体
  テストだった。`table` / `tree_view` / `virtual_list` / `tooltip` / `panel` /
  `modal` はテスト 0 件。**この経路を通した瞬間に実バグが 1 件出た**（上の
  「役割だけを持つ要素が…」）。

### 移行

**`.overflow_scroll()` / `.scroll_offset()` を使っている箇所はコンパイルエラーに
なる。** 上の Changed の対応表のとおり書き換える。判断基準は 1 つだけ:

| スクロール位置を持つのは | 書き方 |
|---|---|
| ランタイム（ホイール・慣性・バウンス込み。プログラム的な移動は `scroll_intents`） | `.scroll(id)` |
| アプリ（`on_scroll_xy` を自前で実装し、値を自分で進める） | `.scroll_manual(x, y)` |

迷ったら `.scroll(id)` が正解。`.scroll_manual` は仮想リストのように「行の描画自体を
オフセットから決める」実装向け。

**`form_text_input` を使っている箇所もコンパイルエラーになる。** 引数 11 個を渡す
代わりに、`TextInputState` を持って `text_input(ctx, id, &state, &style)` を呼ぶ。
状態を持っていない場合は `TextInputState::new(placeholder)` を作り、
`DeclarativeApp::on_focused_input` で `state.on_focused_input(ev)` に流す。

⚠️ `.overflow_scroll()` を消すと rustc は `.overflow` を候補に挙げるが、**それは
違う**（管理対象にならない生の逃げ道）。`.overflow()` の doc に誘導を入れてあるが、
補完に釣られないよう注意。

`sabitori::*` の glob に `Delivery` / `InputEventKind` / `ScrollOwner` が増えるので、
下流に同名の型があれば衝突する（その場合は明示 import で回避）。

**retained ウィジェットを使っている箇所はコンパイルエラーになる。** 上の対応表の
とおり置き換える。いずれも `Element` を返さない型だったので、`view()` の中に
書いていたということは無いはず（書けなかった）。自前で矩形を計算して描いていた
場合は、宣言版に寄せると幾何計算がまるごと不要になる。

**`Role` と `Cursor` に variant が増えた。** 下流で網羅 `match` している箇所は
腕を足すこと。`_ =>` で受けていれば影響なし。

**テキスト欄の手動配線は削除する。** `on_focused_input` / `tick` /
`ime_cursor_area` に書いていたテキスト欄への橋渡しは不要になった。 残していても
二重処理にはならない (登録済みの欄はランタイムが先に消費する) が、 死にコード
なので消してよい。 `state.text` などの公開フィールドはコンパイルエラーになるので、
上の対応表のとおりアクセサに置き換える。

**`Harness` でばねを使う挙動をテストしている場合、`settle()` を足す。**
`frame()` は時間を進めない（意図的にそうしてある——テストは決定的であるべきなので）。
`scroll_intents` や慣性スクロールを見るテストは `h.frame(); h.settle();` の形にする。

## [0.3.21] - 2026-08-12

修飾キーの変化を観測できるようにした版。「⇧を押している間だけ」効かせる操作
（直交スナップ、比率固定、追加選択）が書けるようになる。

### Added
- **`InputEvent::ModifiersChanged(Modifiers)`**
  ([#12](https://github.com/Mutafika/sabitori/issues/12))。修飾キーの状態が変わった時に
  **変化後**の値が届く。ポインタが止まっていても届くので、「⇧を押した瞬間にゴム紐を
  直交へ折る」のような、動きを伴わない切り替えもこれで書ける。3 ランタイム
  （`DeclarativeApp` / `SceneApp` / `SabitoriApp`）すべてから配られる。

- **`InputEvent::PointerMoved` に `modifiers` が載った**
  ([#12](https://github.com/Mutafika/sabitori/issues/12))。`PointerPressed` /
  `PointerReleased` と揃えた。動いている最中の状態が取れないと、ゴム紐が追従している
  間に効かない。

### Fixed
- **`DeclarativeApp` / `SceneApp` がマウス移動で `PointerMoved` を出していなかった。**
  `InputEvent::PointerMoved` の doc は "For mouse, fires for both hover and drag" と
  言っているのに、この 2 ランタイムは**タッチ分しか出していなかった**（`SabitoriApp`
  は出していた）。マウスの移動でも出すようにした。

  これが無いと、上の `modifiers` を `PointerMoved` に載せてもマウス操作では一度も
  届かない — 仕組みだけ足しても症状が消えない形だった。

### Changed
- **`InputEvent::KeyInput.modifiers` は修飾キー自身のイベントでは「変化前」の値**
  である旨を doc に明記した。macOS の winit は `flagsChanged:` で `KeyboardInput` を
  先に、`ModifiersChanged` を後に積む（`platform_impl/macos/view.rs` の
  `update_modifiers` で確認）ので、⇧の押下イベントは `shift: false` を、解放イベントは
  `shift: true` を載せて届く。**挙動は変えていない** — 直すには winit のキュー順に
  手を入れる必要があり、`ModifiersChanged` を見る方が素直なため。

  修飾キー**以外**のキーでは正しい値が載る（マウスイベントも同様に正しい。winit が
  `update_modifiers` を先に呼ぶため）。

  ⚠️ **破壊的変更**: `PointerMoved` に `modifiers` が増え、`InputEvent` に variant が
  1 つ増えた。構築側と、全 variant / 全フィールドを列挙している `match` は修正が要る
  （`..` や `_ =>` で受けているなら無傷）。

## [0.3.20] - 2026-08-11

macOS で非アクティブ窓の 1 クリック目が届くようになった版。

### Fixed
- **macOS で非アクティブな窓への最初のクリックが content に届かない**。winit の
  既定が `acceptsFirstMouse = false` なので、他アプリ（Finder 等）から戻った
  1 クリック目が「窓を前面に出すだけ」で吸われていた。ダッシュボード系は他窓と
  行き来しながら操作するため、「押しても効かない」として実害が出る。
  `run_declarative` の窓生成で `WindowAttributesExtMacOS::with_accepts_first_mouse(true)`
  を渡すようにした。

## [0.3.19] - 2026-08-11

Windows でリンクが通るようになった版。

### Fixed
- **Windows (MSVC) で `sabitori-widgets` を含む実行ファイルがリンクできない**
  ([#11](https://github.com/Mutafika/sabitori/pull/11))。

  ```
  error LNK2019: unresolved external symbol localtime_s
    referenced in function sabitori_widgets::file_browser::format_modified
  ```

  `file_browser::local_utc_offset` の Windows 分岐が `time` / `localtime_s` /
  `_mkgmtime` を `extern "C"` で名指していた。**この 3 つは UCRT が輸出している
  関数ではない** — `<time.h>` の中で `__inline` ラッパとして定義されていて、
  `_USE_32BIT_TIME_T` に応じて 32/64bit 版へ振り分けているだけ。C から呼ぶと
  ラッパが呼び出し側に展開されるので通るが、Rust から名指すとリンカが一度も
  生成されていないシンボルを探しに行って落ちる。

  輸出されている `_time64` / `_localtime64_s` / `_mkgmtime64` を名指すようにした。
  接尾辞を明示したので、`time_t` が下で使っている `i64` に固定される副次効果もある。

  **リンク時にしか出ない**ので Mac と Linux のビルドは緑のまま通る。そのため、
  Windows 分岐の綴りをソースから検査するテストを足してある（どのホストでも走る）。

## [0.3.18] - 2026-08-10

tooltip がカーソルと重ならなくなり、窓の外へ伸びなくなった版。

### Fixed
- **tooltip がカーソルの矢印と重なって文頭が読めない**
  ([#9](https://github.com/Mutafika/sabitori/issues/9))。`tooltip_popup` の位置が
  `y + 14` / `x + 0` で、矢印はホットスポットが先端（左上）で本体が右下へ伸びるため、
  14pt では矢印が箱の上辺に乗る。しかも横のずらしが 0 なので、`p_px(8)` の内側にある
  **文頭がちょうど矢印の真下**に来ていた。いちばん読みたい所が読めない。

  カーソルの当たり判定ぶん `(14, 20)` 逃がして右下に出すようにした（各 OS の慣行）。
  doc コメントは "appears above the cursor" と書いてあって実装と食い違っていたので、
  実装（下）に合わせて書き直した。上に出すには箱の高さが要るが、`tooltip_popup` は
  高さを計算していなかったため。

- **窓の端で tooltip が画面外へ伸びる**
  ([#9](https://github.com/Mutafika/sabitori/issues/9))。`est_w` は最大 360pt あるのに
  位置にクランプが無く、右端の要素にホバーすると箱が窓の外へ出ていた（下端も同じ）。

  はみ出す側では位置を返すようにした — 右が足りなければカーソルの左へ、下が
  足りなければカーソルの**上**へ回し、それでも入らなければ縁で止める。高さは幅と
  同じく内容から推定する（多め側に倒してあるので、返す判断は早めに出る）。

  ⚠️ **破壊的変更**: `tooltip_popup` に `viewport_w` / `viewport_h` が増えた
  （`text, x, y, viewport_w, viewport_h, bg, text_color, border`）。「はみ出すか」を
  知っているのは窓の寸法を持つ側だけなので、消費側では直せない類の変更。
  runtime 経由（`.tooltip()`）で使っているなら影響は無い。

## [0.3.17] - 2026-08-10

コントラストを toolkit 側で計算できるようにした版。「この地の上でこの色は読めるか」に
`Color` が答えられるようになり、消費側が手書きで持っていた実測の対応表が要らなくなる。

### Added
- **`Color::luminance()` / `contrast_ratio()` / `over()` / `readable_on()`**
  ([#7](https://github.com/Mutafika/sabitori/issues/7))。WCAG 2.x の相対輝度と
  コントラスト比、source-over 合成、そして「地の上で基準を満たすまで最小限だけ寄せた色」。

  ```rust
  let fg = accent.readable_on(theme.bg, 4.5);          // 足りていれば素通し
  let on_paper = veil.with_alpha(0.85).over(paper);    // 下に別の絵がある時
  assert!(on_paper.contrast_ratio(paper) >= 4.5);
  ```

  ⚠️ **`Color` は linear 保持なので、輝度にガンマ戻しは要らない。** hex から手で
  計算するコードが `((c+0.055)/1.055).powf(2.4)` を挟むのは出発点が sRGB 符号化値
  だからで、`Color` に対して同じことをすると二重に戻る（`#808080` の輝度が 0.216 では
  なく 0.041 になる）。移植する時はここを落とすこと。

  成分は `[0, 1]` に丸めてから計算する。バネ補間は行き過ぎるので、アニメーション中の
  色は一時的に範囲外の成分を持ちうる。画面に出るのは丸めた後の色なので、比もそちらに
  合わせる（でないと 21 を超える）。

  ⚠️ **`over()` は線形空間で合成する。** サーフェスが sRGB でハードウェアが
  デコード/エンコードするため、ブレンドは線形値の上で起きる — つまりこの計算が
  実際に画面へ出る色になる。**hex を 0–255 のまま混ぜた手計算とは違う値が出る**:

  | 混ぜ方 | 黒を α0.5 で白に | `#1a1b26` を α0.85 で白に |
  |---|---|---|
  | sRGB 空間（手計算） | `#808080` / 3.98:1 | `#3c3d47` / 10.74:1 |
  | 線形空間（実際の描画） | `#BCBCBC` / 1.91:1 | `#6f6f71` / **5.01:1** |

  濃く敷いた地ほど差が大きい。手計算で「十分暗い」と見積もった覆いが、実際には
  その半分以下しか効いていないことがある。

### Fixed
- **プリセットのコントラスト検査を追加した**（`AppTheme` の 5 つすべて）。
  主文字はどのプリセットのどの面の上でも本文コントラスト (4.5) を満たすことを保証する。

  検査を入れて分かった**既知の弱点**: 選択行の上の副文字は、5 つ中 4 つで本文
  コントラストに届かない（midnight 3.86 / tokyo_night 3.68 / catppuccin 3.35 /
  dracula 3.83、nord のみ 5.46）。パレットを動かすとアプリの見た目が変わるので、
  今回は UI 部品の下限 (3.0) を割らないことの保証に留めてある。

## [0.3.16] - 2026-08-10

v0.3.15 で入れた `scale` の取りこぼしを 2 件塞いだ版。どちらも「画面 px と
レイアウト px を混ぜた」形の取り違えで、scale が 1.0 のままなら踏まない。

### Fixed
- **scale されたコンテナのクリップ枠が余計に食い込む。** クリップの content box を
  出すとき、`rect` は画面 px なのに padding を**素の px のまま引いて**いた。
  0.5 に縮んだ 100px・padding 10px のコンテナなら、正しくは `50 - 5*2 = 40` の枠が
  `50 - 10*2 = 30` になり、中身が余計に欠ける。padding も scale してから引くようにした。

- **scale 下でスクロールのクランプが壊れる。** `ScrollMeasure` の `content_*` は
  taffy のレイアウト px、`scroll_y` もレイアウト px なのに、`viewport_*` だけを
  画面 px で報告していた。縮んだパネルの中のスクロールが「まだ余地があるのに止まる」
  形になる。viewport もレイアウト px に揃えた。

  どちらも `.scaled()` か、hover / press の scale（**v0.3.15 から `button()` が既定で
  持つ**）が効いている要素の中でだけ起きる。

## [0.3.15] - 2026-08-10

押下の手応えが出る版。`.active()` / `.pressable()` が `DeclarativeApp` の上で
黙って無視されていたのを直し、その過程で「誰にも読まれていなかった」`scale` を
描画経路に通した。

### Fixed
- **`.active()` / `.pressable()` が `DeclarativeApp` で効かない**
  ([#3](https://github.com/Mutafika/sabitori/issues/3))。runtime に押されている要素を
  追う状態が無く、`Element::active_style` を読む所が declarative 経路に一つも
  無かった。コンパイルは通り hover は効くので、**押下だけが黙って無視される**
  — 消費側からは「書き方を間違えた」のか「効かない」のか区別が付かなかった。

  `pressed_id` を持ち、押下でカーソル下の要素を覚え、解放・キャンセル・ウィンドウ外
  への離脱で消す。`active_style` は hover の**後**に畳むので押下が hover に勝つ
  （`NodeStyle::effective_style` と同じ規約）。タッチでも同じく効く。

- **`StateStyle::scale` が誰にも読まれていなかった**
  ([#3](https://github.com/Mutafika/sabitori/issues/3))。`ElementStyle` に `scale`
  フィールドが無く、declarative でも scene でも参照されていなかったため、
  `.hover(|s| s.scale(1.1))` も `.active(|s| s.scale(0.95))` も無反応だった。

  つまり `pressable()`（中身が `scale` + hover bg）は **cursor と hover の bg しか
  効いていなかった**。押下状態を追うだけでは issue の例は動かないままなので、
  併せて塞いだ。

- **`apply_hover_styles` が `StateStyle` の半分を落としていた**。`scale` /
  `translate_x` / `translate_y` / `gap` / `width` / `height` / `padding` が畳まれず、
  15 フィールド中 7 つが declarative 経路では死んでいた。全フィールドを畳むようにした。
  レイアウトを変えるフィールドも扱えるのは、畳みが build の前に走るため。

### Added
- **`ElementStyle::scale`（と `Element::scaled()`）** — 要素の**中心**を軸にした
  視覚のみの拡大縮小。CSS の `transform: scale()` と同じく**レイアウトはやり直さない**
  ので、押されたボタンが縮んでも隣の行は動かない。

  subtree 全部に乗る（子の位置・寸法、文字サイズ、角丸、線幅、影、polyline の点）。
  **hit region も一緒に変換される**ので、見えている場所と押せる場所がずれない。
  `opacity` と同じく乗算で継承する。

### Changed
- **`button()` が既定で押し込みの手応えを持つようになった。** hover で 1.02、
  押下で 0.96 に scale する。色に触らないのは、正しい hover 色がパレット依存で、
  `.accent()` を付けたボタンと決め打ちの色が喧嘩するため。`.hover()` / `.active()`
  を書けば丸ごと上書きされる。

  ⚠️ **既存アプリの button の見た目が変わる**（今まで無反応だったものが動く）。

- `hover` / `active` の畳み込みを `sabitori_core::element::apply_state_styles` に
  一本化した。`DeclarativeApp` と `SceneApp` が同じ関数を呼ぶので、どちらの
  ランタイムで動かしても状態解決が一致する（`SceneApp` 側は "ported verbatim" の
  複製を持っていて、放っておくと片方だけ直って乖離する形だった）。
  呼び出し元ゼロで `scale` を落としていた `StateStyle::apply_to` も同じ実体に寄せた。

- `StyleAnimator::update` が `pressed_id` を受け取るようになった。transitions を
  持つ要素（`button()` は既定で持つ）でも `active_style` が効く。
  ⚠️ **破壊的変更**: 引数が 1 つ増えている。

## [0.3.14] - 2026-08-10

修飾キーつきのポインタ操作（⇧+クリック = 選択に足す／外す、⌥+ドラッグ = 複製）が
`DeclarativeApp` の上で書けるようになった版。値は runtime が既に握っていたのに、
façade が落としていた 2 箇所を塞ぐ。

同じ組織の別アプリ（bamiri）は winit のイベントループを自前で持っているので同じことが
できていた。差は「情報が無い」ことではなく「`DeclarativeApp` に乗ると届かない」ことだった。

### Fixed
- **キーの解放がアプリに届かない**
  ([#1](https://github.com/Mutafika/sabitori/issues/1))。`DeclarativeApp` の
  `WindowEvent::KeyboardInput` が `ElementState::Pressed` で絞っていたため、
  `InputEvent::KeyInput { pressed: false, .. }` が一度も発行されていなかった。

  ⇧単独の押下は `Key::Shift` として届くのに離したことが分からないので、アプリ側で
  「押しっぱなし」を保持すると**二度と落ちない**。⇧を押している間だけオルソ、のような
  モードが成立しなかった。押下・解放の両方を転送するようにした。

  副作用（コピー・選択解除・フォーカス移動・文字入力）は従来どおり**押下でだけ**走る。
  解放でも走らせると、⇧を離しただけで選択が消える別のバグになる。

### Added
- **`InputEvent::PointerPressed` / `PointerReleased` に `modifiers` が載った**
  ([#1](https://github.com/Mutafika/sabitori/issues/1))。押した**瞬間**に握られていた
  修飾キーが読める。

  ```rust
  InputEvent::PointerPressed { position, modifiers, .. } => {
      if modifiers.shift { self.toggle_at(*position) } else { self.select_at(*position) }
  }
  ```

  押下時の状態が無いと、アプリは `KeyInput` を自前で追って状態機械を持つしかなく、
  それは上の解放イベントが来て初めて成立する。ポインタ操作にはこちらが素直。

  `PointerReleased` の値は押下時と違いうる（押してから⇧を足す／離す）ので、
  `PointerPressed` の値を使い回さないこと。3 ランタイム（`DeclarativeApp` /
  `SceneApp` / `SabitoriApp`）とも、既に保持している修飾キー状態から埋めている。

  ⚠️ **破壊的変更**: この 2 variant を**構築**しているコード、および全フィールドを
  列挙して `match` しているコードは修正が要る（`..` で受けているなら無傷）。

### Changed
- `DeclarativeApp` のキーボード処理を `AppState::handle_key_input` に切り出した。
  winit 依存が `WindowEvent::KeyboardInput` の数行だけになり、押下／解放の
  ルーティングをヘッドレスのテストで検査できるようになった。

## [0.3.13] - 2026-08-07

**下流が v0.3.12 に対してビルドできない**のを直した緊急版。`0.2` 系では通っていた
コードが `0.3` 系で通らなくなっていた。

`sabitori` のファサード re-export から `sabitori-input` の公開項目が半分ほど漏れており、
ポインタ入力をファサード経由で扱えない状態だった。`sabitori-input` を直接依存に足せば
回避できるが、それはファサードを持つ意味を失う。

### Fixed
- **`sabitori::PointerKind` 等が re-export から漏れていて、ポインタ入力が
  ファサード経由で扱えない** ([#74](https://github.com/Mutafika/sabitori/pull/74))。

  `PointerKind` は `InputEvent::PointerMoved` / `PointerPressed` / `PointerReleased` /
  `PointerCancelled` の**必須フィールドの型**。`InputEvent` 自体は re-export されて
  いるので**イベントは見えているのに構築も match もできない**、という形の漏れだった。

  並べたところ同じ漏れが他にもあったので、`sabitori-input` の公開項目を全部出した:

  | 項目 | 無いと何が困るか |
  |---|---|
  | `PointerKind` | ポインタ系イベント 4 種を構築・match できない |
  | `ActivePointer` | `PointerState::find()` の戻り値、`upsert()` の引数を名前で書けない |
  | `PointerId` | 上記の `id` フィールドの型 |
  | `BUTTON_PRIMARY` / `BUTTON_SECONDARY` / `BUTTON_MIDDLE` | `ActivePointer::buttons` のビットマスクを判定できない |
  | `MOUSE_POINTER_ID` | マウスとタッチを id で区別できない |

  `PointerState` は re-export 済みだったが、そのメソッドの入出力型が軒並み書けないため
  **ファサード越しでは実質使えない状態**だった。

### Added
- **`crates/sabitori/tests/facade.rs`** ([#74](https://github.com/Mutafika/sabitori/pull/74))
  — re-export の漏れをコンパイル時に検出する integration test。

  **integration test であることが要点。** crate の外から `use sabitori::…` するので、
  下流と全く同じ解決経路を通る。`#[cfg(test)] mod tests` では `sabitori_input::` に直接
  触れてしまい、この種の漏れは原理的に検出できない。

  この漏れがリリースまで残った理由も同じで、`sabitori` は 13 クレートを**ワークスペース
  内から直接参照**するため、`scene_app.rs` は `use sabitori_input::PointerKind` と書けて
  しまう。**ファサードに何が出ているかはワークスペースのビルドで一切検証されておらず、
  下流がリンクするまで誰も踏まない。** 項目を足したらこのテストにも足すこと。

## [0.3.12] - 2026-08-07

**要素が「警告も出さずに消える」バグを 3 つまとめて潰した版**と、Windows でリンクできない
問題の修正。

前者は全部同じ根を持っていた。`SceneApp` / サブウィンドウの描画経路が、declarative 本体の
描画順（images → rings → polylines → glyphs）を**それぞれ手書きで複製**していて、複製が
7 箇所まで増えた結果、新しい描画種別を足すたびに「どこか 1 つ書き忘れる」が起きていた。
書き忘れても**コンパイルは通り、警告も例外も出ず、ただ描かれない**ので、利用側からは
「そのプラットフォームでは image が使えないらしい」としか見えない。

今回は 3 件を個別に直すのではなく、**描画順とパイプラインの網羅を関数 1 つに集約**して
根ごと畳んだ。以後この種のバグは構造的に起きない。

### Fixed
- **`SceneApp` で `image()` / `ring()` / `polyline()` が描画されない**
  ([#72](https://github.com/Mutafika/sabitori/pull/72),
  [#73](https://github.com/Mutafika/sabitori/pull/73))。`run_scene` の描画経路には
  `ImageRenderer` も `RingRenderer` も `LineRenderer` も**そもそも存在せず**、該当要素は
  render list に積まれた時点で誰にも読まれずに捨てられていた。3 種とも配線した。

  テクスチャのアップロードは `queue.write_texture` 経由なので、render pass の encode 中に
  呼んでも次の `submit` より前に必ず反映される。描画はパスごと・種別ごとに 1 回だけ
  発行する — `queue.write_buffer` は submit ごとに 1 度しか効かないため、バッチ単位で
  ループして描くと共有の instance buffer を後続の書き込みが潰してしまう。

- **サブウィンドウの 2D 経路で `polyline()` が描画されない**
  ([#73](https://github.com/Mutafika/sabitori/pull/73))。extra window の 3D 分岐には
  `render_lines` があるのに 2D 分岐だけ抜けていた。上記の集約で解消。

- **Windows でリンクが通らない**
  ([#71](https://github.com/Mutafika/sabitori/pull/71))。`sabitori-widgets` の
  `local_utc_offset()` が `localtime_r` と `tm_gmtoff` を直接 extern していた。どちらも
  glibc/BSD の拡張で Windows の CRT には無く、`struct tm` に `tm_gmtoff` フィールドも
  無い。**`cargo check` では気づけず、リンクまで進んで初めて `undefined symbol` で落ちる。**

  Windows では `localtime_s` で local の壁時計を取り、`_mkgmtime` で UTC として解釈し直した
  値と実 timestamp の差を取る方式にした（DST は `localtime_s` が現在時刻に対して判定する
  ので織り込み済み）。unix 側は**挙動不変**。

  `_mkgmtime` に渡す前に `tm_isdst` を 0 にしている。`localtime_s` は DST 中にこれを立てるが
  `_mkgmtime` 側の扱いが仕様上はっきりしないため、無視される実装では no-op、そうでなければ
  1 時間の誤補正を防ぐ。**DST のある地域でしか顕在化しない**ので、日本国内のテストでは
  絶対に踏めない類のずれ。

- **32bit unix で更新日時が壊れる**
  ([#73](https://github.com/Mutafika/sabitori/pull/73))。`tm_gmtoff` を `i64` 決め打ちで
  宣言していたが、glibc/BSD の実体は `long` で 32bit ターゲットでは 4 バイト。構造体の
  後続フィールドがずれて別物を読んでいた。`std::ffi::c_long` に変更（64bit では同一の
  ため挙動不変）。

### Changed
- **描画レイヤの共通化** ([#73](https://github.com/Mutafika/sabitori/pull/73))。
  `sabitori::bridge` に `UiDrawLists` / `UiRenderers` / `draw_ui_layer` を追加し、
  rect レイヤより上に載る全要素の抽出と描画をここに集約した。`declarative` 5 箇所 +
  `scene_app` 2 箇所の複製を置き換え済みで、**描画種別を足すときに触るのはこの 1 箇所だけ**。

- **`sabitori_gpu::wgpu` の re-export** ([#73](https://github.com/Mutafika/sabitori/pull/73))。
  下流クレートが自前の `wgpu` 依存を足さずに wgpu 型をシグネチャに書けるようにする。
  依存を二重に持つと別バージョンに解決されて型が unify しなくなるため。

### Known limitations
- `ring()` / `polyline()` / `image()` が `SceneApp` で実際に描画されることの**目視確認は
  取れていない**。同梱の `scene_ui` example がこれらの要素を使っておらず、GPU 描画の
  自動テストも無いため、検証はビルド・型・起動スモークまで。

## [0.3.11] - 2026-08-04

テキスト選択が「操作の副作用として勝手に起きる」のを止めた版。
本文の無い業務 UI で、パネルを掴んで動かすたびにラベルが青く染まっていたのが消える。

### Fixed
- **文字の上でなくてもテキストを掴んでしまう問題**
  ([#68](https://github.com/Mutafika/sabitori/issues/68))。`hit_test_text` に距離の
  足切りが無く、全テキストをスコアして**グローバル最小のものを必ず返して**いた。
  clip されていないラベルは画面のどこを押しても候補に残るので、文字の無いキャンバスを
  押した時点で遠くのラベルに anchor が立ち、そのままドラッグすると anchor〜head の
  間のテキストが全部選択されて画面が端から端まで染まっていた。

  mouse_down では、**文字か、その行ボックス＋わずかな許容に実際に当たった時だけ**
  選択を始めるようにした。許容は縦が行高の半分（行間の中点まで）、横が行高 1 つぶん
  （行末の余白クリックを行末 caret に snap させるのに要る幅）。

  ドラッグ中の head 更新は従来どおり最近傍 snap のまま。anchor が既に実テキスト上に
  立っている以上、段落の外へ払っても選択が伸び続けるのが期待値で（ブラウザも同じ）、
  ここまで厳密にすると余白を通るドラッグで選択が途切れる。**選択の開始だけが厳密。**

### Added
- **`Element::no_select()`** — CSS の `user-select: none` 相当
  ([#67](https://github.com/Mutafika/sabitori/issues/67))。付けた要素と**その subtree
  全部**が選択の対象外になる（anchor/head にならない・選択背景を塗らない・
  clipboard 抽出でも飛ばす）。継承するので、パネルの根に 1 回書けば中は全部効く。

  ```rust
  div().no_select().children([...])   // サイドバー・ツールバー・見出し
  div().children([text(body)])        // 本文は選択できたまま
  ```

  「散文は選択させたいが chrome は選択させたくない」が同じアプリの中に普通にあるので、
  切り分けは要素側に置いた。ビューア系なら本文だけ選択可、ツールバー・サイドバーは
  不可、が既定として自然。

- **`DeclarativeApp::text_selection_enabled()`**（既定 `true`）。`false` を返すと
  アプリ全体で選択が起きなくなる。UI がほぼ全部 chrome で、選択が事故でしか起きない
  アプリ（ダッシュボード・ビューア・caret を自前で描くエディタ）向けの雑なスイッチ。
  混在するなら `no_select()` の方を使う。

  ⚠️ 既存の `selection_style()` は色だけを決める口で、選択そのものは止められない
  （`Some((bg, fg))` を返すと選択中の glyph が一律 `fg` に塗り替わるので、
  「背景 alpha=0 で見えなくする」は成立しない）。止めたい時はこちらを使う。

### Changed
- **button のラベルが選択できなくなった。** コントロールのキャプションであって本文では
  ないので、`no_select()` の指定に関係なく常に非選択。ツールバーを横断ドラッグしても
  ボタン文字がハイライトされない。

  ⚠️ **破壊的変更**: `TextDraw` を直接構築しているコードは `no_select: bool` の追加が
  要る（`false` = 従来どおり選択可能）。`Element` を直接構築している場合も同様。
  builder 経由なら無傷。

## [0.3.10] - 2026-08-02

1 つのテキストの中を 2 系統の色で塗り分けられるようにした版。
新旧対照表の見え消し（削除=赤地・追加=緑地が 1 文の中に交互に来る）が、
テキストを 1 要素に保ったまま書けるようになる。

### Changed
- **`highlight()` が上書きでなく追加になった**
  ([#64](https://github.com/Mutafika/sabitori/issues/64))。`ElementStyle.highlight` と
  `TextDraw.highlight` が `Option<HighlightSpec>` から `Vec<HighlightSpec>` になり、
  `Element::highlight()` は push する。**1 回だけ呼ぶ既存コードは挙動不変。**

  1 つの `HighlightSpec` が塗れるのは「全範囲 1 色 + `current` の 1 範囲だけ別色」で、
  find-in-page にはこれで足りていた。足りないのが**新旧対照表の見え消し**で、
  赤地 N 箇所・緑地 M 箇所が 1 つの文の中に交互に来る。spec を 2 つ重ねれば書ける:

  ```rust
  text(body)
      .highlight(HighlightSpec { ranges: deleted,  color: del_bg, ..Default::default() })
      .highlight(HighlightSpec { ranges: inserted, color: add_bg, ..Default::default() })
  ```

  spec は**呼んだ順に塗られる**ので、範囲が重なれば後の spec が上に載る。
  find-in-page と diff 塗りを同居させられる（対照表の中を検索する、が書ける）。

  回避策だった「片ごとに要素を分けて `wrap()` の flex 行に並べる」が不要になる。
  あれは片の境目でしか行が折れないので右端が不揃いになり、実データでは 1 文が
  最多 589 個の flex item に割れていた。テキストが 1 要素のままなら、折り返しも
  CJK シェーピングも素直に効く。

  ⚠️ **破壊的変更**: `ElementStyle` / `TextDraw` を直接構築しているコードは
  `highlight: None` → `highlight: Vec::new()` の修正が要る。builder 経由なら無傷。

## [0.3.9] - 2026-08-02

`overflow_scroll()` が効かない問題を、レイアウトの根から直した版。
縦（スクロール）と横（テキストの折り返し）で 2 度出ていた同じ罠が消える。

### Fixed
- **`overflow_scroll()` が効かない問題**
  ([#60](https://github.com/Mutafika/sabitori/issues/60))。コンテナの min-size を
  CSS の `min-*: auto` ではなく **`0`** に解決するようにした。

  `auto` の下では flex item が content より小さくなれないため、`grow(1.0)` の row が
  content 高さまで膨らみ、その中で高さを決める子（`h_full()` や `overflow_scroll` の
  pane）が膨らんだ数字に対して解決される。結果 pane の viewport が content と等しくなり、
  **切り取るものが無くなってスクロールだけが静かに死ぬ**（レイアウトは正しく見えるので
  気づけない）。スクロールする箱自身に `min_h(0)` を足しても直らず、膨らんだ祖先を
  探して置く必要があった。

  横でも同じ根で、flex row 内のテキストが折り返さない（row が自然幅より縮めない）。
  こちらは `min_w(0)` を祖先に置く回避策が要っていた。**どちらも回避策が不要になる。**

  `min-*: auto` はブラウザがテキストを潰さないための配慮だが、sabitori はアプリの
  ツールキットで、テキスト要素は自前の intrinsic minimum を持つ（`Text` / `Button`）。
  よってコンテナ側の `auto` は利点が無く罠だけが残っていた。テキストが min-content
  未満に潰れない性質は変わらない。

  ⚠️ 明示的な `min_w` / `min_h` は従来どおり優先される。影響を受けるのは
  「Auto サイズで縮みうるコンテナ」だけで、明示幅・`shrink(0)`・`Text` / `Button` は
  変わらない。

## [0.3.8] - 2026-08-01

> リリース当時に記載を落としていたものを後追いで復元した。タグ `v0.3.8` は
> lightweight のまま残してある（`Cargo.toml` の version も当時 `0.3.7` のまま）。

### Added
- **マウスの押下 / 解放が `on_input` に届くようになった**
  ([#62](https://github.com/Mutafika/sabitori/issues/62))。タッチと同様、左 / 中
  ボタンの押下・解放を `InputEvent::Pointer*` としてアプリへ転送する。CAD 系
  キャンバスのドラッグパンが押下状態を観測できるようになる。既存のクリック /
  フォーカス / 選択の配線は不変（転送は追加のみで、`on_input` の戻り値も見ない）。

## [0.3.7] - 2026-07-29

`run_declarative` に `on_build` を配線し、**画面外の要素の位置**を引ける probe を足した版。
長い一覧の N 行目を先頭に持ってくる（scroll-to-element）が、利用側で書けるようになる。

### Added
- **`DeclarativeApp::on_build` が `run_declarative` からも呼ばれるようになった**
  ([#57](https://github.com/Mutafika/sabitori/issues/57))。trait には前からあったが、
  実際に呼んでいたのは `run_scene` だけで、declarative アプリは `BuildResult` に
  一切触れなかった。**保存と通知を `commit_build` 1 操作に畳んだ**ので、
  「`last_build` に入れたのに app に渡し忘れる」形自体が作れなくなった。
- **`DeclarativeApp::build_probes` と `BuildResult::probe_positions`** — 申告した id の
  位置を、**画面外にあっても**返す。`hit_regions` は可視要素しか持たない（親の clip と
  交差しない要素はゼロ矩形になって捨てられる）ため、「400 行目はどこか」に答えられず
  **scroll-to-element が書けなかった**。レイアウトは culling と無関係に全要素の位置を
  知っているので、それを表に出しただけ。申告が空なら分岐ごとスキップ＝コストゼロ。
  - 記録は **clip 判定より前**に行う。clip が消す位置を答えるのが目的なので、後ろでは遅い。
  - cull 地点（スクロール範囲外の直下の子・ゼロ面積 clip）でも `record_probes` で
    位置だけ拾う。ここを落とすと「スクロールコンテナの直接の子」だけ位置が取れない、
    という気づきにくい穴が残る（追加したテストが実際にこれを捕まえた）。
- **ヘッドレスなフレーム harness**（`AppState::new` / `build_frame`）。ウィンドウも GPU も
  無しで 1 フレーム回せる。native/wasm で重複していた初期化子もこれに集約した。
  テキスト計測は決定論的な stub を使う（実シェーピングはマシンのフォント解決に依存し、
  期待 rect が環境依存になるため）。

### Changed
- 非破壊: `build_tree_probed` / `build_tree_measured_probed` を追加。既存の
  `build_tree` / `build_tree_measured` は空の probe 集合で委譲するだけなので挙動不変。

## [0.3.6] - 2026-07-28

テキスト計測をヘッドレスで呼べるようにし、**ベースライン**を返すようにした版。
CAD/DXF のようにベースライン基準で座標を持つ利用側が、画面と同じ数字を
1 か所から取れる。公開 API に破壊的変更あり（下記 Changed）。

### Added
- **`TextShaper` — GPU 非依存のテキスト計測**
  ([#54](https://github.com/Mutafika/sabitori/issues/54))。フォントスタックと、
  GPU 無しで答えられること全部を持つ公開型。`TextRenderer` はこれを内包して委譲するので、
  **画面と同じフェイス・ロケール・量子化**で計測できる。
  `TextRenderer::new` は `&wgpu::Device` を要求するため、DXF 取込・紙面レイアウト・
  PDF 書き出しのようなヘッドレス処理からは計測を**呼べなかった**。
  - **ロケール正規化がコンストラクタに入った。** cosmic-text の han-unification は
    ロケール文字列を完全一致で見るため、`ja-JP` は既定の PingFang SC（簡体字）に落ち、
    漢字だけ中国語フェイス・かなは Hiragino という分裂が起きる。自前で `FontSystem` を
    持つ利用側がこの正規化を写し忘れて実際にこれを踏んでいた。今は 1 か所にある。
- **`TextMetrics`** — 計測結果に**ベースライン**（箱の上端 → 1 行目のベースライン）が付いた。
  sabitori は**行ボックス**（既定 1.4em）の上端を要素位置に置くのに対し、DXF/CAD は
  「TOP = ベースラインの 1.0em 上」と定義するため、両者を突き合わせる術が無かった。
  - **ベースラインは定数ではない。** 解決されたフェイスに従うので、100px 実測で
    `"室名"` が **108.0**、`"R-101"` が **104.7** と**同じサイズでも違う**（CJK と Latin で
    ascent が違う）。単一のオフセットを持つと英数字だけの注記が 0.033em ずれるため、
    文字列ごとに計測する必要がある。
- **`examples/measure_headless.rs`** — ウィンドウもアダプタも作らずに、送り幅・
  ベースライン・DXF TOP との差を出す実演。

### Changed
- **破壊的: `TextMeasure::measure` の戻りが `Size` → `TextMetrics`**。レイアウトは
  `metrics.size` しか見ないので、実装側は `Size::new(w, h)` を
  `TextMetrics::new(w, h, baseline)` に替える。
- **破壊的: `TextRenderer::measure_text` の戻りが `(f32, f32)` → `TextMetrics`**。
- **破壊的: `TextRenderer` の `font_system` / `preferred_family` /
  `preferred_monospace_family` が `shaper` の下へ移動**（`tr.font_system` →
  `tr.shaper.font_system`）。`set_preferred_family` などのメソッドは名前もシグネチャも不変で、
  キャッシュ破棄の連動もそのまま。
- `FONT_SIZE_QUANTUM` / ロケール正規化 / フェイス解決が `sabitori-text::shaper` へ移動
  （`FONT_SIZE_QUANTUM` は引き続き公開）。

## [0.3.5] - 2026-07-28

テキスト選択を持つアプリが **UI 全体を毎フレーム再シェーピングしていた**のを止めた版。
描画時間の約半分を占めていたコストが、変化していないテキストについて丸ごと消える。

### Fixed
- **`prepare_text_with_hits` が shaping cache を通らず、テキスト選択を持つアプリは
  UI 全体を毎フレーム再シェーピングしていた**
  ([#49](https://github.com/Mutafika/sabitori/issues/49))。`TextRenderer` の
  `glyph_cache` は `prepare_text_styled` にしか効いておらず、hits 側は毎回
  `Buffer::new` + `set_text` + `Shaping::Advanced` を新規に走らせていた。
  `bridge.rs` の `render_list_to_gpu_with_hits` は**すべての `RenderCommand::Text`**
  をこの経路に流すため、影響は UI 全体に及ぶ。実測では**描画時間の約半分が shaping**
  で、うち `FontFallbackIter::next`（フォールバック候補の走査 + フォント名の memcmp）が
  支配的だった。最悪ケースは端末グリッドのように 1 セル = 1 text 要素を出すアプリで、
  1 文字ごとにフォールバック探索込みのフル shaping が走っていた。
  - `GlyphHit` は位置以外が原点非依存なので、既存の「原点相対で持ち、ヒット時に
    `(x, y)` を足す」方式をそのまま hits にも広げた。**キャッシュキーは変更なし**
    （shaping 入力のみをハッシュし、`x` / `y` / `color` は含まない）。
  - hits は atlas 検索の**前**に積む契約を維持。アトラスが溢れて glyph が落ちても、
    `max_width` でクリップされても、選択とキャレットは全文字の位置を知っている
    （`hits.len() >= glyphs.len()` で、2 つは添字非対応）。

### Changed
- **shaping の実体を `shape_run` 1 本に集約**。`prepare_text_styled` と
  `prepare_text_with_hits` は約 150 行を丸ごとコピペで共有しており、「両関数で同じ
  Buffer / Attrs / 設定を使うこと」というコメントだけが同期を保証していた。両者は
  キャッシュ済みの結果を平行移動して色を差し替えるだけの薄いラッパになり、
  `max_lines` の反復切り詰めロジックも 1 本になった。公開シグネチャは不変。
- **shaping 経路が GPU に依存しなくなった**（内部のみ）。`TextRenderer::new` は
  `wgpu::Device` を要求するため、`&mut self` メソッドとして書かれたコードはテストから
  到達できない。CPU 側の状態（`FontSystem` / `SwashCache` / `GlyphAtlas` のピクセル
  バッファ）を `ShapeCtx` として借用で渡す形にしたので、キャッシュ挙動をアダプタ無しで
  ユニットテストできる。
- `GlyphHit` に `Copy` を追加（全フィールドが plain な値）。

### 受け入れたトレードオフ
- `prepare_text_with_hits` はキャッシュが無かったぶん、**毎フレーム実際の `(x, y)` で
  sub-pixel bin を取り直していた**。キャッシュが効くと bin は「最初にシェープした位置」
  のものに固定されるため、**小数座標を連続的に動くテキストは sub-pixel の追従が
  わずかに落ちる**。これは `prepare_text_styled` が以前から受け入れている挙動で、
  `quantize_font_size` も同種の割り切り。再シェーピングの除去に見合うと判断した。

## [0.3.4] - 2026-07-28

テキストまわり 2 件。**注記の回転**を画面でも描けるようにし、**グリフアトラスの
全面再アップロード**（1 グリフ追加でも 16 MiB）を dirty 行だけの部分転送に変えた。

### Added
- **テキストの回転描画** ([#47](https://github.com/Mutafika/sabitori/issues/47))。
  `ElementStyle::rotation` は `RectDraw` にしか渡っておらず（コメントも「線描画用」）、
  text 要素に `.rotation()` を付けても**黙って無視されていた**。`TextDraw` と
  `GlyphInstance` に `rotation: f32` を追加し、`glyph.wgsl` でクォッドを回すようにした。
  DXF 由来の回転注記（実案件の電灯設備1階平面図では TEXT 294 件中 **41 件が回転付き**）が
  画面でも傾く。取込・PDF 出力側は対応済みで、画面だけが水平に寝たままだった。
  - **ピボットはテキスト原点**（レイアウト後の左上）。CAD の注記が挿入点まわりに回る
    仕様に合わせたため。`RectDraw` は矩形**中心**まわりなので、背景付きの要素を回すと
    箱とラベルはずれる。
  - **回転は shaping の後**に、CPU（グリフの配置＝`rotate_glyphs`）と GPU（各クォッド）で
    分担する。shaping cache は原点相対の位置を持ち**キーに角度を含まない**ので、同じ
    文字列を別角度で描いても再シェーピングは起きない。折返し（`max_width`）と
    `max_lines` の切り詰めは回転前の水平レイアウトで決まる。
  - 符号は `RectDraw::rotation` と同じ。画面座標が Y 下向きなので**正 = 画面上で時計回り**。
  - `text_cull_rect` は回転時に**回した 4 隅の AABB** で判定する。無回転の箱のままだと、
    原点まわりに振れて画面内に入ってきた注記が丸ごと消えていた。
  - 既定 `rotation = 0.0` は従来と bit 一致なので、既存アプリの見た目は不変。
- **`sabitori::rotate_glyphs`** を公開。シェーピング済みの glyph run を原点まわりに回す。
  `TextDraw` を経由せず自前で `prepare_text` している呼び出し側（`examples/text.rs` の
  回転デモが実例）が使う。**冪等ではない** — 掛け直すと曲がるので、用意した run に 1 回だけ。

### Changed
- **`TextDraw` / `GlyphInstance` に `rotation` フィールドが増えた**（どちらも `Default`
  実装なし）。構造体リテラルで直接組んでいる場合は `rotation: 0.0` の追加が必要。
  `Element` の builder API 経由なら影響なし。
- **`GlyphAtlas::dirty: bool` を廃止**し、`dirty_rows() -> Option<(u32, u32)>` /
  `mark_uploaded()` に置き換えた。`GlyphAtlas` は公開型だが、`dirty` を読み書きして
  いたのは `TextRenderer::upload_atlas` だけ。

### Fixed
- **グリフアトラスが 1 グリフ追加でも全面 16 MiB を再アップロードしていた**
  ([#48](https://github.com/Mutafika/sabitori/issues/48))。`upload_atlas` は
  `dirty` が立つたびに 2048 × 2048 × 4 = **16 MiB** のテクスチャ全体を
  `write_texture` していた。実測（mearie に計測器を入れたもの）で **6 フレームに
  3 回**発火し、同フレームの実グリフデータ約 52 KB に対して **約 320 倍**を
  GPU に送っていた。日本語のように文字種が多いテキストは新しいグリフが延々と
  現れるため、`dirty` は落ち着かず立ち続ける。
  `GlyphAtlas` が**書き換えた行の範囲**（`min_y..=max_y` の bounding band）を持つように
  し、`upload_atlas` はその帯だけを書く。12px のグリフ 1 個なら 16 MiB → 約 96 KB。
  - **帯であって矩形ではない**。`pixels` は row-major なので行範囲なら連続した 1 スライス
    = `write_texture` 1 回で済む。x 方向も持つと行ごとのコピーかストライド転送が必要に
    なる割に、シェルフアロケータが新規グリフを幅方向にばらまくため実効はほぼ無い。
  - 帯は**和集合**で広げる。上書きすると、同じアップロード間に入った先行グリフが
    CPU 側に取り残されて空白で描画される。
  - `clear`（スケール変更・アトラス溢れの self-heal）は**全行を dirty に戻す**。
    `fill(0)` で GPU がまだ保持している行まで消えるため、保留中の帯だけを送ると
    古いビットマップがテクスチャに残り、再割り当てされたスロットで 2 つのグリフが混ざる。

### Known limitations
- **クリップ（scissor）とヒットテストは軸並行のまま**。`clip_rect` は per-instance の
  軸並行矩形なので回転テキストのクリップは近似になり、`prepare_text_with_hits` が返す
  グリフ矩形も回らない。つまり回転テキストの選択・キャレット・リンク hit-test は
  「回さなかったら在ったはずの位置」を指す。注記（非対話テキスト）が用途なので現状は許容。

## [0.3.3] - 2026-07-27

winit → 入力イベントの変換を 1 箇所に集約した版。3 ランタイムに同じ変換が
コピペで存在し、片方だけ直る事故が続いていたのを構造的に止める。

### Fixed
- **`run_scene` でテキスト入力にゲートが無く、制御文字と Cmd 併用文字が漏れていた**。
  `CharInput` を `event.text` から素通しで発火していたため、macOS で Backspace が
  届ける `"\x7f"` が**テキストとして挿入され**、`Cmd+C` の `"c"` も focus 中の
  フィールドへ漏れていた。他 2 ランタイムには入っていたフィルタが `run_scene` にだけ
  無かった。
- **`run_declarative` / `run_scene` に `F1`〜`F12` / `PageUp` / `PageDown` / `Insert`
  が配線されていなかった** ([#11](https://github.com/Mutafika/sabitori/issues/11))。
  `Key` enum には v0.2.1 で追加済みだったが、`NamedKey` → `Key` の変換が
  ランタイムごとに 3 箇所コピペで存在し、配線されたのは `sabitori-window::run`
  だけだった。残り 2 つでは `Key::Other` に落ち、**F キー・ページ送り・Insert が
  アプリに届いていなかった**（端末エミュレータや TUI で実害）。`Key::Shift` も
  `run_declarative` だけ抜けていた。

### Changed
- **`sabitori-window::keymap` を新設し、winit → `sabitori-input` の変換を集約**。
  `key_from_winit` / `modifiers_from_winit` / `char_inputs` の 3 関数に対し、
  3 ランタイムはルーティングだけを行う。ハンドラ側は正味 124 行減った。
  `sabitori-window` は winit と `sabitori-input` の両方に依存する唯一のクレートなので
  ここが最下層になる（`sabitori-input` は意図的に winit 非依存のまま）。
- **配線漏れをコンパイルエラーにした**。`keymap` のテストが `Key` の全 variant に
  対する網羅 `match` を持つため、**enum に variant を足して変換を書き忘れると
  ビルドが落ちる**。今回の「enum に足したが 3 箇所中 1 箇所しか配線しなかった」は
  この形なら起き得ない。
- テキスト入力の判定方針を `char_inputs` に明文化して統一した。押下時のみ／
  `event.text` を使う／制御文字は落とす／Cmd 押下中は全部落とす／Ctrl は制御文字
  フィルタに任せる／**Alt は通す**（Option+文字は `é`, `©` など実在のテキストを作る）。
  この過程で `sabitori-window::run` の明示的な Ctrl ゲートは外れたが、Ctrl 併用は
  制御文字として届くため挙動は変わらない。
- **ドキュメントをリリース実態に追随させた**
  ([#24](https://github.com/Mutafika/sabitori/issues/24))。
  - CHANGELOG に欠けていた 17 版（`0.1.4` / `0.2.1` / `0.2.3`〜`0.2.10` /
    `0.2.14`〜`0.2.20`）の節と compare リンクを `git log` から復元。これで
    **全タグが CHANGELOG から追える**。
  - ROADMAP の Current Status を `0.1.0` → `0.3.x` に更新し、リリースラインが
    `0.3.x` 一本であること（`0.2.x` は `v0.3.1` で合流済み）を明記。パッチごとに
    腐らないよう、具体的なパッチ番号は書かず CHANGELOG を参照させる形にした。
  - RELEASING.md の手順 2 を「`[Unreleased]` が空ならリリースしない」ゲートにし、
    全タグの記載漏れを検査するワンライナーを追加。`Cargo.lock` が `.gitignore`
    対象でコミットされない旨も注記（手順と実態のずれを解消）。

## [0.3.2] - 2026-07-27

### Fixed
- **wasm 側の `console_log` 初期化で二重初期化 panic** (#46)。3 つの `run*` の wasm 分岐が
  `console_log::init_with_level(...).expect(...)` になっており、ホストが既に logger を
  張っていると `set_boxed_logger` が `Err` を返して **canvas が出る前に panic** していた。
  v0.3.1 までで native 側は `try_init()` に揃えたが、wasm だけ取り残されていた。
  `let _ = console_log::init_with_level(...)` に変更し、native と同じ「まだ誰も張って
  いないときだけ既定を入れる」ポリシーへ統一。対象は `run_declarative` /
  `run_scene` / `sabitori-window::run` の 3 箇所。API 変更なし、ホストが何もしない
  場合の挙動も不変。

## [0.3.1] - 2026-07-27

0.2.x 保守ラインを main へ合流し、**リリースラインを 0.3.x 一本に統合**した版。
v0.3.0 と v0.2.24 は `v0.2.23` から分岐した兄弟で互いの修正を持っていなかったため、
どちらに上げても何かが欠ける状態だった。0.3.1 が初めて両方を含む。

### Fixed
- **`run_scene` / `sabitori-window::run` の `tracing_subscriber` を `init()` →
  `try_init()` に** (#45)。ホストアプリが先に subscriber を張っていると
  `SetGlobalDefaultError` で panic し、**ウィンドウが開く前にプロセスごと落ちていた**
  （クラッシュログも残らない）。v0.3.0 で `run_declarative` だけ直していた残り 2 箇所。
  これで 3 つの `run*` すべてが「まだ誰も張っていないときだけ既定を入れる」に揃った。
  ホストが何もしない場合の挙動は不変。
- **`overlay_view`（モーダル）内のスクロール修正を main に取り込み**（v0.2.24 相当、
  下記 [0.2.24] 参照）。v0.3.0 はこの修正を含んでおらず、モーダル内リストがスクロール
  せず背景が動くバグを抱えていた。

## [0.3.0] - 2026-07-21

### Added
- **polyline プリミティブ** — 折れ線を宣言的に描画する `polyline()` エレメント。
  `sabitori-core` に `polyline()`、`sabitori-gpu` に `LineInstance` / `LineRenderer` /
  `shaders/line.wgsl` を追加。bridge が `RenderList` から `Vec<LineInstance>` を生成し、
  `render_list_to_gpu_with_rings` / `_with_hits` でスレッドする（main の glyph-atlas
  自己修復 `maybe_recover_atlas` と併存）。stroke 幅・色を指定可能。2D キャンバス／CAD の
  線描画向け。
- **オフスクリーン描画の検証テスト** — headless device で polyline をオフスクリーン
  テクスチャに描画し PNG に読み戻す verify テスト（`sabitori-gpu::line_renderer` /
  `sabitori::bridge`）。

### Fixed
- `run_declarative` の `tracing_subscriber` を `init()` → `try_init()` にして
  二重初期化での panic を回避。

## [0.2.24] - 2026-07-26

> `v0.2.23` から分岐した 0.2.x 保守ラインでのリリース。日付は 0.3.0 より後だが、
> 0.3.0 には含まれない（`[Unreleased]` の合流で main に入った）。

### Fixed
- **`overlay_view`（モーダル）内のスクロールが効かず、背景も一緒に動くバグを修正**。
  `patch_scroll_offsets` / `apply_scroll_measures`（`declarative.rs`）は main の `root`
  ツリーにしか掛からず、overlay ツリーの `overflow_scroll` コンテナが `scroll_states` に
  登録されなかった。結果、(1) モーダル内リスト（分野別一覧・改正版一覧など）がスクロール
  しない、(2) full-screen scrim がスクロールを吸えず `route_wheel` が背景ツリーに落ちて
  **モーダルの裏が動く**。overlay 構築時にも main と同じ scroll 登録（offset patch →
  build → measure apply）を通すことで、overlay の scroll コンテナが `scroll_states` に
  入り、wheel/touch ルーティング（overlay hit を prepend した merged build を参照）が
  最前面のモーダルリストを正しくスクロールし、scrim が背景スクロールを吸って**背景ロック**
  される。overlay_view のまま解決＝main ツリー合成で起きる文字貫通も無し。

## [0.2.23] - 2026-07-18

### Fixed
- **glyph atlas の溢れが `lazy_render` で自己修復しない問題を修正**。atlas は固定
  2048²・eviction 無しで、長いセッションで glyph が蓄積して溢れると新規 glyph が blank に
  落ちる（CJK 大量描画で顕在化）。自己修復 `maybe_recover_atlas` は「溢れた**次フレーム**で
  flush + 再 shape」する設計だが、`lazy_render` がアイドル時に loop を park するため次
  フレームが来ず、**文字が欠けたフレームが操作するまで残る**。`TextRenderer::atlas_overflowed()`
  を追加し、render 後に溢れていたら `must_draw` を強制＝復帰フレームを 1 枚走らせる。復帰
  フレームでも溢れるなら強制を止めて busy-loop を防ぐ。

## [0.2.22] - 2026-07-18

### Fixed
- **背の高いテキスト段落がスクロールで消えるバグを修正**。`bridge.rs` の Text 可視判定
  （clip との交差テスト）が、テキスト高さを `font_size * 1.5`（≒1.5 行）と決め打ちして
  いた。複数行に折り返す背の高い段落だと、その先頭が viewport 上端を ~1.5 行ぶん超えて
  スクロールした瞬間、1.5 行分の近似 rect が viewport 外と判定され**段落まるごと cull**
  される（下の十数行はまだ画面内なのに消える）。1 行=1 要素で組む画面では出ず、長い段落を
  1 要素で描くと踏む。`TextDraw.max_height`（`build.rs` が taffy 実高さ＝`TextMeasure` の
  折り返し込み測定値から設定）を可視判定に使い、退化に備え `font_size*1.5` を下限に。
  `text_cull_rect` に共通化し hits/rings 両経路を統一。

## [0.2.21] - 2026-07-18

### Fixed
- **`max_lines` で切り詰めるラベルの中央寄せズレを修正**。`TextMeasure::measure`
  が `max_lines` を受け取らず、`.max_lines(1)` のラベルを**折り返し後の全行高**で
  レイアウトしていた（描画は 1 行）。固定高・中央寄せの親（例: 法令カード）に入れると
  長いタイトルの測定ボックスが過大になり、描画される 1 行が箱から**上へはみ出して隣の
  行と重なる**。リストを長いタイトルまでスクロールした時だけ出る（先頭の短いタイトルでは
  出ない）ため HiDPI/スクロール由来に見えていた。`measure_text` は元々 `max_lines` を
  受けるので、trait → `TextNodeContext` → taffy leaf/estimate まで配線し、measure
  キャッシュ鍵にも畳み込んだ（同一文字列の clamp 有無で衝突しないように）。

### Added
- **iOS ソフトキーボード + 日本語テキスト入力**（`run_declarative` ランタイム）。
  winit の `WinitUIView` は既に `UIKeyInput` 準拠だが marked-text 非対応で日本語が
  届かない。隠し `UITextField`（full `UITextInput`）を winit view に addSubview し、
  focus に合わせ first responder 化 → iOS がキーボードを出し、変換も駆動する。
  `editingChanged` の全文差分を `CharInput` / `Backspace` に変換し、物理キーと同じ
  経路で route（`crates/sabitori/src/ios_keyboard.rs`）。

### Fixed
- **iOS: 「キーボードは出るが 1 文字も入らない」を修正**。declarative ランタイムが
  毎フレーム無条件で `ControlFlow::WaitUntil(+16ms)` を張っていたため、iOS の
  redraw phase に定周期で再突入し **UIKit の text-input run-loop source を starve**
  していた（`insertText:` / `editingChanged` がどの view にも来ない）。idle 時は
  `ControlFlow::Wait` で完全 park するよう変更（非 iOS は従来どおり）。

## [0.2.20] - 2026-07-18

### Added
- **iOS ソフトキーボード + 日本語テキスト入力**（`run_declarative`）。

## [0.2.19] - 2026-07-17

未合流だった 4 本を v0.2.18 の上へ合流（追加・修正のためパッチ）。

### Added
- **`DeclarativeApp::ime_allowed`** — アプリのポリシーとして IME の有効/無効を宣言する。

### Fixed
- `font_size` を量子化し、シェイプ/atlas キャッシュのスラッシングを防止。
- **グリフアトラス枯渇を自己修復**（テキスト欠落の恒久バグ、
  [#30](https://github.com/Mutafika/sabitori/issues/30)）。
- sRGB と linear の境目を、名前とテストと警告で見えるようにした。

## [0.2.18] - 2026-07-17

### Added
- **interactive text link ranges** + tooltip まわりの修正。

## [0.2.17] - 2026-07-17

### Added
- 横方向にオーバーフローするコンテナに**横スクロールバー**を描画。

## [0.2.16] - 2026-07-17

### Added
- 縦ホイールを x に転送した横スクロールのステップをブースト。

## [0.2.15] - 2026-07-16

### Fixed
- 横ストリップが優位なスクロール軸を消費するように（トラックパッド）。

## [0.2.14] - 2026-07-16

### Fixed
- 横方向優位のストリップで、縦ホイールを x に転送。

## [0.2.13] - 2026-07-16

### Added
- **`DeclarativeApp::ime_allowed() -> bool`** — プラットフォーム IME の有効/無効を
  アプリのポリシーで制御するフック（毎フレームポーリング、dedup 済み、デフォルト
  `true` なので既存アプリは無変更）。`false` を返すと winit が IME を無効化し、
  **進行中の変換も破棄される**ため、変換中にダイアログを閉じても候補ウィンドウが
  画面に取り残されない。テキスト入力対象が存在する時だけ `true` を返す実装を推奨
  （ターミナルのように focused field なしで IME を受けるアプリはデフォルトのまま）。
  declarative / scene_app 両ランタイムに配線。

## [0.2.12] - 2026-07-16

### Added
- **`DeclarativeApp::on_scroll_xy(delta_x, delta_y)`** — スクロールを両軸で受け取る
  フック（logical px、デフォルト実装は no-op なので既存アプリは無変更）。従来の
  `on_scroll(delta_y)` は縦のみの convenience として併存し、両方が発火する。winit の
  `MouseWheel` ハンドラは元々 `delta_x` を計算済みだったので、managed scroll container が
  消費しなかった場合に app へ両軸を転送するだけの配線。`SceneApp` は
  `DeclarativeApp` を継承するため両パス（declarative / scene_app）で発火する。
  2D キャンバスのパン等、横スクロールが意味を持つアプリ向け。

## [0.2.11] - 2026-07-12

### Added
- **`text_input` ウィジェット** (`sabitori-widgets::text_input(id, &TextInputState, &TextInputStyle)`)
  — `TextInputState` から focusable な単一行フィールドを描画する declarative ウィジェット。
  確定済みテキスト＋IME preedit をインライン表示 (`display_text_with_preedit`)、空欄時は
  `TextInputStyle::placeholder` 色で placeholder を表示。
- **`TextInputState::on_focused_input(&InputEvent) -> bool`** — focus 中フィールド用の標準
  ルーター。`CharInput`/`KeyInput`/`ImePreedit`/`ImeCommit` を対応メソッドへ振り分け、各アプリが
  `on_focused_input` 内でコピペしてた per-field match を不要にする。
- **`TextInputStyle`** — `text_input` の配色・サイズ指定。

## [0.2.10] - 2026-07-07

### Added
- **`SizeClass` レスポンシブブレークポイント**（`ViewContext` 上）。

### Fixed
- HiDPI のグリフオフセットと、幅制約下の計測を修正。

## [0.2.9] - 2026-07-06

### Added
- **find-in-page 用の per-range background highlight**（`sabitori-text`）。
- 製品紹介 LP `sabitori_home` ＋ landing 実験群（examples）。
- GPU ネイティブ LP パターン 3 種（flow / gravity / site）（examples）。
- ギャラリーの TUI↔Modern スキン切替 / LP の多画面ルーティング化（examples）。
- `sabitori_home` のバックドロップを斜め視点オレリーに刷新（examples）。

## [0.2.8] - 2026-07-03

### Added
- **タイポグラフィ拡張** — `font_weight` / `letter_spacing` / `line_height` を配線。
- landing LP をリッチ化 — 背景グロー / trust pills / コードパネル（examples）。

### Fixed
- ウィンドウのライブリサイズ中のクラッシュを修正。

## [0.2.7] - 2026-07-03

### Fixed
- 不透明ウィンドウで surface の `alpha_mode` に `Opaque` を優先
  ([#27](https://github.com/Mutafika/sabitori/pull/27))。

## [0.2.6] - 2026-07-03

### Added
- **SceneApp ランタイムに DeclarativeApp 機能を配線**
  ([#25](https://github.com/Mutafika/sabitori/pull/25))。

## [0.2.5] - 2026-06-30

### Added
- **`Key::Shift`** — Shift 単独押下を配送（ゲームのダッシュ等で拾えるよう `Other` と分離）。
- **`present_mode` の選択** — Mailbox → Immediate → AutoVsync の順で選び、高リフレッシュに対応。
- **`SceneApp::on_raw_motion`** — 生のデバイスモーションを配送。
- **`DeclarativeApp::on_build`** フック。

## [0.2.4] - 2026-06-26

### Added
- IME 変換候補ウィンドウを caret 位置にアンカー。

### Changed
- **グリフ・シェイピングキャッシュ**で UI 再描画を高速化（`sabitori-text`）。

## [0.2.3] - 2026-06-19

### Added
- **カラー絵文字を実色で描画** — glyph シェーダにカラーグリフ分岐を追加。

## [0.2.2] - 2026-06-17

### Added
- **DockGroup** (`sabitori-widgets::DockGroup` / `DockAxis` / `drop_split`) —
  N枚のパネルを1つのフロート矩形へ束ね、単一軸(Row/Col)で並べて一体移動/
  リサイズし、ペイン間の継ぎ目をドラッグ可能なスプリッタにする純幾何ウィジェット。
  `WindowDragState` と同じく id・ヒットテスト・描画を知らない。`pane_rect`/
  `splitter_rect`/`drag_splitter`(最小幅クランプ)/`move_by`/`set_size`/
  `split_pane`/`remove`/`drop_split`。ユニットテスト18件。bamiri のパネル合体
  =分割ドッキングの基盤。
- `window_drag::set_rect` — タイル配置用に位置＋寸法を上書き。
- **FocusManager** (`sabitori-widgets::FocusManager` / `FocusChange` /
  `FocusKeyResult`) — 複数 text_input のフォーカス管理の一般化。
  `HashMap<id, TextInputState>` + フォーカス中 id を所有し、
  クリックでフォーカス移動/喪失（`handle_press`）、Tab/Shift+Tab の
  順送り（登録順、`tab_navigation` で無効化可）、キー/文字/IME イベントの
  フォーカス先ルーティング（`on_key`/`on_char`/`on_ime_*`）、
  `wants_keyboard()`（egui の `wants_keyboard_input()` 相当）、
  カーソル点滅（`tick`/`cursor_visible_for`）を提供。Enter は
  `FocusKeyResult::Submit(id)`、Escape は `Escape(id)` で**ホストに委ねる**
  （勝手に確定/blur しない）。IME 変換中の Enter/Tab は解釈しない。
  埋め込みホスト（bamiri 等）のダイアログ入力・検索ボックス・rename 向け。
  ユニットテスト 11 件。
- **ColorPicker** (`sabitori-widgets::ColorPickerState` / `ColorPickerStyle`) —
  プリセットパレット格子 + RGB 微調整（`NumericInputState` を 3 ch 内蔵
  再利用、0–255 sRGB 表示）+ プレビューの複合ウィジェット。スウォッチは
  `handle_click`、RGB ドラッグ/直接入力は NumericInput と同じポインタ
  プロトコル（`on_pointer_down/move/up` + `on_key`/`on_char`）。
  `Color` が線形 RGB を持つため、表示・入力は新設の `Color::to_srgb8()` /
  既存の `from_srgb8` で往復する。ユニットテスト 9 件。
- `Color::to_srgb8()` (`sabitori-core`) — 線形 → sRGB 8bit の逆変換
  （`from_srgb8` の対）。RGB 値のユーザー向け表示用。往復テスト付き。
- **DropdownState** (`sabitori-widgets::DropdownState` / `DropdownEvent`) —
  element-id 駆動の dropdown/select（egui `ComboBox` 相当）。既存の
  `Dropdown` は画面座標 `Rect` 自前管理の immediate 系で declarative/
  埋め込みホストから使いにくかった（巡回ボタン代替の原因）。MenuBar と
  同じ「state ↔ visuals 分離」方式で、`trigger()`（選択中ラベル + ▼）、
  `menu_inline()`（レイアウトフロー展開、位置計算不要）、
  `overlay_at(anchor, …)`（`BuildResult::region_rect` のアンカー矩形に
  絶対配置 + 全画面バックドロップ、下端で収まらなければ上に展開）、
  `handle_click()` → `Opened/Closed/Selected(i)/Ignored`。
  スタイルは既存 `DropdownStyle` を共用。ユニットテスト 8 件。
- **DatePicker** (`sabitori-widgets::DatePickerState` / `DatePickerStyle`) —
  年月ヘッダ（◀▶ 月送り）+ カレンダー格子の簡易日付選択（工程表・
  タイムライン用）。chrono 非依存（閏年 + Sakamoto 法の曜日計算を内蔵、
  `is_leap_year`/`days_in_month`/`weekday` も公開）。`handle_click` が
  月送りを内部処理し、日セルで `Some((y, m, d))` を返す。
  ユニットテスト 10 件。
- **slider_sync** (`sabitori::slider_sync`) — 埋め込みホスト向け Slider
  配送ヘルパー（`scroll_sync` と同系）。`SliderState` はトラックの
  スクリーン座標を要求するが、埋め込みホストはそれを `BuildResult` から
  しか知れない — これが「Slider があるのに ±ステッパーで代替」の原因
  だったので、`HashMap<id, SliderState>` × `BuildResult` の突き合わせを
  公式化: `route_press`（ヒット id のトラックでドラッグ開始）/
  `route_move`（ドラッグ中の追従、トラック外も可）/ `route_release` /
  `any_dragging`（ポインタキャプチャ判定）。GPU 不要のユニットテスト 3 件。
- `BuildResult::region_rect(id)` (`sabitori-core`) — レイアウト結果から
  id 指定で hit region の矩形を引く。Slider のトラック座標・Dropdown
  オーバーレイのアンカー等、埋め込みホストの座標取得の公式口。
- **progress_bar / labeled_progress_bar** (`sabitori-core::forms`、
  re-export は `form_progress_bar` / `labeled_progress_bar`) — div ベースの
  GUI 塗りつぶしバーの公式化（`tui::progress_bar` のテキスト版とは別物）。
  占積率・進捗率の頻出パターン。fraction は 0–1 にクランプ、fill は
  Percent 幅なので外側を `.w()` で自由にサイズできる。
- `examples/cad_widgets.rs` に Phase-2c ウィジェット一式を追加:
  FocusManager の 2 フィールド（Tab 巡回 / Enter 確定 / IME）、
  ColorPicker、DropdownState（inline モード）、DatePicker、
  SliderState + `labeled_slider`、`labeled_progress_bar`。
- **scroll_sync** (`sabitori::scroll_sync`) — declarative ランナーを使わず
  build→GPU パイプラインを直接駆動する埋め込みホスト（bamiri 等）向けの
  ScrollView 状態同期ヘルパー。`patch_scroll_offsets`（overflow_scroll
  要素への id 合成 + 状態登録 + オフセット注入）/ `apply_scroll_measures`
  （レイアウト実測の content/viewport を状態へ反映）/ `route_wheel`
  （ポインタ直下のスクロールコンテナへのホイール配送）/ `tick_all`。
  declarative ランナー内部の private 実装をこのモジュールへ抽出して
  委譲したので、ランナーと埋め込みホストの挙動は常に一致する。
  GPU 不要のユニットテスト 3 件付き。
- `MeasureCache::len()` / `is_empty()` — 「リサイズ時にクリアする」運用の
  テストと肥大診断用。
- **UiOverlayRenderer** (`sabitori-gpu::UiOverlayRenderer`) — surface /
  device を所有しない軽量 UI レンダラー。自前の winit ループ + wgpu
  パイプラインを持つホストアプリ（bamiri 等）が、既存 device と surface
  texture への render pass に sabitori UI（rect 群 + glyph）を重ね描き
  するための埋め込み口。base / overlay の 2 層を 1 つの instance buffer に
  同居させ `draw_base()` / `draw_overlay()` で範囲描画する。
  `globals_bind_group()` は `TextRenderer::render_glyphs` にそのまま渡せる。
  headless GPU テスト 3 件付き。`EmbeddedRunner` と違い 2 つ目の surface を
  作らないので、同一ウィンドウ上の 3D シーンと安全に共存できる。
- **MenuBar** (`sabitori-widgets::MenuBarState` / `MenuDef` / `MenuBarStyle`) —
  水平メニューバー (File/Edit/View…)。クリックでドロップダウン展開、
  オープン中のホバーで隣メニューへ切替、1段サブメニュー（フライアウト）、
  ショートカット表示、セパレータ。`MenuItemDef` を ContextMenu と共用し、
  `submenu` フィールド + `.with_submenu()` / `.has_submenu()` を追加。
  オーバーレイは「不可視のバー複製」を同一レイアウトで重ねる方式なので、
  ウィジェットが要素の実座標を知らなくてもドロップダウンがラベル直下に揃う。
- **NumericInput** (`sabitori-widgets::NumericInputState` +
  `sabitori-core::forms::numeric_input`) — egui `DragValue` 相当。
  横ドラッグで増減（`step`/px、slop 内のクリックは値を変えない）、
  クリックでテキスト編集モード（`TextInputState` を内蔵再利用 — カーソル/
  選択/IME そのまま）、`min`/`max`/`precision`/`suffix`（"mm" 等の単位表示）、
  f64 ベース。Enter 確定 / Escape キャンセル、数値文字フィルタ付き。
- **CollapsingHeader** (`sabitori-core::forms::collapsing_header` /
  `collapsing_section`) — 折りたたみセクション。開閉状態は app 持ちの
  `bool`、ヘッダ id のクリックでトグルする stateless builder。
- **ポインタ/キーボード排他 API** — egui の `wants_pointer_input()` /
  `wants_keyboard_input()` 相当:
  - `BuildResult::hit_region_at(x, y)` / `BuildResult::wants_pointer(x, y)`
    （sabitori-core）。
  - `UiCapture { wants_pointer, wants_keyboard }` +
    `DeclarativeApp::on_ui_capture(capture)` — runtime（declarative /
    scene_app 両方）が hover・focus・drag・レイアウト変化のたびに push する
    スナップショット。3D ホスト (SceneApp) はこれを見てカメラ操作と UI を
    排他する。装飾だけの背景 div は捕捉しない — ポインタをブロックしたい
    パネルには `.id()` を付ける。
- `DeclarativeApp::on_hover_change(id)` — hover 要素が変わった瞬間の push 通知
  （`ctx.hovered` の毎フレーム read の補完）。MenuBar のホバー切替が使う。
- `Element::basis(d)` — CSS `flex-basis` の builder（`ElementStyle::flex_basis`、
  default `Auto`）。
- `Cursor::ResizeEw` — 水平リサイズカーソル（NumericInput のドラッグ表示用）。
- example `cad_widgets` — MenuBar + NumericInput + checkbox +
  collapsing_section + `flex_1().overflow_scroll()` リストの CAD 向けデモ。

### Fixed
- **高さ 0 に潰れた scroll/hidden コンテナが子を clip 無しで漏らす件** —
  `flex_1().overflow_scroll()` リストが兄弟に高さを食い潰されて 0 になると、
  `emit_commands` の zero-size 早期 return が PushClip を積まずに子へ再帰し、
  数百行が画面全体とウィンドウ下端を越えて描画されていた（bamiri の
  ドックパネルで観測）。零サイズの clip コンテナはサブツリーごと cull する
  ように修正。padding が content box を零面積にするケースも同様に cull。
  併せて bridge の `is_clipped` が退化 clip（幅/高さ 0 — 重ならない入れ子
  clip の交差等）を「全てを clip」と判定するようにし、GPU インスタンスの
  `clip_rect` の `w==0||h==0` =「clip 無し」センチネルと衝突して clip が
  無効化される経路を遮断した（BUGS.md 2026-06-12 の件）。
- **`flex_1()` を Tailwind `flex-1` (`flex: 1 1 0%`) と同義に修正** —
  従来は `flex_grow: 1` のみで `flex_basis: auto` のままだったため、
  flex base size がコンテンツ高さになり、`flex_1().overflow_scroll()` が
  スロットをはみ出して兄弟を押し潰し、スクロールがロックされていた
  （BUGS.md 2026-05-10 の件、根本修正）。明示の Px 高さ workaround は不要に。
  コンテンツ起点で伸ばしたい場合は `.grow(1.0)`（basis auto のまま）を使う。
- examples: 古い `RectInstance` フィールド構成に追従（`rotation` / `clip_rect` の
  追加、`_pad0` を `f32` 化）。`anim` / `effects` / `gpu_flex` / `hello` / `layout` /
  `showcase` / `text` がコンパイル不可だった件。ライブラリ API への影響なし。

## [0.2.1] - 2026-06-15

### Added
- **端末用キーを配送** — `ArrowUp` / `ArrowDown` を追加し、`F1`〜`F12` /
  `PageUp` / `PageDown` / `Insert` を届けるようにした。

## [0.2.0] - 2026-06-06

### Added
- SceneApp ランタイムに自動スクロールを実装。`overflow_scroll` コンテナを毎フレーム
  登録し、カーソル下の領域へホイールをルーティング、慣性/スプリングと上限クランプを
  managed scroll state で管理する（DeclarativeApp ランタイム同等）。これまで SceneApp
  ではホイールが `on_scroll` に丸投げされるだけで `overflow_scroll` が効かなかった。
  アプリ側は `.overflow_scroll()` を付けるだけでよい。

## [0.1.8] - 2026-06-05

### Fixed
- 別ウィンドウから pointer が入るとカーソルが再設定されない件 (#8):
  `WindowEvent::CursorEntered` を処理して `last_cursor` を無効化し、直後の
  `CursorMoved` で必ず自分のカーソルを再設定させる。winit は macOS の cursor
  rects を使わないため OS が境界でカーソルをリセットせず、`apply_cursor` の
  dedup が stale な `last_cursor` を信じて前ウィンドウのカーソル（I-beam 等）を
  残してしまっていた。

## [0.1.7] - 2026-06-04

### Added
- Element ごとのフォント指定 `.font_family(name)` — generic / app-preferred の
  解決を上書きして、その要素のテキストを**名前付きフェイス**で shaping する。
  フォントピッカーが各行を自フォントでプレビューする（Word 方式）等に使う。
  指定フェイスに無いグリフは通常の fallback チェーンで描画。
- Framework 描画のスクロールバー `.scrollbar(color)` — `overflow_scroll`
  コンテナの右端に、コンテンツが縦に溢れている間だけ細い丸サムを描画。
  managed scroll state のアニメ済みオフセットに追従して滑らかに動く。
  hit region を追加しないため click/wheel ルーティングは不変
  （インジケータのみ・ドラッグ不可。`None` デフォルトで既存アプリは無変化）。

## [0.1.6] - 2026-06-04

### Fixed
- 日本語フォントの fallback と、DPR 変更時の glyph atlas flush。
  （リリース時に CHANGELOG 更新が漏れていたため遡って記載。）

## [0.1.5] - 2026-06-03

### Fixed
- グリッド/ターミナル状レイアウトのテキスト選択コピーを修正 (#7):
  - `selected_text()` を要素 index ではなく**視覚的ジオメトリ**で連結
    — 1 セル 1 要素のレイアウト（ターミナルグリッド等）で行コピーが
    縦書きになっていた件。行が下がった時のみ改行、行内の x-gap は
    空白に復元（空セルは要素として出ないため推定）。
  - Cmd 押下時に `event.text` の文字を `CharInput` として流さない
    — `Cmd+C`/`Cmd+V` がコピペ＋リテラル `c`/`v` 入力を両方起こしていた件
    （Option/Alt は実文字を出すため維持、Ctrl は元々 control char で除外）。
  - コピー以外のキー入力で選択を解除（裸の修飾キー `Key::Other` は除外）
    — paste 後に選択ハイライトが残り新しいテキストに重なって描画される件。

### Added
- テーマ対応の可読なテキスト選択 — `DeclarativeApp::selection_style() -> Option<(Color, Color)>`。
  選択ハイライトを app 提供の `(bg, fg)` で描画し、選択中グリフを `fg` に recolor して
  可読性を確保。`None` (default) は従来の半透明 system blue のままで後方互換 (#5)。

## [0.1.4] - 2026-06-02

### Added
- **テーマ対応の可読なテキスト選択** — `DeclarativeApp::selection_style()`。

## [0.1.3] - 2026-06-02

### Fixed
- `overflow_scroll` コンテナの子が viewport に潰されず自然なサイズを保つように
  （スクロールが効かなかった件）

## [0.1.2] - 2026-06-02

### Added
- `ViewContext.mono_advance` — 等幅セルの実 advance を計測して公開

## [0.1.1] - 2026-06-02

### Added
- 等幅フォントの family 上書き API (`preferred_monospace_family`)

### Fixed
- HiDPI でテキストがボケる — グリフを device scale factor でラスタライズ

## [0.1.0] - 初回リリース (baseline)

最初に公開するバージョン。さびとり (sabitori) は Warp 風の TUI デザインを
GPU レンダリングの GUI として表現する Rust フレームワーク。
以下は初回リリース時点での機能セット。

### Added

#### レンダリング / GPU (`sabitori-gpu`, `sabitori-scene`)
- wgpu ベースの 2D レンダラ + WASM/WebGL2 対応
- 3D シーンシェーダー + `OrbitCamera`
- linear gradient / arc・ring SDF primitive / `translate_x`・`translate_y`
- overflow clip を per-fragment scissor で実装（部分はみ出しも GPU でカット）
- backdrop blur trait + `NSVisualEffectView` attach（macOS）
- surface サイズを `max_texture_dimension_2d` でクランプ
- opacity cascade（premultiplied）

#### テキスト (`sabitori-text`, `sabitori-markdown`)
- cosmic-text ベースのテキスト整形・計測
- グリフ品質向上（サブピクセル整列 + 輝度補正コントラスト）
- 日本語 / CJK フォント対応 + Noto Sans JP Regular 同梱
- `max_lines` による切り詰め（UTF-8 境界安全）
- テキストドラッグ選択 + Cmd+C コピー（macOS）
- markdown レンダリング + 孤立マーカー除去 + 画像 fallback truncation

#### レイアウト / 宣言的 API (`sabitori-layout`, `sabitori-core`)
- taffy ベースのレイアウトエンジン
- `DeclarativeApp` 宣言的 API + `extra_windows`（複数 NSWindow）
- absolute positioning
- `target_frame_interval`（default 8ms / ~120Hz）
- extra_windows への 3D scene hooks

#### ウィジェット / UI (`sabitori-widgets`, `sabitori-anim`, `sabitori-style`)
- file manager（flat sidebar / keyboard nav / scroll）
- context menu / modal overlay / theme presets + opacity
- form controls + focus system（Tab・Shift+Tab / IME ルーティング）
- `SliderState` / `VirtualList` / `EmbeddedRunner`
- 画像レンダリング（`image_url`）
- `MotionState` + easing / `SplashPreset`（10 種の splash アニメ）
- window_icon trait / Cursor preference（opt-in）

#### 入力 (`sabitori-input`)
- IME / 日本語入力 / プログラム制御スクロールの基盤
- Pointer 抽象 — タッチ + ピンチ + 慣性 + バウンス + 2D スクロール

#### ネットワーク (`sabitori-net`)
- 127.0.0.1 宛に `SABITORI_LOCAL_BEARER` で auth 付与

#### パフォーマンス
- `lazy_render` モード — idle 時の 60fps 描画ループを停止
  （scroll spring / fling 中は redraw 継続）

#### ライセンス / ドキュメント
- MIT LICENSE + 全クレートに license フィールド統一
- cargo-deny（AGPL/GPL 系を排除）/ cargo-about / NOTICE / 第三者ライセンス html
- README / ROADMAP（英語版 + 日本語版 + 言語切替リンク）

[Unreleased]: https://github.com/Mutafika/sabitori/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/Mutafika/sabitori/compare/v0.3.21...v0.4.0
[0.3.21]: https://github.com/Mutafika/sabitori/compare/v0.3.20...v0.3.21
[0.3.20]: https://github.com/Mutafika/sabitori/compare/v0.3.19...v0.3.20
[0.3.19]: https://github.com/Mutafika/sabitori/compare/v0.3.18...v0.3.19
[0.3.18]: https://github.com/Mutafika/sabitori/compare/v0.3.17...v0.3.18
[0.3.17]: https://github.com/Mutafika/sabitori/compare/v0.3.16...v0.3.17
[0.3.16]: https://github.com/Mutafika/sabitori/compare/v0.3.15...v0.3.16
[0.3.15]: https://github.com/Mutafika/sabitori/compare/v0.3.14...v0.3.15
[0.3.14]: https://github.com/Mutafika/sabitori/compare/v0.3.13...v0.3.14
[0.3.13]: https://github.com/Mutafika/sabitori/compare/v0.3.12...v0.3.13
[0.3.12]: https://github.com/Mutafika/sabitori/compare/v0.3.11...v0.3.12
[0.3.11]: https://github.com/Mutafika/sabitori/compare/v0.3.10...v0.3.11
[0.3.10]: https://github.com/Mutafika/sabitori/compare/v0.3.9...v0.3.10
[0.3.9]: https://github.com/Mutafika/sabitori/compare/v0.3.8...v0.3.9
[0.3.8]: https://github.com/Mutafika/sabitori/compare/v0.3.7...v0.3.8
[0.3.7]: https://github.com/Mutafika/sabitori/compare/v0.3.6...v0.3.7
[0.3.6]: https://github.com/Mutafika/sabitori/compare/v0.3.5...v0.3.6
[0.3.5]: https://github.com/Mutafika/sabitori/compare/v0.3.4...v0.3.5
[0.3.4]: https://github.com/Mutafika/sabitori/compare/v0.3.3...v0.3.4
[0.3.3]: https://github.com/Mutafika/sabitori/compare/v0.3.2...v0.3.3
[0.3.2]: https://github.com/Mutafika/sabitori/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/Mutafika/sabitori/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Mutafika/sabitori/compare/v0.2.23...v0.3.0
[0.2.24]: https://github.com/Mutafika/sabitori/compare/v0.2.23...v0.2.24
[0.2.23]: https://github.com/Mutafika/sabitori/compare/v0.2.22...v0.2.23
[0.2.22]: https://github.com/Mutafika/sabitori/compare/v0.2.21...v0.2.22
[0.2.21]: https://github.com/Mutafika/sabitori/compare/v0.2.20...v0.2.21
[0.2.20]: https://github.com/Mutafika/sabitori/compare/v0.2.19...v0.2.20
[0.2.19]: https://github.com/Mutafika/sabitori/compare/v0.2.18...v0.2.19
[0.2.18]: https://github.com/Mutafika/sabitori/compare/v0.2.17...v0.2.18
[0.2.17]: https://github.com/Mutafika/sabitori/compare/v0.2.16...v0.2.17
[0.2.16]: https://github.com/Mutafika/sabitori/compare/v0.2.15...v0.2.16
[0.2.15]: https://github.com/Mutafika/sabitori/compare/v0.2.14...v0.2.15
[0.2.14]: https://github.com/Mutafika/sabitori/compare/v0.2.13...v0.2.14
[0.2.13]: https://github.com/Mutafika/sabitori/compare/v0.2.12...v0.2.13
[0.2.12]: https://github.com/Mutafika/sabitori/compare/v0.2.11...v0.2.12
[0.2.11]: https://github.com/Mutafika/sabitori/compare/v0.2.10...v0.2.11
[0.2.10]: https://github.com/Mutafika/sabitori/compare/v0.2.9...v0.2.10
[0.2.9]: https://github.com/Mutafika/sabitori/compare/v0.2.8...v0.2.9
[0.2.8]: https://github.com/Mutafika/sabitori/compare/v0.2.7...v0.2.8
[0.2.7]: https://github.com/Mutafika/sabitori/compare/v0.2.6...v0.2.7
[0.2.6]: https://github.com/Mutafika/sabitori/compare/v0.2.5...v0.2.6
[0.2.5]: https://github.com/Mutafika/sabitori/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/Mutafika/sabitori/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/Mutafika/sabitori/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/Mutafika/sabitori/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/Mutafika/sabitori/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Mutafika/sabitori/compare/v0.1.8...v0.2.0
[0.1.8]: https://github.com/Mutafika/sabitori/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/Mutafika/sabitori/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/Mutafika/sabitori/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/Mutafika/sabitori/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/Mutafika/sabitori/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/Mutafika/sabitori/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/Mutafika/sabitori/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/Mutafika/sabitori/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Mutafika/sabitori/releases/tag/v0.1.0
