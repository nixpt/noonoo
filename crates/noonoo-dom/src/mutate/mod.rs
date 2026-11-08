//! Checked mutation.
//!
//! The replaced engine's mutator quietly no-oped on type mismatches, which
//! forced one consumer to delete its own error enum as dead code and another to
//! reconstruct `textContent` by hand because `set_node_text` silently did
//! nothing on elements. Noonoo inverts that: a mutation that cannot be honoured
//! says so, and [`DocumentMutator::replace_children`] *replaces* (it is the
//! primitive a correct `innerHTML` is built on — the old engine's
//! `DomController::set_inner_html` appended instead).

use crate::Document;
use crate::changes::{Change, Changes};
use crate::node::NodeData;
use crate::tree::{NodeId, Tree};

/// Why a mutation was rejected.
#[derive(Debug, PartialEq, Eq)]
pub enum MutError {
    /// The target id is not in this tree.
    NoSuchNode(NodeId),
    /// The operation needs an element and got something else.
    NotAnElement(NodeId),
    /// The target cannot contain children (e.g. a text node).
    CannotHaveChildren(NodeId),
    /// The reference node has no parent to insert against.
    NoParent(NodeId),
    /// The move would create a cycle (a node into its own descendant).
    Cycle(NodeId),
}

/// A checked writer over a document tree.
///
/// Each edit it makes is recorded in the document's change log
/// ([`Document::take_changes`]); a rejected one records nothing.
pub struct DocumentMutator<'a> {
    tree: &'a mut Tree<NodeData>,
    changes: &'a mut Changes,
}

impl<'a> DocumentMutator<'a> {
    pub(crate) fn new(tree: &'a mut Tree<NodeData>, changes: &'a mut Changes) -> Self {
        Self { tree, changes }
    }

    fn record(&mut self, change: Change) {
        self.changes.record(change, Document::MAX_CHANGES);
    }

    /// Detach `child`, recording that its parent (if any) lost it.
    fn detach(&mut self, child: NodeId) {
        if let Some(parent) = self.tree.parent(child) {
            self.record(Change::Children(parent));
        }
        self.tree.detach(child);
    }

    /// Borrow the underlying tree, read-only.
    pub fn tree(&self) -> &Tree<NodeData> {
        self.tree
    }

