use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Size {
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// A measured text box, plus where its first baseline sits inside it.
///
/// `size` alone is enough to lay text out, but not to place it against a
/// coordinate system that anchors on the **baseline** rather than the box.
/// CAD/DXF annotations are the motivating case: there, "top" is defined as
/// exactly `1.0em` above the baseline, whereas sabitori puts the top of the
/// *line box* at the element's position — and the line box is `line_height`
/// tall (1.4em by default), so the baseline lands lower. Without `baseline`
/// there is no way to convert between the two conventions, and the same
/// annotation drifts between screen and paper.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextMetrics {
    pub size: Size,
    /// Distance from the top of the box down to the **first** line's baseline,
    /// in logical px. Later lines sit one `Typography::line_height_px` apart,
    /// so line `n`'s baseline is `baseline + n * line_height_px`.
    ///
    /// Always inside the box (`0 < baseline < size.height`) for non-empty text.
    /// Note this is *not* the font ascent: cosmic-text centers the glyphs in
    /// the line box, so extra leading pushes the baseline down by half of it.
    ///
    /// **Not a constant you can hard-code.** It follows the face the string
    /// actually resolved through, so the same size yields different baselines
    /// for different scripts — measured at 100px, `"室名"` gives 108.0 while
    /// `"R-101"` gives 104.7, because the CJK and Latin faces have different
    /// ascents. A caller converting to a baseline-anchored coordinate system
    /// has to measure each string rather than apply one offset.
    pub baseline: f32,
}

impl TextMetrics {
    pub const ZERO: Self = Self {
        size: Size::ZERO,
        baseline: 0.0,
    };

    pub const fn new(width: f32, height: f32, baseline: f32) -> Self {
        Self {
            size: Size::new(width, height),
            baseline,
        }
    }

    /// Shorthand for `self.size.width`.
    pub const fn width(&self) -> f32 {
        self.size.width
    }

    /// Shorthand for `self.size.height`.
    pub const fn height(&self) -> f32 {
        self.size.height
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            origin: Point::new(x, y),
            size: Size::new(width, height),
        }
    }

    pub fn center(&self) -> Point {
        Point::new(
            self.origin.x + self.size.width / 2.0,
            self.origin.y + self.size.height / 2.0,
        )
    }

    pub fn contains(&self, point: Point) -> bool {
        point.x >= self.origin.x
            && point.x <= self.origin.x + self.size.width
            && point.y >= self.origin.y
            && point.y <= self.origin.y + self.size.height
    }

    /// Axis-aligned intersection of two rects. Returns `None` when the rects
    /// do not overlap (or overlap only on an edge with zero area).
    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let left = self.origin.x.max(other.origin.x);
        let top = self.origin.y.max(other.origin.y);
        let right = (self.origin.x + self.size.width).min(other.origin.x + other.size.width);
        let bottom = (self.origin.y + self.size.height).min(other.origin.y + other.size.height);
        if right <= left || bottom <= top {
            None
        } else {
            Some(Rect::new(left, top, right - left, bottom - top))
        }
    }
}

/// Per-corner values (e.g., border-radius).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Corners<T: Copy> {
    pub top_left: T,
    pub top_right: T,
    pub bottom_right: T,
    pub bottom_left: T,
}

impl<T: Copy> Corners<T> {
    pub const fn new(top_left: T, top_right: T, bottom_right: T, bottom_left: T) -> Self {
        Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }

    pub const fn all(value: T) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }
}

impl Corners<f32> {
    pub fn to_array(self) -> [f32; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }
}

/// Per-edge values (e.g., padding, margin).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Edges<T: Copy> {
    pub top: T,
    pub right: T,
    pub bottom: T,
    pub left: T,
}

impl<T: Copy> Edges<T> {
    pub const fn new(top: T, right: T, bottom: T, left: T) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    pub const fn all(value: T) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }
}
