//! Style resolution: stylesheet ingestion and the cascade.
//!
//! ## Borrow boundary
//!
//! This module is the **only** place in the workspace that may name Servo's
//! `stylo` (and its `selectors` / `cssparser` companions). It parses a
//! stylesheet, matches selectors against the [`crate::dom`] tree, and produces
//! computed values. Callers above it see a small, Vaji-shaped result, not
//! `stylo` types.
//!
//! The engine itself is not reimplemented: a spec-complete CSS cascade is
//! Servo's crown jewel and not where Vaji's value lives (see
//! `docs/DEPENDENCIES.md`). What is ours is the *integration*: the driver, the
//! element-data bridge, and the shape of the computed values handed onward.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.
