//! # vaji-dom
//!
//! The document tree and the vocabulary for changing it. This is the crate
//! every other layer names when it refers to "a node": identity is defined
//! here and nowhere else.
//!
//! ## Why it is its own crate
//!
//! In the engine Vaji replaces, the DOM, the style driver, the layout bridge,
//! text, events, networking and the parser all lived in one ~19k-LOC crate.
//! Every consumer — including ones that only wanted to *read* a tree — pulled
//! the whole thing. Here, the tree is a leaf with no dependencies: you can take
//! it alone, and every layer above it is one boundary thick.
//!
//! ## Invariants
//!
//! - **No `unsafe`.** A `Vec`-backed arena; parallelism shares data by value,
//!   never through a hand-written `Sync` promise (the replaced engine's
//!   `unsafe impl Sync for Node` is exactly what this design removes).
//! - **Ids are never reused.** A [`tree::NodeId`] keeps referring to the same
//!   node for the life of the tree, so a handle held across a mutation cannot
//!   silently alias a different node (the replaced engine's recyclable slab
//!   indices forced every consumer to mint its own stable handles).
//! - **Mutation is checked.** [`mutate::DocumentMutator`] returns results and
//!   never silently no-ops; the replaced engine's mutator did the opposite,
//!   which is why one consumer deleted its own error enum as dead code.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod mutate;
pub mod query;
pub mod tree;

pub use tree::{Node, NodeId, Tree};

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
