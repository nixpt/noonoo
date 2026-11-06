//! # vaji-style
//!
//! Stylesheet ingestion and the cascade.
//!
//! ## Borrow boundary
//!
//! This crate is the **only** place that may name Servo's `stylo` (with its
//! `selectors` / `cssparser` companions). It parses stylesheets, matches
//! selectors against [`vaji_dom`], and produces computed values. Callers above
//! it see a small, Vaji-shaped result, never `stylo` types.
//!
//! The engine is not reimplemented: a spec-complete cascade is Servo's crown
//! jewel and not where Vaji's value lives. What is ours is the integration —
//! the driver, the element-data bridge, and the shape of the computed values
//! handed onward. See `docs/DEPENDENCIES.md`.
//!
//! Two things the replaced engine did not offer and Vaji will:
//! **incremental** restyle by default (the old engine shipped it behind a
//! feature most consumers never enabled, so every edit rebuilt all layout), and
//! a computed-style snapshot that is always valid — the old engine panicked
//! Stylo when asked to snapshot a never-styled element.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
