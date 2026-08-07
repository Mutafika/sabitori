//! Linked dock group: N panels sharing one floating rect along a single axis.
//!
//! Where [`crate::WindowDragState`] is one floating panel, a [`DockGroup`] binds
//! several panels into a single floating unit laid out side-by-side (`Row`) or
//! stacked (`Col`). The group moves and resizes as one, and the seams between
//! panes are draggable splitters that re-proportion the neighbours.
//!
//! Like `WindowDragState`, this is **pure geometry** — it knows nothing about
//! element ids beyond holding the member panel-id strings, nothing about
//! hit-testing chrome, and nothing about rendering. The host (sabitori_ui)
//! maps splitter-handle ids and title grabs onto the methods here and reads the
//! derived pane rects each frame.
//!
//! Scope (v1): a **single split axis** per group (no nested trees). Merging a
//! third panel adds another pane along the same axis via [`DockGroup::split_pane`].

/// Split axis of a dock group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockAxis {
    /// Panes run left→right; splitters are vertical; ratios divide width.
    Row,
    /// Panes run top→bottom; splitters are horizontal; ratios divide height.
    Col,
}

/// Splitter strip thickness (logical px) — both the visual handle and hit band.
pub const SPLITTER_PX: f32 = 6.0;
/// Smallest a single pane may be shrunk to along the split axis (logical px).
pub const MIN_PANE_PX: f32 = 120.0;

/// Decide how a panel dropped at `cursor` should split `target` `(x, y, w, h)`:
/// the nearest edge picks the axis and which side the dropped panel lands on.
///
/// Returns `(axis, dropped_before)` where `dropped_before` is true when the
/// dropped panel should sit ahead of the target along `axis` (left for `Row`,
/// top for `Col`). Mirrors the nearest-edge rule used for the snap preview.
pub fn drop_split(target: (f32, f32, f32, f32), cursor: (f32, f32)) -> (DockAxis, bool) {
    let (tx, ty, tw, th) = target;
    let nx = if tw > 0.0 { ((cursor.0 - tx) / tw).clamp(0.0, 1.0) } else { 0.5 };
    let ny = if th > 0.0 { ((cursor.1 - ty) / th).clamp(0.0, 1.0) } else { 0.5 };
    let (dl, dr, dt, db) = (nx, 1.0 - nx, ny, 1.0 - ny);
    let m = dl.min(dr).min(dt).min(db);
    if m == dt {
        (DockAxis::Col, true) // top edge nearest → dropped panel on top
    } else if m == db {
        (DockAxis::Col, false) // bottom → dropped below
    } else if m == dl {
        (DockAxis::Row, true) // left → dropped on the left
    } else {
        (DockAxis::Row, false) // right → dropped on the right
    }
}

/// A group of two-or-more docked panels sharing one floating rect.
///
/// Invariants: `members.len() == ratios.len() >= 2`, and `ratios` sum to 1.0.
#[derive(Debug, Clone, PartialEq)]
pub struct DockGroup {
    /// Group rect `(x, y, w, h)` in logical px.
    rect: (f32, f32, f32, f32),
    axis: DockAxis,
    /// Member panel ids in pane order.
    members: Vec<String>,
    /// Pane size fractions, one per member, summing to 1.0.
    ratios: Vec<f32>,
}

impl DockGroup {
    /// Create a 2-pane group from `first` (pane 0) and `second` (pane 1),
    /// split evenly, filling `rect`.
    pub fn new(
        rect: (f32, f32, f32, f32),
        axis: DockAxis,
        first: impl Into<String>,
        second: impl Into<String>,
    ) -> Self {
        Self {
            rect,
            axis,
            members: vec![first.into(), second.into()],
            ratios: vec![0.5, 0.5],
        }
    }

