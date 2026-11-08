//! The gate and the gated views: attaching scopes to nodes, and refusing the
//! reads and writes a subject is not authorized for.

use core::fmt;
use std::collections::BTreeMap;

use noonoo_css::{InlineStyleError, remove_inline_property, set_inline_property};
use noonoo_dom::{Document, ElementData, MutError, NodeData, NodeId};

use crate::policy::{Authorizer, Decision, Denied};
use crate::scope::{Action, Scope, Subject};

/// A gated access failure: either the subject was refused, or the mutation it
/// *was* allowed to attempt was itself invalid.
#[derive(Debug, PartialEq, Eq)]
pub enum CapsError {
    /// The subject lacks the capability the target required.
    Denied(Denied),
    /// The subject was allowed, but the DOM rejected the change.
    Mutation(MutError),
    /// The subject was allowed, but the style edit was not valid CSS.
    Style(InlineStyleError),
}

impl From<MutError> for CapsError {
    fn from(error: MutError) -> Self {
        CapsError::Mutation(error)
    }
}

impl fmt::Display for CapsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CapsError::Denied(denied) => write!(f, "{denied}"),
            CapsError::Mutation(error) => write!(f, "mutation rejected: {error:?}"),
            CapsError::Style(error) => write!(f, "style edit rejected: {error}"),
        }
    }
}

impl std::error::Error for CapsError {}

/// The map from nodes to the capability they require.
///
/// A guard covers the node **and its whole subtree**; the *nearest* guard at or
/// above a node is the one that applies, so a nested guard can tighten (or, to
/// a differently-scoped subject, loosen) the requirement for its branch.
#[derive(Debug, Clone, Default)]
pub struct Gate {
    guards: BTreeMap<NodeId, Scope>,
}

impl Gate {
    /// An empty gate: every node is open.
    pub fn new() -> Self {
        Self::default()
    }

    /// Require `scope` for `node` and everything beneath it.
    pub fn guard(&mut self, node: NodeId, scope: Scope) -> &mut Self {
        self.guards.insert(node, scope);
        self
    }

    /// The scope required to reach `node`, if a guard covers it.
    pub fn required(&self, document: &Document, node: NodeId) -> Option<&Scope> {
        if let Some(scope) = self.guards.get(&node) {
            return Some(scope);
        }
        document
            .tree()
            .ancestors(node)
            .into_iter()
            .find_map(|ancestor| self.guards.get(&ancestor))
    }

    /// The guards strictly below `node`, in its subtree.
    fn guards_within<'g>(
        &'g self,
        document: &'g Document,
        node: NodeId,
    ) -> impl Iterator<Item = &'g Scope> + 'g {
        self.guards
            .iter()
            .filter(move |&(&guarded, _)| {
                guarded != node && document.tree().is_ancestor_of(node, guarded)
            })
            .map(|(_, scope)| scope)
    }

    /// Whether any node is guarded.
    pub fn is_empty(&self) -> bool {
        self.guards.is_empty()
    }

    /// How many nodes carry a guard.
    pub fn len(&self) -> usize {
        self.guards.len()
    }
}

/// A read-only view of a document that hides what the subject may not read.
pub struct GatedDocument<'a> {
    document: &'a Document,
    gate: &'a Gate,
    authorizer: &'a dyn Authorizer,
    subject: &'a Subject,
}

impl<'a> GatedDocument<'a> {
    /// Gated read access to `document` for `subject`, under `gate` and
    /// `authorizer`.
    pub fn new(
        document: &'a Document,
        gate: &'a Gate,
        authorizer: &'a dyn Authorizer,
        subject: &'a Subject,
    ) -> Self {
        Self {
            document,
            gate,
            authorizer,
            subject,
        }
    }

