//! # vaji-layout
//!
//! Box layout: computed style plus measured text into boxes and geometry.
//!
//! ## Borrow boundary
//!
//! This crate is the **only** place that may name `taffy`. It reads computed
//! style from [`vaji_style`], asks [`vaji_text`] for line boxes, and produces a
//! positioned box tree.
//!
//! This is the crate the replaced engine's `bliss-dom` and its `stylo_taffy`
//! bridge jointly embodied, now split so the style→layout contract is a
//! named, testable seam rather than a helper sub-crate.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
