//! Building the accessibility tree from the DOM, computed style, and layout.

use std::collections::HashMap;

use noonoo_dom::{Document, ElementData, NodeData, NodeId};
use noonoo_geometry::Rect;
use noonoo_layout::LayoutTree;
use noonoo_style::{Display, Styles, Visibility};

use crate::node::{A11yNode, A11yTree, Checked, States};
use crate::role::{Role, input_type, is_excluded_tag, is_presentational, role_for};

/// Build a document's accessibility tree without layout (no bounds).
pub fn build(doc: &Document, styles: &Styles) -> A11yTree {
    build_in(doc, styles, None, None)
}

/// Build a document's accessibility tree, optionally with layout bounds and a
/// focused node.
pub fn build_in(
    doc: &Document,
    styles: &Styles,
    layout: Option<&LayoutTree>,
    focus: Option<NodeId>,
) -> A11yTree {
    let mut builder = Builder {
        doc,
        styles,
        rects: layout.map(collect_rects).unwrap_or_default(),
        focus,
        next_id: 1,
    };
    let root = builder
        .visit(doc.root(), false)
        .into_iter()
        .next()
        .unwrap_or_else(|| A11yNode {
            id: 1,
            node: None,
            layer: None,
            role: Role::Document,
            name: String::new(),
            description: String::new(),
            value: None,
            level: None,
            states: States::default(),
            bounds: None,
            children: Vec::new(),
        });
    let focus = root.focused();
    A11yTree { root, focus }
}

struct Builder<'a> {
    doc: &'a Document,
    styles: &'a Styles,
    rects: HashMap<NodeId, Rect>,
    focus: Option<NodeId>,
    next_id: u64,
}

impl Builder<'_> {
    fn visit(&mut self, id: NodeId, parent_hidden: bool) -> Vec<A11yNode> {
        let Some(data) = self.doc.node(id) else {
            return Vec::new();
        };
        match data {
            NodeData::Document => {
                let name = self.document_name();
                let a11y_id = self.take_id();
                let children = self.children(id, false);
                vec![A11yNode {
                    id: a11y_id,
                    node: Some(id),
                    layer: None,
                    role: Role::Document,
                    name,
                    description: String::new(),
                    value: None,
                    level: None,
                    states: States::default(),
                    bounds: self.rects.get(&id).copied(),
                    children,
                }]
            }
            NodeData::Element(element) => {
                let element = element.clone();
                self.visit_element(id, &element)
            }
            NodeData::Text(text) => {
                let name = collapse(text);
                if parent_hidden || name.is_empty() {
                    return Vec::new();
                }
                vec![A11yNode {
                    id: self.take_id(),
                    node: Some(id),
                    layer: None,
                    role: Role::StaticText,
                    name,
                    description: String::new(),
                    value: None,
                    level: None,
                    states: States::default(),
                    bounds: self.rects.get(&id).copied(),
                    children: Vec::new(),
                }]
            }
            NodeData::Doctype { .. } | NodeData::Comment(_) => Vec::new(),
        }
    }

    fn visit_element(&mut self, id: NodeId, element: &ElementData) -> Vec<A11yNode> {
        if is_excluded_tag(&element.name)
            || element.attr("hidden").is_some()
            || aria_true(element, "aria-hidden")
            || (element.name == "input" && input_type(element) == "hidden")
            || self.is_display_none(id)
        {
            return Vec::new();
        }

        // `visibility` is inherited. Recurse with the inherited state so text
        // under a hidden ancestor stays out, while a descendant that sets
        // `visibility: visible` (its computed value differs) is exposed again.
        let hidden = self.is_visibility_hidden(id);

        // `role="none"`/`"presentation"` and `visibility: hidden` contribute no
        // node of their own; their children are promoted. Unlike display:none
        // or aria-hidden, content that restores visibility stays in the tree.
        if is_presentational(element) || hidden {
            return self.children(id, hidden);
        }

        let a11y_id = self.take_id();
        let children = self.children(id, false);
        let role = role_for(element);
        let (name, title_used) = accessible_name(self.doc, id, element, role);
        let description = accessible_description(self.doc, element, title_used);
        let value = value_for(self.doc, id, element);
        let states = states_for(element, role, self.focus == Some(id));
        let level = heading_level(element);

        vec![A11yNode {
            id: a11y_id,
            node: Some(id),
            layer: None,
            role,
            name,
            description,
            value,
            level,
            states,
            bounds: self.rects.get(&id).copied(),
            children,
        }]
    }

    fn children(&mut self, id: NodeId, parent_hidden: bool) -> Vec<A11yNode> {
        let ids = self.doc.tree().children(id).to_vec();
        let mut children = Vec::new();
        for child in ids {
            children.extend(self.visit(child, parent_hidden));
        }
        children
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn is_display_none(&self, id: NodeId) -> bool {
        self.styles
            .get(id)
            .is_some_and(|style| style.display == Display::None)
    }

    fn is_visibility_hidden(&self, id: NodeId) -> bool {
        self.styles
            .get(id)
            .is_some_and(|style| style.visibility != Visibility::Visible)
    }

    fn document_name(&self) -> String {
        self.doc
            .get_elements_by_tag_name("title")
            .first()
            .map(|&title| collapse(&collect_text(self.doc, title)))
            .unwrap_or_default()
    }
}

