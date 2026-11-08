//! The document: a tree with a root, plus node creation and queries.
//!
//! A [`Document`] is the smallest thing a host can hand around and call a
//! "page tree". It owns the arena and the root id; everything structural goes
//! through [`Document::mutate`], everything read-only through the query methods.

use crate::mutate::{DocumentMutator, MutError};
use crate::node::{ElementData, NodeData};
use crate::query::{Selector, SelectorError};
use crate::tree::{NodeId, Tree};

/// A DOM document: a tree plus its root node.
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
    /// Create a document with a single root node of kind
    /// [`NodeData::Document`].
    pub fn new() -> Self {
        let mut tree = Tree::new();
        let root = tree.insert_raw(NodeData::Document);
        Self { tree, root }
    }

    /// The root node.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Whether `id` is in the document: the root, or a node under it. A
    /// created or removed node is not connected, but stays writable.
    pub fn is_connected(&self, id: NodeId) -> bool {
        self.tree.get(id).is_some() && self.tree.is_ancestor_of(self.root, id)
    }

    /// Borrow the tree.
    pub fn tree(&self) -> &Tree<NodeData> {
        &self.tree
    }

    /// Mutably borrow the tree.
    pub fn tree_mut(&mut self) -> &mut Tree<NodeData> {
        &mut self.tree
    }

    /// Open a checked mutation session.
    pub fn mutate(&mut self) -> DocumentMutator<'_> {
        DocumentMutator::new(&mut self.tree)
    }

    /// This node's payload, if `id` is present.
    pub fn node(&self, id: NodeId) -> Option<&NodeData> {
        self.tree.get(id).map(|node| &node.data)
    }

    /// This node's element data, if it is an element.
    pub fn element(&self, id: NodeId) -> Option<&ElementData> {
        self.node(id).and_then(NodeData::as_element)
    }

    /// Create a detached element with the given tag name.
    pub fn create_element(&mut self, name: impl Into<String>) -> NodeId {
        self.tree
            .insert_raw(NodeData::Element(ElementData::new(name)))
    }

    /// Create a detached element with the given tag name and attributes, in
    /// order. Names are lowercased; a repeated name keeps its first position
    /// and its last value.
    pub fn create_element_with<N, V>(
        &mut self,
        name: impl Into<String>,
        attributes: impl IntoIterator<Item = (N, V)>,
    ) -> NodeId
    where
        N: AsRef<str>,
        V: Into<String>,
    {
        let mut data = ElementData::new(name);
        for (name, value) in attributes {
            data.set_attr(&name.as_ref().to_ascii_lowercase(), value);
        }
        self.tree.insert_raw(NodeData::Element(data))
    }

    /// A detached copy of `id` (`cloneNode`): just the node, or with `deep`
    /// its whole subtree. The copy shares nothing with the original.
    pub fn clone_node(&mut self, id: NodeId, deep: bool) -> Result<NodeId, MutError> {
        let data = self.node(id).ok_or(MutError::NoSuchNode(id))?.clone();
        let copy = self.tree.insert_raw(data);
        if !deep {
            return Ok(copy);
        }
        // Iterative, so a deep subtree can't overflow the stack.
        let mut pending: Vec<(NodeId, NodeId)> = self
            .tree
            .children(id)
            .iter()
            .rev()
            .map(|&child| (child, copy))
            .collect();
        while let Some((original, parent)) = pending.pop() {
            let data = self.tree.get(original).map(|node| node.data.clone());
            let Some(data) = data else { continue };
            let child = self.tree.insert_raw(data);
            self.tree.push_child(parent, child);
            pending.extend(
                self.tree
                    .children(original)
                    .iter()
                    .rev()
                    .map(|&grandchild| (grandchild, child)),
            );
        }
        Ok(copy)
    }

    /// Create a detached text node.
    pub fn create_text(&mut self, text: impl Into<String>) -> NodeId {
        self.tree.insert_raw(NodeData::Text(text.into()))
    }

    /// Create a detached comment node.
    pub fn create_comment(&mut self, text: impl Into<String>) -> NodeId {
        self.tree.insert_raw(NodeData::Comment(text.into()))
    }

    /// The concatenated text of `id`'s subtree, in document order.
    ///
    /// For a text node this is its text; for an element or the root it walks
    /// the descendants and joins their text.
    pub fn text_content(&self, id: NodeId) -> String {
        let mut out = String::new();
        self.collect_text(id, &mut out);
        out
    }

    fn collect_text(&self, id: NodeId, out: &mut String) {
        match self.node(id) {
            Some(NodeData::Text(text)) => out.push_str(text),
            Some(_) => {
                for &child in self.tree.children(id) {
                    self.collect_text(child, out);
                }
            }
            None => {}
        }
    }

    /// Every element in document order (the root's descendants).
    pub fn elements(&self) -> Vec<NodeId> {
        self.tree
            .descendants(self.root)
            .into_iter()
            .filter(|&id| self.node(id).is_some_and(NodeData::is_element))
            .collect()
    }

    /// The first element carrying the given `id` attribute.
    pub fn get_element_by_id(&self, id: &str) -> Option<NodeId> {
        self.elements()
            .into_iter()
            .find(|&node| self.element(node).and_then(ElementData::id) == Some(id))
    }

    /// All elements whose tag name is `tag` (case-insensitive).
    pub fn get_elements_by_tag_name(&self, tag: &str) -> Vec<NodeId> {
        let tag = tag.to_ascii_lowercase();
        self.elements()
            .into_iter()
            .filter(|&node| {
                self.element(node)
                    .is_some_and(|element| element.name == tag)
            })
            .collect()
    }

    /// The first element matching `selector`, in document order.
    pub fn query_selector(&self, selector: &str) -> Result<Option<NodeId>, SelectorError> {
        let selector = Selector::parse(selector)?;
        Ok(self
            .elements()
            .into_iter()
            .find(|&node| self.element(node).is_some_and(|e| selector.matches(e))))
    }

    /// Every element matching `selector`, in document order.
    pub fn query_selector_all(&self, selector: &str) -> Result<Vec<NodeId>, SelectorError> {
        let selector = Selector::parse(selector)?;
        Ok(self
            .elements()
            .into_iter()
            .filter(|&node| self.element(node).is_some_and(|e| selector.matches(e)))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Document {
        let mut doc = Document::new();
        let root = doc.root();
        let body = doc.create_element("body");
        let list = doc.create_element("ul");
        list_attr(&mut doc, list, "id", "items");
        let item1 = doc.create_element("li");
        let item2 = doc.create_element("li");
        list_attr(&mut doc, item1, "class", "row active");
        let text = doc.create_text("one");
        doc.mutate().append_child(root, body).unwrap();
        doc.mutate().append_child(body, list).unwrap();
        doc.mutate().append_child(list, item1).unwrap();
        doc.mutate().append_child(list, item2).unwrap();
        doc.mutate().append_child(item1, text).unwrap();
        doc
    }

    fn list_attr(doc: &mut Document, node: NodeId, name: &str, value: &str) {
        doc.mutate().set_attribute(node, name, value).unwrap();
    }

    #[test]
    fn get_element_by_id() {
        let doc = sample();
        let list = doc.get_element_by_id("items").unwrap();
        assert_eq!(doc.element(list).unwrap().name, "ul");
        assert!(doc.get_element_by_id("missing").is_none());
    }

    #[test]
    fn text_content_walks_the_subtree() {
        let doc = sample();
        let body = doc.get_elements_by_tag_name("body")[0];
        assert_eq!(doc.text_content(body), "one");
        let list = doc.get_element_by_id("items").unwrap();
        assert_eq!(doc.text_content(list), "one");
    }

    #[test]
    fn tag_and_class_queries() {
        let doc = sample();
        assert_eq!(doc.get_elements_by_tag_name("li").len(), 2);
        assert_eq!(doc.query_selector_all(".row").unwrap().len(), 1);
        assert_eq!(
            doc.query_selector("ul#items").unwrap(),
            doc.get_element_by_id("items")
        );
        assert!(doc.query_selector("li.active").unwrap().is_some());
    }

    #[test]
    fn bad_selector_is_reported() {
        let doc = sample();
        assert!(doc.query_selector("div span").is_err());
    }
}
