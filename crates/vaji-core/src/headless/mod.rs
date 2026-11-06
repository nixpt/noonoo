//! Headless operation: run Vaji with no display.
//!
//! Three capabilities, all deterministic:
//!
//! 1. **Tree dump** — the resolved tree as structured text (JSON), so an agent
//!    or a test can assert on structure without a screenshot.
//! 2. **Virtual clock** — time advances only when the caller says so, so a
//!    frame is reproducible and animation is testable.
//! 3. **Synthetic input** — feed [`crate::events`] from a script (click, type,
//!    scroll, ...) and observe the result.
//!
//! This is a first-class feature, not a debug aid: an engine that can be
//! observed and driven without a screen is one an agent can verify.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.
