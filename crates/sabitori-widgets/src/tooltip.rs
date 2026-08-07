//! Tooltip hover-delay state management.
//!
//! Tracks which element is being hovered, waits for a configurable delay,
//! then activates the tooltip. Uses a spring-animated opacity for smooth
//! fade-in/fade-out.

use sabitori_anim::{Animated, AnimationMode, Spring};

/// Manages tooltip display state: hover delay, position, and fade animation.
pub struct TooltipState {
    /// ID of the element currently being hovered (if it has a tooltip).
    pending_id: Option<String>,
    /// Tooltip text of the pending element.
    pending_text: Option<String>,
    /// Accumulated hover time on the current element (seconds).
    hover_time: f32,
    /// Delay before showing the tooltip (seconds).
    delay: f32,
    /// Currently active (visible) tooltip text.
    pub active_text: Option<String>,
    /// X position of the tooltip (logical pixels).
    pub x: f32,
    /// Y position of the tooltip (logical pixels).
    pub y: f32,
    /// Opacity animation for smooth fade-in/fade-out.
    pub opacity: Animated<f32>,
}

impl TooltipState {
    /// Create a new TooltipState with default 0.2s delay.
    pub fn new() -> Self {
        Self {
            pending_id: None,
            pending_text: None,
            hover_time: 0.0,
            delay: 0.2,
            active_text: None,
            x: 0.0,
            y: 0.0,
            opacity: Animated::new(0.0).with_mode(AnimationMode::Spring(Spring::snappy())),
        }
    }

    /// Notify the tooltip state that the hover target has changed.
    ///
    /// * `id` — ID of the hovered element (None if nothing is hovered).
    /// * `tooltip` — tooltip text of the hovered element (None if no tooltip).
    /// * `x`, `y` — mouse position in logical pixels.
    pub fn on_hover_change(&mut self, id: Option<&str>, tooltip: Option<&str>, x: f32, y: f32) {
        match (id, tooltip) {
            (Some(id_str), Some(text)) => {
                // Same element — just update position
                if self.pending_id.as_deref() == Some(id_str) {
                    self.x = x;
                    self.y = y;
                    return;
                }
                // New element with tooltip
                self.pending_id = Some(id_str.to_string());
                self.pending_text = Some(text.to_string());
                self.hover_time = 0.0;
                self.x = x;
                self.y = y;
                // If a tooltip was already active, hide it first
                if self.active_text.is_some() {
                    self.active_text = None;
                    self.opacity.set_target(0.0);
                }
            }
            _ => {
                // No tooltip target — clear everything
                if self.pending_id.is_some() || self.active_text.is_some() {
                    self.pending_id = None;
                    self.pending_text = None;
                    self.hover_time = 0.0;
                    if self.active_text.is_some() {
                        self.opacity.set_target(0.0);
                    }
                }
            }
        }
    }

    /// Advance the tooltip timer. Call every frame with delta time.
    pub fn tick(&mut self, dt: f32) {
        self.opacity.tick(dt);

        // Clean up active text when fully faded out
        if self.active_text.is_some() && !self.opacity.running && self.opacity.value() < 0.01 {
            self.active_text = None;
        }

        // Accumulate hover time and activate if delay has passed
        if self.pending_text.is_some() && self.active_text.is_none() {
            self.hover_time += dt;
            if self.hover_time >= self.delay {
                self.active_text = self.pending_text.clone();
                self.opacity.set_target(1.0);
            }
        }
    }

    /// True while the tooltip is fading or while a hover delay is counting up.
    /// Used by the runtime to decide whether to keep redrawing.
    pub fn is_pending(&self) -> bool {
        self.opacity.running
            || (self.pending_text.is_some() && self.active_text.is_none())
    }

    /// Returns `(text, x, y)` if a tooltip is currently active and visible.
    pub fn info(&self) -> Option<(String, f32, f32)> {
        self.active_text.as_ref().map(|text| {
            (text.clone(), self.x, self.y)
        })
    }

    /// Immediately hide and reset all tooltip state.
    pub fn clear(&mut self) {
        self.pending_id = None;
        self.pending_text = None;
        self.hover_time = 0.0;
        self.active_text = None;
        self.opacity.set_immediate(0.0);
    }
}

impl Default for TooltipState {
    fn default() -> Self {
        Self::new()
    }
}
