//! # noonoo-dom
//!
//! The document tree and the vocabulary for changing it. This is the crate
//! every other layer names when it refers to "a node": identity and the node
//! model are defined here and nowhere else.
//!
//! ## What is here
//!
//! - [`node`] — the payload model: [`node::NodeData`], [`node::ElementData`],
//!   [`node::Attribute`].
//! - [`tree`] — the arena, [`tree::NodeId`], and traversal.
//! - [`document`] — [`document::Document`]: a tree with a root, node creation,
//!   and queries.
//! - [`mutate`] — the checked write API, [`mutate::DocumentMutator`].
//! - [`query`] — [`query::Selector`] and the query surface.
//!
//! ## Why it is its own crate
//!
//! In the engine Noonoo replaces, the DOM, style, layout, text, events, parsing
//! and networking all lived in one ~19k-LOC crate, and every consumer — even
//! one that only wanted to *read* a tree — pulled the whole thing. Here the tree
//! is a leaf with no dependencies.
//!
//! ## Invariants
//!
//! - **No `unsafe`.** A `Vec`-backed arena; parallel work shares data by value,
//!   never through a hand-written `Sync` promise.
//! - **Ids are never reused.** A [`tree::NodeId`] means the same node for the
//!   life of the tree, so a handle held across a mutation cannot silently alias
//!   another node.
//! - **Mutation is checked.** [`mutate::DocumentMutator`] returns results and
//!   never silently no-ops.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod document;
pub mod mutate;
pub mod node;
pub mod query;
pub mod tree;

pub use document::Document;
pub use mutate::{DocumentMutator, MutError};
pub use node::{Attribute, ElementData, NodeData};
pub use query::{Selector, SelectorError};
pub use tree::{Node, NodeId, Tree};

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
