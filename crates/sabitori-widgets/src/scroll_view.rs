use sabitori_anim::{Animated, Spring};

/// Virtual scrolling container (2D).
///
/// Tracks both axes. Horizontal is a no-op when `content_width <= viewport_width`
/// (i.e., `max_scroll_x == 0`), which is the default after [`ScrollView::new`]
/// so existing vertical-only callers keep working unchanged.
pub struct ScrollView {
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub content_width: f32,
    pub content_height: f32,
    /// Current scroll offset along X (animated, spring-backed).
    pub scroll_x: Animated<f32>,
    /// Current scroll offset along Y (animated, spring-backed).
    pub scroll_y: Animated<f32>,
    /// Fling velocity in px/sec along each axis. Non-zero during inertia.
    velocity_x: f32,
    velocity_y: f32,
    /// Whether the user is actively dragging (suppresses fling integration).
    pub scrolling: bool,
}

/// Fling deceleration rate. `velocity *= exp(-FLING_DECAY * dt)` each tick.
const FLING_DECAY: f32 = 4.0;
/// Below this |velocity| the fling is considered stopped (px/sec).
const FLING_EPSILON: f32 = 5.0;
/// Hard cap on captured drag velocity (px/sec). Prevents pathological
/// micro-samples from producing runaway flings.
const MAX_FLING_VELOCITY: f32 = 8000.0;
/// Rubber-band coefficient. Higher = stiffer resistance.
/// iOS-style asymptote: `displayed = sign * (1 - 1/(k*|raw|/dim + 1)) * dim`.
const RUBBER_COEFF: f32 = 0.55;

/// Multiplier applied when a vertical wheel is redirected onto a horizontal
/// strip. The runtime's ~20px/notch step suits text lines but is tiny across a
/// wide carousel; boosting it makes a notch travel roughly one item.
const H_WHEEL_BOOST: f32 = 5.0;

/// Asymptotic rubber-band: maps an unbounded overscroll offset to a
/// bounded displayed offset (approaches `dim` but never reaches it).
/// `offset` is the raw distance past the boundary (can be negative).
fn rubber_band(offset: f32, dim: f32) -> f32 {
    if dim <= 0.0 || offset == 0.0 {
        return 0.0;
    }
    let sign = offset.signum();
    let abs_off = offset.abs();
    sign * (1.0 - 1.0 / (abs_off * RUBBER_COEFF / dim + 1.0)) * dim
}

impl ScrollView {
    /// Vertical-only constructor (back-compat). Horizontal starts disabled.
    pub fn new(viewport_height: f32, content_height: f32) -> Self {
        Self::new_2d(0.0, viewport_height, 0.0, content_height)
    }

    /// Two-axis constructor.
    pub fn new_2d(
        viewport_width: f32,
        viewport_height: f32,
        content_width: f32,
        content_height: f32,
    ) -> Self {
        let spring = Spring { stiffness: 800.0, damping: 56.0, mass: 1.0 };
        Self {
            viewport_width,
            viewport_height,
            content_width,
            content_height,
            scroll_x: Animated::new(0.0).with_spring(spring),
            scroll_y: Animated::new(0.0).with_spring(spring),
            velocity_x: 0.0,
            velocity_y: 0.0,
            scrolling: false,
        }
    }

    fn max_scroll_x(&self) -> f32 {
        (self.content_width - self.viewport_width).max(0.0)
    }

    fn max_scroll_y(&self) -> f32 {
        (self.content_height - self.viewport_height).max(0.0)
    }

    /// Vertical-only scroll handler (mouse wheel / trackpad). Clamps to bounds.
    pub fn on_scroll(&mut self, delta_y: f32) {
        self.on_scroll_xy(0.0, delta_y);
    }

