//! Reusable animation state types.
//!
//! Each type computes values over time. They do NOT build Elements —
//! pair them with renderer functions from `sabitori_core::tui` for display.

use sabitori_core::Color;
use crate::{EasingFunction, Lerp};

// ---------------------------------------------------------------------------
// TypewriterState
// ---------------------------------------------------------------------------

/// Typewriter animation: reveals text one character at a time.
pub struct TypewriterState {
    full_text: String,
    cycle: f32,
    typing_ratio: f32,
    cursor_blink_hz: f32,
    elapsed: f32,
}

impl TypewriterState {
    pub fn new(text: impl Into<String>, cycle: f32) -> Self {
        Self {
            full_text: text.into(),
            cycle,
            typing_ratio: 0.6,
            cursor_blink_hz: 2.0,
            elapsed: 0.0,
        }
    }

    pub fn tick(&mut self, dt: f32) { self.elapsed += dt; }

    pub fn visible_count(&self) -> usize {
        let progress = (self.elapsed % self.cycle) / (self.cycle * self.typing_ratio);
        ((progress.min(1.0) * self.full_text.len() as f32) as usize).min(self.full_text.len())
    }

    pub fn visible_text(&self) -> &str {
        let n = self.visible_count();
        // Find char boundary
        let mut end = 0;
        for (i, (idx, _)) in self.full_text.char_indices().enumerate() {
            if i == n { break; }
            end = idx + self.full_text[idx..].chars().next().map_or(0, |c| c.len_utf8());
        }
        &self.full_text[..end]
    }

    pub fn cursor_visible(&self) -> bool {
        let count = self.visible_count();
        count < self.full_text.len() && ((self.elapsed * self.cursor_blink_hz) as u32) % 2 == 0
    }

    pub fn full_text(&self) -> &str { &self.full_text }
}

// ---------------------------------------------------------------------------
// SpinnerState
// ---------------------------------------------------------------------------

/// Frame-based spinner animation.
pub struct SpinnerState {
    frames: Vec<String>,
    interval_ms: u32,
    elapsed: f32,
}

impl SpinnerState {
    pub fn new(frames: Vec<String>, interval_ms: u32) -> Self {
        Self { frames, interval_ms, elapsed: 0.0 }
    }

    fn from_strs(frames: &[&str], interval_ms: u32) -> Self {
        Self::new(frames.iter().map(|s| s.to_string()).collect(), interval_ms)
    }

    pub fn braille() -> Self {
        Self::from_strs(&["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"], 80)
    }
    pub fn line() -> Self {
        Self::from_strs(&["-","\\","|","/"], 100)
    }
    pub fn blocks() -> Self {
        Self::from_strs(&["▖","▘","▝","▗"], 120)
    }
    pub fn bounce() -> Self {
        Self::from_strs(&["⠁","⠂","⠄","⡀","⢀","⠠","⠐","⠈"], 100)
    }
    pub fn growing() -> Self {
        Self::from_strs(&["▏","▎","▍","▌","▋","▊","▉","█","▉","▊","▋","▌","▍","▎","▏"], 80)
    }

    pub fn tick(&mut self, dt: f32) { self.elapsed += dt; }

    pub fn current_frame(&self) -> &str {
        if self.frames.is_empty() { return ""; }
        let idx = ((self.elapsed * 1000.0) as u32 / self.interval_ms) as usize % self.frames.len();
        &self.frames[idx]
    }
}

// ---------------------------------------------------------------------------
// ProgressBarState
// ---------------------------------------------------------------------------

/// Animated progress bar (0.0 → target).
pub struct ProgressBarState {
    target: f32,
    current: f32,
    duration: f32,
    delay: f32,
    easing: EasingFunction,
    elapsed: f32,
}

impl ProgressBarState {
    pub fn new(target: f32, duration: f32) -> Self {
        Self {
            target: target.clamp(0.0, 1.0),
            current: 0.0,
            duration,
            delay: 0.0,
            easing: EasingFunction::EaseOutCubic,
            elapsed: 0.0,
        }
    }

    pub fn with_delay(mut self, delay: f32) -> Self {
        self.delay = delay;
        self
    }

    pub fn with_easing(mut self, easing: EasingFunction) -> Self {
        self.easing = easing;
        self
    }

    pub fn tick(&mut self, dt: f32) {
        self.elapsed += dt;
        let t = ((self.elapsed - self.delay).max(0.0) / self.duration).min(1.0);
        self.current = self.target * self.easing.eval(t);
    }

    pub fn progress(&self) -> f32 { self.current }

    pub fn bar_string(&self, total_chars: usize) -> String {
        let filled = (self.current * total_chars as f32) as usize;
        let empty = total_chars.saturating_sub(filled);
        format!("{}{}", "█".repeat(filled), "░".repeat(empty))
    }

    pub fn filled_count(&self, total_chars: usize) -> usize {
        (self.current * total_chars as f32) as usize
    }

