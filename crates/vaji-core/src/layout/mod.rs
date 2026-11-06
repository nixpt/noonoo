//! Layout: computed style into boxes and geometry.
//!
//! ## Borrow boundary
//!
//! This module is the **only** place that may name `taffy` (box layout) and
//! `parley` (text shaping and line breaking). It reads computed style from
//! [`crate::style`] and produces a positioned box tree plus laid-out text.
//!
//! Text is a first-class part of layout here, not a paint-time afterthought:
//! line breaking determines the box it lives in. The bridge between the style
//! and layout engines is small on purpose — one place to read, one place to
//! change.
//!
//! Wired in a later slice. See `docs/ROADMAP.md`.
