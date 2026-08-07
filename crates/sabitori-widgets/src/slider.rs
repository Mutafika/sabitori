//! Stateful interactive slider helper.
//!
//! Pairs with [`sabitori_core::forms::slider`] (visual builder).
//! Owns the normalized value and dragging state; the app drives it
//! via `begin_drag` / `drag_to` / `end_drag` from pointer events,
//! using the slider's track screen-space `Rect`.

use sabitori_core::{Point, Rect};

#[derive(Debug, Clone)]
pub struct SliderState {
    /// Normalized value in `0.0..=1.0`.
    value: f32,
    pub dragging: bool,
}

impl SliderState {
    pub fn new(normalized: f32) -> Self {
        Self {
            value: normalized.clamp(0.0, 1.0),
            dragging: false,
        }
    }

    pub fn from_ranged(value: f32, min: f32, max: f32) -> Self {
        Self::new(Self::to_normalized(value, min, max))
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    /// Map the normalized value into `[min, max]`.
    pub fn ranged(&self, min: f32, max: f32) -> f32 {
        min + self.value * (max - min)
    }

    pub fn set_value(&mut self, normalized: f32) {
        self.value = normalized.clamp(0.0, 1.0);
    }

    pub fn set_ranged(&mut self, value: f32, min: f32, max: f32) {
        self.value = Self::to_normalized(value, min, max);
    }

    /// Begin dragging and snap to `mouse_x`.
    /// `track_x`/`track_w` are the screen-space track bounds.
    pub fn begin_drag(&mut self, mouse_x: f32, track_x: f32, track_w: f32) {
        self.dragging = true;
        self.update(mouse_x, track_x, track_w);
    }

    /// Continue dragging. Returns true if the value changed.
    pub fn drag_to(&mut self, mouse_x: f32, track_x: f32, track_w: f32) -> bool {
        if !self.dragging {
            return false;
        }
        let prev = self.value;
        self.update(mouse_x, track_x, track_w);
        (self.value - prev).abs() > f32::EPSILON
    }

    pub fn end_drag(&mut self) {
        self.dragging = false;
    }

    /// Convenience: hit-test `point` against an axis-aligned `track_rect`.
    pub fn hit_test(point: (f32, f32), track_rect: Rect) -> bool {
        track_rect.contains(Point::new(point.0, point.1))
    }

    fn update(&mut self, mouse_x: f32, track_x: f32, track_w: f32) {
        let local = (mouse_x - track_x) / track_w.max(1.0);
        self.value = local.clamp(0.0, 1.0);
    }

    fn to_normalized(value: f32, min: f32, max: f32) -> f32 {
        let span = (max - min).abs().max(1e-6);
        ((value - min) / span).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranged_round_trip() {
        let s = SliderState::from_ranged(50.0, 0.0, 100.0);
        assert!((s.value() - 0.5).abs() < 1e-5);
        assert!((s.ranged(0.0, 100.0) - 50.0).abs() < 1e-3);
    }

    #[test]
    fn drag_clamps() {
        let mut s = SliderState::new(0.0);
        s.begin_drag(500.0, 100.0, 200.0); // mouse far right of track
        assert_eq!(s.value(), 1.0);
        s.drag_to(0.0, 100.0, 200.0); // mouse far left
        assert_eq!(s.value(), 0.0);
    }

    #[test]
    fn drag_to_inactive_does_nothing() {
        let mut s = SliderState::new(0.5);
        let changed = s.drag_to(200.0, 100.0, 200.0);
        assert!(!changed);
        assert!((s.value() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn end_drag_stops_updates() {
        let mut s = SliderState::new(0.0);
        s.begin_drag(150.0, 100.0, 200.0);
        s.end_drag();
        let changed = s.drag_to(50.0, 100.0, 200.0);
        assert!(!changed);
    }
}
