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

    /// Shrink the radii so neighbouring corners never overlap, the way CSS
    /// does (CSS Backgrounds 3 §5.5): if the two radii on any side add up to
    /// more than that side's length, *every* radius is scaled by the same
    /// factor. Negative radii are treated as zero.
    ///
    /// This is what makes the CSS pill idiom (`rounded_px(999.0)`) work. The
    /// rounded-rect SDF measures distance from the inner corner circle, so a
    /// radius larger than half the box puts every pixel — the centre
    /// included — outside the shape and the rect is not drawn at all. Clamp
    /// here, where the box size is known, rather than leaving each caller to
    /// pick a radius that happens to fit.
    pub fn clamped_to_size(self, width: f32, height: f32) -> Self {
        let r = Self {
            top_left: self.top_left.max(0.0),
            top_right: self.top_right.max(0.0),
            bottom_right: self.bottom_right.max(0.0),
            bottom_left: self.bottom_left.max(0.0),
        };
        let w = width.max(0.0);
        let h = height.max(0.0);

        let mut f: f32 = 1.0;
        for (sum, side) in [
            (r.top_left + r.top_right, w),
            (r.top_right + r.bottom_right, h),
            (r.bottom_right + r.bottom_left, w),
            (r.bottom_left + r.top_left, h),
        ] {
            if sum > 0.0 {
                f = f.min(side / sum);
            }
        }

        if f >= 1.0 {
            return r;
        }
        Self {
            top_left: r.top_left * f,
            top_right: r.top_right * f,
            bottom_right: r.bottom_right * f,
            bottom_left: r.bottom_left * f,
        }
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

#[cfg(test)]
mod corner_clamp_tests {
    use super::*;

    /// CSS のピル (`border-radius: 999px`) は、半径が箱の半分に丸められて
    /// 端が半円になる。丸めないと SDF が全画素を「外」と判定して矩形が消える。
    #[test]
    fn pill_radius_collapses_to_half_the_short_side() {
        let c = Corners::all(999.0).clamped_to_size(80.0, 22.0);
        assert_eq!(c.to_array(), [11.0, 11.0, 11.0, 11.0]);
    }

    /// 収まる半径は 1px も動かさない (既存の見た目を変えないこと)。
    #[test]
    fn radius_that_fits_is_untouched() {
        let c = Corners::all(8.0).clamped_to_size(120.0, 60.0);
        assert_eq!(c.to_array(), [8.0, 8.0, 8.0, 8.0]);
        // ちょうど半分も収まる
        let exact = Corners::all(30.0).clamped_to_size(120.0, 60.0);
        assert_eq!(exact.to_array(), [30.0, 30.0, 30.0, 30.0]);
    }

    /// 角ごとに違う半径は、辺ごとの和で決まる 1 つの比率で全角を縮める
    /// (角ごとに別々に丸めると、辺の途中で曲率が飛ぶ)。
    #[test]
    fn adjacent_corners_scale_by_one_shared_factor() {
        // 上辺 = 100, tl + tr = 150 → f = 2/3。他の辺はこれより緩い。
        let c = Corners::new(50.0, 100.0, 0.0, 0.0).clamped_to_size(100.0, 400.0);
        assert!((c.top_left - 100.0 / 3.0).abs() < 1e-3, "tl={}", c.top_left);
        assert!((c.top_right - 200.0 / 3.0).abs() < 1e-3, "tr={}", c.top_right);
        assert_eq!((c.bottom_right, c.bottom_left), (0.0, 0.0));
    }

    /// 片側だけ丸い箱は、その辺の長さまで使える (半分ではない)。
    #[test]
    fn a_lone_corner_may_reach_the_full_side() {
        let c = Corners::new(999.0, 0.0, 0.0, 0.0).clamped_to_size(40.0, 90.0);
        assert_eq!(c.to_array(), [40.0, 0.0, 0.0, 0.0]);
    }

    /// 半径 0・サイズ 0 で 0 除算や NaN を作らない。
    #[test]
    fn degenerate_boxes_stay_finite() {
        assert_eq!(Corners::all(0.0).clamped_to_size(0.0, 0.0).to_array(), [0.0; 4]);
        assert_eq!(Corners::all(6.0).clamped_to_size(0.0, 10.0).to_array(), [0.0; 4]);
        assert_eq!(Corners::all(-4.0).clamped_to_size(10.0, 10.0).to_array(), [0.0; 4]);
    }
}