    pub fn set_target(&mut self, target: f32) {
        self.target = target.clamp(0.0, 1.0);
    }
}

// ---------------------------------------------------------------------------
// GradientState
// ---------------------------------------------------------------------------

/// Cycling gradient across a color palette.
pub struct GradientState {
    colors: Vec<Color>,
    speed: f32,
    spread: f32,
    elapsed: f32,
}

impl GradientState {
    pub fn new(colors: Vec<Color>, speed: f32) -> Self {
        Self { spread: 5.0, colors, speed, elapsed: 0.0 }
    }

    pub fn with_spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }

    pub fn tick(&mut self, dt: f32) { self.elapsed += dt; }

    pub fn color_at(&self, char_index: usize) -> Color {
        if self.colors.is_empty() { return Color::WHITE; }
        let n = self.colors.len() as f32;
        let phase = (char_index as f32 / self.spread + self.elapsed * self.speed) % n;
        let idx = phase as usize % self.colors.len();
        let next = (idx + 1) % self.colors.len();
        let frac = phase.fract();
        self.colors[idx].lerp(self.colors[next], frac)
    }
}

// ---------------------------------------------------------------------------
// WaveState
// ---------------------------------------------------------------------------

/// Sine-wave vertical offset animation for text.
pub struct WaveState {
    speed: f32,
    wavelength: f32,
    amplitude: f32,
    elapsed: f32,
}

impl WaveState {
    pub fn new(speed: f32, wavelength: f32, amplitude: f32) -> Self {
        Self { speed, wavelength, amplitude, elapsed: 0.0 }
    }

    pub fn tick(&mut self, dt: f32) { self.elapsed += dt; }

    pub fn offset_at(&self, char_index: usize) -> f32 {
        let phase = char_index as f32 / self.wavelength - self.elapsed * self.speed;
        let sine = (phase * 2.0 * core::f32::consts::PI).sin();
        ((1.0 - sine) * self.amplitude).max(0.0)
    }
}

// ---------------------------------------------------------------------------
// PulseState
// ---------------------------------------------------------------------------

/// Ping-pong brightness oscillation.
pub struct PulseState {
    cycle: f32,
    min_val: f32,
    max_val: f32,
    easing: EasingFunction,
    elapsed: f32,
}

impl PulseState {
    pub fn new(cycle: f32, min_val: f32, max_val: f32) -> Self {
        Self {
            cycle,
            min_val,
            max_val,
            easing: EasingFunction::EaseInOutQuad,
            elapsed: 0.0,
        }
    }

    pub fn tick(&mut self, dt: f32) { self.elapsed += dt; }

    pub fn value(&self) -> f32 {
        let raw = (self.elapsed % self.cycle) / self.cycle;
        let ping = if raw < 0.5 { raw * 2.0 } else { 2.0 - raw * 2.0 };
        self.min_val + (self.max_val - self.min_val) * self.easing.eval(ping)
    }

    /// Apply brightness to a color (multiply RGB by value).
    pub fn apply_to_color(&self, color: Color) -> Color {
        let v = self.value();
        Color::new(color.r * v, color.g * v, color.b * v, color.a)
    }
}

// ---------------------------------------------------------------------------
// ColorCycleState
// ---------------------------------------------------------------------------

/// Cycles through a palette of colors with eased transitions.
pub struct ColorCycleState {
    colors: Vec<Color>,
    cycle_per_color: f32,
    easing: EasingFunction,
    elapsed: f32,
}

impl ColorCycleState {
    pub fn new(colors: Vec<Color>, cycle_per_color: f32) -> Self {
        Self {
            colors,
            cycle_per_color,
            easing: EasingFunction::EaseOutCubic,
            elapsed: 0.0,
        }
    }

    pub fn tick(&mut self, dt: f32) { self.elapsed += dt; }

    pub fn current_color(&self) -> Color {
        if self.colors.is_empty() { return Color::WHITE; }
        let n = self.colors.len() as f32;
        let phase = (self.elapsed / self.cycle_per_color) % n;
        let idx = phase as usize % self.colors.len();
        let next = (idx + 1) % self.colors.len();
        let frac = self.easing.eval(phase.fract());
        self.colors[idx].lerp(self.colors[next], frac)
    }

    pub fn active_index(&self) -> usize {
        if self.colors.is_empty() { return 0; }
        let phase = (self.elapsed / self.cycle_per_color) % self.colors.len() as f32;
        phase as usize % self.colors.len()
    }
}

// ---------------------------------------------------------------------------
// MotionState — composable positional animation
// ---------------------------------------------------------------------------

/// Direction from which the element enters.
#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Left,
    Right,
    Top,
    Bottom,
}

/// Bounce parameters for landing oscillation.
#[derive(Clone, Copy, Debug)]
struct BounceParams {
    amplitude: f32,
    decay: f32,
    frequency: f32,
}

