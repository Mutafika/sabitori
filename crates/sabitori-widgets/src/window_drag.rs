//! Floating-window drag state for dockable panels.
//!
//! A panel is either *docked* (laid out by the parent flex flow, `pos == None`)
//! or *floating* (absolutely positioned on the overlay layer, `pos == Some`).
//! Grabbing the panel's title bar calls [`WindowDragState::begin_drag`], which
//! floats it at its current on-screen origin (so it does not jump), then the
//! host follows the pointer with [`WindowDragState::drag_to`] and finishes with
//! [`WindowDragState::end_drag`].
//!
//! This mirrors the `begin_drag` / `drag_to` / `end_drag` protocol used by
//! [`crate::SliderState`]: the host owns the pointer capture (it keeps routing
//! moves while [`WindowDragState::dragging`] is true) and a per-frame view reads
//! [`WindowDragState::pos`] to decide flex-vs-overlay placement.
//!
//! It is intentionally pure geometry — it knows nothing about element ids,
//! hit-testing, or snapping. The host maps a title-handle id to a panel id and
//! supplies the panel's measured rect; snapping (to an edge or another panel)
//! is layered on top by inspecting / rewriting [`WindowDragState::pos`] in
//! [`WindowDragState::end_drag`]'s caller.

/// Per-panel float + drag state. `Default` is the docked, idle state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct WindowDragState {
    /// Top-left corner in logical pixels. `None` while docked.
    pos: Option<(f32, f32)>,
    /// Floating size `(width, height)`, captured from the docked rect at
    /// `begin_drag`. A docked panel often sizes its height to the parent
    /// (`100%`), which collapses once it is absolutely positioned — so the
    /// host applies this explicit size to the floating wrapper. `None` while
    /// docked.
    size: Option<(f32, f32)>,
    /// Whether the host is actively following the pointer to **move** this panel.
    /// The host uses this to keep capturing the pointer past the panel bounds.
    pub dragging: bool,
    /// Whether the host is actively following the pointer to **resize** this
    /// panel (from its bottom-right grip). Mutually exclusive with `dragging`.
    pub resizing: bool,
    /// Grab offset captured when a gesture starts: pointer→top-left for
    /// `begin_drag`, pointer→bottom-right for `begin_resize`. Only meaningful
    /// while the matching gesture is active (the two never overlap).
    grab_dx: f32,
    grab_dy: f32,
}

/// Smallest a panel may be shrunk to while resizing (logical px).
const MIN_W: f32 = 180.0;
const MIN_H: f32 = 120.0;