    pub fn rect(&self) -> (f32, f32, f32, f32) {
        self.rect
    }
    pub fn axis(&self) -> DockAxis {
        self.axis
    }
    pub fn members(&self) -> &[String] {
        &self.members
    }
    pub fn ratios(&self) -> &[f32] {
        &self.ratios
    }
    pub fn len(&self) -> usize {
        self.members.len()
    }
    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
    /// Whether `id` is a member of this group.
    pub fn contains(&self, id: &str) -> bool {
        self.members.iter().any(|m| m == id)
    }
    /// Pane index of `id`, if a member.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.members.iter().position(|m| m == id)
    }

    /// Cumulative fraction at the left/top edge of pane `i` (sum of ratios `0..i`).
    fn prefix(&self, i: usize) -> f32 {
        self.ratios[..i].iter().sum()
    }

    /// Derived rect `(x, y, w, h)` of pane `i`, or `None` if out of range.
    /// Panes abut exactly; the splitter handle is drawn straddling the seam.
    pub fn pane_rect(&self, i: usize) -> Option<(f32, f32, f32, f32)> {
        if i >= self.members.len() {
            return None;
        }
        let (gx, gy, gw, gh) = self.rect;
        let start = self.prefix(i);
        let frac = self.ratios[i];
        Some(match self.axis {
            DockAxis::Row => (gx + gw * start, gy, gw * frac, gh),
            DockAxis::Col => (gx, gy + gh * start, gw, gh * frac),
        })
    }

    /// Derived rect of the splitter strip between pane `k` and `k + 1`, centered
    /// on their shared seam, spanning the cross axis. `None` if `k` has no
    /// following pane.
    pub fn splitter_rect(&self, k: usize) -> Option<(f32, f32, f32, f32)> {
        if k + 1 >= self.members.len() {
            return None;
        }
        let (gx, gy, gw, gh) = self.rect;
        let boundary = self.prefix(k + 1);
        let half = SPLITTER_PX / 2.0;
        Some(match self.axis {
            DockAxis::Row => (gx + gw * boundary - half, gy, SPLITTER_PX, gh),
            DockAxis::Col => (gx, gy + gh * boundary - half, gw, SPLITTER_PX),
        })
    }

    /// Translate the whole group (every pane follows).
    pub fn move_by(&mut self, dx: f32, dy: f32) {
        self.rect.0 += dx;
        self.rect.1 += dy;
    }

    /// Move the group's top-left corner to `(x, y)` without resizing.
    pub fn set_pos(&mut self, x: f32, y: f32) {
        self.rect.0 = x;
        self.rect.1 = y;
    }

    /// Resize the whole group; panes re-derive proportionally (ratios unchanged).
    /// Width/height are clamped so every pane keeps at least `MIN_PANE_PX` along
    /// the split axis.
    pub fn set_size(&mut self, w: f32, h: f32) {
        let n = self.members.len() as f32;
        let min_total = MIN_PANE_PX * n;
        match self.axis {
            DockAxis::Row => {
                self.rect.2 = w.max(min_total);
                self.rect.3 = h.max(MIN_PANE_PX);
            }
            DockAxis::Col => {
                self.rect.2 = w.max(MIN_PANE_PX);
                self.rect.3 = h.max(min_total);
            }
        }
    }

    /// Drag the splitter between pane `k` and `k + 1` so their shared seam tracks
    /// `pointer` (x for `Row`, y for `Col`). The two panes' combined fraction is
    /// preserved; each is clamped to `MIN_PANE_PX`. Returns true if ratios moved.
    pub fn drag_splitter(&mut self, k: usize, pointer: f32) -> bool {
        if k + 1 >= self.members.len() {
            return false;
        }
        let (gx, gy, gw, gh) = self.rect;
        let (origin, extent) = match self.axis {
            DockAxis::Row => (gx, gw),
            DockAxis::Col => (gy, gh),
        };
        if extent <= 0.0 {
            return false;
        }
        let min_frac = MIN_PANE_PX / extent;
        let combined = self.ratios[k] + self.ratios[k + 1];
        // Not enough room for both panes at minimum → leave as-is.
        if combined < min_frac * 2.0 {
            return false;
        }
        let left_edge = self.prefix(k);
        let want = (pointer - origin) / extent - left_edge;
        let new_k = want.clamp(min_frac, combined - min_frac);
        if (new_k - self.ratios[k]).abs() < 1e-6 {
            return false;
        }
        self.ratios[k] = new_k;
        self.ratios[k + 1] = combined - new_k;
        true
    }

    /// Pane index containing `(px, py)`, or `None` if outside the group rect.
    pub fn pane_at(&self, px: f32, py: f32) -> Option<usize> {
        if !self.contains_point(px, py) {
            return None;
        }
        let (gx, gy, gw, gh) = self.rect;
        let (along, origin, extent) = match self.axis {
            DockAxis::Row => (px, gx, gw),
            DockAxis::Col => (py, gy, gh),
        };
        if extent <= 0.0 {
            return Some(0);
        }
        let frac = ((along - origin) / extent).clamp(0.0, 1.0);
        let mut acc = 0.0;
        for (i, r) in self.ratios.iter().enumerate() {
            acc += r;
            if frac <= acc {
                return Some(i);
            }
        }
        Some(self.members.len() - 1)
    }

    /// Splitter index whose handle strip contains `(px, py)`, if any.
    pub fn splitter_at(&self, px: f32, py: f32) -> Option<usize> {
        (0..self.members.len().saturating_sub(1)).find(|&k| {
            self.splitter_rect(k)
                .map(|(rx, ry, rw, rh)| {
                    px >= rx && px <= rx + rw && py >= ry && py <= ry + rh
                })
                .unwrap_or(false)
        })
    }

    /// Whether `(px, py)` lies within the group rect.
    pub fn contains_point(&self, px: f32, py: f32) -> bool {
        let (gx, gy, gw, gh) = self.rect;
        px >= gx && px <= gx + gw && py >= gy && py <= gy + gh
    }

    /// Whether `(px, py)` lies **outside** the group rect — the pull-out test for
    /// detaching a member by dragging its title past the group edge.
    pub fn outside(&self, px: f32, py: f32) -> bool {
        !self.contains_point(px, py)
    }

    /// Split the pane holding `target_id` to insert `new_id` beside it, each
    /// taking half of the target pane's fraction. `before` puts `new_id` ahead of
    /// the target. No-op (returns false) if `target_id` is not a member.
    pub fn split_pane(&mut self, target_id: &str, new_id: impl Into<String>, before: bool) -> bool {
        let Some(ti) = self.index_of(target_id) else {
            return false;
        };
        let half = self.ratios[ti] / 2.0;
        self.ratios[ti] = half;
        let insert_at = if before { ti } else { ti + 1 };
        self.members.insert(insert_at, new_id.into());
        self.ratios.insert(insert_at, half);
        true
    }

    /// Remove `id` from the group and renormalize the remaining ratios to sum to
    /// 1.0. Returns true if removed. The caller should dissolve the group (and
    /// restore the lone member as an independent floating panel) once
    /// [`DockGroup::len`] drops below 2.
    pub fn remove(&mut self, id: &str) -> bool {
        let Some(i) = self.index_of(id) else {
            return false;
        };
        self.members.remove(i);
        self.ratios.remove(i);
        let sum: f32 = self.ratios.iter().sum();
        if sum > 0.0 {
            for r in &mut self.ratios {
                *r /= sum;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    fn group() -> DockGroup {
        // 800×600 group at origin, two even panes side-by-side.
        DockGroup::new((0.0, 0.0, 800.0, 600.0), DockAxis::Row, "a", "b")
    }

    #[test]
    fn new_is_two_even_panes() {
        let g = group();
        assert_eq!(g.len(), 2);
        assert_eq!(g.members(), &["a".to_string(), "b".to_string()]);
        assert!(approx(g.ratios()[0], 0.5) && approx(g.ratios()[1], 0.5));
        assert!(g.contains("a") && g.contains("b") && !g.contains("c"));
    }

    #[test]
    fn pane_rects_split_row_in_half() {
        let g = group();
        assert_eq!(g.pane_rect(0), Some((0.0, 0.0, 400.0, 600.0)));
        assert_eq!(g.pane_rect(1), Some((400.0, 0.0, 400.0, 600.0)));
        assert_eq!(g.pane_rect(2), None);
    }

    #[test]
    fn pane_rects_split_col_in_half() {
        let g = DockGroup::new((10.0, 20.0, 300.0, 400.0), DockAxis::Col, "a", "b");
        assert_eq!(g.pane_rect(0), Some((10.0, 20.0, 300.0, 200.0)));
        assert_eq!(g.pane_rect(1), Some((10.0, 220.0, 300.0, 200.0)));
    }

    #[test]
    fn splitter_straddles_the_seam() {
        let g = group();
        // Seam at x=400; strip centered on it, full height.
        let (x, y, w, h) = g.splitter_rect(0).unwrap();
        assert!(approx(x, 400.0 - SPLITTER_PX / 2.0));
        assert!(approx(y, 0.0) && approx(w, SPLITTER_PX) && approx(h, 600.0));
        assert_eq!(g.splitter_rect(1), None); // no seam after the last pane
    }

    #[test]
    fn move_by_translates_every_pane() {
        let mut g = group();
        g.move_by(100.0, 50.0);
        assert_eq!(g.rect(), (100.0, 50.0, 800.0, 600.0));
        assert_eq!(g.pane_rect(0), Some((100.0, 50.0, 400.0, 600.0)));
        assert_eq!(g.pane_rect(1), Some((500.0, 50.0, 400.0, 600.0)));
    }

    #[test]
    fn set_size_rescales_panes_proportionally() {
        let mut g = group();
        g.set_size(1000.0, 400.0);
        assert_eq!(g.pane_rect(0), Some((0.0, 0.0, 500.0, 400.0)));
        assert_eq!(g.pane_rect(1), Some((500.0, 0.0, 500.0, 400.0)));
    }

    #[test]
    fn set_size_clamps_to_min_total() {
        let mut g = group(); // 2 panes → min total width = 2*MIN_PANE_PX
        g.set_size(10.0, 10.0);
        assert!(approx(g.rect().2, MIN_PANE_PX * 2.0), "width clamped to 2 panes min");
        assert!(approx(g.rect().3, MIN_PANE_PX), "height clamped to min");
    }

    #[test]
    fn drag_splitter_repropotions_and_preserves_sum() {
        let mut g = group(); // seam at x=400
        assert!(g.drag_splitter(0, 600.0)); // push seam right to x=600
        assert!(approx(g.ratios()[0], 0.75), "pane 0 grows to 75%");
        assert!(approx(g.ratios()[1], 0.25), "pane 1 shrinks to 25%");
        assert!(approx(g.ratios()[0] + g.ratios()[1], 1.0), "sum preserved");
        assert_eq!(g.pane_rect(0), Some((0.0, 0.0, 600.0, 600.0)));
        assert_eq!(g.pane_rect(1), Some((600.0, 0.0, 200.0, 600.0)));
    }

    #[test]
    fn drag_splitter_clamps_to_min_pane() {
        let mut g = group();
        // Drag the seam far left, past where pane 0 would go below MIN_PANE_PX.
        g.drag_splitter(0, -1000.0);
        let p0 = g.pane_rect(0).unwrap();
        assert!(p0.2 >= MIN_PANE_PX - 1e-3, "pane 0 never narrower than MIN_PANE_PX");
    }

    #[test]
    fn pane_at_picks_the_right_pane() {
        let g = group();
        assert_eq!(g.pane_at(100.0, 300.0), Some(0)); // left half
        assert_eq!(g.pane_at(600.0, 300.0), Some(1)); // right half
        assert_eq!(g.pane_at(-5.0, 300.0), None); // outside
        assert_eq!(g.pane_at(900.0, 300.0), None);
    }

    #[test]
    fn splitter_at_hits_only_the_seam_band() {
        let g = group();
        assert_eq!(g.splitter_at(400.0, 300.0), Some(0)); // on the seam
        assert_eq!(g.splitter_at(200.0, 300.0), None); // mid-pane, not a seam
    }

    #[test]
    fn outside_is_the_pullout_test() {
        let g = group();
        assert!(!g.outside(400.0, 300.0)); // inside
        assert!(g.outside(400.0, 700.0)); // below the group → detach
        assert!(g.outside(-1.0, 300.0)); // left of the group
    }

    #[test]
    fn split_pane_adds_a_third_pane_keeping_sum() {
        let mut g = group(); // a=0.5, b=0.5
        assert!(g.split_pane("b", "c", false)); // insert c after b
        assert_eq!(g.members(), &["a".to_string(), "b".to_string(), "c".to_string()]);
        // b's 0.5 split into b=0.25, c=0.25
        assert!(approx(g.ratios()[0], 0.5));
        assert!(approx(g.ratios()[1], 0.25));
        assert!(approx(g.ratios()[2], 0.25));
        assert!(approx(g.ratios().iter().sum::<f32>(), 1.0));
        assert!(!g.split_pane("nope", "x", false)); // unknown target → no-op
    }

    #[test]
    fn split_pane_before_inserts_ahead_of_target() {
        let mut g = group();
        g.split_pane("a", "z", true); // z before a
        assert_eq!(g.members()[0], "z");
        assert_eq!(g.members()[1], "a");
    }

    #[test]
    fn remove_renormalizes_remaining_ratios() {
        let mut g = group();
        g.split_pane("b", "c", false); // a=0.5, b=0.25, c=0.25
        assert!(g.remove("a")); // remove the big pane
        assert_eq!(g.len(), 2);
        // remaining 0.25/0.25 renormalized → 0.5/0.5
        assert!(approx(g.ratios()[0], 0.5) && approx(g.ratios()[1], 0.5));
        assert!(approx(g.ratios().iter().sum::<f32>(), 1.0));
        assert!(!g.remove("ghost"));
    }

    #[test]
    fn drop_split_picks_axis_and_side_by_nearest_edge() {
        let target = (0.0, 0.0, 800.0, 600.0);
        // Near the left edge → Row split, dropped panel on the left.
        assert_eq!(drop_split(target, (40.0, 300.0)), (DockAxis::Row, true));
        // Near the right edge → Row, dropped on the right.
        assert_eq!(drop_split(target, (760.0, 300.0)), (DockAxis::Row, false));
        // Near the top → Col, dropped on top.
        assert_eq!(drop_split(target, (400.0, 30.0)), (DockAxis::Col, true));
        // Near the bottom → Col, dropped below.
        assert_eq!(drop_split(target, (400.0, 570.0)), (DockAxis::Col, false));
    }

    #[test]
    fn drop_split_then_new_group_matches_decision() {
        // The host builds the group from drop_split: order by `dropped_before`.
        let target = (0.0, 0.0, 800.0, 600.0);
        let (axis, before) = drop_split(target, (760.0, 300.0)); // right → Row, after
        let g = if before {
            DockGroup::new(target, axis, "dropped", "target")
        } else {
            DockGroup::new(target, axis, "target", "dropped")
        };
        assert_eq!(g.axis(), DockAxis::Row);
        assert_eq!(g.members(), &["target".to_string(), "dropped".to_string()]);
    }

    #[test]
    fn remove_until_one_signals_caller_to_dissolve() {
        let mut g = group();
        g.remove("a");
        // Host dissolves the group when fewer than 2 remain.
        assert!(g.len() < 2, "caller restores the lone member as a floating panel");
        assert_eq!(g.members(), &["b".to_string()]);
    }
}
