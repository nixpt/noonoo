//! # vaji-html
//!
//! Markup ingestion: HTML (and, later, SVG and XML) into [`vaji_dom`].
//!
//! ## Borrow boundary
//!
//! This crate is the **only** place that may name an HTML parser. It parses
//! once, into the format-neutral tree; downstream layers never see the parser's
//! types.
//!
//! A correctness note the replaced engine earned the hard way: installing the
//! parser provider was optional and its absence was a *silent* no-op, so
//! `set_inner_html` quietly parsed nothing until every consumer remembered to
//! pass it. Here, parsing is the crate's whole reason to exist — there is no
//! configuration under which markup silently fails to parse.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
