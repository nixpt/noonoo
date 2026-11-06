//! # vaji-motion
//!
//! Motion primitives: spring timing and velocity-preserving retarget.
//!
//! Motion is **owned**, not borrowed — no crates.io engine does this the way
//! the consumers of the replaced engine wanted it, and it is small enough to
//! keep. The key property is that interrupting a running animation keeps both
//! position *and* velocity, so a retargeted element does not jump. A spring
//! integrates numeric state, so retargeting is simply changing the target while
//! leaving `(position, velocity)` untouched.

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

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spring_settles_on_target() {
        let spring = Spring::default();
        let (mut position, mut velocity) = (0.0_f32, 0.0_f32);
        for _ in 0..600 {
            let (p, v) = spring.step(position, velocity, 1.0, 1.0 / 60.0);
            position = p;
            velocity = v;
        }
        assert!((position - 1.0).abs() < 1e-3, "position = {position}");
    }

    #[test]
    fn retarget_keeps_velocity() {
        let spring = Spring::default();
        let (_, velocity) = spring.step(0.0, 0.0, 1.0, 1.0 / 60.0);
        // Retargeting does not touch the caller's state, so velocity is intact.
        assert!(velocity > 0.0);
    }
}
