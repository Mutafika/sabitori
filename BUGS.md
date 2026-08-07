# Sabitori バグ記録

## [修正済み 2026-06-12] 高さ 0 に潰れた scroll コンテナが子を clip 無しで画面全体に漏らす
**日付**: 2026-06-12
**箇所**: `crates/sabitori-core/src/build.rs::emit_commands`（zero-size 早期 return）と `crates/sabitori/src/bridge.rs::is_clipped`

### 症状

bamiri のドックパネル（固定 root → `flex_1` padded column の兄弟数個 → `flex_1().overflow_scroll()` リストに数百行）で、行がスクロールコンテナの外に描画され、画面の残り全体とウィンドウ下端を越えて漏れる。**clip rect が一切効いていないように見える**。

### 真因（2 段構え）

1. **`emit_commands` の zero-size 早期 return が clip を積まずに子へ再帰していた。**
   兄弟がパネルの高さを食い潰すと `flex_1` の scroll コンテナは高さ 0 になる
   （basis 0 + 負の free space で割当 0）。`w <= 0.0 || h <= 0.0` の早期 return は
   「counter 整合のため」子へ再帰するが、**PushClip を積まず・scroll cull もせず・
   scroll measure も記録しない**。Taffy は高さ 0 のコンテナの子も自然サイズで
   レイアウトするので、数百行がフルサイズ・clip 無しで render list に流れていた。
2. **GPU インスタンスの `clip_rect` は `w==0||h==0` を「clip 無し」センチネルに
   使っている**（`sabitori-gpu/src/instance.rs`）。そのため退化（面積 0）の clip を
   インスタンスに書くと clip が**無効化**されて全面描画になる。bridge の
   `is_clipped` は「完全に外側」しか cull しないため、退化 clip の anchor 線を
   跨ぐ要素や、重ならない入れ子 clip の交差（`intersect_clip` が零サイズ rect を
   返すケース）が素通りしてセンチネルと衝突し得た。

### 修正

- `build.rs`: zero-size 早期 return で、要素が clip コンテナ
  （overflow Hidden/Scroll）なら**サブツリーごと cull**（`count_elements` で
  counter のみ整合）。overflow Visible の零サイズ wrapper は従来どおり再帰
  （子のはみ出しは合法）。
- `build.rs`: padding がコンテナを食い潰して content box が零面積になる clip
  コンテナも同様に子を cull（零サイズ PushClip を発行しない）。scroll
  コンテナなら ScrollMeasure は Taffy レイアウトから記録して状態の陳腐化を防ぐ。
- `bridge.rs::is_clipped`: 退化 clip（幅または高さ ≤ 0）は「**全てを clip**」と
  判定。CPU 側で必ず cull され、退化 clip が GPU インスタンスに書かれることは
  なくなった（センチネルとの衝突経路を遮断）。シェーダのセンチネル自体は維持。

### 再現テスト

- `sabitori-core/src/build.rs`:
  - `zero_height_scroll_container_culls_rows_instead_of_leaking`（修正前 FAIL）
  - `degenerate_padded_clip_culls_children`（修正前 FAIL）
  - `nested_flex_scroll_rows_clipped_to_container`（健全形のガード、bamiri の
    dock_panel 形そのまま: 行の clip ⊆ コンテナ rect、下端以下の行は cull）
- `sabitori/src/bridge.rs`:
  - `degenerate_clip_clips_everything`（修正前 FAIL）
  - `disjoint_nested_clips_cull_contents` / `normal_clip_culls_only_fully_outside`

### 発見経緯

bamiri の右ドックパネルで、`flex_1` 兄弟に高さを食われた
`flex_1().overflow_scroll()` リストの数百行が画面全体に漏れた。
2026-06-10 の flex_basis 修正は「高さの計算」の修正で、本件は
「高さ 0 になったときの clip 発行」の別バグ。

## [修正済み 2026-06-10] `flex_1().overflow_scroll()` 単独だと scroll がロックされる
**日付**: 2026-05-10
**箇所**: `crates/sabitori/src/declarative.rs::patch_scroll_inner` と Taffy への overflow=Scroll マッピング (`crates/sabitori-core/src/build.rs`)

### 症状

```rust
div().flex_1().flex_col().children([
    header, hsep(t.border),
    // ↓ explicit Px height を与えないと wheel event が来ても動かない
    div().flex_1().flex_col().overflow_scroll().children(items),
])
```

ホイール / トラックパッドで scroll しても画面が動かない。 scroll container 自体は hit-test に登録されており、 `on_scroll_xy` も呼ばれているが `scroll_y` が 0 から動かない。

`.h(Px(some_concrete_height))` を明示で指定する、 もしくは VirtualList (内部的に overflow_scroll を使わず scroll_y を直に消費する) を使うと正常に scroll する。

### 想定原因 (要調査)

