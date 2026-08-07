//! Shared pointer/touch gesture plumbing used by both `declarative.rs` and
//! `scene_app.rs`. The runtimes differ in what they do with gestures (managed
//! scroll, drag-and-drop, custom scene panning) but they share the same
//! state model: which modality owns the primary flow, which finger is
//! driving the single-touch path, and whether a two-finger pinch is active.

/// Minimum distance (logical px) a touch must travel before it's considered a
/// scroll/drag rather than a tap. Matches Android's default touch slop range.
pub(crate) const TOUCH_SLOP: f32 = 10.0;

/// Per-touch state for the primary finger. Drives tap-vs-scroll disambiguation.
pub(crate) struct TouchDrag {
    pub id: u64,
    pub start: (f32, f32),
    pub last: (f32, f32),
    /// Wall-clock of the previous Moved sample, used to compute velocity for fling.
    pub last_move_time: Option<web_time::Instant>,
    /// Id of the topmost clickable region under the initial touch, if any.
    pub click_target: Option<String>,
    /// Id of the nearest managed scroll container under the initial touch, if any.
    /// Only used by runtimes that have managed scroll containers.
    pub scroll_target: Option<String>,
    /// Once the finger crosses [`TOUCH_SLOP`] this is set; no tap will fire on release.
    pub moved_beyond_slop: bool,
}

/// Two-finger pinch gesture state.
pub(crate) struct PinchGesture {
    pub id_a: u64,
    pub id_b: u64,
    /// Distance between the two fingers when the gesture started. Used to
    /// compute the absolute scale factor (current / start).
    pub start_distance: f32,
}

/// Which input modality currently owns the primary-pointer flow.
///
/// First-come wins: once set to `Mouse` or `Touch`, events from the other
/// modality are ignored for primary routing (click / scroll / drag / hover)
/// until this returns to `None`. Raw `InputEvent::Pointer*` still fires for
/// apps that want both streams.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrimaryInput {
    None,
    Mouse,
    Touch,
}

/// Distance + midpoint between two active touches, or `None` if either is missing.
pub(crate) fn pinch_metrics(
    active: &std::collections::HashMap<u64, (f32, f32)>,
    id_a: u64,
    id_b: u64,
) -> Option<(f32, (f32, f32))> {
    let a = active.get(&id_a)?;
    let b = active.get(&id_b)?;
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    let distance = (dx * dx + dy * dy).sqrt();
    let center = ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5);
    Some((distance, center))
}
