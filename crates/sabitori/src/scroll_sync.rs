//! Scroll-state synchronization helpers for hosts that drive the
//! build→GPU pipeline manually.
//!
//! The declarative runner manages `.scroll(id)` containers internally;
//! embedded hosts (e.g. a CAD app overlaying sabitori UI on its own wgpu
//! scene via [`UiOverlayRenderer`](sabitori_gpu::UiOverlayRenderer)) need the
//! same bookkeeping without the runner. The per-frame protocol is:
//!
//! ```ignore
//! // 1. (each frame) advance scroll springs
//! scroll_sync::tick_all(&mut states, dt);
//! // 2. build the element tree, then patch managed offsets into it
//! scroll_sync::patch_scroll_offsets(&mut root, &mut states);
//! let build = build_tree_measured(&root, w, h, &measurer);
//! // 3. feed measured content extents back into the states
//! scroll_sync::apply_scroll_measures(&build, &mut states);
//!
//! // On MouseWheel:
//! let handled = scroll_sync::route_wheel(&build, &mut states, x, y, dx, dy);
//! ```
//!
//! The declarative runner delegates to these functions, so embedded hosts
//! and the runner share one behavior.

use std::collections::HashMap;

use sabitori_core::build::BuildResult;
use sabitori_core::element::{Dimension, Element, Overflow, ScrollOwner};
use sabitori_widgets::ScrollView;

/// Walk the element tree and wire up scroll containers.
///
/// 対象は **`.scroll(id)` で作られたコンテナだけ** — `scroll_owner` が
/// [`ScrollOwner::Runtime`] かつ id を持つもの。 `.scroll_manual(x, y)` の
/// コンテナには触れない。
///
/// 以前はここで `Overflow::Scroll` の要素を**全部**管理対象にし、 id が無ければ
/// ツリー上の位置から合成していた。 それが 2 つのバグを生んでいた (issue #14):
///
/// - アプリが自分で持っているつもりのオフセットを毎フレーム上書きしていた
///   (= 手動モードが事実上存在しなかった)
/// - 合成 id (`__scroll:0.2.1`) は子インデックス由来なので、 兄弟が 1 つ増減
///   しただけで別 id になり、 スクロール位置が 0 に飛んだ
///
/// どちらも `.scroll(id)` が安定した id を要求することで根から消えている。
///
/// For every runtime-owned scroll container:
/// - Register a [`ScrollView`] in `states` on first sight, keyed by id.
/// - Set `scroll_x`/`scroll_y` from the managed state so the build picks
///   up the current scroll offset.
/// - Pre-fill `viewport_height` from either an explicit `.h(Px(..))`
///   OR the previous frame's measured height (if any). The authoritative
///   post-layout value is written back via [`apply_scroll_measures`];
///   this just avoids a first-frame jump.
///
/// `content_height` is deliberately NOT touched here — the authoritative
/// value comes from `scroll_measures` after layout. An earlier bug used
/// `children.len() * 32` as an estimate and kept clamping `scroll_y` to
/// 0 every frame because it understates content_height when children
/// are structured (e.g. one div containing a long article).
pub fn patch_scroll_offsets(root: &mut Element, states: &mut HashMap<String, ScrollView>) {
    patch_scroll_inner(root, states);
}

fn patch_scroll_inner(element: &mut Element, states: &mut HashMap<String, ScrollView>) {
    if element.style.overflow == Overflow::Scroll
        && element.style.scroll_owner == ScrollOwner::Runtime
    {
        // id はスクロール状態のキー。 `.scroll(id)` が必ず設定するので、 ここが
        // `None` になるのは生の `.overflow(Overflow::Scroll)` を使った場合だけ。
        // その場合はキーが無いので管理できない = アプリ所有として扱う。
        if let Some(ref id) = element.id {
            // Explicit height wins; otherwise reuse the previous frame's
            // measurement (0 on the very first frame — one jank frame is
            // fine and self-corrects immediately).
            let explicit_h = match element.style.height {
                Dimension::Px(h) => Some(h),
                _ => None,
            };
            let viewport_h = explicit_h
                .or_else(|| states.get(id).map(|sv| sv.viewport_height))
                .unwrap_or(0.0);

            let sv = states.entry(id.clone()).or_insert_with(|| {
                ScrollView::new(viewport_h.max(1.0), viewport_h.max(1.0))
            });
            if let Some(h) = explicit_h {
                sv.viewport_height = h;
            }
            element.style.scroll_x = sv.scroll_x.value();
            element.style.scroll_y = sv.scroll_y.value();
        }
    }
    for child in element.children.iter_mut() {
        patch_scroll_inner(child, states);
    }
}