`patch_scroll_inner` で `viewport_h` を以下の優先順位で決めている:

1. `element.style.height` が `Px(h)` なら `h`
2. 前フレームの measured viewport_height
3. それ以外は `0.0`

初回フレームでは (1) も (2) も無く `viewport_h = 0` で `ScrollView::new(1.0, 1.0)` 相当に init される。 1.0 にクランプされているので `max_scroll_y = (1.0 - 1.0).max(0) = 0`、 wheel イベントを `on_scroll_xy` が受けても `clamp(0, 0)` で 0 のまま。

理論上は (a) build_tree_measured が走る → (b) `scroll_measures` を `scroll_states` に書き戻して `viewport_height` / `content_height` が正しい値に更新される → (c) 次フレームから動く、 という流れだが**実際には動かない**。 何かが続けて 0 にロックしているように見える。

仮説:

- **(A) measured viewport_height も 0**: `flex_1().overflow_scroll()` のとき、 Taffy 側で flex item の高さが正しく計算されていない。 `overflow: Scroll` の場合 Taffy の min-content 計算が変わって flex-grow が効かないとか。
- **(B) measured content_height が viewport と同じ**: Taffy が overflow:Scroll の子要素を viewport 内に詰め込もうとして、 子供の合計高さが viewport と一致してしまう。 結果 `max_scroll_y = 0`。
- **(C) scroll_measures の書き戻しタイミング**: build → 書き戻し → 次フレームの patch_scroll_inner、 の流れで何か順序ズレがある。

**(A) か (B) のどちらか** が濃厚 (実際 explicit Px height を与えると直る、 つまり Taffy の自動高さ計算が問題)。

### 暫定回避

```rust
let scroll_h = (ctx.height - 180.0).max(200.0);
div().id("article-scroll")
    .w_full().h(Px(scroll_h))   // ← 明示
    .flex_col()
    .overflow_scroll()
    .children(items)
```

`naruhodo/web/src/main.rs::detail_view` で採用 (commit b1052a9)。

### 修正方針 (要検討)

- `patch_scroll_inner` の `viewport_h.unwrap_or(0.0)` を `unwrap_or_else(|| /* 何らかの fallback */)` にする — 例えば parent の measured height など。 ただし parent は patch 時点では確定していない。
- Taffy への style 変換で `overflow: Scroll` のときに `min_size_y = 0, max_size_y = ∞` 的に拡張、 flex-grow と矛盾しないようにする。
- もしくは VirtualList のようにそもそも overflow_scroll を使わず scroll_y で render 側が制御する設計に統一する。

### 発見経緯

`naruhodo/web` の法令詳細 view で 70 件程度の可変高条文を縦に並べたとき、 ホイール scroll が完全に効かなかった。 explicit height を入れたら直ったので、 sabitori の overflow_scroll の自動 viewport_h 推定に問題がある可能性が高い。

### 解決 (2026-06-10)

**真因は `patch_scroll_inner` でも Taffy の overflow マッピングでもなく、 `flex_1()` の意味論だった。**

`flex_1()` は `flex_grow: 1` しか設定しておらず、 `flex_basis` は `auto` のままだった。 CSS / Tailwind の `flex-1` は `flex: 1 1 0%` — **basis 0** が本体。 basis auto だと flex base size = コンテンツ高さ (条文 70 件分 ≒ 数千 px) になり:

1. 負の free space の shrink 再配分はコンテンツ量に比例して効くため、 scroll container は自分のスロットを大きくはみ出した高さに確定する (再現テストでは 500px のスロットに対して 571px)。
2. 兄弟 (header 等) が逆に押し潰される。
3. 入れ子の深さや内容量によっては `viewport_height ≒ content_height` になり `max_scroll = 0` → 「scroll が 0 にロック」 という見え方になる。

`patch_scroll_inner` → `scroll_measures` 書き戻しの feedback loop は正常に動いていた。 書き戻される **measured viewport_height 自体が Taffy レイアウトの段階で壊れていた** ので、 何 frame 回しても直らなかった、 が正体。 仮説でいうと (A) の変種。

修正: `ElementStyle` に `flex_basis` を追加 (default `Auto`、 `.basis(d)` builder)、 `flex_1()` が `flex_grow: 1` + `flex_basis: Px(0)` を設定するように変更 (Tailwind `flex-1` と同義に)。 `grow(v)` は従来どおり grow のみ (basis auto のコンテンツ起点で伸ばしたい場合はこちら)。

再現テスト: `sabitori-core/src/build.rs` の
`flex_grow_scroll_box_measures_viewport_and_content` (単層) と
`nested_flex_grow_scroll_box_matches_bugs_md_shape` (BUGS 報告そのままの入れ子形)。

`naruhodo/web/src/main.rs::detail_view` の `.h(Px(scroll_h))` workaround は不要になった (残しても無害)。