    /// Two-axis scroll handler (mouse wheel with horizontal component, etc).
    /// Sign convention: positive `delta_y` reveals content above; positive
    /// `delta_x` reveals content to the left.
    ///
    /// Uses spring-backed target updates (nudge_target) so continuous wheel /
    /// trackpad input feels smooth — each event extends the target while the
    /// integrator chases with damped velocity, instead of stuttering jumps.
    pub fn on_scroll_xy(&mut self, delta_x: f32, delta_y: f32) {
        let max_x = self.max_scroll_x();
        let max_y = self.max_scroll_y();
        // Horizontally-dominant containers (carousels, timelines) have little or
        // no vertical range (content ~= viewport height), so a vertical scroll
        // gesture would be lost. A mouse wheel sends delta_x == 0; a TRACKPAD
        // vertical swipe sends a large delta_y AND a small non-zero delta_x, so a
        // `delta_x == 0` gate misses it and the strip barely moves. Instead, when
        // horizontal scroll room dominates (>= 2x the vertical), route whichever
        // input axis is larger to the x-axis. Standard carousel UX.
        let (delta_x, delta_y) = if max_x > 0.0 && max_x >= max_y * 2.0 {
            if delta_x.abs() >= delta_y.abs() {
                (delta_x, 0.0) // real horizontal input — pass through 1:1
            } else {
                // A vertical wheel redirected onto a wide strip: the runtime's
                // ~20px/notch step (tuned for text lines) barely moves a wide
                // carousel — feels like a tiny scrollbar with lag. Boost it so a
                // notch travels ~1 item and the spring reads as momentum.
                (delta_y * H_WHEEL_BOOST, 0.0)
            }
        } else {
            (delta_x, delta_y)
        };
        if delta_x != 0.0 {
            let new_x = (self.scroll_x.target() - delta_x).clamp(0.0, max_x);
            self.scroll_x.set_target(new_x);
        }
        if delta_y != 0.0 {
            let new_y = (self.scroll_y.target() - delta_y).clamp(0.0, max_y);
            self.scroll_y.set_target(new_y);
        }
    }

    /// Mark a touch drag as starting on this container. Cancels any active
    /// fling so the finger "grabs" the scrolling content.
    pub fn begin_drag(&mut self) {
        self.velocity_x = 0.0;
        self.velocity_y = 0.0;
        self.scrolling = true;
    }

    /// Apply a per-axis drag delta in logical pixels. When past a boundary
    /// the delta is attenuated via a rubber-band curve and the displayed
    /// scroll offset can go beyond `[0, max_scroll]`. On release the
    /// [`Animated`] spring pulls it back to the nearest edge.
    /// Sign: positive `dy` reveals content above, positive `dx` reveals
    /// content to the left.
    pub fn drag_by(&mut self, dx: f32, dy: f32, dt: f32) {
        // Compute the immutable reads up-front — the `&mut self.scroll_*`
        // borrow below makes `self.max_scroll_*()` illegal once passed in.
        let max_x = self.max_scroll_x();
        let max_y = self.max_scroll_y();
        let vw = self.viewport_width;
        let vh = self.viewport_height;
        drag_axis(&mut self.scroll_x, &mut self.velocity_x, dx, dt, max_x, vw);
        drag_axis(&mut self.scroll_y, &mut self.velocity_y, dy, dt, max_y, vh);
    }

    /// Finish a touch drag. If the displayed scroll is past a boundary,
    /// target the nearest edge so the spring bounces back; otherwise leave
    /// `velocity` captured from [`drag_by`] so the next [`tick`] flings.
    pub fn end_drag(&mut self) {
        self.scrolling = false;
        // Compute bounds first — `&mut self.scroll_*` below blocks any
        // `self.max_scroll_*()` read.
        let max_x = self.max_scroll_x();
        let max_y = self.max_scroll_y();
        end_drag_axis(&mut self.scroll_x, &mut self.velocity_x, max_x);
        end_drag_axis(&mut self.scroll_y, &mut self.velocity_y, max_y);
    }

    /// Cancel any in-progress fling (e.g. on `TouchPhase::Cancelled`).
    pub fn cancel_fling(&mut self) {
        self.velocity_x = 0.0;
        self.velocity_y = 0.0;
        self.scrolling = false;
        // Snap back to bounds if we were past them.
        let cx = self.scroll_x.value().clamp(0.0, self.max_scroll_x());
        let cy = self.scroll_y.value().clamp(0.0, self.max_scroll_y());
        self.scroll_x.set_target(cx);
        self.scroll_y.set_target(cy);
    }

    /// Smooth scroll to a Y position (uses spring animation). Vertical-only
    /// for back-compat; use [`smooth_scroll_to_xy`] for 2D.
    pub fn smooth_scroll_to(&mut self, y: f32) {
        self.scroll_y.set_target(y.clamp(0.0, self.max_scroll_y()));
    }

    pub fn smooth_scroll_to_xy(&mut self, x: f32, y: f32) {
        self.scroll_x.set_target(x.clamp(0.0, self.max_scroll_x()));
        self.scroll_y.set_target(y.clamp(0.0, self.max_scroll_y()));
    }

    /// Set content height (e.g., when items change).
    pub fn set_content_height(&mut self, height: f32) {
        self.content_height = height;
        let max_y = self.max_scroll_y();
        if self.scroll_y.value() > max_y {
            self.scroll_y.set_target(max_y);
        }
    }

    pub fn set_content_width(&mut self, width: f32) {
        self.content_width = width;
        let max_x = self.max_scroll_x();
        if self.scroll_x.value() > max_x {
            self.scroll_x.set_target(max_x);
        }
    }

