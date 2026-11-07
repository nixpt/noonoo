//! The accessible node and tree types.

use noonoo_dom::NodeId;
use noonoo_geometry::Rect;

use crate::Role;

/// A checkable control's checked state.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Checked {
    /// Explicitly unchecked.
    False,
    /// Checked.
    True,
    /// Indeterminate (`aria-checked="mixed"`).
    Mixed,
}

impl Checked {
    /// The stable JSON/bridge spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::False => "false",
            Self::True => "true",
            Self::Mixed => "mixed",
        }
    }
}

/// The states that apply to a node.
///
/// `expanded` and `checked` are optional because "absent" means the state does
/// not apply, while `false` means it applies and is false — a distinction
/// assistive technology relies on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct States {
    /// The control is disabled.
    pub disabled: bool,
    /// The node holds keyboard focus.
    pub focused: bool,
    /// The node is selected (tabs, options, rows).
    pub selected: bool,
    /// The control is required.
    pub required: bool,
    /// The control is read-only.
    pub read_only: bool,
    /// The node is busy.
    pub busy: bool,
    /// Expanded (`true`) or collapsed (`false`); `None` when not applicable.
    pub expanded: Option<bool>,
    /// Checked state; `None` when the node is not checkable.
    pub checked: Option<Checked>,
}

/// One node in the accessibility tree.
#[derive(Clone, PartialEq, Debug)]
pub struct A11yNode {
    /// A tree-wide identifier, stable within one built tree.
    pub id: u64,
    /// The source DOM node, or `None` for a synthesized root (a window).
    pub node: Option<NodeId>,
    /// The host layer the node belongs to, for a composed window tree.
    pub layer: Option<usize>,
    /// What this node is.
    pub role: Role,
    /// The computed accessible name.
    pub name: String,
    /// The computed accessible description.
    pub description: String,
    /// The current value, for controls that have one.
    pub value: Option<String>,
    /// The heading level, for headings.
    pub level: Option<u8>,
    /// The node's states.
    pub states: States,
    /// The node's bounds in page coordinates, when it was laid out.
    pub bounds: Option<Rect>,
    /// Children, in document order.
    pub children: Vec<A11yNode>,
}

impl A11yNode {
    /// The exposed descendant whose source node is `node`, if any.
    pub fn find(&self, node: NodeId) -> Option<&Self> {
        if self.node == Some(node) {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(node))
    }

    /// The exposed descendant with tree-wide identifier `id`, if any.
    pub fn find_by_id(&self, id: u64) -> Option<&Self> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find_by_id(id))
    }

    /// The number of nodes in this subtree, including this one.
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Self::count).sum::<usize>()
    }

    /// Move every bound by `(dx, dy)`.
    pub fn translate(&mut self, dx: f32, dy: f32) {
        if let Some(bounds) = &mut self.bounds {
            bounds.x += dx;
            bounds.y += dy;
        }
        for child in &mut self.children {
            child.translate(dx, dy);
        }
    }

    pub(crate) fn renumber(&mut self, next: &mut u64) {
        self.id = *next;
        *next += 1;
        for child in &mut self.children {
            child.renumber(next);
        }
    }

    pub(crate) fn set_layer(&mut self, layer: usize) {
        self.layer = Some(layer);
        for child in &mut self.children {
            child.set_layer(layer);
        }
    }

    pub(crate) fn focused(&self) -> Option<u64> {
        if self.states.focused {
            return Some(self.id);
        }
        self.children.iter().find_map(Self::focused)
    }
}

/// A document's (or a window's) accessibility tree.
#[derive(Clone, PartialEq, Debug)]
pub struct A11yTree {
    /// The root node.
    pub root: A11yNode,
    /// The focused node's tree-wide identifier, if any.
    pub focus: Option<u64>,
}

impl A11yTree {
    /// The exposed node whose source DOM node is `node`, if any.
    pub fn find(&self, node: NodeId) -> Option<&A11yNode> {
        self.root.find(node)
    }

    /// The node with tree-wide identifier `id`, if any.
    pub fn find_by_id(&self, id: u64) -> Option<&A11yNode> {
        self.root.find_by_id(id)
    }

    /// The number of nodes, including the root.
    pub fn count(&self) -> usize {
        self.root.count()
    }
}
