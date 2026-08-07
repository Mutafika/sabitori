use sabitori_core::Rect;
use sabitori_anim::{Animated, Spring};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

/// Resizable split pane with draggable divider.
pub struct SplitPane {
    pub bounds: Rect,
    pub direction: SplitDirection,
    /// Ratio of the first pane (0.0 to 1.0).
    pub ratio: Animated<f32>,
    pub divider_width: f32,
    pub dragging: bool,
    pub hover_divider: bool,
    pub min_ratio: f32,
    pub max_ratio: f32,
}

impl SplitPane {
    pub fn new(bounds: Rect, direction: SplitDirection, initial_ratio: f32) -> Self {
        Self {
            bounds,
            direction,
            ratio: Animated::new(initial_ratio).with_spring(Spring::snappy()),
            divider_width: 4.0,
            dragging: false,
            hover_divider: false,
            min_ratio: 0.15,
            max_ratio: 0.85,
        }
    }

    /// Get the bounds of the first pane.
    pub fn first_pane(&self) -> Rect {
        let r = self.ratio.value();
        match self.direction {
            SplitDirection::Horizontal => {
                let w = self.bounds.size.width * r - self.divider_width / 2.0;
                Rect::new(self.bounds.origin.x, self.bounds.origin.y, w.max(0.0), self.bounds.size.height)
            }
            SplitDirection::Vertical => {
                let h = self.bounds.size.height * r - self.divider_width / 2.0;
                Rect::new(self.bounds.origin.x, self.bounds.origin.y, self.bounds.size.width, h.max(0.0))
            }
        }
    }

    /// Get the bounds of the second pane.
    pub fn second_pane(&self) -> Rect {
        let r = self.ratio.value();
        match self.direction {
            SplitDirection::Horizontal => {
                let x = self.bounds.origin.x + self.bounds.size.width * r + self.divider_width / 2.0;
                let w = self.bounds.size.width * (1.0 - r) - self.divider_width / 2.0;
                Rect::new(x, self.bounds.origin.y, w.max(0.0), self.bounds.size.height)
            }
            SplitDirection::Vertical => {
                let y = self.bounds.origin.y + self.bounds.size.height * r + self.divider_width / 2.0;
                let h = self.bounds.size.height * (1.0 - r) - self.divider_width / 2.0;
                Rect::new(self.bounds.origin.x, y, self.bounds.size.width, h.max(0.0))
            }
        }
    }

    /// Get divider rect.
    pub fn divider_rect(&self) -> Rect {
        let r = self.ratio.value();
        match self.direction {
            SplitDirection::Horizontal => {
                let x = self.bounds.origin.x + self.bounds.size.width * r - self.divider_width / 2.0;
                Rect::new(x, self.bounds.origin.y, self.divider_width, self.bounds.size.height)
            }
            SplitDirection::Vertical => {
                let y = self.bounds.origin.y + self.bounds.size.height * r - self.divider_width / 2.0;
                Rect::new(self.bounds.origin.x, y, self.bounds.size.width, self.divider_width)
            }
        }
    }

    /// Handle drag to resize.
    pub fn on_drag(&mut self, position: f32) {
        let new_ratio = match self.direction {
            SplitDirection::Horizontal => {
                (position - self.bounds.origin.x) / self.bounds.size.width
            }
            SplitDirection::Vertical => {
                (position - self.bounds.origin.y) / self.bounds.size.height
            }
        };
        let clamped = new_ratio.clamp(self.min_ratio, self.max_ratio);
        self.ratio.set_target(clamped);
    }

    pub fn tick(&mut self, dt: f32) {
        self.ratio.tick(dt);
    }
}