/// Union every laid-out box per source node (wrapped text has several boxes).
fn collect_rects(layout: &LayoutTree) -> HashMap<NodeId, Rect> {
    let mut rects: HashMap<NodeId, Rect> = HashMap::new();
    for box_ in layout.boxes() {
        let Some(node) = box_.node else {
            continue;
        };
        match rects.get_mut(&node) {
            Some(rect) => {
                let left = rect.x.min(box_.rect.x);
                let top = rect.y.min(box_.rect.y);
                let right = rect.right().max(box_.rect.right());
                let bottom = rect.bottom().max(box_.rect.bottom());
                *rect = Rect::new(left, top, right - left, bottom - top);
            }
            None => {
                rects.insert(node, box_.rect);
            }
        }
    }
    rects
}

fn accessible_name(
    doc: &Document,
    id: NodeId,
    element: &ElementData,
    role: Role,
) -> (String, bool) {
    if let Some(name) = element.attr("aria-labelledby") {
        let name = referenced_text(doc, name);
        if !name.is_empty() {
            return (name, false);
        }
    }
    if let Some(label) = element.attr("aria-label") {
        let label = collapse(label);
        if !label.is_empty() {
            return (label, false);
        }
    }

    if (element.name == "img" || element.name == "area")
        && let Some(alt) = element.attr("alt")
    {
        return (collapse(alt), false);
    }

    if element.name == "input" {
        let kind = input_type(element);
        if kind == "image"
            && let Some(alt) = element.attr("alt")
        {
            let alt = collapse(alt);
            if !alt.is_empty() {
                return (alt, false);
            }
        }
        if matches!(kind, "button" | "submit" | "reset" | "image") {
            if let Some(value) = element.attr("value") {
                let value = collapse(value);
                if !value.is_empty() {
                    return (value, false);
                }
            }
            let default = match kind {
                "submit" => "Submit",
                "reset" => "Reset",
                _ => "",
            };
            if !default.is_empty() {
                return (default.to_owned(), false);
            }
        }
    }

    if matches!(element.name.as_str(), "input" | "textarea" | "select") {
        let label = label_text(doc, id, element);
        if !label.is_empty() {
            return (label, false);
        }
    }

    if role.names_from_contents() {
        let text = collapse(&collect_text(doc, id));
        if !text.is_empty() {
            return (text, false);
        }
    }

    if let Some(title) = element.attr("title") {
        let title = collapse(title);
        if !title.is_empty() {
            return (title, true);
        }
    }

    (String::new(), false)
}

fn accessible_description(doc: &Document, element: &ElementData, title_used: bool) -> String {
    if let Some(description) = element.attr("aria-describedby") {
        let description = referenced_text(doc, description);
        if !description.is_empty() {
            return description;
        }
    }
    if !title_used && let Some(title) = element.attr("title") {
        let title = collapse(title);
        if !title.is_empty() {
            return title;
        }
    }
    if matches!(element.name.as_str(), "input" | "textarea")
        && let Some(placeholder) = element.attr("placeholder")
    {
        let placeholder = collapse(placeholder);
        if !placeholder.is_empty() {
            return placeholder;
        }
    }
    String::new()
}

