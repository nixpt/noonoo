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

pub use noonoo_caison as caison;
pub use noonoo_css as css;
pub use noonoo_dom as dom;
pub use noonoo_engine as engine;
pub use noonoo_events as events;
pub use noonoo_headless as headless;
pub use noonoo_html as html;
pub use noonoo_json as json;
pub use noonoo_layout as layout;
pub use noonoo_motion as motion;
pub use noonoo_net as net;
pub use noonoo_paint as paint;
pub use noonoo_parse as parse;
pub use noonoo_shell as shell;
pub use noonoo_style as style;
pub use noonoo_text as text;

/// The version of the Noonoo engine, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The usual way in: the engine, as a set of modules.
pub mod prelude {
    pub use crate::{
        caison, css, dom, engine, events, headless, html, json, layout, motion, net, paint, parse,
        style, text,
    };
}
