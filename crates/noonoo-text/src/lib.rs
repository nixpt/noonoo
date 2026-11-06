//! # noonoo-text
//!
//! Text shaping, font fallback, and line breaking.
//!
//! ## Borrow boundary
//!
//! This crate is the **only** place that may name `parley` (and, through it,
//! `fontique` / `swash` / `skrifa`). It answers one question for the layout
//! crate — "given this string, this style, and this available width, what are
//! the lines and their metrics?" — and returns a Noonoo-shaped value.
//!
//! Text is split out from layout (which the replaced engine did not do) because
//! the two are genuinely different engines with genuinely different failure
//! modes: a layout bug is boxes in the wrong place, a text bug is glyphs in the
//! wrong place. Keeping them in separate crates makes each boundary one place
//! to look and one place to measure.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
