//! # vaji-engine
//!
//! Orchestration: the crate that gives you a *working* page — the equivalent of
//! the replaced engine's `BaseDocument` / `HtmlDocument`, but assembled from the
//! small crates instead of a monolith.
//!
//! An [`Engine`] owns a [`vaji_dom::Document`] and (once the `resolve` feature
//! is on) the style and layout caches that keep it presentable. Everything a
//! host needs to drive a page — style, layout, events, loading, sub-documents —
//! is reached through this type, and each of those abilities is a different
//! crate underneath.
//!
//! ## The `Send` question
//!
//! The replaced engine's `Document` was `!Send`, which blocked running a page
//! off the UI thread and forced consumers into a texture-overlay workaround for
//! sub-documents. Vaji's tree owns no `Rc`/`RefCell` and promises nothing
//! unsafe, so an engine is `Send` whenever its payload is.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use vaji_dom::Document;

/// A page: a DOM document plus the machinery that resolves it.
pub struct Engine {
    dom: Document,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    /// Create an empty engine with a fresh document.
    pub fn new() -> Self {
        Self {
            dom: Document::new(),
        }
    }

    /// Borrow the underlying DOM document.
    pub fn dom(&self) -> &Document {
        &self.dom
    }

    /// Mutably borrow the underlying DOM document.
    pub fn dom_mut(&mut self) -> &mut Document {
        &mut self.dom
    }

    /// Consume the engine and return its document.
    pub fn into_dom(self) -> Document {
        self.dom
    }
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_engine_has_a_document_root() {
        let engine = Engine::new();
        let root = engine.dom().root();
        assert!(engine.dom().tree().contains(root));
    }
}
