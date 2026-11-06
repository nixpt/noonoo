//! # vaji-headless
//!
//! Run Vaji with no display. Three deterministic capabilities:
//!
//! 1. **Tree dump** — the resolved tree as structured text (JSON), so an agent
//!    or a test can assert on structure without a screenshot.
//! 2. **Virtual clock** — time advances only when the caller says so, so a
//!    frame is reproducible and animation is testable.
//! 3. **Synthetic input** — feed [`vaji_events`] from a script (click, type,
//!    scroll, ...) and observe the result.
//!
//! This is a first-class crate, not a debug aid: an engine that can be observed
//! and driven without a screen is one an agent can verify. Consumers of the
//! replaced engine built a devtools node registry and a headless browse path
//! precisely because the engine did not offer these.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
