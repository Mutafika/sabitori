/// Standard CSS easing functions.
#[derive(Clone, Copy, Debug)]
pub enum EasingFunction {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    EaseInQuad,
    EaseOutQuad,
    EaseInOutQuad,
    EaseOutCubic,
    EaseOutBack,
    EaseOutElastic,
    EaseInBack,
    CubicBezier(f32, f32, f32, f32),
}

impl EasingFunction {
    /// Evaluate the easing function at time t (0..1).
    pub fn eval(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::EaseIn => t * t * t,
            Self::EaseOut => {
                let t = 1.0 - t;
                1.0 - t * t * t
            }
            Self::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    let t = 1.0 - t;
                    1.0 - 4.0 * t * t * t
                }
            }
            Self::EaseInQuad => t * t,
            Self::EaseOutQuad => {
                let inv = 1.0 - t;
                1.0 - inv * inv
            }
            Self::EaseInOutQuad => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    let inv = 2.0 * t - 1.0;
                    1.0 - 0.5 * inv * inv
                }
            }
            Self::EaseOutCubic => {
                let inv = 1.0 - t;
                1.0 - inv * inv * inv
            }
            Self::EaseOutBack => {
                const C1: f32 = 1.70158;
                const C3: f32 = C1 + 1.0;
                let s = t - 1.0;
                1.0 + C3 * s * s * s + C1 * s * s
            }
            Self::EaseOutElastic => {
                if t <= 0.0 || t >= 1.0 {
                    t
                } else {
                    let power = (-10.0 * t).exp2();
                    let angle = (10.0 * t - 0.75) * core::f32::consts::TAU / 3.0;
                    power * angle.sin() + 1.0
                }
            }
            Self::EaseInBack => {
                const C1: f32 = 1.70158;
                const C3: f32 = C1 + 1.0;
                C3 * t * t * t - C1 * t * t
            }
            Self::CubicBezier(x1, y1, x2, y2) => {
                cubic_bezier(x1, y1, x2, y2, t)
            }
        }
    }
}

/// Approximate cubic bezier curve evaluation.
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    // Newton's method to find the t parameter for the given x
    let mut guess = t;
    for _ in 0..8 {
        let x = bezier_component(x1, x2, guess) - t;
        let dx = bezier_derivative(x1, x2, guess);
        if dx.abs() < 1e-6 {
            break;
        }
        guess -= x / dx;
    }
    bezier_component(y1, y2, guess)
}

fn bezier_component(p1: f32, p2: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    3.0 * (1.0 - t) * (1.0 - t) * t * p1 + 3.0 * (1.0 - t) * t2 * p2 + t3
}

fn bezier_derivative(p1: f32, p2: f32, t: f32) -> f32 {
    let t2 = t * t;
    3.0 * (1.0 - t) * (1.0 - t) * p1 + 6.0 * (1.0 - t) * t * (p2 - p1) + 3.0 * t2 * (1.0 - p2)
}
