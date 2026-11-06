//! # vaji
//!
//! The umbrella crate: it re-exports the whole engine under one name, and
//! hosts the `vaji` command line.
//!
//! Prefer depending on the specific crate you need (`vaji-core`,
//! `vaji-paint`, ...) so your build graph stays honest about what it uses.
//! Reach for `vaji` when you want the whole thing and don't want to track the
//! split.

#![warn(missing_docs)]

pub mod cli;

pub use vaji_core as core;
pub use vaji_net as net;
pub use vaji_paint as paint;
pub use vaji_shell as shell;

/// The version of the Vaji engine, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The usual way in: the engine, as a set of modules.
pub mod prelude {
    pub use crate::core::{dom, events, headless, html, layout, style};
}
