//! The arena-backed tree.
//!
//! Storage is an index-based arena rather than a graph of `Rc<RefCell<_>>`.
//! That choice keeps the crate free of `unsafe`, makes `Send`-ing a tree for
//! off-thread work a matter of the payload type rather than a hand-written
//! `Sync` promise, and gives every layer a cheap, stable, `Copy` handle.

use core::fmt;

/// A stable handle to a node inside a [`Tree`].
///
/// Ids are dense indices and are **never reused**: once a node is inserted, its
/// id means that node for the life of the tree.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

impl NodeId {
    /// The first node inserted into a fresh tree.
    pub const ROOT: NodeId = NodeId(0);

    /// The underlying index, for use as a slab key or in debug output.
    #[inline]
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NodeId({})", self.0)
    }
}

/// A single node: a payload plus its structural neighbours.
#[derive(Debug)]
pub struct Node<T> {
    /// The parent, or `None` for a node that was inserted as a root.
    pub parent: Option<NodeId>,
    /// Children, in document order.
    pub children: Vec<NodeId>,
    /// The caller's payload (an element, a text run, ...).
    pub data: T,
}

/// A flat, index-based tree of `T`.
#[derive(Debug)]
pub struct Tree<T> {
    nodes: Vec<Option<Node<T>>>,
}

impl<T> Default for Tree<T> {
    fn default() -> Self {
        Self { nodes: Vec::new() }
    }
}

impl<T> Tree<T> {
    /// Create an empty tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert `data` as a child of `parent`, or as a root when `parent` is
    /// `None`, and return its handle.
    pub fn insert(&mut self, parent: Option<NodeId>, data: T) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Some(Node {
            parent,
            children: Vec::new(),
            data,
        }));
        if let Some(p) = parent {
            self.nodes[p.index()]
                .as_mut()
                .expect("insert: parent id is out of range")
                .children
                .push(id);
        }
        id
    }

    /// Borrow the node behind `id`, if any.
    pub fn get(&self, id: NodeId) -> Option<&Node<T>> {
        self.nodes.get(id.index()).and_then(Option::as_ref)
    }

    /// Mutably borrow the node behind `id`, if any.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node<T>> {
        self.nodes.get_mut(id.index()).and_then(Option::as_mut)
    }

    /// The number of live nodes.
    pub fn len(&self) -> usize {
        self.nodes.iter().filter(|n| n.is_some()).count()
    }

    /// Whether the tree holds no nodes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Detach `id` from its parent, if it has one.
    ///
    /// The node itself remains in the arena; only its parent's child list is
    /// updated. Removal-and-reuse is deliberately not offered, so an id can
    /// never come to mean a different node.
    pub(crate) fn detach_from_parent(&mut self, id: NodeId) {
        let Some(parent) = self.get(id).and_then(|node| node.parent) else {
            return;
        };
        let siblings = &mut self.nodes[parent.index()]
            .as_mut()
            .expect("detach: parent id is out of range")
            .children;
        siblings.retain(|&child| child != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_links_parent_and_child() {
        let mut tree = Tree::new();
        let root = tree.insert(None, "root");
        let a = tree.insert(Some(root), "a");
        let b = tree.insert(Some(root), "b");

        assert_eq!(root, NodeId::ROOT);
        assert_eq!(tree.get(root).unwrap().children, vec![a, b]);
        assert_eq!(tree.get(a).unwrap().parent, Some(root));
        assert_eq!(tree.get(root).unwrap().parent, None);
        assert_eq!(tree.len(), 3);
        assert!(!tree.is_empty());
    }

    #[test]
    fn empty_tree_reports_empty() {
        let tree: Tree<()> = Tree::new();
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
    }

    #[test]
    fn ids_are_never_reused() {
        let mut tree = Tree::new();
        let first = tree.insert(None, 1);
        let second = tree.insert(None, 2);
        assert_ne!(first, second);
    }
}
