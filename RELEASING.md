# リリース手順

さびとり (sabitori) は **単一バージョン** のワークスペース。13 クレート全部が
`[workspace.package]` の `version` を共有するので、**リリースごとにタグは 1 個**。
迷子防止のため、必ずこの手順を踏む。

## バージョニング方針 (0.x)

1.0.0 までは API が動く可能性がある。

| 変更内容 | 上げる桁 | 例 |
|---|---|---|
| 機能追加 / 破壊的変更 | **minor** | `0.1.0` → `0.2.0` |
| 修正のみ | **patch** | `0.1.0` → `0.1.1` |

API が安定したら `1.0.0` を切る。

## 手順

1. `main` を最新にする（リリース対象の PR はマージ済みにしておく）。

   ```sh
   git switch main && git pull
   ```

2. **CHANGELOG.md** の `[Unreleased]` を新バージョンに繰り上げ、日付を入れる。
   下部の compare リンクも更新する。

   > **⚠️ ここは飛ばせない。** 過去に 17 版ぶん記載が漏れ、利用側が「その機能が
   > 既にあること」に気づけず自前実装する事故が起きた
   > ([#24](https://github.com/Mutafika/sabitori/issues/24))。
   > **`[Unreleased]` が空のままならリリースしない。** 書くことが無いなら、
   > そもそも上げる必要が無い。

   書けたら、全タグに節と compare リンクがあることを確認する:

   ```sh
   for t in $(git tag --sort=v:refname); do v=${t#v}
     grep -q "^## \[$v\]" CHANGELOG.md || echo "節が無い: $t"
     grep -q "^\[$v\]:"   CHANGELOG.md || echo "リンクが無い: $t"
   done
   ```

3. **ワークスペースのバージョンを上げる** — `Cargo.toml` の `[workspace.package]`:

   ```toml
   [workspace.package]
   version = "0.2.0"   # ← bump
   ```

   `Cargo.lock` も更新する:

   ```sh
   cargo update --workspace
   ```

   > `Cargo.lock` は**追跡している**ので、これもコミット対象になる
   > （ワークスペースメンバーのバージョンが書き換わる）。ライブラリだが lock を
   > 追跡しているのは、CI を再現可能にするため — 追跡しないと CI が毎回まっさらに
   > 解決してしまい、依存ツリーから作る成果物を検査できない。下流の利用者には
   > 影響しない。

4. **`THIRD-PARTY-LICENSES.html` を再生成する。**

   ```sh
   cargo about --version                                # 0.9.2 であること
   cargo install cargo-about --version 0.9.2 --locked --features cli   # 違えば入れ直す
   cargo about generate about.hbs -o THIRD-PARTY-LICENSES.html --all-features --locked
   ```

   > **⚠️ バージョンを確認してから走らせる。** 版が違うと**黙って壊れた出力を
   > 出す**。0.8.4 は `self_cell` の `GPL-2.0` を捌けず、標準エラーに ERROR を
   > 1 行吐くだけで**終了コードは 0**、`ring` の Apache-2.0 全文を含む
   > 47 エントリを落とした HTML を書く。パイプで握り潰していると気付けない。
   > 生成後は必ず `git diff --stat` を見て、**工程 3 で上げた自クレートの
   > バージョンだけが動いていること**を確かめる。
   >
   > `--features cli` は要る。無いとバイナリが入らず、古い版が残ったままになる
   > （`cargo install` 自体は成功したように見える）。

   > CI (`.github/workflows/license.yml`) も同じ検査をしているので、工程 3 で
   > 依存が動いていなければ差分は出ない。出たらコミットする。
   >
   > `--locked` は必須。付けないと解決し直してしまい、CI の結果と食い違う。

   > **⚠️ 飛ばさない。** 実際に v0.3.13 から v0.7.0 まで再生成されず、
   > `arboard` ほか 5 本の帰属が抜けたまま 4 マイナー分配られた。MIT と
   > Apache-2.0 は配布物への著作権表示を要求しているので、これは実害のある漏れ。

5. コミットする:

   ```sh
   git commit -am "chore: release v0.2.0"
   ```

6. **annotated タグ** を打つ（lightweight は使わない — メッセージを残す）:

   ```sh
   git tag -a v0.2.0 -m "v0.2.0 — <一行サマリ>"
   ```

7. push する:

   ```sh
   git push origin main
   git push origin v0.2.0
   ```

8. （任意）GitHub Release を作る。CHANGELOG の該当セクションを本文にする:

   ```sh
   gh release create v0.2.0 --title "v0.2.0" --notes-file <(...)
   # または --generate-notes でコミットから自動生成
   ```

## 初回 (v0.1.0) について

`version` はすでに `0.1.0`。初回はベースラインを刻むだけなので、`main` がきれいな
状態で:

```sh
git tag -a v0.1.0 -m "v0.1.0 — 初回リリース (baseline)"
git push origin v0.1.0
```
