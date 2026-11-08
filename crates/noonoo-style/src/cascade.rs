//! The cascade.
//!
//! Collects rules from the UA sheet, author stylesheets, and inline `style`
//! attributes; matches them against the tree; and resolves a winning value per
//! property by origin, `!important`, specificity, and source order — then
//! applies inheritance and initial values into a [`ComputedStyle`].

use std::collections::HashMap;

use crate::Interaction;
use noonoo_css::value::Value;
use noonoo_css::{
    Declaration, Property, Stylesheet, expand_declaration, parse_declaration, parse_declarations,
    pending_shorthand,
};
use noonoo_dom::{Document, NodeId};
use noonoo_query::ElementRef;
use noonoo_selectors::{MatchContext, SelectorList};

use crate::Viewport;
use crate::apply::{apply, resolve_font_size};
use crate::computed::*;
use crate::ua::UA_STYLESHEET;
use crate::vars::{self, CustomProperties};

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

/// The winning `values` of `property` with every `var()` substituted, or
/// `None` if the declaration is invalid at computed-value time.
///
/// A pending-substitution longhand (its shorthand held `var()`) substitutes
/// the shorthand's raw value, expands it, and takes `property`'s part. If
/// the substituted shorthand doesn't set `property`, that's invalid too.
fn computed_values(
    property: &Property,
    values: &[Value],
    props: &CustomProperties,
) -> Option<Vec<Value>> {
    let Some((shorthand, raw)) = pending_shorthand(values) else {
        return vars::substitute(values, props);
    };
    let substituted = vars::substitute(raw, props)?;
    expand_declaration(shorthand, substituted, false)
        .into_iter()
        .find(|declaration| &declaration.property == property)
        .map(|declaration| declaration.values)
}

/// Build the rule list: the UA sheet, then the author stylesheets. A
/// `@media` rule whose prelude matches the viewport expands in place, so its
/// rules keep their source order in the cascade; a prelude that doesn't
/// match (or a malformed block) contributes nothing.
fn collect_rules(stylesheets: &[Stylesheet], viewport: &Viewport) -> Vec<Rule> {
    let mut rules = Vec::new();
    let mut order = 0u32;
    let ua = Stylesheet::parse(UA_STYLESHEET).unwrap_or_default();
    push_sheet(&ua, Origin::Ua, viewport, &mut rules, &mut order);
    for sheet in stylesheets {
        push_sheet(sheet, Origin::Author, viewport, &mut rules, &mut order);
    }
    rules
}

fn push_sheet(
    sheet: &Stylesheet,
    origin: Origin,
    viewport: &Viewport,
    rules: &mut Vec<Rule>,
    order: &mut u32,
) {
    for rule in &sheet.rules {
        // `@media`: a rule whose prelude matches recurses into the block's
        // parsed rules; anything else (other at-rules, a non-matching
        // prelude, a malformed block) contributes nothing here.
        if let noonoo_css::Rule::At {
            name,
            prelude,
            block,
        } = rule
            && name.eq_ignore_ascii_case("media")
        {
            if crate::media::matches(prelude, viewport)
                && let Some(block) = block
                && let Ok(inner) = Stylesheet::parse(block)
            {
                push_sheet(&inner, origin, viewport, rules, order);
            }
            continue;
        }
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

fn winners_for(
    doc: &Document,
    id: NodeId,
    rules: &[Rule],
    interaction: Interaction,
) -> HashMap<Property, Candidate> {
    let Some(view) = ElementRef::new(doc, id) else {
        return HashMap::new(); // Not an element: no rule applies.
    };
    // Dynamic pseudo-classes describe *this* element: `:hover` matches only
    // the hovered node, not its ancestors or the whole tree.
    let ctx = MatchContext {
        hovered: interaction.hovered == Some(id),
        focused: interaction.focused == Some(id),
        active: interaction.active == Some(id),
    };
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
pub(crate) fn compute(
    doc: &Document,
    stylesheets: &[Stylesheet],
    viewport: Viewport,
    interaction: Interaction,
) -> Vec<Option<ComputedStyle>> {
    let rules = collect_rules(stylesheets, &viewport);
    let mut out: Vec<Option<ComputedStyle>> = vec![None; doc.tree().len()];
    let mut custom: Vec<CustomProperties> = vec![CustomProperties::new(); doc.tree().len()];
    let mut root_font_size = 16.0_f32;
    let mut first_element = true;

    for id in doc.elements() {
        let parent_id = doc.tree().parent(id);
        let parent = parent_id.and_then(|p| out[p.index()].clone());
        let parent_font = parent.as_ref().map(|s| s.font_size).unwrap_or(16.0);
        let parent_custom = parent_id
            .map(|p| custom[p.index()].clone())
            .unwrap_or_default();

        let winners = winners_for(doc, id, &rules, interaction);

        // Custom properties inherit, then this element's winners override them.
        // Build the full map *before* substituting, so declaration order within
        // an element does not matter (`color: var(--x); --x: red` works).
        let mut props = parent_custom;
        for (property, candidate) in &winners {
            if let Property::Custom(name) = property {
                props.insert(name.clone(), candidate.values.clone());
            }
        }
        custom[id.index()] = props.clone();

        let font_size = winners
            .get(&Property::FontSize)
            .and_then(|c| computed_values(&Property::FontSize, &c.values, &props))
            .and_then(|values| resolve_font_size(&values, parent_font, root_font_size, viewport))
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
            if matches!(property, Property::FontSize | Property::Custom(_)) {
                continue;
            }
            // A `var()` that cannot be resolved invalidates the declaration;
            // the property then keeps its inherited or initial value.
            let Some(values) = computed_values(property, &candidate.values, &props) else {
                continue;
            };
            apply(
                &mut style,
                property,
                &values,
                font_size,
                root_font_size,
                viewport,
                parent.as_ref(),
            );
        }

        out[id.index()] = Some(style);
    }

    out
}
