//! # vaji-net
//!
//! Document loading, from whatever scheme the host is allowed to reach:
//! `http(s)://`, `file://`, `data:`, and the capability-gated `exo://` of the
//! host OS.
//!
//! ## Why its own crate
//!
//! In the replaced engine the network traits lived in a shared "traits"
//! grab-bag that every consumer had to depend on to get at anything, and the
//! provider lived beside them. Here the net surface — the `NetProvider` seam
//! and the built-in schemes — is one crate with no dependency on the DOM, style,
//! or layout. A host can replace it or drop it without touching the engine.
//!
//! Loading is a leaf: it hands bytes to [`vaji_html`] and nothing more.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
