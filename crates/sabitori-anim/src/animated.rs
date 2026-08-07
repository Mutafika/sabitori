use crate::{EasingFunction, Lerp, Spring};

/// Animation mode.
#[derive(Clone, Copy, Debug)]
pub enum AnimationMode {
    /// Spring physics (default for UI).
    Spring(Spring),
    /// Duration-based easing.
    Easing {
        duration: f32,
        function: EasingFunction,
    },
    /// Instant (no animation).
    Instant,
}

impl Default for AnimationMode {
    fn default() -> Self {
        Self::Spring(Spring::default())
    }
}

/// Repeat behavior for animations.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RepeatMode {
    /// Play once and stop.
    Once,
    /// Loop N times (0 = infinite).
    Loop(u32),
    /// Ping-pong: forward then reverse, N times (0 = infinite).
    PingPong(u32),
}

impl Default for RepeatMode {
    fn default() -> Self {
        Self::Once
    }
}

/// A single keyframe in a keyframe animation.
#[derive(Clone, Debug)]
pub struct Keyframe<T: Lerp> {
    /// Progress at which this keyframe is reached (0.0 to 1.0).
    pub progress: f32,
    /// Value at this keyframe.
    pub value: T,
    /// Easing from the previous keyframe to this one.
    pub easing: EasingFunction,
}

/// Animated value wrapper. Smoothly transitions from current to target.
/// Supports springs, easing, keyframes, delays, chaining, and looping.
#[derive(Clone, Debug)]
pub struct Animated<T: Lerp> {
    current: T,
    start: T,
    target: T,
    velocity: f32,
    elapsed: f32,
    mode: AnimationMode,
    pub running: bool,

    // Delay
    delay: f32,
    delay_remaining: f32,

    // Repeat / loop
    repeat: RepeatMode,
    repeat_count: u32,
    /// For ping-pong: true = forward, false = reverse.
    forward: bool,

    // Keyframes (optional, overrides simple start→target when non-empty)
    keyframes: Vec<Keyframe<T>>,

    // Chain: animations to play after this one completes
    chain: Vec<ChainedAnimation<T>>,
    chain_index: usize,

    // Completion callback index (for external tracking)
    pub completed_count: u32,
}

/// A queued animation in a chain.
#[derive(Clone, Debug)]
pub struct ChainedAnimation<T: Lerp> {
    pub target: T,
    pub mode: AnimationMode,
    pub delay: f32,
}

impl<T: Lerp> Animated<T> {
    pub fn new(value: T) -> Self {
        Self {
            current: value,
            start: value,
            target: value,
            velocity: 0.0,
            elapsed: 0.0,
            mode: AnimationMode::default(),
            running: false,
            delay: 0.0,
            delay_remaining: 0.0,
            repeat: RepeatMode::Once,
            repeat_count: 0,
            forward: true,
            keyframes: Vec::new(),
            chain: Vec::new(),
            chain_index: 0,
            completed_count: 0,
        }
    }

    // -- Configuration --

