//! # noonoo-a11y
//!
//! Noonoo's **accessibility tree**: the DOM, computed style, and layout
//! projected into a model assistive technology can consume.
//!
//! This crate owns the *tree* — roles, accessible names and descriptions,
//! values, states, heading levels, and bounds. It deliberately owns no protocol
//! and no platform code: the bridge to AccessKit/AT-SPI/UiKit/WinRT is a
//! separate, opt-in integration (`integrations/noonoo-accesskit`), so the
//! engine's default build and lockfile stay free of the borrowed stack.
//!
//! The tree is a projection, not a parallel document: every node points back at
//! its [`noonoo_dom::NodeId`], and the build never mutates the DOM.
//!
//! ## What is exposed
//!
//! - **Roles** from HTML semantics and explicit `role=` ([`Role`]).
//! - **Names** from `aria-labelledby`, `aria-label`, host-language labels and
//!   `alt`, content for name-from-content roles, then `title`.
//! - **Descriptions** from `aria-describedby`, `title`, and `placeholder`.
//! - **States**: disabled, focused, selected, required, read-only, busy,
//!   expanded, and checked (including `mixed`).
//! - **Bounds** from the layout tree, in page coordinates.
//!
//! ## What is hidden
//!
//! `display: none`, `[hidden]`, `aria-hidden="true"`, and non-rendered tags
//! (`<head>`, `<script>`, `<style>`, ...) are absent with their subtrees.
//! `role="none"`/`"presentation"`, `visibility: hidden`, and `<img alt="">`
//! are *presentational*: they contribute no node of their own, but their
//! children are promoted, matching how a browser exposes a `visibility:
//! visible` child inside a hidden parent.
//!
//! ## Composing documents
//!
//! A host window is several documents; [`compose`] joins their trees under one
//! [`Role::Window`] root, offsets their bounds, and renumbers the nodes so ids
//! stay unique across the window.
//!
//! ```no_run
//! # let doc = noonoo_dom::Document::new();
//! # let styles = noonoo_style::Styles::default();
//! let tree = noonoo_a11y::build(&doc, &styles);
//! assert_eq!(tree.root.role, noonoo_a11y::Role::Document);
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod build;
mod node;
mod role;

pub use build::{build, build_in};
pub use node::{A11yNode, A11yTree, Checked, States};
pub use role::{Role, is_excluded_tag, is_presentational, role_for};

use noonoo_geometry::Point;

/// Compose several document trees into one window tree.
///
/// Each layer's bounds are translated by its offset (typically the layer's
/// rectangle minus its scroll), and every node is renumbered from the root so
/// identifiers are unique across the window. A focused node in any layer
/// becomes the window's focus.
pub fn compose(name: &str, layers: &[(&A11yTree, Point)]) -> A11yTree {
    let mut root = A11yNode {
        id: 1,
        node: None,
        role: Role::Window,
        name: name.to_owned(),
        description: String::new(),
        value: None,
        level: None,
        states: States::default(),
        bounds: None,
        children: Vec::new(),
    };

    let mut next_id = 2;
    let mut focus = None;
    for (tree, offset) in layers {
        let mut child = tree.root.clone();
        child.translate(offset.x, offset.y);
        child.renumber(&mut next_id);
        if let Some(id) = child.focused() {
            focus = Some(id);
        }
        root.children.push(child);
    }

    A11yTree { root, focus }
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
