//! # vaji-document
//!
//! Orchestration: the crate that gives you a *working* document — the
//! equivalent of the replaced engine's `BaseDocument` / `HtmlDocument`, but
//! assembled from the small crates instead of a monolith.
//!
//! A [`Document`] owns a [`vaji_dom::Tree`] of [`NodeData`], a root, and (once
//! the `resolve` feature is on) the style and layout caches that keep it
//! presentable. Everything a host needs to drive a page — style, layout,
//! events, loading, sub-documents — is reached through this type, and each of
//! those abilities is a different crate underneath.
//!
//! ## The `Send` question
//!
//! `bliss-dom`'s `Document` was `!Send`, which blocked running a page off the
//! UI thread and forced consumers into a texture-overlay workaround for
//! sub-documents. Vaji's tree owns no `Rc`/`RefCell` and promises nothing
//! unsafe, so a document is `Send` whenever its payload is — the property that
//! makes worker-owned sub-documents (the "sub-document" capability consumers
//! asked for) a design in reach rather than a rewrite.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use vaji_dom::{NodeId, Tree};

/// The payload of a node in a document tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeData {
    /// The document root.
    Document,
    /// An element, by its (lowercase) tag name.
    Element {
        /// The element's local name.
        name: String,
    },
    /// A text run.
    Text(String),
    /// A comment.
    Comment(String),
}

/// A parsed document: a tree plus its root.
pub struct Document {
    tree: Tree<NodeData>,
    root: NodeId,
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    /// Create an empty document with a single root node.
    pub fn new() -> Self {
        let mut tree = Tree::new();
        let root = tree.insert(None, NodeData::Document);
        Self { tree, root }
    }

    /// The document root.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Borrow the tree.
    pub fn tree(&self) -> &Tree<NodeData> {
        &self.tree
    }

    /// Mutably borrow the tree.
    pub fn tree_mut(&mut self) -> &mut Tree<NodeData> {
        &mut self.tree
    }
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_document_has_a_root() {
        let doc = Document::new();
        assert_eq!(doc.root(), NodeId::ROOT);
        assert_eq!(doc.tree().get(doc.root()).unwrap().data, NodeData::Document);
        assert_eq!(doc.tree().len(), 1);
    }
}
