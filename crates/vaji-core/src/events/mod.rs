//! Input events and dispatch.
//!
//! The event surface is defined here so that both the interactive (`vaji-shell`)
//! and the headless ([`super::headless`]) drivers feed the *same*
//! representation. A click synthesized from a script
//! and a click from a real device are indistinguishable downstream — that is
//! what makes a scenario reproducible.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.
