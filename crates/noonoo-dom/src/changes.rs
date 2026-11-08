//! What changed since someone last looked: the log incremental restyle and
//! relayout start from.
//!
//! The checked mutator ([`crate::DocumentMutator`]) records each edit it
//! makes. Writes through [`crate::Document::tree_mut`] can't be seen, so
//! borrowing the raw tree marks the log [overflowed](Changes::overflowed):
//! anything may have changed. So does a log that grows past
//! [`crate::Document::MAX_CHANGES`] entries without being taken — past
//! that, redoing everything is as cheap as going through the list.

use crate::tree::NodeId;

/// One edit, named by the node whose state it changed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Change {
    /// An element's attributes changed.
    Attributes(NodeId),
    /// A node gained, lost or reordered children.
    Children(NodeId),
    /// A text or comment node's text changed.
    Text(NodeId),
}

/// The edits since the log was last taken, in order.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Changes {
    list: Vec<Change>,
    overflowed: bool,
}

impl Changes {
    /// The recorded edits; empty once the log has overflowed.
    pub fn iter(&self) -> impl Iterator<Item = &Change> {
        self.list.iter()
    }

    /// Whether nothing changed.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty() && !self.overflowed
    }

    /// Whether the log lost track: anything may have changed, so treat the
    /// whole document as changed.
    pub fn overflowed(&self) -> bool {
        self.overflowed
    }

    pub(crate) fn record(&mut self, change: Change, limit: usize) {
        if self.overflowed {
            return;
        }
        if self.list.len() >= limit {
            self.overflow();
        } else {
            self.list.push(change);
        }
    }

    pub(crate) fn overflow(&mut self) {
        self.list = Vec::new();
        self.overflowed = true;
    }
}
