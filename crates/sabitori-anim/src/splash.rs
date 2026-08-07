//! Splash screen animation presets.
//!
//! Each preset computes per-character (x, y, alpha) offsets for a given elapsed time.
//! The framework provides 10 built-in styles; users can implement custom ones.

use crate::EasingFunction;

/// A splash animation preset.
#[derive(Clone, Copy, Debug)]
pub enum SplashPreset {
    /// Characters fly in from right with bounce on landing.
    BounceIn,
    /// Characters fade in one by one, left to right.
    FadeIn,
    /// Characters appear with typewriter cursor.
    Typewriter,
    /// Characters drop from above with gravity.
    DropIn,
    /// Characters scale up from zero.
    ScaleUp,
    /// Characters slide in from below with stagger.
    SlideUp,
    /// Characters spin in from random angles.
    Scatter,
    /// Characters appear with a wave ripple.
    Ripple,
    /// Characters flash with a glitch effect.
    Glitch,
    /// All characters zoom in simultaneously.
    ZoomIn,
}

impl SplashPreset {
    /// Returns all 10 built-in presets.
    pub fn all() -> &'static [SplashPreset] {
        &[
            Self::BounceIn,
            Self::FadeIn,
            Self::Typewriter,
            Self::DropIn,
            Self::ScaleUp,
            Self::SlideUp,
            Self::Scatter,
            Self::Ripple,
            Self::Glitch,
            Self::ZoomIn,
        ]
    }

    /// Display name of this preset.
    pub fn name(&self) -> &'static str {
        match self {
            Self::BounceIn => "Bounce In",
            Self::FadeIn => "Fade In",
            Self::Typewriter => "Typewriter",
            Self::DropIn => "Drop In",
            Self::ScaleUp => "Scale Up",
            Self::SlideUp => "Slide Up",
            Self::Scatter => "Scatter",
            Self::Ripple => "Ripple",
            Self::Glitch => "Glitch",
            Self::ZoomIn => "Zoom In",
        }
    }

    /// Total recommended duration in seconds.
    pub fn duration(&self) -> f32 {
        match self {
            Self::Typewriter => 2.5,
            Self::Glitch => 2.0,
            Self::ZoomIn => 1.5,
            _ => 2.0,
        }
    }

    /// Compute the (dx, dy, alpha) for a character at index `i` of `total` characters.
    ///
    /// - `elapsed`: seconds since splash started
    /// - `i`: character index (0-based)
    /// - `total`: total number of characters
    /// - `viewport_w`: viewport width for off-screen calculations
    ///
    /// Returns `(dx, dy, alpha)` where dx/dy are pixel offsets from final position.
    pub fn char_state(&self, elapsed: f32, i: usize, total: usize, viewport_w: f32) -> (f32, f32, f32) {
        let char_delay = 0.08;
        let fi = i as f32;
        let ft = total as f32;

        match self {
            Self::BounceIn => {
                let start = fi * char_delay;
                let t = ((elapsed - start).max(0.0) / 0.8).min(1.0);
                if t <= 0.0 { return (viewport_w, 0.0, 0.0); }
                let x = (1.0 - EasingFunction::EaseOutCubic.eval(t)) * viewport_w;
                let bounce = if t > 0.3 {
                    let bt = (t - 0.3) / 0.7;
                    (-4.0 * bt).exp() * (bt * 12.0).sin() * 30.0
                } else { 0.0 };
                (x, bounce, (t * 4.0).min(1.0))
            }
            Self::FadeIn => {
                let start = fi * 0.1;
                let t = ((elapsed - start).max(0.0) / 0.4).min(1.0);
                let alpha = EasingFunction::EaseOutCubic.eval(t);
                (0.0, 0.0, alpha)
            }
            Self::Typewriter => {
                let chars_visible = ((elapsed / 0.08) as usize).min(total);
                if i < chars_visible {
                    (0.0, 0.0, 1.0)
                } else if i == chars_visible {
                    // Cursor blink
                    let blink = ((elapsed * 3.0) as u32) % 2 == 0;
                    (0.0, 0.0, if blink { 0.5 } else { 0.0 })
                } else {
                    (0.0, 0.0, 0.0)
                }
            }
            Self::DropIn => {
                let start = fi * char_delay;
                let t = ((elapsed - start).max(0.0) / 0.6).min(1.0);
                if t <= 0.0 { return (0.0, -200.0, 0.0); }
                let eased = EasingFunction::EaseOutCubic.eval(t);
                let y = (1.0 - eased) * -200.0;
                // Small bounce at landing
                let bounce = if t > 0.6 {
                    let bt = (t - 0.6) / 0.4;
                    (-8.0 * bt).exp() * (bt * 16.0).sin() * 10.0
                } else { 0.0 };
                (0.0, y + bounce, (t * 3.0).min(1.0))
            }
            Self::ScaleUp => {
                let start = fi * char_delay;
                let t = ((elapsed - start).max(0.0) / 0.5).min(1.0);
                let eased = EasingFunction::EaseOutBack.eval(t);
                // Scale simulated via y offset (characters "grow" from baseline)
                let y = (1.0 - eased) * 20.0;
                (0.0, y, eased)
            }
            Self::SlideUp => {
                let start = fi * 0.06;
                let t = ((elapsed - start).max(0.0) / 0.5).min(1.0);
                let eased = EasingFunction::EaseOutCubic.eval(t);
                let y = (1.0 - eased) * 60.0;
                (0.0, y, eased)
            }
            Self::Scatter => {
                let start = fi * 0.05;
                let t = ((elapsed - start).max(0.0) / 0.7).min(1.0);
                if t <= 0.0 { return (0.0, 0.0, 0.0); }
                let eased = EasingFunction::EaseOutCubic.eval(t);
                // Each char starts from a pseudo-random offset
                let seed = (fi * 7.31 + 2.71).sin();
                let sx = seed * 150.0 * (1.0 - eased);
                let sy = (seed * 3.14).cos() * 100.0 * (1.0 - eased);
                (sx, sy, eased)
            }
            Self::Ripple => {
                let center = ft / 2.0;
                let dist = (fi - center).abs();
                let start = dist * 0.1;
                let t = ((elapsed - start).max(0.0) / 0.5).min(1.0);
                let eased = EasingFunction::EaseOutCubic.eval(t);
                let y = (1.0 - eased) * 40.0;
                (0.0, y, eased)
            }
            Self::Glitch => {
                let t = (elapsed / 1.5).min(1.0);
                if t < 0.3 {
                    // Rapid random flicker
                    let hash = ((fi * 13.37 + elapsed * 50.0) as u32) % 7;
                    let alpha = if hash < 3 { 1.0 } else { 0.0 };
                    let jx = ((fi * 7.0 + elapsed * 30.0).sin() * 5.0) as f32;
                    (jx, 0.0, alpha)
                } else if t < 0.6 {
                    // Settle with occasional flicker
                    let hash = ((fi * 17.0 + elapsed * 20.0) as u32) % 10;
                    let jx = if hash < 2 { ((fi + elapsed * 10.0).sin() * 3.0) as f32 } else { 0.0 };
                    (jx, 0.0, 1.0)
                } else {
                    (0.0, 0.0, 1.0)
                }
            }
            Self::ZoomIn => {
                let t = (elapsed / 0.8).min(1.0);
                let eased = EasingFunction::EaseOutCubic.eval(t);
                // All characters zoom from center simultaneously
                let center_offset = (fi - ft / 2.0) * 20.0;
                let x = center_offset * (1.0 - eased);
                let y = (1.0 - eased) * 30.0;
                (x, y, eased)
            }
        }
    }
}
