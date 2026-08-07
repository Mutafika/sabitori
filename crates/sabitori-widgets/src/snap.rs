//! Edge/guide snapping for floating panels (docking + panel-to-panel align).
//!
//! Pure geometry, paired with [`crate::WindowDragState`]: when a floating panel
//! is released, the host collects candidate guide coordinates (viewport edges,
//! other panels' edges) into [`SnapGuides`] and calls [`snap_rect`] to pull the
//! panel flush if one of its edges lands within `threshold` of a guide. X and Y
//! snap independently, so a panel can dock to the left wall while also aligning
//! its top with a neighbour.
//!
//! The host owns the meaning of each guide — this module only does
//! "nearest-edge-within-threshold" math, so it serves edge docking and
//! panel-to-panel adjacency with the same primitive.

/// Candidate guide coordinates a moving rect's edges may snap to.
///
/// - `x_left`: x-coords the mover's **left** edge aligns to (e.g. viewport
///   left, a neighbour's left for alignment, a neighbour's right for adjacency).
/// - `x_right`: x-coords the mover's **right** edge aligns to.
/// - `y_top` / `y_bottom`: the horizontal-line equivalents.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SnapGuides {
    pub x_left: Vec<f32>,
    pub x_right: Vec<f32>,
    pub y_top: Vec<f32>,
    pub y_bottom: Vec<f32>,
}

impl SnapGuides {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a rectangle's four edges as guides the mover can both align to
    /// (left↔left, right↔right) and butt against (left↔right, right↔left).
    /// Used for panel-to-panel snapping.
    pub fn add_rect_edges(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (left, right, top, bottom) = (x, x + w, y, y + h);
        self.x_left.push(left);
        self.x_left.push(right);
        self.x_right.push(left);
        self.x_right.push(right);
        self.y_top.push(top);
        self.y_top.push(bottom);
        self.y_bottom.push(top);
        self.y_bottom.push(bottom);
    }
}

/// Snap a moving rect `(x, y, w, h)` to its nearest guides within `threshold`.
/// X and Y are resolved independently; size is preserved. Returns the adjusted
/// top-left `(x, y)` (unchanged on an axis with no guide in range).
pub fn snap_rect(x: f32, y: f32, w: f32, h: f32, g: &SnapGuides, threshold: f32) -> (f32, f32) {
    let nx = snap_axis(x, w, &g.x_left, &g.x_right, threshold).unwrap_or(x);
    let ny = snap_axis(y, h, &g.y_top, &g.y_bottom, threshold).unwrap_or(y);
    (nx, ny)
}

/// Resolve one axis: try to pull either the low edge (`lo`) onto a `lo_guides`
/// entry or the high edge (`lo + size`) onto a `hi_guides` entry, whichever is
/// nearest within `threshold`. Returns the new low-edge coordinate, or `None`.
fn snap_axis(
    lo: f32,
    size: f32,
    lo_guides: &[f32],
    hi_guides: &[f32],
    threshold: f32,
) -> Option<f32> {
    let hi = lo + size;
    let mut best: Option<(f32, f32)> = None; // (new_lo, distance)
    for &g in lo_guides {
        let d = (lo - g).abs();
        if d <= threshold && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((g, d));
        }
    }
    for &g in hi_guides {
        let d = (hi - g).abs();
        if d <= threshold && best.is_none_or(|(_, bd)| d < bd) {
            best = Some((g - size, d));
        }
    }
    best.map(|(new_lo, _)| new_lo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_guides_leaves_rect_unmoved() {
        let g = SnapGuides::new();
        assert_eq!(snap_rect(37.0, 41.0, 200.0, 300.0, &g, 24.0), (37.0, 41.0));
    }

    #[test]
    fn left_edge_snaps_to_left_guide_within_threshold() {
        let mut g = SnapGuides::new();
        g.x_left.push(0.0); // viewport left wall
        // left edge at 10 is within 24 of 0 → pull flush to 0
        let (x, _) = snap_rect(10.0, 50.0, 200.0, 300.0, &g, 24.0);
        assert_eq!(x, 0.0);
    }

    #[test]
    fn right_edge_snaps_to_right_guide_preserving_width() {
        let mut g = SnapGuides::new();
        g.x_right.push(1000.0); // viewport right wall
        // right edge at 990 (x=790,w=200) is within 24 of 1000 → x becomes 800
        let (x, _) = snap_rect(790.0, 50.0, 200.0, 300.0, &g, 24.0);
        assert_eq!(x, 800.0); // 1000 - 200, width preserved
    }

    #[test]
    fn outside_threshold_does_not_snap() {
        let mut g = SnapGuides::new();
        g.x_left.push(0.0);
        let (x, y) = snap_rect(100.0, 100.0, 200.0, 300.0, &g, 24.0);
        assert_eq!((x, y), (100.0, 100.0));
    }

    #[test]
    fn nearest_guide_wins() {
        let mut g = SnapGuides::new();
        g.x_left.push(0.0); // dist 18 from lo=18
        g.x_left.push(25.0); // dist 7 from lo=18 (nearer)
        let (x, _) = snap_rect(18.0, 50.0, 200.0, 300.0, &g, 24.0);
        assert_eq!(x, 25.0);
    }

    #[test]
    fn axes_snap_independently() {
        let mut g = SnapGuides::new();
        g.x_left.push(0.0); // x snaps
        g.y_top.push(500.0); // y far, no snap
        let (x, y) = snap_rect(8.0, 100.0, 200.0, 300.0, &g, 24.0);
        assert_eq!(x, 0.0);
        assert_eq!(y, 100.0);
    }

    #[test]
    fn panel_to_panel_adjacency_via_add_rect_edges() {
        // neighbour panel A at (100, 100) size (240, 300): right edge = 340.
        let mut g = SnapGuides::new();
        g.add_rect_edges(100.0, 100.0, 240.0, 300.0);
        // mover B left edge at 345 is within 24 of A.right (340) → butt flush.
        let (x, _) = snap_rect(345.0, 110.0, 280.0, 300.0, &g, 24.0);
        assert_eq!(x, 340.0, "B.left snaps onto A.right (side-by-side)");
    }

    #[test]
    fn panel_to_panel_top_alignment() {
        // neighbour A top = 100. mover B top at 108 aligns to 100.
        let mut g = SnapGuides::new();
        g.add_rect_edges(100.0, 100.0, 240.0, 300.0);
        let (_, y) = snap_rect(500.0, 108.0, 280.0, 300.0, &g, 24.0);
        assert_eq!(y, 100.0, "B.top aligns with A.top");
    }
}