fn value_for(doc: &Document, id: NodeId, element: &ElementData) -> Option<String> {
    if let Some(text) = element.attr("aria-valuetext") {
        let text = collapse(text);
        if !text.is_empty() {
            return Some(text);
        }
    }

    let direct = element
        .attr("value")
        .or_else(|| element.attr("aria-valuenow"))
        .map(str::to_owned)
        .filter(|value| !value.is_empty());

    match role_for(element) {
        Role::TextBox if element.name == "textarea" => {
            let text = collapse(&collect_text(doc, id));
            (!text.is_empty()).then_some(text)
        }
        Role::TextBox | Role::SpinButton | Role::Slider | Role::ProgressBar => direct,
        Role::ComboBox | Role::ListBox => selected_option_text(doc, id),
        _ => None,
    }
}

fn selected_option_text(doc: &Document, id: NodeId) -> Option<String> {
    let mut options = doc.get_elements_by_tag_name("option");
    options.retain(|&option| doc.tree().is_ancestor_of(id, option));
    let chosen = options
        .iter()
        .copied()
        .find(|&option| {
            doc.element(option)
                .is_some_and(|element| element.attr("selected").is_some())
        })
        .or_else(|| options.first().copied())?;
    let text = collapse(&collect_text(doc, chosen));
    (!text.is_empty()).then_some(text)
}

fn states_for(element: &ElementData, role: Role, focused: bool) -> States {
    let checked = match element.attr("aria-checked") {
        Some("true") => Some(Checked::True),
        Some("false") => Some(Checked::False),
        Some("mixed") => Some(Checked::Mixed),
        _ if role.is_checkable() => Some(if element.attr("checked").is_some() {
            Checked::True
        } else {
            Checked::False
        }),
        _ => None,
    };

    States {
        disabled: element.attr("disabled").is_some() || aria_true(element, "aria-disabled"),
        focused,
        selected: aria_true(element, "aria-selected")
            || (element.name == "option" && element.attr("selected").is_some()),
        required: element.attr("required").is_some() || aria_true(element, "aria-required"),
        read_only: element.attr("readonly").is_some() || aria_true(element, "aria-readonly"),
        busy: aria_true(element, "aria-busy"),
        expanded: aria_bool(element, "aria-expanded"),
        checked,
    }
}

fn heading_level(element: &ElementData) -> Option<u8> {
    if let Some(level) = element
        .attr("aria-level")
        .and_then(|value| value.parse().ok())
    {
        return Some(level);
    }
    match element.name.as_str() {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

fn aria_bool(element: &ElementData, name: &str) -> Option<bool> {
    match element.attr(name) {
        Some("true") => Some(true),
        Some("false") => Some(false),
        _ => None,
    }
}

fn aria_true(element: &ElementData, name: &str) -> bool {
    element
        .attr(name)
        .is_some_and(|value| value.eq_ignore_ascii_case("true"))
}

/// The text of every element referenced by a whitespace-separated id list.
fn referenced_text(doc: &Document, ids: &str) -> String {
    let mut parts = Vec::new();
    for id in ids.split_whitespace() {
        if let Some(node) = doc.get_element_by_id(id) {
            parts.push(collect_text(doc, node));
        }
    }
    collapse(&parts.join(" "))
}

/// The text of the `<label>` associated with a control, if any.
fn label_text(doc: &Document, id: NodeId, element: &ElementData) -> String {
    if let Some(control_id) = element.id() {
        for label in doc.get_elements_by_tag_name("label") {
            let target = doc.element(label).and_then(|label| label.attr("for"));
            if target == Some(control_id) {
                let text = collapse(&collect_text(doc, label));
                if !text.is_empty() {
                    return text;
                }
            }
        }
    }
    for ancestor in doc.tree().ancestors(id) {
        if doc
            .element(ancestor)
            .is_some_and(|element| element.name == "label")
        {
            let text = collapse(&collect_text(doc, ancestor));
            if !text.is_empty() {
                return text;
            }
        }
    }
    String::new()
}

/// Concatenate the text below `id`, skipping elements that never expose text.
fn collect_text(doc: &Document, id: NodeId) -> String {
    match doc.node(id) {
        Some(NodeData::Text(text)) => text.clone(),
        Some(NodeData::Element(_)) | Some(NodeData::Document) => {
            let mut text = String::new();
            for &child in doc.tree().children(id) {
                if doc
                    .element(child)
                    .is_some_and(|element| is_excluded_tag(&element.name))
                {
                    continue;
                }
                text.push_str(&collect_text(doc, child));
                text.push(' ');
            }
            text
        }
        _ => String::new(),
    }
}

/// Collapse whitespace the way an accessible name is normalized.
fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
