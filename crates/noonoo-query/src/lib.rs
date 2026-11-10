//! DOM queries with full CSS selectors.
//!
//! `noonoo-dom` stays a leaf crate, so its own `Document::query_selector`
//! only matches simple compounds (`tag`, `#id`, `.class`). This crate runs
//! the full [`noonoo_selectors`] engine over a [`Document`]: combinators,
//! attribute operators, structural and logical pseudo-classes, and
//! selector lists.
//!
//! The scoped queries ([`query_selector_in`], [`query_selector_all_in`])
//! follow the DOM's `element.querySelector` rule. They return descendants of
//! the root, never the root itself, but the selector matches against the
//! whole document, so `body .card` finds a `.card` under the root even
//! though `body` is above it. Results are always in document order, whatever
//! order a selector list names its parts in.
//!
//! [`ElementRef`] is the read-only element view the matcher walks. The style
//! cascade uses the same view.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use noonoo_dom::{Document, NodeId};
pub use noonoo_parse::ParseError;
use noonoo_selectors::{Element, MatchContext, SelectorList};

/// One element of a [`Document`], viewed for selector matching.
///
/// Sibling helpers count element siblings only, as selectors require. The
/// parent of a top-level element is `None`: the document node is not an
/// element, which is what makes `:root` match `<html>`.
#[derive(Clone, Copy)]
pub struct ElementRef<'a> {
    doc: &'a Document,
    id: NodeId,
}

impl<'a> ElementRef<'a> {
    /// A view of `id`, if it is an element of `doc`.
    pub fn new(doc: &'a Document, id: NodeId) -> Option<Self> {
        doc.element(id).is_some().then_some(Self { doc, id })
    }

    /// The node this view is of.
    pub fn id(&self) -> NodeId {
        self.id
    }

    fn element_children(&self, parent: NodeId) -> impl Iterator<Item = NodeId> + 'a {
        let doc = self.doc;
        doc.tree()
            .children(parent)
            .iter()
            .copied()
            .filter(move |&child| doc.element(child).is_some())
    }
}

impl Element for ElementRef<'_> {
    fn tag_name(&self) -> String {
        self.doc
            .element(self.id)
            .map(|element| element.name.clone())
            .unwrap_or_default()
    }
    fn id(&self) -> Option<String> {
        self.doc
            .element(self.id)
            .and_then(|element| element.id().map(str::to_owned))
    }
    fn classes(&self) -> Vec<String> {
        self.doc
            .element(self.id)
            .map(|element| element.classes().map(str::to_owned).collect())
            .unwrap_or_default()
    }
    fn attr(&self, name: &str) -> Option<String> {
        self.doc
            .element(self.id)
            .and_then(|element| element.attr(name).map(str::to_owned))
    }
    fn is_html(&self) -> bool {
        self.doc
            .element(self.id)
            .is_none_or(|element| element.namespace == noonoo_dom::Namespace::Html)
    }
    fn parent(&self) -> Option<Self> {
        ElementRef::new(self.doc, self.doc.tree().parent(self.id)?)
    }
    fn previous_siblings(&self) -> Vec<Self> {
        let Some(parent) = self.doc.tree().parent(self.id) else {
            return Vec::new();
        };
        self.element_children(parent)
            .take_while(|&child| child != self.id)
            .map(|id| ElementRef { doc: self.doc, id })
            .collect()
    }
    fn sibling_index(&self) -> usize {
        let Some(parent) = self.doc.tree().parent(self.id) else {
            return 0;
        };
        self.element_children(parent)
            .position(|child| child == self.id)
            .unwrap_or(0)
    }
    fn sibling_count(&self) -> usize {
        let Some(parent) = self.doc.tree().parent(self.id) else {
            return 1;
        };
        self.element_children(parent).count()
    }
}

fn parse(selector: &str) -> Result<SelectorList, ParseError> {
    SelectorList::parse(selector)
}

fn is_match(doc: &Document, node: NodeId, selectors: &SelectorList) -> bool {
    ElementRef::new(doc, node)
        .is_some_and(|element| selectors.matches(&element, MatchContext::default()))
}

/// The elements under `root` matching `selectors`, in document order.
fn matching_under<'a>(
    doc: &'a Document,
    root: NodeId,
    selectors: &'a SelectorList,
) -> impl Iterator<Item = NodeId> + 'a {
    doc.tree()
        .descendants(root)
        .into_iter()
        .filter(move |&node| is_match(doc, node, selectors))
}

/// Whether `node` is an element matching `selector`.
pub fn matches(doc: &Document, node: NodeId, selector: &str) -> Result<bool, ParseError> {
    Ok(is_match(doc, node, &parse(selector)?))
}

/// The first element in the document matching `selector`.
pub fn query_selector(doc: &Document, selector: &str) -> Result<Option<NodeId>, ParseError> {
    query_selector_in(doc, doc.root(), selector)
}

/// Every element in the document matching `selector`, in document order.
pub fn query_selector_all(doc: &Document, selector: &str) -> Result<Vec<NodeId>, ParseError> {
    query_selector_all_in(doc, doc.root(), selector)
}

/// The first descendant of `root` matching `selector`: `element.querySelector`.
///
/// `root` itself is never a result. The selector matches against the whole
/// document, not just the subtree.
pub fn query_selector_in(
    doc: &Document,
    root: NodeId,
    selector: &str,
) -> Result<Option<NodeId>, ParseError> {
    let selectors = parse(selector)?;
    Ok(matching_under(doc, root, &selectors).next())
}

/// Every descendant of `root` matching `selector`, in document order:
/// `element.querySelectorAll`.
///
/// `root` itself is never a result. The selector matches against the whole
/// document, not just the subtree.
pub fn query_selector_all_in(
    doc: &Document,
    root: NodeId,
    selector: &str,
) -> Result<Vec<NodeId>, ParseError> {
    let selectors = parse(selector)?;
    Ok(matching_under(doc, root, &selectors).collect())
}
