//! Slider routing helpers for hosts that drive the build→GPU pipeline
//! manually (same家系 as [`crate::scroll_sync`]).
//!
//! [`SliderState`] already owns the drag math, but it needs the track's
//! **screen-space rect** — which an embedded host only knows from the
//! frame's [`BuildResult`] hit regions. これが「ウィジェットはあるのに
//! 埋め込みホストから使えず ±ステッパーで代替される」原因だったので、
//! id → [`SliderState`] の HashMap と BuildResult を突き合わせる配送
//! ヘルパーを公式化する:
//!
//! ```ignore
//! // app state:
//! let mut sliders: HashMap<String, SliderState> = HashMap::new();
//! sliders.insert("sld-opacity".into(), SliderState::from_ranged(0.5, 0.0, 1.0));
//!
//! // view(): 描画は forms::slider / labeled_slider (track が id を持つ)
//! labeled_slider("sld-opacity", "透過率", &format!("{:.0}%", v * 100.0),
//!                state.value(), 70.0, 140.0, 40.0, /* colors… */);
//!
//! // 左ボタン press (UI ヒット時):
//! if let Some(id) = slider_sync::route_press(&build, &mut sliders, x, y) {
//!     /* id のスライダーがドラッグ開始 — クリック位置へ即ジャンプ済み */
//! }
//! // pointer move (ボタン押下中):
//! if slider_sync::route_move(&build, &mut sliders, x) { /* 値が変わった */ }
//! // 左ボタン release:
//! slider_sync::route_release(&mut sliders);
//! ```
//!
//! Note: ドラッグ中はポインタがトラック矩形の外に出ても `route_move` が
//! 追従する（トラック x 範囲へのクランプは `SliderState` 側の仕事）。

use std::collections::HashMap;

use sabitori_core::build::BuildResult;
use sabitori_widgets::SliderState;

/// On left-press at `(x, y)`: if the topmost hit region's id has a
/// managed [`SliderState`], begin a drag (the value snaps to the click
/// position). Returns the slider id when one grabbed the pointer.
pub fn route_press(
    build: &BuildResult,
    sliders: &mut HashMap<String, SliderState>,
    x: f32,
    y: f32,
) -> Option<String> {
    let region = build.hit_region_at(x, y)?;
    let id = region.id.clone()?;
    let state = sliders.get_mut(&id)?;
    state.begin_drag(x, region.rect.origin.x, region.rect.size.width);
    Some(id)
}

/// On pointer move while the button is held: continue any in-progress
/// drag, looking the track rect up from the current frame's build (so
/// layout shifts mid-drag stay correct). Returns true when a value
/// changed.
pub fn route_move(
    build: &BuildResult,
    sliders: &mut HashMap<String, SliderState>,
    x: f32,
) -> bool {
    let mut changed = false;
    for (id, state) in sliders.iter_mut() {
        if !state.dragging {
            continue;
        }
        if let Some(rect) = build.region_rect(id) {
            changed |= state.drag_to(x, rect.origin.x, rect.size.width);
        }
    }
    changed
}

/// On left-release: end all drags. Returns true when a drag was active.
pub fn route_release(sliders: &mut HashMap<String, SliderState>) -> bool {
    let mut any = false;
    for state in sliders.values_mut() {
        if state.dragging {
            state.end_drag();
            any = true;
        }
    }
    any
}

/// Whether any managed slider is currently being dragged (pointer
/// capture: the host should keep routing moves here and suppress camera
/// drag even if the pointer leaves the track).
pub fn any_dragging(sliders: &HashMap<String, SliderState>) -> bool {
    sliders.values().any(|s| s.dragging)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sabitori_core::build::build_tree;
    use sabitori_core::element::{div, Element, Px};
    use sabitori_core::forms::slider;
    use sabitori_core::Color;

    /// A 200px-wide slider track at x=50, y=10 (inside a fixed column).
    fn tree(value: f32) -> Element {
        div()
            .w(Px(400.0))
            .h(Px(300.0))
            .flex_col()
            .child(
                div().w_full().h(Px(44.0)).pl(Px(50.0)).pt(Px(10.0)).child(slider(
                    "sld",
                    value,
                    200.0,
                    Color::from_hex("#333333"),
                    Color::from_hex("#6c63ff"),
                    Color::WHITE,
                )),
            )
    }

    fn states(v: f32) -> HashMap<String, SliderState> {
        let mut m = HashMap::new();
        m.insert("sld".to_string(), SliderState::new(v));
        m
    }

    #[test]
    fn press_on_track_starts_drag_and_snaps() {
        let build = build_tree(&tree(0.0), 400.0, 300.0);
        let rect = build.region_rect("sld").expect("slider laid out");
        let mut sliders = states(0.0);

        let mid_x = rect.origin.x + rect.size.width / 2.0;
        let mid_y = rect.origin.y + rect.size.height / 2.0;
        let grabbed = route_press(&build, &mut sliders, mid_x, mid_y);
        assert_eq!(grabbed.as_deref(), Some("sld"));
        assert!(any_dragging(&sliders));
        let v = sliders["sld"].value();
        assert!((v - 0.5).abs() < 0.05, "snapped to click position, got {v}");
    }

    #[test]
    fn move_updates_only_while_dragging_and_clamps() {
        let build = build_tree(&tree(0.0), 400.0, 300.0);
        let rect = build.region_rect("sld").unwrap();
        let mut sliders = states(0.25);

        // Not dragging → no change.
        assert!(!route_move(&build, &mut sliders, rect.origin.x + 100.0));
        assert!((sliders["sld"].value() - 0.25).abs() < 1e-6);

        // Drag to far beyond the right edge → clamped to 1.0.
        route_press(
            &build,
            &mut sliders,
            rect.origin.x + 10.0,
            rect.origin.y + 5.0,
        );
        assert!(route_move(&build, &mut sliders, rect.origin.x + 9999.0));
        assert_eq!(sliders["sld"].value(), 1.0);

        // Release stops further updates.
        assert!(route_release(&mut sliders));
        assert!(!route_move(&build, &mut sliders, rect.origin.x));
        assert_eq!(sliders["sld"].value(), 1.0);
        assert!(!route_release(&mut sliders), "no active drag left");
    }

    #[test]
    fn press_outside_or_on_unmanaged_id_is_ignored() {
        let build = build_tree(&tree(0.5), 400.0, 300.0);
        let mut sliders = states(0.5);

        // Outside every region.
        assert!(route_press(&build, &mut sliders, 390.0, 290.0).is_none());
        assert!(!any_dragging(&sliders));

        // Over a region whose id has no managed state.
        let mut empty: HashMap<String, SliderState> = HashMap::new();
        let rect = build.region_rect("sld").unwrap();
        assert!(route_press(
            &build,
            &mut empty,
            rect.origin.x + 5.0,
            rect.origin.y + 5.0
        )
        .is_none());
    }
}