    /// Append `child` to `parent`, moving it if it already has a parent.
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), MutError> {
        self.require(parent)?;
        self.require(child)?;
        self.require_container(parent)?;
        if self.tree.is_ancestor_of(child, parent) {
            return Err(MutError::Cycle(child));
        }
        self.detach(child);
        self.tree.push_child(parent, child);
        self.record(Change::Children(parent));
        Ok(())
    }

    /// Insert `child` immediately before `sibling`, moving it if necessary.
    pub fn insert_before(&mut self, sibling: NodeId, child: NodeId) -> Result<(), MutError> {
        self.require(sibling)?;
        self.require(child)?;
        let parent = self
            .tree
            .parent(sibling)
            .ok_or(MutError::NoParent(sibling))?;
        if self.tree.is_ancestor_of(child, parent) {
            return Err(MutError::Cycle(child));
        }
        if child == sibling {
            return Ok(());
        }
        // Detach first: moving an earlier sibling shifts `sibling` left.
        self.detach(child);
        let index = self.position(parent, sibling);
        self.tree.insert_child(parent, index, child);
        self.record(Change::Children(parent));
        Ok(())
    }

    /// Insert `child` immediately after `sibling`, moving it if necessary
    /// (`sibling.after(child)`).
    pub fn insert_after(&mut self, sibling: NodeId, child: NodeId) -> Result<(), MutError> {
        self.require(sibling)?;
        self.require(child)?;
        let parent = self
            .tree
            .parent(sibling)
            .ok_or(MutError::NoParent(sibling))?;
        if self.tree.is_ancestor_of(child, parent) {
            return Err(MutError::Cycle(child));
        }
        if child == sibling {
            return Ok(());
        }
        self.detach(child);
        let index = self.position(parent, sibling) + 1;
        self.tree.insert_child(parent, index, child);
        self.record(Change::Children(parent));
        Ok(())
    }

    /// Put `new` where `old` is, detaching `old` (`old.replaceWith(new)`).
    /// `new` moves if it is already in the tree; replacing a node with
    /// itself changes nothing.
    pub fn replace_with(&mut self, old: NodeId, new: NodeId) -> Result<(), MutError> {
        self.require(old)?;
        self.require(new)?;
        let parent = self.tree.parent(old).ok_or(MutError::NoParent(old))?;
        if self.tree.is_ancestor_of(new, parent) {
            return Err(MutError::Cycle(new));
        }
        if old == new {
            return Ok(());
        }
        self.detach(new);
        let index = self.position(parent, old);
        self.tree.detach(old);
        self.tree.insert_child(parent, index, new);
        self.record(Change::Children(parent));
        Ok(())
    }

    /// Where `child` sits among `parent`'s children. The callers have
    /// checked that it is one of them.
    fn position(&self, parent: NodeId, child: NodeId) -> usize {
        self.tree
            .children(parent)
            .iter()
            .position(|&id| id == child)
            .unwrap_or(0)
    }

    /// Detach `child` from its parent. The node itself is kept, and can be
    /// written to and reattached.
    ///
    /// A node with no parent (already removed, never attached, or the root)
    /// is [`MutError::NoParent`]: the remove would have no effect.
    pub fn remove(&mut self, child: NodeId) -> Result<(), MutError> {
        self.require(child)?;
        if self.tree.parent(child).is_none() {
            return Err(MutError::NoParent(child));
        }
        self.detach(child);
        Ok(())
    }

    /// Replace every child of `parent` with `new_children`, in order.
    ///
    /// This is the primitive behind `innerHTML`: it empties first, then fills,
    /// so it can never append-on-top.
    pub fn replace_children(
        &mut self,
        parent: NodeId,
        new_children: &[NodeId],
    ) -> Result<(), MutError> {
        self.require(parent)?;
        self.require_container(parent)?;
        for &child in new_children {
            self.require(child)?;
            if self.tree.is_ancestor_of(child, parent) {
                return Err(MutError::Cycle(child));
            }
        }

        self.tree.take_children(parent);
        for &child in new_children {
            self.detach(child);
            self.tree.push_child(parent, child);
        }
        self.record(Change::Children(parent));
        Ok(())
    }

    /// Set an attribute on an element, replacing it if present.
    pub fn set_attribute(
        &mut self,
        element: NodeId,
        name: &str,
        value: impl Into<String>,
    ) -> Result<(), MutError> {
        let node = self
            .tree
            .get_mut(element)
            .ok_or(MutError::NoSuchNode(element))?;
        let data = node
            .data
            .as_element_mut()
            .ok_or(MutError::NotAnElement(element))?;
        data.set_attr(&name.to_ascii_lowercase(), value);
        self.record(Change::Attributes(element));
        Ok(())
    }

    /// Remove an attribute from an element. Absent attributes are not an error.
    pub fn remove_attribute(&mut self, element: NodeId, name: &str) -> Result<(), MutError> {
        let node = self
            .tree
            .get_mut(element)
            .ok_or(MutError::NoSuchNode(element))?;
        let data = node
            .data
            .as_element_mut()
            .ok_or(MutError::NotAnElement(element))?;
        data.remove_attr(&name.to_ascii_lowercase());
        self.record(Change::Attributes(element));
        Ok(())
    }

    /// Set the text of `node`, with `textContent` semantics.
    ///
    /// On a text or comment node this replaces the string in place. On an
    /// element it replaces the element's children with a single text node —
    /// which is what the old engine's `set_node_text` failed to do.
    pub fn set_text(&mut self, node: NodeId, text: impl Into<String>) -> Result<(), MutError> {
        let text = text.into();
        let is_text_like = match self.tree.get(node) {
            Some(node) => matches!(node.data, NodeData::Text(_) | NodeData::Comment(_)),
            None => return Err(MutError::NoSuchNode(node)),
        };

        if is_text_like {
            match &mut self.tree.get_mut(node).unwrap().data {
                NodeData::Text(s) | NodeData::Comment(s) => *s = text,
                _ => unreachable!("checked above"),
            }
            self.record(Change::Text(node));
            Ok(())
        } else if self
            .tree
            .get(node)
            .is_some_and(|n| n.data.can_have_children())
        {
            let text_node = self.tree.insert_raw(NodeData::Text(text));
            self.replace_children(node, &[text_node])
        } else {
            Err(MutError::CannotHaveChildren(node))
        }
    }

    fn require(&self, id: NodeId) -> Result<(), MutError> {
        if self.tree.contains(id) {
            Ok(())
        } else {
            Err(MutError::NoSuchNode(id))
        }
    }

    fn require_container(&self, id: NodeId) -> Result<(), MutError> {
        match self.tree.get(id) {
            Some(node) if node.data.can_have_children() => Ok(()),
            Some(_) => Err(MutError::CannotHaveChildren(id)),
            None => Err(MutError::NoSuchNode(id)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Document;

    fn doc_with_body() -> (Document, NodeId) {
        let mut doc = Document::new();
        let body = doc.create_element("body");
        let root = doc.root();
        doc.mutate().append_child(root, body).unwrap();
        (doc, body)
    }

    #[test]
    fn append_and_remove() {
        let (mut doc, body) = doc_with_body();
        let p = doc.create_element("p");
        doc.mutate().append_child(body, p).unwrap();
        assert_eq!(doc.tree().children(body), &[p]);
        doc.mutate().remove(p).unwrap();
        assert!(doc.tree().children(body).is_empty());
    }

    #[test]
    fn append_moves_rather_than_duplicates() {
        let (mut doc, body) = doc_with_body();
        let root = doc.root();
        let p = doc.create_element("p");
        doc.mutate().append_child(body, p).unwrap();
        doc.mutate().append_child(root, p).unwrap();
        assert!(doc.tree().children(body).is_empty());
        assert!(doc.tree().children(root).contains(&p));
        assert_eq!(doc.tree().parent(p), Some(root));
    }

    #[test]
    fn cycles_are_rejected() {
        let (mut doc, body) = doc_with_body();
        let child = doc.create_element("span");
        doc.mutate().append_child(body, child).unwrap();
        assert_eq!(
            doc.mutate().append_child(child, body),
            Err(MutError::Cycle(body))
        );
    }

    #[test]
    fn replace_children_empties_first() {
        let (mut doc, body) = doc_with_body();
        let a = doc.create_element("a");
        let b = doc.create_element("b");
        doc.mutate().append_child(body, a).unwrap();
        doc.mutate().replace_children(body, &[b]).unwrap();
        assert_eq!(doc.tree().children(body), &[b]);
    }

    #[test]
    fn set_text_on_element_replaces_children() {
        let (mut doc, body) = doc_with_body();
        let a = doc.create_element("a");
        doc.mutate().append_child(body, a).unwrap();
        doc.mutate().set_text(body, "hello").unwrap();
        let children = doc.tree().children(body).to_vec();
        assert_eq!(children.len(), 1);
        assert_eq!(doc.node(children[0]).unwrap().as_text(), Some("hello"));
    }

    #[test]
    fn attribute_ops_require_an_element() {
        let (mut doc, body) = doc_with_body();
        doc.mutate().set_attribute(body, "class", "x y").unwrap();
        assert!(doc.element(body).unwrap().has_class("y"));

        let text = doc.create_text("hi");
        assert_eq!(
            doc.mutate().set_attribute(text, "id", "nope"),
            Err(MutError::NotAnElement(text))
        );
    }

    #[test]
    fn missing_nodes_are_reported() {
        let mut doc = Document::new();
        let ghost = NodeId::from_index(99);
        assert_eq!(doc.mutate().remove(ghost), Err(MutError::NoSuchNode(ghost)));
    }

    #[test]
    fn removing_a_node_with_no_parent_is_reported() {
        // A second remove has no effect, so it says so instead of `Ok(())`.
        let (mut doc, body) = doc_with_body();
        let p = doc.create_element("p");
        doc.mutate().append_child(body, p).unwrap();
        doc.mutate().remove(p).unwrap();
        assert_eq!(doc.mutate().remove(p), Err(MutError::NoParent(p)));
        let root = doc.root();
        assert_eq!(doc.mutate().remove(root), Err(MutError::NoParent(root)));
    }

    #[test]
    fn a_detached_node_stays_writable_and_reports_disconnected() {
        // Created nodes start detached, and create → set → append is the
        // normal flow, so writes to a detached node are honoured; a caller
        // that only wants to touch the live page checks `is_connected`.
        let (mut doc, body) = doc_with_body();
        let p = doc.create_element("p");
        assert!(!doc.is_connected(p));
        doc.mutate().set_attribute(p, "id", "x").unwrap();
        doc.mutate().append_child(body, p).unwrap();
        assert!(doc.is_connected(p));
        assert!(doc.is_connected(doc.root()));
        doc.mutate().remove(body).unwrap();
        assert!(!doc.is_connected(p), "a removed subtree is disconnected");
        assert!(!doc.is_connected(NodeId::from_index(99)));
    }
}