    /// The underlying document (ungated — the caller is trusted here).
    pub fn document(&self) -> &'a Document {
        self.document
    }

    /// The subject this view speaks for.
    pub fn subject(&self) -> &'a Subject {
        self.subject
    }

    /// The gate in force.
    pub fn gate(&self) -> &'a Gate {
        self.gate
    }

    /// The decision for `action` on `node`.
    pub fn decision(&self, action: Action, node: NodeId) -> Decision {
        match self.gate.required(self.document, node) {
            Some(scope) => self.authorizer.authorize(self.subject, action, scope),
            None => Decision::Allow,
        }
    }

    /// Whether the subject may perform `action` on `node`.
    pub fn can(&self, action: Action, node: NodeId) -> bool {
        self.decision(action, node).is_allowed()
    }

    /// Whether the subject may read `node`.
    pub fn visible(&self, node: NodeId) -> bool {
        self.can(Action::Read, node)
    }

    /// `Err(Denied)` if the subject may not read `node`.
    fn require(&self, action: Action, node: NodeId) -> Result<(), Denied> {
        match self.gate.required(self.document, node) {
            Some(scope)
                if self
                    .authorizer
                    .authorize(self.subject, action, scope)
                    .is_denied() =>
            {
                Err(Denied {
                    subject: self.subject.clone(),
                    action,
                    scope: scope.clone(),
                })
            }
            _ => Ok(()),
        }
    }

    /// The element data for `node`, if it is an element — gated on read.
    pub fn element(&self, node: NodeId) -> Result<Option<&'a ElementData>, Denied> {
        self.require(Action::Read, node)?;
        let document = self.document;
        Ok(document.element(node))
    }

    /// The concatenated text of `node`'s subtree — gated on read of `node`.
    ///
    /// Note this does **not** consult guards *inside* the subtree; use
    /// [`GatedDocument::visible_text`] to render safely for an untrusted reader.
    pub fn text(&self, node: NodeId) -> Result<String, Denied> {
        self.require(Action::Read, node)?;
        Ok(self.document.text_content(node))
    }

    /// The direct children of `node` — gated on read of `node`.
    pub fn children(&self, node: NodeId) -> Result<Vec<NodeId>, Denied> {
        self.require(Action::Read, node)?;
        Ok(self.document.tree().children(node).to_vec())
    }

    /// The subtree text with every denied branch elided.
    ///
    /// This is the safe default for showing a tree to an untrusted reader: it
    /// descends only into nodes the subject may read, and never materializes
    /// text from a guarded branch.
    pub fn visible_text(&self, node: NodeId) -> String {
        let mut out = String::new();
        self.collect_visible_text(node, &mut out);
        out
    }

    fn collect_visible_text(&self, node: NodeId, out: &mut String) {
        if !self.visible(node) {
            return;
        }
        match self.document.node(node) {
            Some(NodeData::Text(text)) => out.push_str(text),
            Some(_) => {
                for &child in self.document.tree().children(node) {
                    self.collect_visible_text(child, out);
                }
            }
            None => {}
        }
    }

    /// The visible descendants of `node` in document order, pruned at any
    /// branch the subject may not read.
    pub fn visible_descendants(&self, node: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack: Vec<NodeId> = self
            .document
            .tree()
            .children(node)
            .iter()
            .rev()
            .copied()
            .collect();
        while let Some(current) = stack.pop() {
            if !self.visible(current) {
                continue;
            }
            out.push(current);
            for &child in self.document.tree().children(current).iter().rev() {
                stack.push(child);
            }
        }
        out
    }
}

/// A write view: a mutation is refused unless the subject may write the node.
///
/// The permission check runs against every node a write changes: the parent
/// for an insertion, the node itself for a removal or attribute change. A
/// moved node is also removed from where it was, so it needs write too, and
/// replacing a node's children removes each of them. Removing a subtree takes
/// every guarded node inside it along, so it needs write on each of those
/// guards as well. A guarded subtree cannot be edited, emptied, moved out of,
/// or removed with an open ancestor by a subject who cannot write its guard.
pub struct GatedMutator<'a> {
    document: &'a mut Document,
    gate: &'a Gate,
    authorizer: &'a dyn Authorizer,
    subject: &'a Subject,
}

