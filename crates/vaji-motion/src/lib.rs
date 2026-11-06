//! # vaji-motion
//!
//! Motion primitives: spring timing and velocity-preserving retarget.
//!
//! Motion is **owned**, not borrowed — no crates.io engine does this the way the
//! consumers of the replaced engine wanted it, and it is small enough to keep.
//! The key property is that interrupting a running animation keeps both position
//! *and* velocity, so a retargeted element does not jump. A [`SpringState`]
//! carries that state; changing the target between steps preserves it.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// A damped-spring model.
///
/// Units are abstract; `stiffness`/`damping`/`mass` behave like their physical
/// namesakes. Higher `stiffness` and lower `mass` make the spring snappier;
/// higher `damping` settles it faster without overshoot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spring {
    /// Pull toward the target.
    pub stiffness: f32,
    /// Resistance to motion.
    pub damping: f32,
    /// Inertial mass.
    pub mass: f32,
}

impl Default for Spring {
    fn default() -> Self {
        Self {
            stiffness: 180.0,
            damping: 20.0,
            mass: 1.0,
        }
    }
}

impl Spring {
    /// A spring approximating "settle in `duration_secs`, with `bounce`".
    ///
    /// `bounce` in `[-1, 1]`: `0` is critically damped (fastest settle without
    /// overshoot), positive overshoots, negative is overdamped. This is a
    /// convenience mapping onto `stiffness`/`damping`, not a closed-form
    /// solution; if you need an exact curve, set the three fields directly.
    pub fn from_duration_bounce(duration_secs: f32, bounce: f32) -> Self {
        let duration = duration_secs.max(1e-3);
        let mass = 1.0_f32;
        let omega = std::f32::consts::TAU / duration;
        let stiffness = omega * omega * mass;
        let critical = 2.0 * (stiffness * mass).sqrt();
        let damping = critical * (1.0 - bounce.clamp(-1.0, 1.0));
        Self {
            stiffness,
            damping,
            mass,
        }
    }

    /// Advance one step of `dt` seconds toward `target`.
    ///
    /// Returns the new `(position, velocity)`. Because the state is carried by
    /// the caller, changing `target` between calls preserves velocity — the
    /// retarget property.
    pub fn step(&self, position: f32, velocity: f32, target: f32, dt: f32) -> (f32, f32) {
        let force = -self.stiffness * (position - target) - self.damping * velocity;
        let acceleration = force / self.mass;
        let velocity = velocity + acceleration * dt;
        let position = position + velocity * dt;
        (position, velocity)
    }
}

/// A spring's animated state: position and velocity.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SpringState {
    /// Current position.
    pub position: f32,
    /// Current velocity.
    pub velocity: f32,
}

impl SpringState {
    /// State starting at `position` with `velocity`.
    pub fn new(position: f32, velocity: f32) -> Self {
        Self { position, velocity }
    }

    /// Step this state toward `target` by `dt` seconds.
    ///
    /// Retargeting is just passing a different `target` next call.
    pub fn step(&mut self, spring: &Spring, target: f32, dt: f32) {
        let (position, velocity) = spring.step(self.position, self.velocity, target, dt);
        self.position = position;
        self.velocity = velocity;
    }

    /// Whether the state is at rest at `target`, within `epsilon`.
    pub fn is_settled(&self, target: f32, epsilon: f32) -> bool {
        (self.position - target).abs() < epsilon && self.velocity.abs() < epsilon
    }
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_settles_on_target() {
        let spring = Spring::default();
        let mut state = SpringState::new(0.0, 0.0);
        for _ in 0..600 {
            state.step(&spring, 1.0, 1.0 / 60.0);
        }
        assert!(state.is_settled(1.0, 1e-3), "state = {state:?}");
    }

    #[test]
    fn retarget_keeps_velocity() {
        let spring = Spring::default();
        let mut state = SpringState::new(0.0, 0.0);
        state.step(&spring, 1.0, 1.0 / 60.0);
        let velocity_before = state.velocity;
        // Retarget only changes what we pass; the state is untouched.
        state.step(&spring, 2.0, 0.0);
        assert_eq!(state.velocity, velocity_before);
    }

    #[test]
    fn duration_bounce_settles_near_duration() {
        let spring = Spring::from_duration_bounce(0.3, 0.0);
        let mut state = SpringState::new(0.0, 0.0);
        let dt = 1.0 / 240.0;
        let mut elapsed = 0.0;
        while elapsed < 1.0 {
            state.step(&spring, 1.0, dt);
            elapsed += dt;
            if state.is_settled(1.0, 1e-2) {
                break;
            }
        }
        // The mapping is a heuristic: "duration" is roughly the settle time,
        // not an exact bound.
        assert!(elapsed <= 0.8, "settled in {elapsed}s");
    }
}