/// Feed measured scroll extents (from a completed build) back into the
/// managed states. Updates both axes' viewport + content sizes; clamps
/// the scroll offset if the content shrank.
pub fn apply_scroll_measures(build: &BuildResult, states: &mut HashMap<String, ScrollView>) {
    for (id, measure) in &build.scroll_measures {
        if let Some(sv) = states.get_mut(id) {
            sv.viewport_width = measure.viewport_width;
            sv.viewport_height = measure.viewport_height;
            sv.set_content_size(measure.content_width, measure.content_height);
        }
    }
}

/// Route a wheel/trackpad scroll to the managed scroll container under
/// the pointer, if any. Returns `true` when a container consumed the
/// delta. Hit regions are stored front-to-back, so an inner scroller
/// wins over an outer one. Deltas are in logical pixels (winit
/// `LineDelta` is conventionally multiplied by 20 before this call).
pub fn route_wheel(
    build: &BuildResult,
    states: &mut HashMap<String, ScrollView>,
    x: f32,
    y: f32,
    delta_x: f32,
    delta_y: f32,
) -> bool {
    let pt = sabitori_core::Point::new(x, y);
    for region in &build.hit_regions {
        if region.rect.contains(pt) {
            if let Some(ref id) = region.id {
                if let Some(sv) = states.get_mut(id) {
                    sv.on_scroll_xy(delta_x, delta_y);
                    return true;
                }
            }
        }
    }
    false
}

