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
/// the pointer that **can still move in that direction**. Returns `true`
/// when a container consumed the delta. Hit regions are stored
/// front-to-back, so an inner scroller is asked first; one sitting at its
/// end is skipped and the wheel reaches the next container out (and, when
/// none can move, the app via `on_scroll_xy`). Deltas are in logical pixels
/// (winit `LineDelta` is multiplied by [`sabitori_input::LINE_DELTA_PX`]
/// before this call).
///
/// 以前は最初に見つかった管理コンテナに**無条件で**渡して `true` を返していた
/// ([#58](https://github.com/Mutafika/sabitori/issues/58))。内側のリストが下端に
/// 居ても外側のページが動かず、ホイールがそこで死ぬ。
///
/// トラックパッドの 1 ジェスチャの間は届け先を固定したい (途中で内側が端に
/// 達した瞬間に外側が動き出す「跳ね」を防ぐ) ので、ランタイムは位相を知っている
/// [`WheelLatch::route`] を使う。位相の無いホスト (刻みホイールだけ、埋め込み) は
/// こちらで足りる。
pub fn route_wheel(
    build: &BuildResult,
    states: &mut HashMap<String, ScrollView>,
    x: f32,
    y: f32,
    delta_x: f32,
    delta_y: f32,
) -> bool {
    resolve_and_scroll(build, states, x, y, delta_x, delta_y).is_some()
}

/// カーソル下で `delta` の向きへ動ける最も内側の管理コンテナに渡し、その id を返す。
fn resolve_and_scroll(
    build: &BuildResult,
    states: &mut HashMap<String, ScrollView>,
    x: f32,
    y: f32,
    delta_x: f32,
    delta_y: f32,
) -> Option<String> {
    let pt = sabitori_core::Point::new(x, y);
    for region in &build.hit_regions {
        if !region.rect.contains(pt) {
            continue;
        }
        let Some(ref id) = region.id else { continue };
        let Some(sv) = states.get_mut(id) else { continue };
        if sv.can_consume_wheel(delta_x, delta_y) {
            sv.on_scroll_xy(delta_x, delta_y);
            return Some(id.clone());
        }
    }
    None
}

/// カーソル下に管理コンテナが 1 つでも在るか (動けるかは問わない)。
fn any_container_under(build: &BuildResult, states: &HashMap<String, ScrollView>, x: f32, y: f32) -> bool {
    let pt = sabitori_core::Point::new(x, y);
    build
        .hit_regions
        .iter()
        .any(|r| r.rect.contains(pt) && r.id.as_ref().is_some_and(|id| states.contains_key(id)))
}

/// トラックパッドの 1 ジェスチャの間、ホイールの届け先を固定する (macOS の latching)。
///
/// [`route_wheel`] の「動けるコンテナへ」だけだと、内側のリストを下端まで払った
/// **その指の続き**で外側のページが動き出す。macOS のネイティブも Chrome も、
/// ジェスチャの最初に決めた届け先を `Ended` まで変えない (端では止まる／ゴムで
/// 伸びる) ので同じにする。慣性 (`Ended` の後に `Moved` として続く) も同じ
/// 届け先へ流し、端に達したらそこで**止める** — 止まりかけの慣性を外側へ横流し
/// すると、指を離した後にページが勝手に動き出す。
///
/// 刻みホイール (`precise == false`。位相も常に `Moved`) にジェスチャは無いので、
/// ノッチごとに解決し直す。ラッチは精密入力の `Started` を見たジェスチャの中でしか
/// 掛からない。⚠️ 位相を配るのは winit では macOS / iOS だけで、Windows の
/// precision touchpad は `PixelDelta` + `Moved` しか来ない。そこではラッチは
/// 掛からず、イベントごとに「動けるコンテナへ」で解決する (跳ねは防げない)。
#[derive(Debug, Default)]
pub struct WheelLatch {
    /// `Started` から `Ended` / `Cancelled` まで。
    in_gesture: bool,
    /// このジェスチャ (と続く慣性) の届け先。
    target: Option<String>,
}

impl WheelLatch {
    pub fn new() -> Self {
        Self::default()
    }

