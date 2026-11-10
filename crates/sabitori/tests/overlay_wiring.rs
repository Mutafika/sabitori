//! **外付け overlay の畳み込みが 1 本であること。**
//!
//! 0.58 までは `DeclarativeApp` と `SceneApp` が winit のループを別々に回していて
//! `ApplicationHandler` の実装が分かれていた (0.59 から `run_scene` も同じランタイムを
//! 回す — 下の `run_scene_has_no_runtime_of_its_own`)。overlay の畳み込み
//! (`overlay_list` へ描画を足し、当たり領域を手前へ差し込む) は**両方に必要**で、
//! かつては**両方に手書きされていた**。
//!
//! それが [#84] で表に出た。overlay は別のツリーとして組まれるので
//! `element_index` が 0 から振り直される。混ぜたまま番号から id を作ると
//! [`sabitori::a11y`] が同じ子を 2 つ並べた `TreeUpdate` を作り、
//! **menu を開いたまま読み上げが起きていると accesskit が panic して窓が落ちる**。
//! 最初の版は declarative だけに番号のずらしを入れていて、scene_app には
//! 同じ splice が残っていた — **どちらのテストも緑のまま**。当たり判定も描画も
//! 矩形で動いていて番号を見ないので、差は読み上げが動いている時にしか出ない。
//!
//! 畳み込みは `runtime_shared::absorb_overlay` の 1 本にまとめてある。ここは
//! **ランタイム側に手書きが戻っていないこと**を押さえる。中身の正しさは
//! `runtime_shared::tests::an_overlays_hit_regions_do_not_reuse_the_base_trees_numbers`。
//!
//! [#84]: https://github.com/Mutafika/sabitori/pull/84

/// 当たり領域を手前へ差し込む操作。`runtime_shared` の外に在ってはいけない。
const SPLICE: &str = "hit_regions.splice";

/// overlay の描画を地の overlay ストリームへ足す操作。同上。
const EXTEND: &str = "overlay_list.commands.extend";

fn lines_with(src: &str, needle: &str) -> Vec<(usize, String)> {
    src.lines()
        .enumerate()
        .filter(|(_, l)| l.contains(needle) && !l.trim_start().starts_with("//"))
        .map(|(i, l)| (i + 1, l.trim().to_string()))
        .collect()
}

/// **ランタイムは自分で畳み込まない。**
///
/// 落ちたら、そのランタイムの手書きを消して
/// `crate::runtime_shared::absorb_overlay(&mut build_result, overlay_build)`
/// を呼ぶこと。2 本に戻すと、次に番号の付け方を変えたとき片方が忘れられる。
#[test]
fn neither_runtime_folds_the_overlay_in_by_hand() {
    for (name, src) in [("declarative.rs", include_str!("../src/declarative.rs"))] {
        for needle in [SPLICE, EXTEND] {
            let found = lines_with(src, needle);
            assert!(
                found.is_empty(),
                "{name} が overlay を自分で畳み込んでいる ({needle}):\n{}\n\
                 → runtime_shared::absorb_overlay を呼ぶこと",
                found
                    .iter()
                    .map(|(n, l)| format!("  {name}:{n}  {l}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }
}

/// **畳み込みは `runtime_shared` に在る。**
///
/// 上のテストだけだと、畳み込みごと消しても通ってしまう (overlay の当たり領域が
/// どこにも入らなくなり、menu が押せなくなる) ので、置き場の側も見る。
#[test]
fn the_fold_lives_in_runtime_shared() {
    let src = include_str!("../src/runtime_shared.rs");
    for needle in [SPLICE, EXTEND] {
        assert!(
            !lines_with(src, needle).is_empty(),
            "runtime_shared から {needle} が消えている"
        );
    }
}

/// **ランタイムが実際に呼んでいる。**
#[test]
fn the_runtime_calls_the_shared_fold() {
    assert!(
        !lines_with(include_str!("../src/declarative.rs"), "absorb_overlay").is_empty(),
        "declarative.rs が absorb_overlay を呼んでいない"
    );
}

/// **`run_scene` は自分のランタイムを持たない。**
///
/// 0.58 までは `scene_app.rs` に宣言的ランタイムを手で写したものがあり (約 1,500 行)、
/// 直しが届かずにずれていた (無効なボタンが押せる・キーを離したことが届かない・
/// Retina で文字が 1 倍・登録した部品が黙って何もしない)。今は `run_scene` も
/// `declarative::run_with` を回し、主窓の描く段だけを差し込む。ここに winit のループを
/// 書き戻したら、また 2 本に割れる。
#[test]
fn run_scene_has_no_runtime_of_its_own() {
    let src = include_str!("../src/scene_app.rs");
    for needle in ["impl<A: SceneApp> ApplicationHandler", "fn window_event", "fn about_to_wait", "run_app("] {
        assert!(
            lines_with(src, needle).is_empty(),
            "scene_app.rs に自前のランタイム ({needle}) がある → declarative::run_with を使うこと"
        );
    }
    assert!(!lines_with(src, "run_with(").is_empty(), "run_scene が declarative::run_with を回していない");
}
