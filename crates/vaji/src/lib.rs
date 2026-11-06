//! # vaji
//!
//! The umbrella crate: it re-exports the whole engine under one name, and hosts
//! the `vaji` command line.
//!
//! Prefer depending on the specific crate you need (`vaji-dom`, `vaji-style`,
//! ...) so your build graph stays honest about what it uses. Reach for `vaji`
//! when you want the whole thing and don't want to track the split.

#![warn(missing_docs)]

pub mod cli;

pub use vaji_dom as dom;
pub use vaji_engine as engine;
pub use vaji_events as events;
pub use vaji_html as html;
pub use vaji_layout as layout;
pub use vaji_motion as motion;
pub use vaji_net as net;
pub use vaji_paint as paint;
pub use vaji_shell as shell;
pub use vaji_style as style;
pub use vaji_text as text;

#[cfg(feature = "headless")]
pub use vaji_headless as headless;

/// The version of the Vaji engine, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The usual way in: the engine, as a set of modules.
pub mod prelude {
    pub use crate::{dom, engine, events, html, layout, motion, net, paint, style, text};
}
