use sabitori_anim::{Animated, Spring};
use sabitori_core::{Color, Point, Rect};

/// Style for the tab bar.
#[derive(Clone, Debug)]
pub struct TabStyle {
    pub bg: Color,
    pub tab_fg: Color,
    pub tab_active_fg: Color,
    pub indicator_color: Color,
    pub tab_height: f32,
    pub tab_padding_x: f32,
    pub indicator_height: f32,
}

impl TabStyle {
    pub fn default_dark() -> Self {
        Self {
            bg: Color::from_hex("#16161e"),
            tab_fg: Color::from_hex("#6a6a8a"),
            tab_active_fg: Color::from_hex("#e0e0f0"),
            indicator_color: Color::from_hex("#6c8cff"),
            tab_height: 36.0,
            tab_padding_x: 20.0,
            indicator_height: 3.0,
        }
    }
}

/// Tab bar widget.
pub struct Tabs {
    pub bounds: Rect,
    pub labels: Vec<String>,
    pub active: usize,
    pub hovered: Option<usize>,
    pub style: TabStyle,
    /// Animated indicator x position.
    pub indicator_x: Animated<f32>,
    /// Animated indicator width.
    pub indicator_w: Animated<f32>,
}

impl Tabs {
    pub fn new(bounds: Rect, labels: Vec<String>, style: TabStyle) -> Self {
        Self {
            bounds,
            labels,
            active: 0,
            hovered: None,
            style,
            indicator_x: Animated::new(0.0).with_spring(Spring::snappy()),
            indicator_w: Animated::new(0.0).with_spring(Spring::snappy()),
        }
    }

    /// Width of each tab (equally divided).
    pub fn tab_width(&self) -> f32 {
        if self.labels.is_empty() {
            return 0.0;
        }
        self.bounds.size.width / self.labels.len() as f32
    }

    /// Get the rect for a specific tab.
    pub fn tab_rect(&self, idx: usize) -> Rect {
        let w = self.tab_width();
        Rect::new(
            self.bounds.origin.x + idx as f32 * w,
            self.bounds.origin.y,
            w,
            self.style.tab_height,
        )
    }

    /// Get the rect for the active indicator.
    pub fn indicator_rect(&self) -> Rect {
        Rect::new(
            self.indicator_x.value(),
            self.bounds.origin.y + self.style.tab_height - self.style.indicator_height,
            self.indicator_w.value(),
            self.style.indicator_height,
        )
    }

    /// Set the active tab index.
    pub fn set_active(&mut self, idx: usize) {
        if idx >= self.labels.len() {
            return;
        }
        self.active = idx;
        let w = self.tab_width();
        self.indicator_x.set_target(self.bounds.origin.x + idx as f32 * w);
        self.indicator_w.set_target(w);
    }

    /// Cycle to next tab.
    pub fn next(&mut self) {
        if self.labels.is_empty() {
            return;
        }
        self.set_active((self.active + 1) % self.labels.len());
    }

    /// Cycle to previous tab.
    pub fn prev(&mut self) {
        if self.labels.is_empty() {
            return;
        }
        self.set_active(if self.active == 0 { self.labels.len() - 1 } else { self.active - 1 });
    }

    /// Handle pointer move.
    pub fn on_pointer_move(&mut self, point: Point) {
        if !self.bounds.contains(point) {
            self.hovered = None;
            return;
        }
        let local_x = point.x - self.bounds.origin.x;
        let w = self.tab_width();
        if w > 0.0 {
            let idx = (local_x / w) as usize;
            if idx < self.labels.len() {
                self.hovered = Some(idx);
            } else {
                self.hovered = None;
            }
        }
    }

    /// Handle click — returns the clicked tab index if changed.
    pub fn on_click(&mut self, point: Point) -> Option<usize> {
        if !self.bounds.contains(point) {
            return None;
        }
        let local_x = point.x - self.bounds.origin.x;
        let w = self.tab_width();
        if w > 0.0 {
            let idx = (local_x / w) as usize;
            if idx < self.labels.len() && idx != self.active {
                self.set_active(idx);
                return Some(idx);
            }
        }
        None
    }

    /// Get foreground color for a tab.
    pub fn tab_fg(&self, idx: usize) -> Color {
        if idx == self.active {
            self.style.tab_active_fg
        } else {
            self.style.tab_fg
        }
    }

    /// Tick animations.
    pub fn tick(&mut self, dt: f32) {
        self.indicator_x.tick(dt);
        self.indicator_w.tick(dt);
    }
}
