//! # vaji-core
//!
//! The heart of Vaji: the document tree, the style resolver, and the layout
//! pipeline. Everything else in the workspace reads from this crate or turns
//! its output into pixels.
//!
//! ## Layout of this crate
//!
//! - [`dom`] — the arena-backed document tree (format-neutral; the one place
//!   node identity is defined).
//! - [`style`] — stylesheet ingestion and the cascade. **Borrowed:** drives
//!   Servo's `stylo` behind a single module.
//! - [`layout`] — turns computed style into boxes and geometry. **Borrowed:**
//!   drives `taffy` (+ `parley` for text) behind a single module.
//! - [`html`] — markup ingestion into [`dom`].
//! - [`events`] — input and the event dispatch surface.
//! - [`headless`] — deterministic, display-free operation: tree dump, virtual
//!   clock, synthetic input.
//!
//! ## Non-negotiable invariants
//!
//! 1. **No `unsafe`.** This crate is `#![forbid(unsafe_code)]`. The tree is a
//!    plain `Vec` arena; parallel work shares data by value, never through a
//!    hand-rolled `Sync` promise.
//! 2. **One boundary per borrowed engine.** `stylo` and `taffy`/`parley` are
//!    reached from exactly one module each (see [`style`] and [`layout`]);
//!    no other module may name their types.
//! 3. **No wrap-for-wrap's-sake traits.** The borrowed engines are not going
//!    to be swapped, so they are not abstracted behind traits.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod dom;
pub mod events;
pub mod headless;
pub mod html;
pub mod layout;
pub mod style;

pub use dom::{Node, NodeId, Tree};

/// The version of the Vaji engine, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