/// Advance all scroll springs/flings by `dt` seconds.
pub fn tick_all(states: &mut HashMap<String, ScrollView>, dt: f32) {
    for sv in states.values_mut() {
        sv.tick(dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sabitori_core::build::build_tree;
    use sabitori_core::element::{div, Px};
    use sabitori_core::Color;

    /// 300px viewport with 50 rows of 40px → scrollable test tree.
    fn scroll_tree() -> Element {
        let rows: Vec<Element> = (0..50)
            .map(|_| div().w_full().h(Px(40.0)).bg(Color::WHITE))
            .collect();
        div().w(Px(400.0)).h(Px(300.0)).flex_col().child(
            div()
                .scroll("list")
                .flex_1()
                .flex_col()
                .children(rows),
        )
    }

    /// 50 行 (40px) — 上の `scroll_tree` と同じ中身を、別の外枠で使い回す。
    fn rows_50() -> Vec<Element> {
        (0..50)
            .map(|_| div().w_full().h(Px(40.0)).bg(Color::WHITE))
            .collect()
    }

    #[test]
    fn patch_registers_state_and_applies_offset() {
        let mut states = HashMap::new();
        let mut root = scroll_tree();
        patch_scroll_offsets(&mut root, &mut states);
        assert!(states.contains_key("list"), "id-bearing scroller registered");

        let build = build_tree(&root, 400.0, 300.0);
        apply_scroll_measures(&build, &mut states);
        let sv = states.get("list").unwrap();
        assert!(
            (sv.viewport_height - 300.0).abs() < 1.0,
            "viewport from layout, got {}",
            sv.viewport_height
        );
        assert!(sv.content_height > 1990.0, "content 50*40, got {}", sv.content_height);

        // Scroll down, settle the spring, and verify the offset is patched
        // into the next frame's tree.
        states.get_mut("list").unwrap().on_scroll_xy(0.0, -200.0);
        for _ in 0..120 {
            tick_all(&mut states, 1.0 / 60.0);
        }
        let mut root2 = scroll_tree();
        patch_scroll_offsets(&mut root2, &mut states);
        let scroller = &root2.children[0];
        assert!(
            scroller.style.scroll_y > 100.0,
            "offset patched into style, got {}",
            scroller.style.scroll_y
        );
    }

    /// **issue #14 の回帰テスト (A).** アプリが持つオフセットにランタイムが触らないこと。
    ///
    /// 以前は `Overflow::Scroll` の要素を無条件に管理対象にしていたため、
    /// `.scroll_offset(0.0, 500.0)` は初回フレームで 0 に潰されていた
    /// (`ScrollView::new()` の初期値が書き込まれる)。 id を付けなければ避けられる、
    /// ということも無く、 **id が無ければ合成されて管理対象になった**。
    /// つまり手動スクロールは事実上存在しなかった。
    #[test]
    fn app_owned_offset_is_left_alone() {
        let mut states = HashMap::new();
        let mut root = div()
            .w(Px(100.0))
            .h(Px(100.0))
            .child(div().flex_1().flex_col().scroll_manual(0.0, 500.0));

        patch_scroll_offsets(&mut root, &mut states);

        assert_eq!(
            root.children[0].style.scroll_y, 500.0,
            "アプリ所有のオフセットが上書きされた"
        );
        assert!(
            states.is_empty(),
            "アプリ所有のコンテナに管理状態を作ってはいけない"
        );
    }

    /// **issue #14 の回帰テスト (B).** ツリーの形が変わってもスクロール位置が残ること。
    ///
    /// 以前は id が無いと子インデックスから合成していた (`__scroll:0.2.1`)。
    /// ヘッダが出入りするだけで `__scroll:0` → `__scroll:1` に変わり、
    /// 別の状態を引いて位置が 0 に飛んだ。 `.scroll(id)` が安定した名前を要求する
    /// ので、 同じ id を書いている限りこれは起こらない。
    #[test]
    fn scroll_position_survives_a_sibling_appearing() {
        let mut states = HashMap::new();

        // ヘッダ無し: scroller は index 0。
        let mut a = div()
            .w(Px(400.0))
            .h(Px(300.0))
            .flex_col()
            .child(div().scroll("list").flex_1().flex_col().children(rows_50()));
        patch_scroll_offsets(&mut a, &mut states);
        apply_scroll_measures(&build_tree(&a, 400.0, 300.0), &mut states);

        states.get_mut("list").unwrap().on_scroll_xy(0.0, -500.0);
        for _ in 0..200 {
            tick_all(&mut states, 1.0 / 60.0);
        }
        let scrolled = states.get("list").unwrap().scroll_y.value();
        assert!(scrolled > 100.0, "前提: スクロールできている (got {scrolled})");

        // ヘッダが出現 → scroller は index 1 へずれる。
        let mut b = div().w(Px(400.0)).h(Px(300.0)).flex_col().children([
            div().w_full().h(Px(20.0)),
            div().scroll("list").flex_1().flex_col().children(rows_50()),
        ]);
        patch_scroll_offsets(&mut b, &mut states);

        assert!(
            b.children[1].style.scroll_y > 100.0,
            "兄弟が増えたらスクロール位置が飛んだ (got {})",
            b.children[1].style.scroll_y
        );
    }

    /// 生の `.overflow(Overflow::Scroll)` は id が無ければ管理対象にならない。
    /// キーが無い以上どうしようもないので、 黙って合成せずアプリ所有として扱う。
    #[test]
    fn raw_overflow_scroll_without_id_is_not_managed() {
        let mut states = HashMap::new();
        let mut root = div()
            .w(Px(100.0))
            .h(Px(100.0))
            .child(div().flex_1().flex_col().overflow(Overflow::Scroll));

        patch_scroll_offsets(&mut root, &mut states);

        assert!(states.is_empty(), "キーの無いコンテナを登録してはいけない");
        assert!(
            root.children[0].id.is_none(),
            "id を勝手に合成してはいけない (位置依存の id が #14 の原因だった)"
        );
    }

    #[test]
    fn route_wheel_hits_scroller_and_ignores_outside() {
        let mut states = HashMap::new();
        let mut root = scroll_tree();
        patch_scroll_offsets(&mut root, &mut states);
        let build = build_tree(&root, 400.0, 300.0);
        apply_scroll_measures(&build, &mut states);

        // Over the scroller → consumed, target moves.
        assert!(route_wheel(&build, &mut states, 200.0, 150.0, 0.0, -60.0));
        assert!(states.get("list").unwrap().scroll_y.target() > 0.0);

        // Outside any region → not consumed.
        assert!(!route_wheel(&build, &mut states, 200.0, 350.0, 0.0, -60.0));
    }
}
