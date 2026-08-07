/// Critically damped spring configuration.
/// Produces natural-feeling UI animations without specifying duration.
#[derive(Clone, Copy, Debug)]
pub struct Spring {
    /// Spring stiffness. Higher = snappier. Default: 200.0
    pub stiffness: f32,
    /// Damping coefficient. Default: critically damped.
    pub damping: f32,
    /// Mass. Default: 1.0
    pub mass: f32,
}

impl Spring {
    /// Create a critically damped spring (no overshoot).
    pub fn critical(stiffness: f32) -> Self {
        let mass = 1.0;
        let damping = 2.0 * (stiffness * mass).sqrt();
        Self {
            stiffness,
            damping,
            mass,
        }
    }

    /// Snappy spring (fast, slight overshoot).
    pub fn snappy() -> Self {
        Self {
            stiffness: 400.0,
            damping: 25.0,
            mass: 1.0,
        }
    }

    /// Gentle spring (slow, no overshoot).
    pub fn gentle() -> Self {
        Self::critical(100.0)
    }

    /// Bouncy spring (visible overshoot).
    pub fn bouncy() -> Self {
        Self {
            stiffness: 300.0,
            damping: 15.0,
            mass: 1.0,
        }
    }

    /// Advance the spring simulation by `dt` seconds.
    /// Returns (new_value, new_velocity, is_settled).
    pub fn step(&self, value: f32, velocity: f32, target: f32, dt: f32) -> (f32, f32, bool) {
        let displacement = value - target;
        let spring_force = -self.stiffness * displacement;
        let damping_force = -self.damping * velocity;
        let acceleration = (spring_force + damping_force) / self.mass;

        let new_velocity = velocity + acceleration * dt;
        let new_value = value + new_velocity * dt;

        // Check if settled
        let settled = displacement.abs() < 0.001 && velocity.abs() < 0.001;

        if settled {
            (target, 0.0, true)
        } else {
            (new_value, new_velocity, false)
        }
    }
}

impl Default for Spring {
    fn default() -> Self {
        Self::critical(200.0)
    }
}
