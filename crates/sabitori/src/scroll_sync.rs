//! Scroll-state synchronization helpers for hosts that drive the
//! build→GPU pipeline manually.
//!
//! The declarative runner manages `overflow_scroll` containers internally;
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
use sabitori_core::element::{Dimension, Element, Overflow};
use sabitori_widgets::ScrollView;

/// Walk the element tree and wire up scroll containers.
///
/// For every element with `.overflow_scroll()`:
/// - Synthesize a stable id from the tree path if none is set. The path
///   is a dot-separated list of child indices (e.g. `__scroll:0.2.1`),
///   stable across frames as long as the tree shape is stable. This
///   removes the "scroll doesn't work because I forgot .id()" trap.
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
    let mut path: Vec<usize> = Vec::new();
    patch_scroll_inner(root, states, &mut path);
}

fn patch_scroll_inner(
    element: &mut Element,
    states: &mut HashMap<String, ScrollView>,
    path: &mut Vec<usize>,
) {
    if element.style.overflow == Overflow::Scroll {
        if element.id.is_none() {
            let path_str = path
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(".");
            element.id = Some(format!("__scroll:{path_str}"));
        }
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
    for (i, child) in element.children.iter_mut().enumerate() {
        path.push(i);
        patch_scroll_inner(child, states, path);
        path.pop();
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
                .id("list")
                .flex_1()
                .flex_col()
                .overflow_scroll()
                .children(rows),
        )
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

    #[test]
    fn patch_synthesizes_id_for_anonymous_scroller() {
        let mut states = HashMap::new();
        let mut root = div().w(Px(100.0)).h(Px(100.0)).child(
            div().flex_1().flex_col().overflow_scroll(),
        );
        patch_scroll_offsets(&mut root, &mut states);
        assert_eq!(root.children[0].id.as_deref(), Some("__scroll:0"));
        assert!(states.contains_key("__scroll:0"));
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