impl WindowDragState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the panel should be placed as an absolutely-positioned overlay.
    pub fn is_floating(&self) -> bool {
        self.pos.is_some()
    }

    /// Current top-left corner, or `None` while docked.
    pub fn pos(&self) -> Option<(f32, f32)> {
        self.pos
    }

    /// Floating size `(width, height)` captured at grab time, or `None` while
    /// docked. Apply this to the floating wrapper so the panel keeps the size
    /// it had when docked instead of collapsing.
    pub fn size(&self) -> Option<(f32, f32)> {
        self.size
    }

    /// Place the panel at an explicit floating position (without a drag).
    pub fn set_pos(&mut self, x: f32, y: f32) {
        self.pos = Some((x, y));
    }

    /// Place the panel at an explicit floating rect — position **and** size.
    /// Used for Windows-style tiling (snap a panel to fill a half/quarter of
    /// the viewport): unlike a drag, this rewrites the size too. The size is
    /// taken verbatim (no min clamp) so the caller controls the tile extent.
    pub fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.pos = Some((x, y));
        self.size = Some((w, h));
    }

    /// Grab the title bar. `(panel_x, panel_y)` / `(panel_w, panel_h)` are the
    /// panel's top-left and size at the instant of the grab — when undocking,
    /// pass the measured docked rect so the panel floats exactly where and how
    /// big it already was (no jump, no collapse).
    pub fn begin_drag(
        &mut self,
        pointer_x: f32,
        pointer_y: f32,
        panel_x: f32,
        panel_y: f32,
        panel_w: f32,
        panel_h: f32,
    ) {
        self.dragging = true;
        self.pos = Some((panel_x, panel_y));
        self.size = Some((panel_w, panel_h));
        self.grab_dx = pointer_x - panel_x;
        self.grab_dy = pointer_y - panel_y;
    }

    /// Follow the pointer, preserving the grab offset. Returns true if the
    /// position changed.
    pub fn drag_to(&mut self, pointer_x: f32, pointer_y: f32) -> bool {
        if !self.dragging {
            return false;
        }
        let next = Some((pointer_x - self.grab_dx, pointer_y - self.grab_dy));
        if self.pos != next {
            self.pos = next;
            true
        } else {
            false
        }
    }

    /// Stop following the pointer. The panel stays floating at its last `pos`.
    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    /// Grab the bottom-right resize grip. No-op unless the panel is floating
    /// with a known size. The grip stays under the pointer (no jump) by
    /// capturing the offset from the pointer to the bottom-right corner.
    pub fn begin_resize(&mut self, pointer_x: f32, pointer_y: f32) {
        let (Some((px, py)), Some((sw, sh))) = (self.pos, self.size) else {
            return;
        };
        self.resizing = true;
        self.grab_dx = pointer_x - (px + sw);
        self.grab_dy = pointer_y - (py + sh);
    }

    /// Follow the pointer to resize from the bottom-right (top-left fixed).
    /// Clamped to a minimum size. Returns true if the size changed.
    pub fn resize_to(&mut self, pointer_x: f32, pointer_y: f32) -> bool {
        if !self.resizing {
            return false;
        }
        let Some((px, py)) = self.pos else {
            return false;
        };
        let nw = (pointer_x - self.grab_dx - px).max(MIN_W);
        let nh = (pointer_y - self.grab_dy - py).max(MIN_H);
        let next = Some((nw, nh));
        if self.size != next {
            self.size = next;
            true
        } else {
            false
        }
    }

    /// Stop resizing. The panel keeps its new size.
    pub fn end_resize(&mut self) {
        self.resizing = false;
    }

    /// Return the panel to the docked flow (clears floating position + size).
    pub fn dock(&mut self) {
        self.pos = None;
        self.size = None;
        self.dragging = false;
        self.resizing = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_docked_and_idle() {
        let w = WindowDragState::new();
        assert!(!w.is_floating());
        assert!(!w.dragging);
        assert_eq!(w.pos(), None);
        assert_eq!(w.size(), None);
    }

    #[test]
    fn begin_drag_floats_at_panel_origin_without_jump() {
        let mut w = WindowDragState::new();
        // grab at pointer (50, 30) while the panel rect is (40, 20, 200, 300)
        w.begin_drag(50.0, 30.0, 40.0, 20.0, 200.0, 300.0);
        assert!(w.is_floating());
        assert!(w.dragging);
        // floats exactly where it was — pos == panel origin, not the pointer
        assert_eq!(w.pos(), Some((40.0, 20.0)));
        // and keeps its docked size so it does not collapse
        assert_eq!(w.size(), Some((200.0, 300.0)));
    }

    #[test]
    fn drag_to_preserves_grab_offset_and_size() {
        let mut w = WindowDragState::new();
        w.begin_drag(50.0, 30.0, 40.0, 20.0, 200.0, 300.0); // grab offset = (10, 10)
        let changed = w.drag_to(100.0, 80.0);
        assert!(changed);
        // pointer (100, 80) − offset (10, 10) = (90, 70)
        assert_eq!(w.pos(), Some((90.0, 70.0)));
        // size is unaffected by movement
        assert_eq!(w.size(), Some((200.0, 300.0)));
    }

    #[test]
    fn drag_to_no_move_returns_false() {
        let mut w = WindowDragState::new();
        w.begin_drag(50.0, 30.0, 40.0, 20.0, 200.0, 300.0);
        assert!(!w.drag_to(50.0, 30.0)); // pointer unchanged → pos unchanged
    }

    #[test]
    fn drag_to_while_idle_does_nothing() {
        let mut w = WindowDragState::new();
        assert!(!w.drag_to(100.0, 100.0));
        assert_eq!(w.pos(), None);
    }

    #[test]
    fn end_drag_keeps_float_stops_dragging() {
        let mut w = WindowDragState::new();
        w.begin_drag(50.0, 30.0, 40.0, 20.0, 200.0, 300.0);
        w.drag_to(100.0, 80.0);
        w.end_drag();
        assert!(!w.dragging);
        assert!(w.is_floating());
        assert_eq!(w.pos(), Some((90.0, 70.0)));
        assert_eq!(w.size(), Some((200.0, 300.0)));
        // a stale move after release must not nudge the panel
        assert!(!w.drag_to(200.0, 200.0));
        assert_eq!(w.pos(), Some((90.0, 70.0)));
    }

    #[test]
    fn set_rect_overwrites_pos_and_size() {
        let mut w = WindowDragState::new();
        w.begin_drag(50.0, 30.0, 40.0, 20.0, 200.0, 300.0);
        w.end_drag();
        // タイル（下半分など）: 位置とサイズを同時に上書き
        w.set_rect(0.0, 400.0, 640.0, 400.0);
        assert_eq!(w.pos(), Some((0.0, 400.0)));
        assert_eq!(w.size(), Some((640.0, 400.0)));
        assert!(w.is_floating());
    }

    #[test]
    fn dock_clears_float_and_size() {
        let mut w = WindowDragState::new();
        w.begin_drag(50.0, 30.0, 40.0, 20.0, 200.0, 300.0);
        w.dock();
        assert!(!w.is_floating());
        assert!(!w.dragging);
        assert_eq!(w.pos(), None);
        assert_eq!(w.size(), None);
    }

    #[test]
    fn resize_from_bottom_right_keeps_top_left_fixed() {
        let mut w = WindowDragState::new();
        w.begin_drag(50.0, 30.0, 40.0, 20.0, 200.0, 300.0);
        w.end_drag();
        // bottom-right corner is at (240, 320); grab it at pointer (245, 322)
        w.begin_resize(245.0, 322.0);
        assert!(w.resizing);
        // drag the corner by (+60, +40) → size grows by the same, pos unchanged
        let changed = w.resize_to(305.0, 362.0);
        assert!(changed);
        assert_eq!(w.pos(), Some((40.0, 20.0)), "top-left fixed");
        assert_eq!(w.size(), Some((260.0, 340.0)));
    }

    #[test]
    fn resize_clamps_to_minimum() {
        let mut w = WindowDragState::new();
        w.begin_drag(0.0, 0.0, 100.0, 100.0, 400.0, 400.0);
        w.end_drag();
        w.begin_resize(500.0, 500.0); // corner at (500,500), offset 0
        // drag the corner almost onto the top-left → clamps to MIN
        w.resize_to(100.0, 100.0);
        assert_eq!(w.size(), Some((MIN_W, MIN_H)));
    }

    #[test]
    fn resize_to_while_idle_does_nothing() {
        let mut w = WindowDragState::new();
        w.begin_drag(0.0, 0.0, 0.0, 0.0, 200.0, 300.0);
        w.end_drag();
        assert!(!w.resize_to(999.0, 999.0)); // not resizing
        assert_eq!(w.size(), Some((200.0, 300.0)));
    }

    #[test]
    fn dock_clears_resizing_too() {
        let mut w = WindowDragState::new();
        w.begin_drag(0.0, 0.0, 0.0, 0.0, 200.0, 300.0);
        w.begin_resize(200.0, 300.0);
        w.dock();
        assert!(!w.resizing);
        assert!(!w.is_floating());
    }

    #[test]
    fn regrab_while_floating_recaptures_offset_without_jump() {
        let mut w = WindowDragState::new();
        w.begin_drag(50.0, 30.0, 40.0, 20.0, 200.0, 300.0);
        w.drag_to(100.0, 80.0); // now floating at (90, 70)
        w.end_drag();
        // grab again from the panel's current origin (90, 70) at pointer (95, 73)
        w.begin_drag(95.0, 73.0, 90.0, 70.0, 200.0, 300.0);
        assert_eq!(w.pos(), Some((90.0, 70.0))); // no jump
        w.drag_to(105.0, 83.0); // moved pointer by (10, 10)
        assert_eq!(w.pos(), Some((100.0, 80.0)));
    }
}
