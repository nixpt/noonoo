//! # vaji-shell
//!
//! The interactive driver: a native window, an event loop, and the wiring that
//! turns OS input into [`vaji_core::events`] and layout+paint into frames.
//!
//! Everything here is feature-gated (`window`, `gpu`, `a11y`) so that the
//! engine can be used without a shell at all. A headless test, a server-side
//! renderer, or a host that brings its own windowing all skip this crate
//! entirely.
//!
//! `unsafe` lives here and only here, because the windowing and GPU stack
//! requires it. The engine crates do not.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.

#![warn(missing_docs)]

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
