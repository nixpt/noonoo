//! The arena-backed tree.
//!
//! Storage is an index-based arena rather than a graph of `Rc<RefCell<_>>`.
//! That choice keeps the crate free of `unsafe`, makes `Send`-ing a tree for
//! off-thread work a matter of the payload type rather than a hand-written
//! `Sync` promise, and gives every layer a cheap, stable, `Copy` handle.
//!
//! The `Tree` holds structure only. The checked, public mutation API lives in
//! [`crate::mutate`]; the primitives here are `pub(crate)` and assume their
//! arguments have already been validated.

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

    /// Build an id from a raw index.
    #[cfg(test)]
    #[inline]
    pub(crate) const fn from_index(index: u32) -> Self {
        Self(index)
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
    /// The parent, or `None` for a detached node.
    pub parent: Option<NodeId>,
    /// Children, in document order.
    pub children: Vec<NodeId>,
    /// The caller's payload.
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

    /// Insert `data` (detached) and return its handle.
    pub fn insert_raw(&mut self, data: T) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Some(Node {
            parent: None,
            children: Vec::new(),
            data,
        }));
        id
    }

    fn slot(&self, id: NodeId) -> Option<&Node<T>> {
        self.nodes.get(id.index()).and_then(Option::as_ref)
    }

    fn slot_mut(&mut self, id: NodeId) -> Option<&mut Node<T>> {
        self.nodes.get_mut(id.index()).and_then(Option::as_mut)
    }

    /// Borrow the node behind `id`, if any.
    pub fn get(&self, id: NodeId) -> Option<&Node<T>> {
        self.slot(id)
    }

    /// Mutably borrow the node behind `id`, if any.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node<T>> {
        self.slot_mut(id)
    }

    /// Whether `id` is in this tree.
    pub fn contains(&self, id: NodeId) -> bool {
        self.slot(id).is_some()
    }

    /// The children of `id` (`&[]` if `id` is absent).
    pub fn children(&self, id: NodeId) -> &[NodeId] {
        match self.slot(id) {
            Some(node) => &node.children,
            None => &[],
        }
    }

    /// The parent of `id`, if any.
    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.slot(id).and_then(|node| node.parent)
    }

    /// Ancestors of `id`, nearest first.
    pub fn ancestors(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cursor = self.parent(id);
        while let Some(parent) = cursor {
            out.push(parent);
            cursor = self.parent(parent);
        }
        out
    }

    /// Descendants of `id` in document (pre-order), `id` excluded.
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self.children(id).iter().rev().copied().collect();
        while let Some(node) = stack.pop() {
            out.push(node);
            for &child in self.children(node).iter().rev() {
                stack.push(child);
            }
        }
        out
    }

    /// Whether `ancestor` is `node` or above it.
    pub fn is_ancestor_of(&self, ancestor: NodeId, node: NodeId) -> bool {
        ancestor == node || self.ancestors(node).contains(&ancestor)
    }

    /// The number of live nodes.
    pub fn len(&self) -> usize {
        self.nodes.iter().filter(|n| n.is_some()).count()
    }

    /// Whether the tree holds no nodes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    // --- structural primitives (validated by the caller) -------------------

    pub(crate) fn push_child(&mut self, parent: NodeId, child: NodeId) {
        if let Some(p) = self.slot_mut(parent) {
            p.children.push(child);
        }
        if let Some(c) = self.slot_mut(child) {
            c.parent = Some(parent);
        }
    }

    pub(crate) fn insert_child(&mut self, parent: NodeId, index: usize, child: NodeId) {
        if let Some(p) = self.slot_mut(parent) {
            let index = index.min(p.children.len());
            p.children.insert(index, child);
        }
        if let Some(c) = self.slot_mut(child) {
            c.parent = Some(parent);
        }
    }

    /// Detach `id` from its parent, clearing both sides of the link.
    pub(crate) fn detach(&mut self, id: NodeId) {
        if let Some(parent) = self.parent(id)
            && let Some(p) = self.slot_mut(parent)
        {
            p.children.retain(|&child| child != id);
        }
        if let Some(c) = self.slot_mut(id) {
            c.parent = None;
        }
    }

    /// Detach every child of `parent`, returning them in document order.
    pub(crate) fn take_children(&mut self, parent: NodeId) -> Vec<NodeId> {
        let removed = self.children(parent).to_vec();
        for &child in &removed {
            if let Some(c) = self.slot_mut(child) {
                c.parent = None;
            }
        }
        if let Some(p) = self.slot_mut(parent) {
            p.children.clear();
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_traverse() {
        let mut tree = Tree::new();
        let root = tree.insert_raw("root");
        let a = tree.insert_raw("a");
        let b = tree.insert_raw("b");
        let leaf = tree.insert_raw("leaf");
        tree.push_child(root, a);
        tree.push_child(root, b);
        tree.push_child(a, leaf);

        assert_eq!(tree.children(root), &[a, b]);
        assert_eq!(tree.descendants(root), vec![a, leaf, b]);
        assert_eq!(tree.ancestors(leaf), vec![a, root]);
        assert!(tree.is_ancestor_of(root, leaf));
        assert!(!tree.is_ancestor_of(b, leaf));
    }

    #[test]
    fn detach_clears_both_sides() {
        let mut tree = Tree::new();
        let root = tree.insert_raw("root");
        let child = tree.insert_raw("child");
        tree.push_child(root, child);
        tree.detach(child);
        assert!(tree.children(root).is_empty());
        assert_eq!(tree.parent(child), None);
    }

    #[test]
    fn ids_are_never_reused() {
        let mut tree = Tree::new();
        let first = tree.insert_raw(1);
        let second = tree.insert_raw(2);
        assert_ne!(first, second);
    }
}