/// High-level motion animation: fly-in + optional bounce.
///
/// ```ignore
/// let motion = MotionState::new(0.8)
///     .from(Direction::Right, 500.0)
///     .bounce(40.0)
///     .delay(0.3)
///     .easing(EasingFunction::EaseOutCubic);
///
/// let (dx, dy) = motion.offset(elapsed);
/// let alpha = motion.alpha(elapsed);
/// ```
pub struct MotionState {
    duration: f32,
    delay: f32,
    distance: f32,
    direction: Direction,
    easing: EasingFunction,
    bounce: Option<BounceParams>,
    bounce_start: f32, // fraction of duration when bounce kicks in (0.0-1.0)
}

impl MotionState {
    /// Create a new motion with the given duration in seconds.
    pub fn new(duration: f32) -> Self {
        Self {
            duration,
            delay: 0.0,
            distance: 300.0,
            direction: Direction::Right,
            easing: EasingFunction::EaseOutCubic,
            bounce: None,
            bounce_start: 0.3,
        }
    }

    /// Set the direction and distance to fly in from.
    pub fn from(mut self, direction: Direction, distance: f32) -> Self {
        self.direction = direction;
        self.distance = distance;
        self
    }

    /// Shorthand: fly in from the right.
    pub fn from_right(self, distance: f32) -> Self {
        self.from(Direction::Right, distance)
    }

    /// Shorthand: fly in from the left.
    pub fn from_left(self, distance: f32) -> Self {
        self.from(Direction::Left, distance)
    }

    /// Shorthand: fly in from the top.
    pub fn from_top(self, distance: f32) -> Self {
        self.from(Direction::Top, distance)
    }

    /// Shorthand: fly in from the bottom.
    pub fn from_bottom(self, distance: f32) -> Self {
        self.from(Direction::Bottom, distance)
    }

    /// Add a landing bounce (damped oscillation perpendicular to motion).
    /// `amplitude` is the max bounce in pixels.
    pub fn bounce(mut self, amplitude: f32) -> Self {
        self.bounce = Some(BounceParams {
            amplitude,
            decay: 4.0,
            frequency: 12.0,
        });
        self
    }

    /// Fine-tune bounce: decay rate (higher = faster settle) and frequency.
    pub fn bounce_tuned(mut self, amplitude: f32, decay: f32, frequency: f32) -> Self {
        self.bounce = Some(BounceParams { amplitude, decay, frequency });
        self
    }

    /// Set animation delay in seconds.
    pub fn delay(mut self, delay: f32) -> Self {
        self.delay = delay;
        self
    }

    /// Set easing function for the slide-in.
    pub fn easing(mut self, easing: EasingFunction) -> Self {
        self.easing = easing;
        self
    }

    /// Set when bounce starts as fraction of duration (0.0 - 1.0, default 0.3).
    pub fn bounce_start(mut self, frac: f32) -> Self {
        self.bounce_start = frac.clamp(0.0, 0.9);
        self
    }

    /// Raw progress (0.0 before start, 0.0-1.0 during, 1.0 after).
    fn raw_t(&self, elapsed: f32) -> f32 {
        ((elapsed - self.delay).max(0.0) / self.duration).min(1.0)
    }

    /// Whether the animation has started.
    pub fn started(&self, elapsed: f32) -> bool {
        elapsed >= self.delay
    }

    /// Whether the animation is fully complete.
    pub fn done(&self, elapsed: f32) -> bool {
        self.raw_t(elapsed) >= 1.0
    }

    /// Current (dx, dy) offset from the target position.
    /// Returns `(0.0, 0.0)` when animation is done.
    pub fn offset(&self, elapsed: f32) -> (f32, f32) {
        let t = self.raw_t(elapsed);
        if t <= 0.0 {
            return match self.direction {
                Direction::Right => (self.distance, 0.0),
                Direction::Left => (-self.distance, 0.0),
                Direction::Bottom => (0.0, self.distance),
                Direction::Top => (0.0, -self.distance),
            };
        }

        let eased = self.easing.eval(t);
        let remaining = (1.0 - eased) * self.distance;

        // Main axis offset
        let (mut dx, mut dy) = match self.direction {
            Direction::Right => (remaining, 0.0),
            Direction::Left => (-remaining, 0.0),
            Direction::Bottom => (0.0, remaining),
            Direction::Top => (0.0, -remaining),
        };

        // Bounce on cross-axis
        if let Some(ref b) = self.bounce {
            if t > self.bounce_start {
                let bt = (t - self.bounce_start) / (1.0 - self.bounce_start);
                let osc = (-b.decay * bt).exp() * (bt * b.frequency).sin() * b.amplitude;
                match self.direction {
                    Direction::Left | Direction::Right => dy += osc,
                    Direction::Top | Direction::Bottom => dx += osc,
                }
            }
        }

        (dx.round(), dy.round())
    }

    /// Current alpha (0.0 before start, fades in quickly, 1.0 when settled).
    pub fn alpha(&self, elapsed: f32) -> f32 {
        let t = self.raw_t(elapsed);
        (t * 4.0).min(1.0)
    }
}
