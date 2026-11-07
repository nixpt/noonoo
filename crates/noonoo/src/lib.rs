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

pub use noonoo_a11y as a11y;
pub use noonoo_caison as caison;
pub use noonoo_caps as caps;
pub use noonoo_css as css;
pub use noonoo_csv as csv;
pub use noonoo_dom as dom;
pub use noonoo_engine as engine;
pub use noonoo_events as events;
pub use noonoo_geometry as geometry;
pub use noonoo_headless as headless;
pub use noonoo_html as html;
pub use noonoo_hx as hx;
pub use noonoo_ini as ini;
pub use noonoo_json as json;
pub use noonoo_layout as layout;
pub use noonoo_markdown as markdown;
pub use noonoo_motion as motion;
pub use noonoo_net as net;
pub use noonoo_paint as paint;
pub use noonoo_parse as parse;
pub use noonoo_selectors as selectors;
pub use noonoo_shell as shell;
pub use noonoo_style as style;
pub use noonoo_svg as svg;
pub use noonoo_text as text;
pub use noonoo_toml as toml;
pub use noonoo_ttf as ttf;
pub use noonoo_xml as xml;

/// The version of the Noonoo engine, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The usual way in: the engine, as a set of modules.
pub mod prelude {
    pub use crate::{
        a11y, caison, caps, css, csv, dom, engine, events, geometry, headless, html, hx, ini, json,
        layout, markdown, motion, net, paint, parse, selectors, style, svg, text, toml, ttf, xml,
    };
}
