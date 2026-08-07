//! Drag & drop state manager.
//!
//! Tracks drag state from mouse-down on a draggable element through
//! threshold detection to drop or cancel. The framework owns this;
//! apps just call `.draggable("id")` and `.droppable()` on elements.

use web_time::Instant;

/// Manages the lifecycle of a single drag operation.
pub struct DragManager {
    state: DragState,
    /// Movement threshold in logical pixels before a pending drag becomes active.
    threshold: f32,
}

enum DragState {
    /// No drag in progress.
    None,
    /// Mouse pressed on a draggable element but hasn't moved past threshold yet.
    Pending {
        data: String,
        source_id: Option<String>,
        start_x: f32,
        start_y: f32,
    },
    /// Drag is active (past threshold). Ghost should be rendered.
    Active {
        data: String,
        source_id: Option<String>,
        created: Instant,
    },
}

impl DragManager {
    /// Create a new DragManager with a 5px movement threshold.
    pub fn new() -> Self {
        Self {
            state: DragState::None,
            threshold: 5.0,
        }
    }

    /// Call on mouse press when a draggable element is hit.
    pub fn start_pending(&mut self, data: String, source_id: Option<String>, x: f32, y: f32) {
        self.state = DragState::Pending {
            data,
            source_id,
            start_x: x,
            start_y: y,
        };
    }

    /// Call on mouse move. Returns `true` if the drag just became active
    /// (i.e., the threshold was crossed on this call).
    pub fn on_move(&mut self, x: f32, y: f32) -> bool {
        match &self.state {
            DragState::Pending {
                start_x, start_y, ..
            } => {
                let dx = x - start_x;
                let dy = y - start_y;
                if (dx * dx + dy * dy).sqrt() >= self.threshold {
                    // Promote to active
                    let DragState::Pending { data, source_id, .. } =
                        std::mem::replace(&mut self.state, DragState::None)
                    else {
                        unreachable!();
                    };
                    self.state = DragState::Active {
                        data,
                        source_id,
                        created: Instant::now(),
                    };
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Call on mouse release. Returns `Some((data, source_id))` if a drag
    /// was active (past threshold), allowing the caller to complete the drop.
    /// Returns `None` if there was no active drag (pending drags are just cancelled).
    pub fn on_release(&mut self) -> Option<(String, Option<String>)> {
        match std::mem::replace(&mut self.state, DragState::None) {
            DragState::Active {
                data, source_id, ..
            } => Some((data, source_id)),
            _ => None,
        }
    }

    /// Cancel any in-progress drag (pending or active).
    pub fn cancel(&mut self) {
        self.state = DragState::None;
    }

    /// Whether a drag is currently active (past the movement threshold).
    pub fn is_active(&self) -> bool {
        matches!(self.state, DragState::Active { .. })
    }

    /// Get drag info for populating [`ViewContext`].
    /// Returns `(data, source_id)` if drag is active.
    pub fn drag_info(&self) -> Option<(String, Option<String>)> {
        match &self.state {
            DragState::Active {
                data, source_id, ..
            } => Some((data.clone(), source_id.clone())),
            _ => None,
        }
    }

    /// Tick — cancel stale drags that have been active for more than 5 seconds.
    /// This prevents orphaned drag states if a release event is missed.
    pub fn tick(&mut self, _dt: f32) {
        if let DragState::Active { created, .. } = &self.state {
            if created.elapsed().as_secs_f32() > 5.0 {
                self.state = DragState::None;
            }
        }
    }
}

impl Default for DragManager {
    fn default() -> Self {
        Self::new()
    }
}