    pub fn with_mode(mut self, mode: AnimationMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn with_spring(mut self, spring: Spring) -> Self {
        self.mode = AnimationMode::Spring(spring);
        self
    }

    pub fn with_delay(mut self, delay: f32) -> Self {
        self.delay = delay;
        self
    }

    pub fn with_repeat(mut self, repeat: RepeatMode) -> Self {
        self.repeat = repeat;
        self
    }

    /// Loop infinitely.
    pub fn looping(mut self) -> Self {
        self.repeat = RepeatMode::Loop(0);
        self
    }

    /// Ping-pong infinitely.
    pub fn ping_pong(mut self) -> Self {
        self.repeat = RepeatMode::PingPong(0);
        self
    }

    // -- Keyframes --

    /// Set keyframe animation. Progress values should be 0.0 to 1.0.
    /// The first keyframe should be at progress 0.0 (start) and the last at 1.0 (end).
    pub fn with_keyframes(mut self, keyframes: Vec<Keyframe<T>>) -> Self {
        self.keyframes = keyframes;
        self
    }

    /// Add a single keyframe.
    pub fn keyframe(mut self, progress: f32, value: T, easing: EasingFunction) -> Self {
        self.keyframes.push(Keyframe {
            progress,
            value,
            easing,
        });
        // Keep sorted by progress
        self.keyframes.sort_by(|a, b| a.progress.partial_cmp(&b.progress).unwrap());
        self
    }

    // -- Chaining --

    /// Queue an animation to play after the current one completes.
    pub fn then(mut self, target: T, mode: AnimationMode) -> Self {
        self.chain.push(ChainedAnimation {
            target,
            mode,
            delay: 0.0,
        });
        self
    }

    /// Queue an animation with a delay.
    pub fn then_delay(mut self, delay: f32, target: T, mode: AnimationMode) -> Self {
        self.chain.push(ChainedAnimation {
            target,
            mode,
            delay,
        });
        self
    }

    // -- Runtime --

    /// Get the current interpolated value.
    pub fn value(&self) -> T {
        self.current
    }

    /// Get the current animation target (what `value()` is approaching).
    pub fn target(&self) -> T {
        self.target
    }

    /// Set a new target. Starts animating (resets chain).
    pub fn set_target(&mut self, target: T) {
        if self.target.distance(target) > 0.001 {
            self.start = self.current;
            self.target = target;
            self.elapsed = 0.0;
            self.velocity = 0.0;
            self.delay_remaining = self.delay;
            self.running = true;
            self.forward = true;
            self.repeat_count = 0;
            self.chain_index = 0;
        }
    }

    /// Set value immediately (no animation).
    pub fn set_immediate(&mut self, value: T) {
        self.current = value;
        self.start = value;
        self.target = value;
        self.velocity = 0.0;
        self.running = false;
        self.chain_index = 0;
    }

    /// Update target without resetting velocity / start / elapsed.
    /// Use for continuous input (scroll wheel, trackpad delta) where we
    /// want the spring to keep chasing without restarting each event.
    /// Ensures `running = true` so the integrator advances.
    pub fn nudge_target(&mut self, target: T) {
        self.target = target;
        self.running = true;
    }

    /// Start an animation to a target (like set_target but always starts even if same distance).
    pub fn animate_to(&mut self, target: T, mode: AnimationMode) {
        self.start = self.current;
        self.target = target;
        self.mode = mode;
        self.elapsed = 0.0;
        self.velocity = 0.0;
        self.delay_remaining = self.delay;
        self.running = true;
        self.forward = true;
        self.repeat_count = 0;
        self.chain_index = 0;
    }

    /// Advance the animation by `dt` seconds. Returns true if still running.
    pub fn tick(&mut self, dt: f32) -> bool {
        if !self.running {
            return false;
        }

        // Handle delay
        if self.delay_remaining > 0.0 {
            self.delay_remaining -= dt;
            if self.delay_remaining > 0.0 {
                return true; // still waiting
            }
            // Consume leftover into elapsed
            let leftover = -self.delay_remaining;
            self.delay_remaining = 0.0;
            self.tick_inner(leftover);
        } else {
            self.tick_inner(dt);
        }

        self.running
    }

    fn tick_inner(&mut self, dt: f32) {
        if !self.keyframes.is_empty() {
            self.tick_keyframes(dt);
        } else {
            self.tick_simple(dt);
        }
    }

    fn tick_simple(&mut self, dt: f32) {
        let completed = match self.mode {
            AnimationMode::Spring(spring) => {
                let dist = self.current.distance(self.target);
                let (new_dist, new_vel, settled) =
                    spring.step(dist, self.velocity, 0.0, dt);
                self.velocity = new_vel;

                if settled || dist < 0.001 {
                    self.current = self.target;
                    self.velocity = 0.0;
                    true
                } else {
                    let t = 1.0 - (new_dist / dist).clamp(0.0, 1.0);
                    self.current = self.current.lerp(self.target, t);
                    false
                }
            }
            AnimationMode::Easing { duration, function } => {
                self.elapsed += dt;
                let raw_t = if duration > 0.0 {
                    (self.elapsed / duration).min(1.0)
                } else {
                    1.0
                };
                let eased = function.eval(raw_t);
                self.current = self.start.lerp(self.target, eased);
                raw_t >= 1.0
            }
            AnimationMode::Instant => {
                self.current = self.target;
                true
            }
        };

        if completed {
            self.on_segment_complete();
        }
    }

    fn tick_keyframes(&mut self, dt: f32) {
        let total_duration = match self.mode {
            AnimationMode::Easing { duration, .. } => duration,
            _ => 1.0, // default 1 second for keyframe animations
        };

        self.elapsed += dt;
        let raw_progress = if total_duration > 0.0 {
            (self.elapsed / total_duration).min(1.0)
        } else {
            1.0
        };

        let progress = if self.forward {
            raw_progress
        } else {
            1.0 - raw_progress
        };

        // Find the two keyframes we're between
        self.current = self.eval_keyframes(progress);

        if raw_progress >= 1.0 {
            self.on_segment_complete();
        }
    }

    fn eval_keyframes(&self, progress: f32) -> T {
        if self.keyframes.is_empty() {
            return self.current;
        }
        if self.keyframes.len() == 1 {
            return self.keyframes[0].value;
        }

        // Find surrounding keyframes
        let mut prev_idx = 0;
        let mut next_idx = self.keyframes.len() - 1;
        for (i, kf) in self.keyframes.iter().enumerate() {
            if kf.progress <= progress {
                prev_idx = i;
            }
            if kf.progress >= progress && i > prev_idx {
                next_idx = i;
                break;
            }
        }

        let prev = &self.keyframes[prev_idx];
        let next = &self.keyframes[next_idx];

        if prev_idx == next_idx {
            return prev.value;
        }

        let segment_range = next.progress - prev.progress;
        let local_t = if segment_range > 0.0 {
            ((progress - prev.progress) / segment_range).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let eased_t = next.easing.eval(local_t);

        prev.value.lerp(next.value, eased_t)
    }

    fn on_segment_complete(&mut self) {
        self.completed_count += 1;

        // Check repeat
        match self.repeat {
            RepeatMode::Once => {}
            RepeatMode::Loop(max) => {
                self.repeat_count += 1;
                if max == 0 || self.repeat_count < max {
                    // Restart
                    self.elapsed = 0.0;
                    self.velocity = 0.0;
                    self.current = self.start;
                    return;
                }
            }
            RepeatMode::PingPong(max) => {
                self.repeat_count += 1;
                if max == 0 || self.repeat_count < max * 2 {
                    // Reverse direction
                    self.forward = !self.forward;
                    self.elapsed = 0.0;
                    self.velocity = 0.0;
                    std::mem::swap(&mut self.start, &mut self.target);
                    return;
                }
            }
        }

        // Check chain
        if self.chain_index < self.chain.len() {
            let next = &self.chain[self.chain_index];
            self.start = self.current;
            self.target = next.target;
            self.mode = next.mode;
            self.delay_remaining = next.delay;
            self.elapsed = 0.0;
            self.velocity = 0.0;
            self.chain_index += 1;
            return;
        }

        // Fully done
        self.running = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EasingFunction;

    #[test]
    fn basic_easing() {
        let mut a = Animated::new(0.0f32).with_mode(AnimationMode::Easing {
            duration: 1.0,
            function: EasingFunction::Linear,
        });
        a.set_target(100.0);

        // After 0.5s should be ~50
        a.tick(0.5);
        assert!((a.value() - 50.0).abs() < 1.0);

        // After 1.0s total should be 100
        a.tick(0.5);
        assert!((a.value() - 100.0).abs() < 0.1);
        assert!(!a.running);
    }

    #[test]
    fn delay_works() {
        let mut a = Animated::new(0.0f32)
            .with_mode(AnimationMode::Easing {
                duration: 1.0,
                function: EasingFunction::Linear,
            })
            .with_delay(0.5);
        a.set_target(100.0);

        // During delay, value should stay at 0
        a.tick(0.3);
        assert!((a.value() - 0.0).abs() < 0.1);

        // After delay passes, should start moving
        a.tick(0.5); // 0.3 delay remaining + 0.3 into animation
        assert!(a.value() > 0.0);
    }

    #[test]
    fn loop_repeats() {
        let mut a = Animated::new(0.0f32)
            .with_mode(AnimationMode::Easing {
                duration: 1.0,
                function: EasingFunction::Linear,
            })
            .with_repeat(RepeatMode::Loop(3));
        a.set_target(100.0);

        // Complete 3 loops
        for _ in 0..3 {
            a.tick(1.0);
        }
        assert!(!a.running);
        assert_eq!(a.completed_count, 3);
    }

    #[test]
    fn ping_pong() {
        let mut a = Animated::new(0.0f32)
            .with_mode(AnimationMode::Easing {
                duration: 1.0,
                function: EasingFunction::Linear,
            })
            .with_repeat(RepeatMode::PingPong(1));
        a.set_target(100.0);

        // Forward
        a.tick(1.0);
        assert!((a.value() - 100.0).abs() < 1.0);
        assert!(a.running);

        // Reverse
        a.tick(1.0);
        assert!((a.value() - 0.0).abs() < 1.0);
        assert!(!a.running);
    }

    #[test]
    fn chain_animations() {
        let mut a = Animated::new(0.0f32)
            .with_mode(AnimationMode::Easing {
                duration: 0.5,
                function: EasingFunction::Linear,
            })
            .then(
                200.0,
                AnimationMode::Easing {
                    duration: 0.5,
                    function: EasingFunction::Linear,
                },
            );
        a.set_target(100.0);

        // First: 0 → 100
        a.tick(0.5);
        assert!((a.value() - 100.0).abs() < 1.0);
        assert!(a.running); // chain continues

        // Second: 100 → 200
        a.tick(0.5);
        assert!((a.value() - 200.0).abs() < 1.0);
        assert!(!a.running);
    }

    #[test]
    fn keyframes() {
        let mut a = Animated::new(0.0f32)
            .with_mode(AnimationMode::Easing {
                duration: 1.0,
                function: EasingFunction::Linear,
            })
            .with_keyframes(vec![
                Keyframe { progress: 0.0, value: 0.0, easing: EasingFunction::Linear },
                Keyframe { progress: 0.5, value: 100.0, easing: EasingFunction::Linear },
                Keyframe { progress: 1.0, value: 50.0, easing: EasingFunction::Linear },
            ]);
        a.set_target(50.0);

        // At 0.5s should be at keyframe value 100
        a.tick(0.5);
        assert!((a.value() - 100.0).abs() < 5.0);

        // At 1.0s should be back to 50
        a.tick(0.5);
        assert!((a.value() - 50.0).abs() < 5.0);
    }
}