impl<'a> GatedMutator<'a> {
    /// Gated write access to `document` for `subject`.
    pub fn new(
        document: &'a mut Document,
        gate: &'a Gate,
        authorizer: &'a dyn Authorizer,
        subject: &'a Subject,
    ) -> Self {
        Self {
            document,
            gate,
            authorizer,
            subject,
        }
    }

    /// The subject this mutator speaks for.
    pub fn subject(&self) -> &'a Subject {
        self.subject
    }

    fn require(&self, node: NodeId) -> Result<(), CapsError> {
        let document: &Document = self.document;
        match self.gate.required(document, node) {
            Some(scope) => self.require_scope(scope),
            None => Ok(()),
        }
    }

    fn require_scope(&self, scope: &Scope) -> Result<(), CapsError> {
        self.require_scope_for(Action::Write, scope)
    }

    fn require_scope_for(&self, action: Action, scope: &Scope) -> Result<(), CapsError> {
        if self
            .authorizer
            .authorize(self.subject, action, scope)
            .is_denied()
        {
            return Err(CapsError::Denied(Denied {
                subject: self.subject.clone(),
                action,
                scope: scope.clone(),
            }));
        }
        Ok(())
    }

    /// Read access to `node` and, for a deep copy, every guard inside it:
    /// what cloning it would reveal.
    fn require_readable(&self, node: NodeId, deep: bool) -> Result<(), CapsError> {
        let document: &Document = self.document;
        if let Some(scope) = self.gate.required(document, node) {
            self.require_scope_for(Action::Read, scope)?;
        }
        if deep {
            for scope in self.gate.guards_within(document, node) {
                self.require_scope_for(Action::Read, scope)?;
            }
        }
        Ok(())
    }

    /// Write access to `node` and to every guard inside it: what removing
    /// the subtree takes out of the document.
    fn require_subtree(&self, node: NodeId) -> Result<(), CapsError> {
        self.require(node)?;
        let document: &Document = self.document;
        for scope in self.gate.guards_within(document, node) {
            self.require_scope(scope)?;
        }
        Ok(())
    }

    /// Set one inline style property (`style.setProperty`), if the subject
    /// may write `element`. An empty value removes the property.
    pub fn set_style_property(
        &mut self,
        element: NodeId,
        name: &str,
        value: &str,
        important: bool,
    ) -> Result<(), CapsError> {
        self.require(element)?;
        let style = self.inline_style(element)?;
        let style =
            set_inline_property(&style, name, value, important).map_err(CapsError::Style)?;
        self.write_style(element, &style)
    }

    /// Remove one inline style property (`style.removeProperty`), if the
    /// subject may write `element`.
    pub fn remove_style_property(&mut self, element: NodeId, name: &str) -> Result<(), CapsError> {
        self.require(element)?;
        let style = self.inline_style(element)?;
        self.write_style(element, &remove_inline_property(&style, name))
    }

    fn inline_style(&self, element: NodeId) -> Result<String, CapsError> {
        let document: &Document = self.document;
        let node = document
            .node(element)
            .ok_or(CapsError::Mutation(MutError::NoSuchNode(element)))?;
        let data = node
            .as_element()
            .ok_or(CapsError::Mutation(MutError::NotAnElement(element)))?;
        Ok(data.attr("style").unwrap_or_default().to_owned())
    }

    fn write_style(&mut self, element: NodeId, style: &str) -> Result<(), CapsError> {
        let mut mutator = self.document.mutate();
        if style.is_empty() {
            mutator.remove_attribute(element, "style")
        } else {
            mutator.set_attribute(element, "style", style)
        }
        .map_err(CapsError::Mutation)
    }

    /// Set an attribute, if the subject may write `element`.
    pub fn set_attribute(
        &mut self,
        element: NodeId,
        name: &str,
        value: impl Into<String>,
    ) -> Result<(), CapsError> {
        self.require(element)?;
        self.document
            .mutate()
            .set_attribute(element, name, value)
            .map_err(CapsError::Mutation)
    }

    /// Remove an attribute, if the subject may write `element`.
    pub fn remove_attribute(&mut self, element: NodeId, name: &str) -> Result<(), CapsError> {
        self.require(element)?;
        self.document
            .mutate()
            .remove_attribute(element, name)
            .map_err(CapsError::Mutation)
    }

    /// Write access to each of `parent`'s current children and the guards
    /// inside them: what a replacement removes.
    fn require_children(&self, parent: NodeId) -> Result<(), CapsError> {
        for &child in self.document.tree().children(parent) {
            self.require_subtree(child)?;
        }
        Ok(())
    }

    /// Append `child` to `parent`, if the subject may write `parent` and
    /// `child` (a move takes it from where it was).
    pub fn append_child(&mut self, parent: NodeId, child: NodeId) -> Result<(), CapsError> {
        self.require(parent)?;
        self.require(child)?;
        self.document
            .mutate()
            .append_child(parent, child)
            .map_err(CapsError::Mutation)
    }

    /// Insert `child` before `sibling`, if the subject may write `sibling`
    /// and `child`.
    pub fn insert_before(&mut self, sibling: NodeId, child: NodeId) -> Result<(), CapsError> {
        self.require(sibling)?;
        self.require(child)?;
        self.document
            .mutate()
            .insert_before(sibling, child)
            .map_err(CapsError::Mutation)
    }

    /// Insert `child` after `sibling`, if the subject may write `sibling`
    /// and `child`.
    pub fn insert_after(&mut self, sibling: NodeId, child: NodeId) -> Result<(), CapsError> {
        self.require(sibling)?;
        self.require(child)?;
        self.document
            .mutate()
            .insert_after(sibling, child)
            .map_err(CapsError::Mutation)
    }

    /// Put `new` in `old`'s place, if the subject may write `old` and every
    /// guarded node inside it (they leave the document), and `new`.
    pub fn replace_with(&mut self, old: NodeId, new: NodeId) -> Result<(), CapsError> {
        self.require_subtree(old)?;
        self.require(new)?;
        self.document
            .mutate()
            .replace_with(old, new)
            .map_err(CapsError::Mutation)
    }

    /// A detached copy of `node` (`cloneNode`), if the subject may read it
    /// and, for a deep copy, every guarded node inside it. The copy carries
    /// no guards of its own.
    pub fn clone_node(&mut self, node: NodeId, deep: bool) -> Result<NodeId, CapsError> {
        self.require_readable(node, deep)?;
        self.document
            .clone_node(node, deep)
            .map_err(CapsError::Mutation)
    }

    /// Remove `child`, if the subject may write it and every guarded node
    /// inside it.
    pub fn remove(&mut self, child: NodeId) -> Result<(), CapsError> {
        self.require_subtree(child)?;
        self.document
            .mutate()
            .remove(child)
            .map_err(CapsError::Mutation)
    }

    /// Replace `parent`'s children with `new_children`, if the subject may
    /// write `parent`, each child it removes, and each child it moves in.
    pub fn replace_children(
        &mut self,
        parent: NodeId,
        new_children: &[NodeId],
    ) -> Result<(), CapsError> {
        self.require(parent)?;
        self.require_children(parent)?;
        for &child in new_children {
            self.require(child)?;
        }
        self.document
            .mutate()
            .replace_children(parent, new_children)
            .map_err(CapsError::Mutation)
    }

    /// Set `node`'s text (`textContent` semantics), if the subject may write
    /// `node` and, on an element, each child the new text replaces.
    pub fn set_text(&mut self, node: NodeId, text: impl Into<String>) -> Result<(), CapsError> {
        self.require(node)?;
        self.require_children(node)?;
        self.document
            .mutate()
            .set_text(node, text)
            .map_err(CapsError::Mutation)
    }
}