    /// 現在ラッチしている管理コンテナの id。テストと診断用。
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    /// 位相つきでルーティングする。戻り値は [`route_wheel`] と同じ「管理コンテナが
    /// 消費したか」。`precise` は `InputEvent::Wheel` のそれ (刻みホイールなら
    /// `false`)。
    #[allow(clippy::too_many_arguments)]
    pub fn route(
        &mut self,
        build: &BuildResult,
        states: &mut HashMap<String, ScrollView>,
        x: f32,
        y: f32,
        delta_x: f32,
        delta_y: f32,
        precise: bool,
        phase: sabitori_input::WheelPhase,
    ) -> bool {
        use sabitori_input::WheelPhase;

        if !precise || phase == WheelPhase::Started {
            // 刻みホイールにジェスチャは無い。ノッチごとに解決し直す。
            // 精密入力は `Started` で張り替える。
            self.in_gesture = precise;
            self.target = None;
        }

        // ラッチ先がもう画面に無い / カーソルが外れた (ジェスチャ中に窓が組み
        // 替わった) なら忘れる。
        if let Some(id) = self.target.as_deref() {
            let pt = sabitori_core::Point::new(x, y);
            let still_there = states.contains_key(id)
                && build
                    .hit_regions
                    .iter()
                    .any(|r| r.id.as_deref() == Some(id) && r.rect.contains(pt));
            if !still_there {
                self.target = None;
            }
        }

        let zero = delta_x == 0.0 && delta_y == 0.0;
        let mut consumed = false;

        if let Some(id) = self.target.clone() {
            let sv = states.get_mut(&id).expect("checked above");
            // ジェスチャ中は端でも手放さない (跳ね防止)。慣性は動ける間だけ動かし、
            // 端では黙って飲む (外側へ横流ししない)。どちらも「消費した」。
            if !zero && (self.in_gesture || sv.can_consume_wheel(delta_x, delta_y)) {
                sv.on_scroll_xy(delta_x, delta_y);
            }
            consumed = true;
        }

        if !consumed && !zero {
            let resolved = resolve_and_scroll(build, states, x, y, delta_x, delta_y);
            consumed = resolved.is_some();
            if self.in_gesture && resolved.is_some() {
                self.target = resolved;
            }
        }

        // delta 0 (Started / Ended の通知だけ) は、管理コンテナの上なら「消費」に
        // しておく。アプリの `on_scroll_xy(0, 0)` を鳴らしても意味が無い。
        if !consumed && zero {
            consumed = any_container_under(build, states, x, y);
        }

        if matches!(phase, WheelPhase::Ended | WheelPhase::Cancelled) {
            // 届け先は慣性のために残す。次の `Started` で張り替わる。
            self.in_gesture = false;
        }
        consumed
    }
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

/// #58: 端に達したコンテナはホイールを消費しない (外側へ、最後はアプリへ)。
/// トラックパッドの 1 ジェスチャの間は届け先を固定する。
#[cfg(test)]
mod chaining_tests {
    use super::*;
    use sabitori_core::build::build_tree;
    use sabitori_core::element::{div, Px};
    use sabitori_input::WheelPhase::{Ended, Moved, Started};

    /// 外側 400x300 の中に、ヘッダ 100px + 内側リスト (150px、20 行 × 40px) +
    /// 1000px のフッタ。外側も内側もスクロールできる。内側の矩形は y 100..250。
    fn nested_tree() -> Element {
        div().w(Px(400.0)).h(Px(300.0)).flex_col().child(
            div().scroll("outer").w_full().h(Px(300.0)).flex_col().children([
                div().w_full().h(Px(100.0)),
                div().scroll("inner").w_full().h(Px(150.0)).flex_col().children(
                    (0..20).map(|_| div().w_full().h(Px(40.0))).collect::<Vec<_>>(),
                ),
                div().w_full().h(Px(1000.0)),
            ]),
        )
    }

    fn nested() -> (BuildResult, HashMap<String, ScrollView>) {
        let mut states = HashMap::new();
        let mut root = nested_tree();
        patch_scroll_offsets(&mut root, &mut states);
        let build = build_tree(&root, 400.0, 300.0);
        apply_scroll_measures(&build, &mut states);
        assert!(states["inner"].can_scroll_y(-1.0), "前提: 内側は動ける");
        assert!(states["outer"].can_scroll_y(-1.0), "前提: 外側も動ける");
        (build, states)
    }

    /// 内側の上に居て、内側に余地があれば内側だけが動く (従来どおり)。
    #[test]
    fn inner_scroller_with_room_takes_the_wheel() {
        let (build, mut states) = nested();
        assert!(route_wheel(&build, &mut states, 200.0, 175.0, 0.0, -60.0));
        assert!(states["inner"].scroll_y.target() > 0.0);
        assert_eq!(states["outer"].scroll_y.target(), 0.0);
    }

    /// 内側が下端なら外側へ。以前は内側が無条件に飲んで `true` を返していた。
    #[test]
    fn inner_scroller_at_its_end_lets_the_wheel_through_to_the_outer() {
        let (build, mut states) = nested();
        states.get_mut("inner").unwrap().on_scroll_xy(0.0, -100_000.0);

        assert!(route_wheel(&build, &mut states, 200.0, 175.0, 0.0, -60.0), "外側が消費する");
        assert!(states["outer"].scroll_y.target() > 0.0, "外側が動いた");

        // 戻る向きは内側が動けるので内側へ。
        let outer_before = states["outer"].scroll_y.target();
        assert!(route_wheel(&build, &mut states, 200.0, 175.0, 0.0, 60.0));
        assert_eq!(states["outer"].scroll_y.target(), outer_before, "外側が動いてはいけない");
    }

