//! # vaji-paint
//!
//! Turns the layout output of [`vaji_core`] into a stream of render commands:
//! fill a box, stroke an edge, draw a glyph run, apply a shadow or a gradient,
//! clip, composite.
//!
//! ## Why a command stream
//!
//! The command stream is backend-neutral. It is produced once and may be
//! consumed by a GPU rasterizer, an SVG writer, or a test that only records
//! what *would* be drawn. Making the diff-able artifact (the commands) the
//! interface means a rendering bug can be caught without a screenshot and a
//! pixel diff.
//!
//! The backend itself is borrowed, and reached from one module (see
//! `docs/DEPENDENCIES.md`); this crate owns the translation, which is the part
//! that is actually ours.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
