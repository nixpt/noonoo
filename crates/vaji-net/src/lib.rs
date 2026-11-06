//! # vaji-net
//!
//! Document loading, from whatever scheme the host is allowed to reach:
//! `http(s)://`, `file://`, and the capability-gated `exo://` of the host OS.
//!
//! Loading is deliberately a leaf: it hands bytes to [`vaji_core::html`] and
//! nothing more. It does not know about the DOM, style, or layout, so a host
//! can replace or remove it without touching the engine.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
