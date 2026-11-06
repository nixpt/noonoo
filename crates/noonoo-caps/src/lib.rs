//! # noonoo-caps
//!
//! Capability gating for the DOM: *who* may read or write *which* part of a
//! tree, and the seam that decides it.
//!
//! This is the layer the original Bliss engine promised and never built —
//! "DOM segments are cryptographically gated, ensuring agents only see or
//! mutate what they are authorized to." Noonoo splits that promise in two:
//!
//! - **This crate owns the policy and the seam.** [`Scope`], [`Grant`],
//!   [`Action`], the [`Authorizer`] trait, and a [`Gate`] that maps nodes to
//!   the scope they require. It is dependency-free and complete on its own: a
//!   host can gate a document today with an in-memory [`Policy`].
//! - **The cryptography is borrowed.** Verifying a signed capability token is
//!   the job of an [`Authorizer`] implementation backed by an identity system
//!   (the Exosphere Identity Protocol, via the opt-in `noonoo-xip` crate). This
//!   crate never sees a key or a signature.
//!
//! ## The model
//!
//! A [`Scope`] names a *capability* and the *resource* it ranges over; a
//! [`Grant`] says a [`Subject`] may perform some [`Action`]s within a scope. A
//! [`Gate`] attaches a required scope to a node, and — because a guard covers
//! the node's whole subtree — the **nearest guard at or above a node** is the
//! one that applies. [`GatedDocument`] then refuses reads (and, through
//! [`GatedMutator`], writes) the subject is not authorized for, and can render
//! a subtree with denied branches elided.
//!
//! ## Default deny
//!
//! Unguarded nodes are open; a guarded node is open only to a subject an
//! [`Authorizer`] explicitly allows. There is no implicit "owner": pass an
//! [`AllowAll`] authorizer if you are deliberately not gating.
//!
//! ```
//! use noonoo_caps::{Action, Gate, GatedDocument, Grant, Policy, Scope, Subject};
//! use noonoo_dom::Document;
//!
//! let mut document = Document::new();
//! let secret = document.create_element("secret");
//! let root = document.root();
//! document.mutate().append_child(root, secret).unwrap();
//!
//! let mut gate = Gate::new();
//! gate.guard(secret, Scope::path("noonoo.dom", "#secret"));
//!
//! let alice = Subject::new("did:exo:alice");
//! let policy = Policy::new().grant(Grant::read(
//!     alice.clone(),
//!     Scope::path("noonoo.dom", "#secret"),
//! ));
//!
//! let anonymous = Subject::anonymous();
//! let denied = GatedDocument::new(&document, &gate, &policy, &anonymous);
//! assert!(!denied.visible(secret));
//!
//! let allowed = GatedDocument::new(&document, &gate, &policy, &alice);
//! assert!(allowed.can(Action::Read, secret));
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod gate;
mod policy;
mod scope;

pub use gate::{CapsError, Gate, GatedDocument, GatedMutator};
pub use policy::{AllowAll, Authorizer, Decision, Denied, DenyAll, Policy};
pub use scope::{Action, Actions, Grant, Resource, Scope, Subject};

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
