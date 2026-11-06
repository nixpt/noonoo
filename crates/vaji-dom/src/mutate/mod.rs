//! Checked mutation.
//!
//! The replaced engine's mutator quietly no-oped on type mismatches, which
//! forced one consumer to delete its own error enum as dead code and another to
//! reconstruct `textContent` by hand because `set_node_text` silently did
//! nothing on elements. Vaji inverts that: a mutation that cannot be honoured
//! says so.

use crate::tree::{NodeId, Tree};

/// Why a mutation was rejected.
#[derive(Debug, PartialEq, Eq)]
pub enum MutError {
    /// The target id is not in this tree.
    NoSuchNode(NodeId),
    /// The operation does not apply to the node's kind.
    WrongNodeKind(NodeId),
}

/// A checked writer over a [`Tree`].
///
/// Construct one from a tree (`tree.mutate()`), call the operations, and check
/// the results. Every operation is total: it either changes the tree or returns
/// an error, never neither.
pub struct DocumentMutator<'a, T> {
    tree: &'a mut Tree<T>,
}

impl<T> Tree<T> {
    /// Open a checked mutation session over this tree.
    pub fn mutate(&mut self) -> DocumentMutator<'_, T> {
        DocumentMutator { tree: self }
    }
}

impl<'a, T> DocumentMutator<'a, T> {
    /// Borrow the underlying tree, read-only.
    pub fn tree(&self) -> &Tree<T> {
        self.tree
    }

    /// Detach `id` from its parent.
    ///
    /// Returns [`MutError::NoSuchNode`] if `id` is not in the tree. A node with
    /// no parent is already detached, which is not an error.
    pub fn detach(&mut self, id: NodeId) -> Result<(), MutError> {
        if self.tree.get(id).is_none() {
            return Err(MutError::NoSuchNode(id));
        }
        self.tree.detach_from_parent(id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detach_removes_from_parent() {
        let mut tree = Tree::new();
        let root = tree.insert(None, "root");
        let child = tree.insert(Some(root), "child");

        tree.mutate().detach(child).unwrap();
        assert!(tree.get(root).unwrap().children.is_empty());
        assert_eq!(tree.get(child).unwrap().parent, Some(root)); // node survives
    }

    #[test]
    fn detach_reports_missing_node() {
        let mut tree: Tree<&str> = Tree::new();
        let root = tree.insert(None, "root");
        let bogus = NodeId::ROOT;
        tree.mutate().detach(root).unwrap();
        assert_eq!(tree.mutate().detach(NodeId::ROOT), Ok(()));
        // A fresh tree has no ROOT at all.
        let mut empty: Tree<&str> = Tree::new();
        assert_eq!(
            empty.mutate().detach(bogus),
            Err(MutError::NoSuchNode(bogus))
        );
    }
}
