//! Markup ingestion: HTML (and, later, SVG) into [`crate::dom`].
//!
//! Parsing happens once, into the format-neutral tree. Downstream layers never
//! see the parser's types; they see [`crate::dom::Node`]s. This keeps the
//! choice of parser a private detail and keeps a second syntax (SVG, XML) from
//! leaking a second vocabulary into the engine.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.
