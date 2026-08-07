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

   > `Cargo.lock` は `.gitignore` に入っているため**コミット対象にはならない**
   > （ライブラリなので lock は追跡しない方針）。ローカルのビルドを新バージョンに
   > 揃えるためだけに実行する。差分が出なくても正常。

4. コミットする:

   ```sh
   git commit -am "chore: release v0.2.0"
   ```

5. **annotated タグ** を打つ（lightweight は使わない — メッセージを残す）:

   ```sh
   git tag -a v0.2.0 -m "v0.2.0 — <一行サマリ>"
   ```

6. push する:

   ```sh
   git push origin main
   git push origin v0.2.0
   ```

7. （任意）GitHub Release を作る。CHANGELOG の該当セクションを本文にする:

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