    /// どちらも端なら管理コンテナは消費しない → アプリの `on_scroll_xy` へ落ちる。
    #[test]
    fn when_every_container_is_at_its_end_the_wheel_falls_through() {
        let (build, mut states) = nested();
        states.get_mut("inner").unwrap().on_scroll_xy(0.0, -100_000.0);
        states.get_mut("outer").unwrap().on_scroll_xy(0.0, -100_000.0);
        assert!(!route_wheel(&build, &mut states, 200.0, 175.0, 0.0, -60.0));
    }

    /// ラッチ: ジェスチャの途中で内側が端に達しても外側は動かない (跳ね防止)。
    /// 慣性も同じ届け先で、端なら黙って飲む。次の `Started` で張り替わる。
    #[test]
    fn a_gesture_stays_latched_to_the_container_it_started_on() {
        let (build, mut states) = nested();
        let mut latch = WheelLatch::new();
        let at = (200.0, 175.0);

        assert!(latch.route(&build, &mut states, at.0, at.1, 0.0, 0.0, true, Started));
        assert!(latch.route(&build, &mut states, at.0, at.1, 0.0, -60.0, true, Moved));
        assert_eq!(latch.target(), Some("inner"));

        // 内側を下端まで払う。
        latch.route(&build, &mut states, at.0, at.1, 0.0, -100_000.0, true, Moved);
        assert!(!states["inner"].can_scroll_y(-1.0), "前提: 内側は下端");

        // 同じ指の続き: 外側は動かない。
        assert!(latch.route(&build, &mut states, at.0, at.1, 0.0, -60.0, true, Moved));
        assert_eq!(states["outer"].scroll_y.target(), 0.0, "ジェスチャ中に外側へ跳ねた");

        // 指を離した後の慣性も同じ届け先。端なので飲むだけで、外側へは流さない。
        assert!(latch.route(&build, &mut states, at.0, at.1, 0.0, 0.0, true, Ended));
        assert!(latch.route(&build, &mut states, at.0, at.1, 0.0, -30.0, true, Moved));
        assert_eq!(states["outer"].scroll_y.target(), 0.0, "慣性が外側へ横流しされた");

        // 次のジェスチャは張り替え: 内側が端なので外側へ。
        latch.route(&build, &mut states, at.0, at.1, 0.0, 0.0, true, Started);
        assert!(latch.route(&build, &mut states, at.0, at.1, 0.0, -60.0, true, Moved));
        assert_eq!(latch.target(), Some("outer"));
        assert!(states["outer"].scroll_y.target() > 0.0);
    }

    /// 刻みホイールにジェスチャは無い: ノッチごとに解決し直すので、内側が端に
    /// 達した次のノッチで外側が動く。ラッチも掛からない。
    #[test]
    fn discrete_wheel_notches_resolve_independently() {
        let (build, mut states) = nested();
        let mut latch = WheelLatch::new();
        latch.route(&build, &mut states, 200.0, 175.0, 0.0, -100_000.0, false, Moved);
        assert!(latch.target().is_none(), "刻みホイールでラッチしてはいけない");
        assert!(latch.route(&build, &mut states, 200.0, 175.0, 0.0, -60.0, false, Moved));
        assert!(states["outer"].scroll_y.target() > 0.0);
    }

    /// ラッチ中に精密でないノッチが来たら (トラックパッドとマウスの併用)、
    /// ラッチは捨ててノッチとして解決する。古い届け先に縛られない。
    #[test]
    fn a_discrete_notch_drops_a_stale_latch() {
        let (build, mut states) = nested();
        let mut latch = WheelLatch::new();
        latch.route(&build, &mut states, 200.0, 175.0, 0.0, -100_000.0, true, Started);
        assert_eq!(latch.target(), Some("inner"));
        assert!(latch.route(&build, &mut states, 200.0, 175.0, 0.0, -60.0, false, Moved));
        assert!(latch.target().is_none());
        assert!(states["outer"].scroll_y.target() > 0.0, "ノッチは外側へ");
    }

    /// delta 0 の位相通知 (`Started` / `Ended`) は、管理コンテナの上なら消費扱い、
    /// 外なら素通し。アプリに `on_scroll_xy(0, 0)` を鳴らす意味は無い。
    #[test]
    fn zero_delta_phase_notifications_are_swallowed_over_a_container() {
        let (build, mut states) = nested();
        let mut latch = WheelLatch::new();
        assert!(latch.route(&build, &mut states, 200.0, 175.0, 0.0, 0.0, true, Started));
        assert!(!latch.route(&build, &mut states, 200.0, 350.0, 0.0, 0.0, true, Started));
    }
}
