//! # noonoo-shell
//!
//! The interactive driver: a native window, an event loop, and the wiring that
//! turns OS input into [`noonoo_events`] and layout+paint into frames.
//!
//! Everything here is feature-gated (`window`, `gpu`, `a11y`) so the engine can
//! be used without a shell at all — a headless test, a server-side renderer, or
//! a host that brings its own windowing skips this crate entirely.
//!
//! `unsafe` lives here and only here, because the windowing and GPU stack
//! requires it. The engine crates do not.
//!
//! Two consumer-facing fixes are owned here rather than worked around:
//! **accessibility on Linux works out of the box** (the replaced engine left the
//! AT-SPI backend off by default, so the adapter was a no-op until consumers
//! re-enabled it), and the **event-loop hooks a window host needs** (`resumed`,
//! suspend) are real rather than stubs consumers route around.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.

#![warn(missing_docs)]

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
