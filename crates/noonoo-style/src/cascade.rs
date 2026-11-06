//! The cascade.
//!
//! Collects rules from the UA sheet, author stylesheets, and inline `style`
//! attributes; matches them against the tree; and resolves a winning value per
//! property by origin, `!important`, specificity, and source order — then
//! applies inheritance and initial values into a [`ComputedStyle`].

use std::collections::HashMap;

use noonoo_css::value::Value;
use noonoo_css::{Declaration, Property, Stylesheet, parse_declaration, parse_declarations};
use noonoo_dom::{Document, NodeId};
use noonoo_selectors::{Element, MatchContext, SelectorList};

use crate::apply::{apply, resolve_font_size};
use crate::computed::*;
use crate::ua::UA_STYLESHEET;

/// Where a rule came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    /// The user-agent stylesheet.
    Ua,
    /// An author stylesheet.
    Author,
    /// An inline `style` attribute.
    Inline,
}

/// Cascade precedence: important-origin, then origin, then specificity, then order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    rank: u8,
    specificity: u32,
    order: u32,
}

fn rank(origin: Origin, important: bool) -> u8 {
    match (origin, important) {
        (Origin::Ua, false) => 0,
        (Origin::Author, false) => 1,
        (Origin::Inline, false) => 2,
        (Origin::Author, true) => 3,
        (Origin::Inline, true) => 4,
        (Origin::Ua, true) => 5,
    }
}

struct Rule {
    selector: SelectorList,
    declarations: Vec<Declaration>,
    origin: Origin,
    order: u32,
}

struct Candidate {
    key: Key,
    values: Vec<Value>,
}

/// A read-only element view over a [`Document`], for selector matching.
#[derive(Clone, Copy)]
struct DomView<'a> {
    doc: &'a Document,
    id: NodeId,
}

impl<'a> DomView<'a> {
    fn element_children(&self, id: NodeId) -> Vec<NodeId> {
        self.doc
            .tree()
            .children(id)
            .iter()
            .copied()
            .filter(|&child| self.doc.element(child).is_some())
            .collect()
    }
}

impl Element for DomView<'_> {
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
    fn parent(&self) -> Option<Self> {
        let parent = self.doc.tree().parent(self.id)?;
        self.doc.element(parent).is_some().then_some(DomView {
            doc: self.doc,
            id: parent,
        })
    }
    fn previous_siblings(&self) -> Vec<Self> {
        let Some(parent) = self.doc.tree().parent(self.id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for child in self.element_children(parent) {
            if child == self.id {
                break;
            }
            out.push(DomView {
                doc: self.doc,
                id: child,
            });
        }
        out
    }
    fn sibling_index(&self) -> usize {
        let Some(parent) = self.doc.tree().parent(self.id) else {
            return 0;
        };
        self.element_children(parent)
            .iter()
            .position(|&child| child == self.id)
            .unwrap_or(0)
    }
    fn sibling_count(&self) -> usize {
        let Some(parent) = self.doc.tree().parent(self.id) else {
            return 1;
        };
        self.element_children(parent).len()
    }
}

/// Build the rule list: the UA sheet, then the author stylesheets.
fn collect_rules(stylesheets: &[Stylesheet]) -> Vec<Rule> {
    let mut rules = Vec::new();
    let mut order = 0u32;
    let ua = Stylesheet::parse(UA_STYLESHEET).unwrap_or_default();
    push_sheet(&ua, Origin::Ua, &mut rules, &mut order);
    for sheet in stylesheets {
        push_sheet(sheet, Origin::Author, &mut rules, &mut order);
    }
    rules
}

fn push_sheet(sheet: &Stylesheet, origin: Origin, rules: &mut Vec<Rule>, order: &mut u32) {
    for rule in &sheet.rules {
        let noonoo_css::Rule::Qualified {
            selector,
            declarations,
        } = rule
        else {
            continue;
        };
        let Ok(selector) = SelectorList::parse(selector) else {
            continue;
        };
        let mut typed = Vec::new();
        for raw in declarations {
            if let Ok(mut decls) = parse_declaration(&raw.name, &raw.value, raw.important) {
                typed.append(&mut decls);
            }
        }
        if typed.is_empty() {
            continue;
        }
        *order += typed.len() as u32;
        rules.push(Rule {
            selector,
            declarations: typed,
            origin,
            order: *order,
        });
    }
}

fn insert_best(
    winners: &mut HashMap<Property, Candidate>,
    property: Property,
    key: Key,
    values: Vec<Value>,
) {
    match winners.get(&property) {
        Some(existing) if existing.key >= key => {}
        _ => {
            winners.insert(property, Candidate { key, values });
        }
    }
}

fn winners_for(doc: &Document, id: NodeId, rules: &[Rule]) -> HashMap<Property, Candidate> {
    let view = DomView { doc, id };
    let ctx = MatchContext::default();
    let mut winners = HashMap::new();

    for rule in rules {
        let Some(specificity) = rule.selector.matching_specificity(&view, ctx) else {
            continue;
        };
        let spec = specificity.to_u32();
        for (index, decl) in rule.declarations.iter().enumerate() {
            let key = Key {
                rank: rank(rule.origin, decl.important),
                specificity: spec,
                order: rule.order + index as u32,
            };
            insert_best(
                &mut winners,
                decl.property.clone(),
                key,
                decl.values.clone(),
            );
        }
    }

    // Inline `style` attributes win over author rules of equal importance.
    if let Some(style) = doc.element(id).and_then(|element| element.attr("style")) {
        let mut order = 0u32;
        for raw in parse_declarations(style) {
            let Ok(decls) = parse_declaration(&raw.name, &raw.value, raw.important) else {
                continue;
            };
            for decl in decls {
                let key = Key {
                    rank: rank(Origin::Inline, decl.important),
                    specificity: u32::MAX,
                    order,
                };
                order += 1;
                insert_best(&mut winners, decl.property, key, decl.values);
            }
        }
    }

    winners
}

/// Compute every element's style, in document order (parents before children).
pub(crate) fn compute(doc: &Document, stylesheets: &[Stylesheet]) -> Vec<Option<ComputedStyle>> {
    let rules = collect_rules(stylesheets);
    let mut out: Vec<Option<ComputedStyle>> = vec![None; doc.tree().len()];
    let mut root_font_size = 16.0_f32;
    let mut first_element = true;

    for id in doc.elements() {
        let parent = doc.tree().parent(id).and_then(|p| out[p.index()].clone());
        let parent_font = parent.as_ref().map(|s| s.font_size).unwrap_or(16.0);

        let winners = winners_for(doc, id, &rules);

        let font_size = winners
            .get(&Property::FontSize)
            .and_then(|c| resolve_font_size(&c.values, parent_font, root_font_size))
            .unwrap_or(parent_font);

        let mut style = match &parent {
            Some(parent) => ComputedStyle::inherited_from(parent),
            None => ComputedStyle::default(),
        };
        style.font_size = font_size;
        style.line_height = font_size * 1.2;

        if first_element {
            root_font_size = font_size;
            first_element = false;
        }

        for (property, candidate) in &winners {
            if matches!(property, Property::FontSize) {
                continue;
            }
            apply(
                &mut style,
                property,
                &candidate.values,
                font_size,
                root_font_size,
            );
        }

        out[id.index()] = Some(style);
    }

    out
}