    pub fn set_content_size(&mut self, width: f32, height: f32) {
        self.set_content_width(width);
        self.set_content_height(height);
    }

    /// Vertical-only scrollTo (back-compat).
    pub fn scroll_to(&mut self, y: f32) {
        self.smooth_scroll_to(y);
    }

    pub fn scroll_to_xy(&mut self, x: f32, y: f32) {
        self.smooth_scroll_to_xy(x, y);
    }

    pub fn tick(&mut self, dt: f32) {
        self.scroll_x.tick(dt);
        self.scroll_y.tick(dt);

        if !self.scrolling && dt > 0.0 {
            let max_x = self.max_scroll_x();
            let max_y = self.max_scroll_y();
            fling_axis(&mut self.scroll_x, &mut self.velocity_x, dt, max_x);
            fling_axis(&mut self.scroll_y, &mut self.velocity_y, dt, max_y);
        }
    }

    /// True if scroll springs are still settling or fling velocity is non-trivial.
    pub fn is_animating(&self) -> bool {
        self.scroll_x.running
            || self.scroll_y.running
            || self.velocity_x.abs() > FLING_EPSILON
            || self.velocity_y.abs() > FLING_EPSILON
    }

    /// Get the range of visible items given item height.
    pub fn visible_range(&self, item_height: f32) -> (usize, usize) {
        let scroll = self.scroll_y.value();
        let first = (scroll / item_height).floor() as usize;
        let count = (self.viewport_height / item_height).ceil() as usize + 2;
        (first, first + count)
    }

    /// Vertical scrollbar thumb position and size (0..1 range).
    pub fn scrollbar(&self) -> (f32, f32) {
        scrollbar_metrics(
            self.scroll_y.value(),
            self.content_height,
            self.viewport_height,
        )
    }

    /// Horizontal scrollbar thumb position and size (0..1 range).
    pub fn scrollbar_x(&self) -> (f32, f32) {
        scrollbar_metrics(
            self.scroll_x.value(),
            self.content_width,
            self.viewport_width,
        )
    }
}

fn scrollbar_metrics(scroll: f32, content: f32, viewport: f32) -> (f32, f32) {
    if content <= viewport {
        return (0.0, 1.0);
    }
    let ratio = viewport / content;
    let position = (scroll / (content - viewport)).clamp(0.0, 1.0);
    (position * (1.0 - ratio), ratio)
}

/// Apply a drag delta along one axis, with rubber-band resistance past bounds.
fn drag_axis(
    anim: &mut Animated<f32>,
    velocity: &mut f32,
    delta: f32,
    dt: f32,
    max_scroll: f32,
    viewport: f32,
) {
    if delta == 0.0 {
        return;
    }
    let cur = anim.target();
    let raw_new = cur - delta;

    // If the NEW position would be past a boundary, attenuate the delta so
    // movement slows asymptotically as the user pulls further.
    let displayed_new = if raw_new < 0.0 {
        -rubber_band(-raw_new, viewport.max(1.0))
    } else if raw_new > max_scroll {
        max_scroll + rubber_band(raw_new - max_scroll, viewport.max(1.0))
    } else {
        raw_new
    };
    anim.set_immediate(displayed_new);

    if dt > 0.0 {
        let instant = delta / dt;
        let mixed = *velocity * 0.3 + instant * 0.7;
        *velocity = mixed.clamp(-MAX_FLING_VELOCITY, MAX_FLING_VELOCITY);
    }
}

/// Handle the end-of-drag transition for one axis: if past a boundary, the
/// spring pulls back; otherwise velocity is left in place for fling.
fn end_drag_axis(anim: &mut Animated<f32>, velocity: &mut f32, max_scroll: f32) {
    let cur = anim.value();
    if cur < 0.0 {
        *velocity = 0.0;
        anim.set_target(0.0);
    } else if cur > max_scroll {
        *velocity = 0.0;
        anim.set_target(max_scroll);
    }
    // Within bounds → velocity retained, fling starts on next tick.
}

/// Integrate one tick of fling along one axis. Clips at boundary, zeroing
/// velocity; the Animated spring (if target differs) handles bounce-back.
fn fling_axis(anim: &mut Animated<f32>, velocity: &mut f32, dt: f32, max_scroll: f32) {
    if velocity.abs() <= 0.5 {
        return;
    }
    let cur = anim.target();
    let new_pos = (cur - *velocity * dt).clamp(0.0, max_scroll);
    anim.set_immediate(new_pos);
    if new_pos == 0.0 || new_pos == max_scroll {
        *velocity = 0.0;
    } else {
        *velocity *= (-FLING_DECAY * dt).exp();
        if velocity.abs() < FLING_EPSILON {
            *velocity = 0.0;
        }
    }
}
