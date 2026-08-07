use sabitori_core::{Color, Corners, Point, Rect};
use slotmap::new_key_type;

new_key_type! {
    pub struct NodeId;
}

/// Visual style for a node.
#[derive(Clone, Copy, Debug)]
pub struct NodeStyle {
    pub fill_color: Color,
    pub border_color: Color,
    pub border_width: f32,
    pub corner_radii: Corners<f32>,
    pub shadow_color: Color,
    pub shadow_offset: Point,
    pub shadow_blur: f32,
    pub shadow_spread: f32,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            fill_color: Color::TRANSPARENT,
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            corner_radii: Corners::all(0.0),
            shadow_color: Color::TRANSPARENT,
            shadow_offset: Point::ZERO,
            shadow_blur: 0.0,
            shadow_spread: 0.0,
        }
    }
}

/// A UI node in the tree.
pub struct UiNode {
    pub bounds: Rect,
    pub style: NodeStyle,
    pub hover_style: Option<NodeStyle>,
    pub active_style: Option<NodeStyle>,
    pub children: Vec<NodeId>,
    pub interactive: bool,
    pub on_click: Option<Box<dyn FnMut()>>,
    // Runtime state
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    // Animation: current interpolated values
    pub current_fill: Color,
    pub current_border: Color,
}

impl UiNode {
    pub fn new(bounds: Rect, style: NodeStyle) -> Self {
        let current_fill = style.fill_color;
        let current_border = style.border_color;
        Self {
            bounds,
            style,
            hover_style: None,
            active_style: None,
            children: Vec::new(),
            interactive: false,
            on_click: None,
            hovered: false,
            pressed: false,
            focused: false,
            current_fill,
            current_border,
        }
    }

    /// Check if a point is inside this node (considering corner radii).
    pub fn hit_test(&self, point: Point) -> bool {
        if !self.bounds.contains(point) {
            return false;
        }

        // For rounded corners, check SDF on CPU
        let cx = self.bounds.origin.x + self.bounds.size.width / 2.0;
        let cy = self.bounds.origin.y + self.bounds.size.height / 2.0;
        let hx = self.bounds.size.width / 2.0;
        let hy = self.bounds.size.height / 2.0;

        let px = point.x - cx;
        let py = point.y - cy;

        let r = &self.style.corner_radii;
        let radius = if px > 0.0 {
            if py > 0.0 {
                r.bottom_right
            } else {
                r.top_right
            }
        } else if py > 0.0 {
            r.bottom_left
        } else {
            r.top_left
        };

        if radius <= 0.0 {
            return true;
        }

        let qx = px.abs() - hx + radius;
        let qy = py.abs() - hy + radius;
        let dist = qx.max(qy).min(0.0) + (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() - radius;

        dist <= 0.0
    }

    /// Get the effective style based on interaction state.
    pub fn effective_style(&self) -> &NodeStyle {
        if self.pressed {
            if let Some(ref s) = self.active_style {
                return s;
            }
        }
        if self.hovered {
            if let Some(ref s) = self.hover_style {
                return s;
            }
        }
        &self.style
    }

    /// Smoothly interpolate current visual values toward the target.
    pub fn animate(&mut self, dt: f32) {
        let target_fill = self.effective_style().fill_color;
        let target_border = self.effective_style().border_color;
        let speed = 10.0 * dt; // ~150ms transition at 60fps
        self.current_fill = self.current_fill.lerp(target_fill, speed.min(1.0));
        self.current_border = self.current_border.lerp(target_border, speed.min(1.0));
    }
}
