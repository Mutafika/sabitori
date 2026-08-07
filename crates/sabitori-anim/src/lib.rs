mod spring;
mod easing;
mod animated;
pub mod widgets;
pub mod splash;

pub use splash::SplashPreset;

pub use spring::Spring;
pub use easing::EasingFunction;
pub use animated::{Animated, AnimationMode, ChainedAnimation, Keyframe, RepeatMode};
pub use widgets::{
    TypewriterState, SpinnerState, ProgressBarState,
    GradientState, WaveState, PulseState, ColorCycleState,
    MotionState, Direction,
};

/// Trait for values that can be linearly interpolated.
pub trait Lerp: Copy {
    fn lerp(self, target: Self, t: f32) -> Self;
    fn distance(self, other: Self) -> f32;
}

impl Lerp for f32 {
    fn lerp(self, target: Self, t: f32) -> Self {
        self + (target - self) * t
    }
    fn distance(self, other: Self) -> f32 {
        (self - other).abs()
    }
}

impl Lerp for [f32; 2] {
    fn lerp(self, target: Self, t: f32) -> Self {
        [
            self[0] + (target[0] - self[0]) * t,
            self[1] + (target[1] - self[1]) * t,
        ]
    }
    fn distance(self, other: Self) -> f32 {
        let mut sum = 0.0f32;
        for i in 0..2 {
            sum += (self[i] - other[i]).powi(2);
        }
        sum.sqrt()
    }
}

impl Lerp for [f32; 4] {
    fn lerp(self, target: Self, t: f32) -> Self {
        [
            self[0] + (target[0] - self[0]) * t,
            self[1] + (target[1] - self[1]) * t,
            self[2] + (target[2] - self[2]) * t,
            self[3] + (target[3] - self[3]) * t,
        ]
    }
    fn distance(self, other: Self) -> f32 {
        let mut sum = 0.0f32;
        for i in 0..4 {
            sum += (self[i] - other[i]).powi(2);
        }
        sum.sqrt()
    }
}

impl Lerp for sabitori_core::Color {
    fn lerp(self, target: Self, t: f32) -> Self {
        sabitori_core::Color::new(
            self.r + (target.r - self.r) * t,
            self.g + (target.g - self.g) * t,
            self.b + (target.b - self.b) * t,
            self.a + (target.a - self.a) * t,
        )
    }
    fn distance(self, other: Self) -> f32 {
        ((self.r - other.r).powi(2)
            + (self.g - other.g).powi(2)
            + (self.b - other.b).powi(2)
            + (self.a - other.a).powi(2))
        .sqrt()
    }
}
