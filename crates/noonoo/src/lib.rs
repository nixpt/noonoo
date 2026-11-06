//! # noonoo
//!
//! The umbrella crate: it re-exports the whole engine under one name, and hosts
//! the `noonoo` command line.
//!
//! Prefer depending on the specific crate you need (`noonoo-dom`, `noonoo-style`,
//! ...) so your build graph stays honest about what it uses. Reach for `noonoo`
//! when you want the whole thing and don't want to track the split.

#![warn(missing_docs)]

pub mod cli;

pub use noonoo_dom as dom;
pub use noonoo_engine as engine;
pub use noonoo_events as events;
pub use noonoo_html as html;
pub use noonoo_layout as layout;
pub use noonoo_motion as motion;
pub use noonoo_net as net;
pub use noonoo_paint as paint;
pub use noonoo_shell as shell;
pub use noonoo_style as style;
pub use noonoo_text as text;

#[cfg(feature = "headless")]
pub use noonoo_headless as headless;

/// The version of the Noonoo engine, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The usual way in: the engine, as a set of modules.
pub mod prelude {
    pub use crate::{dom, engine, events, html, layout, motion, net, paint, style, text};
}
