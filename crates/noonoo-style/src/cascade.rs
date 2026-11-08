//! The cascade.
//!
//! Collects rules from the UA sheet, author stylesheets, and inline `style`
//! attributes; matches them against the tree; and resolves a winning value per
//! property by origin, `!important`, specificity, and source order — then
//! applies inheritance and initial values into a [`ComputedStyle`].

use std::collections::HashMap;
use std::rc::Rc;

use crate::{Generated, Interaction, PseudoElement};
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
use crate::rule_index::RuleIndex;
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

/// The winning declarations for `id`'s `::before`/`::after`: rules whose
/// selector ends in that pseudo-element. Inline styles never reach one.
fn pseudo_winners_for(
    doc: &Document,
    id: NodeId,
    rules: &[Rule],
    index: &RuleIndex,
    interaction: Interaction,
    pseudo: PseudoElement,
) -> HashMap<Property, Candidate> {
    let Some(view) = ElementRef::new(doc, id) else {
        return HashMap::new();
    };
    let ctx = match_context(id, interaction);
    let mut winners = HashMap::new();
    for rule in index.pseudo().iter().map(|&i| &rules[i]) {
        let Some(specificity) =
            rule.selector
                .pseudo_matching_specificity(&view, ctx, pseudo.name())
        else {
            continue;
        };
        add_rule(&mut winners, rule, specificity.to_u32());
    }
    winners
}

fn match_context(id: NodeId, interaction: Interaction) -> MatchContext {
    MatchContext {
        hovered: interaction.hovered == Some(id),
        focused: interaction.focused == Some(id),
        active: interaction.active == Some(id),
    }
}

fn add_rule(winners: &mut HashMap<Property, Candidate>, rule: &Rule, spec: u32) {
    for (index, decl) in rule.declarations.iter().enumerate() {
        let key = Key {
            rank: rank(rule.origin, decl.important),
            specificity: spec,
            order: rule.order + index as u32,
        };
        insert_best(winners, decl.property.clone(), key, decl.values.clone());
    }
}

fn winners_for(
    doc: &Document,
    id: NodeId,
    rules: &[Rule],
    index: &RuleIndex,
    candidates: &mut Vec<usize>,
    interaction: Interaction,
) -> HashMap<Property, Candidate> {
    let (Some(view), Some(element)) = (ElementRef::new(doc, id), doc.element(id)) else {
        return HashMap::new(); // Not an element: no rule applies.
    };
    // Dynamic pseudo-classes describe *this* element: `:hover` matches only
    // the hovered node, not its ancestors or the whole tree.
    let ctx = match_context(id, interaction);
    let mut winners = HashMap::new();

    // Only the rules filed under this element's id, classes or tag (or
    // under no key) can match it.
    index.candidates(element, candidates);
    for rule in candidates.iter().map(|&i| &rules[i]) {
        let Some(specificity) = rule.selector.matching_specificity(&view, ctx) else {
            continue;
        };
        add_rule(&mut winners, rule, specificity.to_u32());
    }

    // Inline `style` attributes win over author rules of equal importance.
    if let Some(style) = element.attr("style") {
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

/// The per-element results a child inherits from.
struct Resolved {
    style: ComputedStyle,
    /// Shared with the parent unless this element sets one.
    custom: Rc<CustomProperties>,
    line_height: InheritedLineHeight,
}

/// Compute every element's style, in document order (parents before
/// children), plus the `::before`/`::after` boxes that have `content`.
pub(crate) fn compute(
    doc: &Document,
    stylesheets: &[Stylesheet],
    viewport: Viewport,
    interaction: Interaction,
) -> (Vec<Option<ComputedStyle>>, Vec<Generated>) {
    let rules = collect_rules(stylesheets, &viewport);
    let index = RuleIndex::new(rules.iter().map(|rule| &rule.selector));
    let mut candidates = Vec::new();
    let mut out: Vec<Option<ComputedStyle>> = vec![None; doc.tree().len()];
    let mut custom: Vec<Rc<CustomProperties>> = vec![Rc::default(); doc.tree().len()];
    let mut line_heights: Vec<InheritedLineHeight> =
        vec![InheritedLineHeight::Normal; doc.tree().len()];
    let mut root_font_size = 16.0_f32;
    let mut first_element = true;
    let mut generated: Vec<(Generated, ComputedStyle)> = Vec::new();

    for id in doc.elements() {
        let parent_id = doc.tree().parent(id);
        let parent = parent_id.and_then(|p| out[p.index()].clone());
        let parent_custom = parent_id
            .map(|p| custom[p.index()].clone())
            .unwrap_or_default();
        let inherited_line = parent_id
            .map(|p| line_heights[p.index()])
            .unwrap_or(InheritedLineHeight::Normal);

        let winners = winners_for(doc, id, &rules, &index, &mut candidates, interaction);
        let resolved = resolve(
            &winners,
            parent.as_ref(),
            parent_custom,
            inherited_line,
            &mut root_font_size,
            &mut first_element,
            viewport,
        );

        if generates_content(doc, id) {
            for pseudo in [PseudoElement::Before, PseudoElement::After] {
                let winners = pseudo_winners_for(doc, id, &rules, &index, interaction, pseudo);
                let Some(content) = winners
                    .get(&Property::Content)
                    .and_then(|c| computed_values(&Property::Content, &c.values, &resolved.custom))
                    .and_then(|values| content_text(&values, doc, id))
                else {
                    continue;
                };
                let pseudo_style = resolve(
                    &winners,
                    Some(&resolved.style),
                    resolved.custom.clone(),
                    resolved.line_height,
                    &mut root_font_size,
                    &mut false,
                    viewport,
                )
                .style;
                let k = generated.len();
                generated.push((
                    Generated {
                        owner: id,
                        pseudo,
                        element: doc.tree().upcoming_id(2 * k),
                        text: doc.tree().upcoming_id(2 * k + 1),
                        content,
                    },
                    pseudo_style,
                ));
            }
        }

        custom[id.index()] = resolved.custom;
        line_heights[id.index()] = resolved.line_height;
        out[id.index()] = Some(resolved.style);
    }

    // Generated elements take the ids the document would hand out next, so
    // a layout copy that inserts them finds their styles by id.
    out.resize(doc.tree().len() + 2 * generated.len(), None);
    let mut list = Vec::with_capacity(generated.len());
    for (item, style) in generated {
        out[item.element.index()] = Some(style);
        list.push(item);
    }
    (out, list)
}

/// Cascade `winners` over what the parent passes down.
#[allow(clippy::too_many_arguments)]
fn resolve(
    winners: &HashMap<Property, Candidate>,
    parent: Option<&ComputedStyle>,
    parent_custom: Rc<CustomProperties>,
    inherited_line: InheritedLineHeight,
    root_font_size: &mut f32,
    first_element: &mut bool,
    viewport: Viewport,
) -> Resolved {
    let parent_font = parent.map(|s| s.font_size).unwrap_or(16.0);

    // Custom properties inherit, then this element's winners override them.
    // Build the full map *before* substituting, so declaration order within
    // an element does not matter (`color: var(--x); --x: red` works).
    // The parent's map is shared until this element sets its own.
    let mut props = parent_custom;
    for (property, candidate) in winners {
        if let Property::Custom(name) = property {
            Rc::make_mut(&mut props).insert(name.clone(), candidate.values.clone());
        }
    }

    let font_size = winners
        .get(&Property::FontSize)
        .and_then(|c| computed_values(&Property::FontSize, &c.values, &props))
        .and_then(|values| resolve_font_size(&values, parent_font, *root_font_size, viewport))
        .unwrap_or(parent_font);

    let mut style = match parent {
        Some(parent) => ComputedStyle::inherited_from(parent),
        None => ComputedStyle::default(),
    };
    style.font_size = font_size;
    style.line_height = inherited_line.resolve(font_size);
    let mut line_height = inherited_line;

    if *first_element {
        *root_font_size = font_size;
        *first_element = false;
    }

    // `color` first, so `currentColor` in any other property sees it.
    let mut ordered: Vec<(&Property, &Candidate)> = winners.iter().collect();
    ordered.sort_by_key(|(property, _)| !matches!(property, Property::Color));
    for (property, candidate) in ordered {
        if matches!(
            property,
            Property::FontSize | Property::Custom(_) | Property::Content
        ) {
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
            *root_font_size,
            viewport,
            parent,
        );
    }

    if let Some(candidate) = winners.get(&Property::LineHeight)
        && let Some(values) = computed_values(&Property::LineHeight, &candidate.values, &props)
    {
        line_height = match values.as_slice() {
            [Value::Number(factor)] => InheritedLineHeight::Factor(*factor),
            [Value::Keyword(k)] if k.eq_ignore_ascii_case("inherit") => inherited_line,
            [Value::Keyword(k)] if k.eq_ignore_ascii_case("unset") => inherited_line,
            [Value::Keyword(k)]
                if k.eq_ignore_ascii_case("normal") || k.eq_ignore_ascii_case("initial") =>
            {
                InheritedLineHeight::Normal
            }
            _ => InheritedLineHeight::Px(style.line_height),
        };
    }

    Resolved {
        style,
        custom: props,
        line_height,
    }
}

/// Whether `element` can have `::before`/`::after`: replaced and void
/// elements have no content box to put them in.
fn generates_content(doc: &Document, element: NodeId) -> bool {
    doc.element(element).is_some_and(|data| {
        !matches!(
            data.name.as_str(),
            "img"
                | "input"
                | "br"
                | "hr"
                | "textarea"
                | "select"
                | "iframe"
                | "video"
                | "audio"
                | "canvas"
                | "embed"
                | "object"
                | "meter"
                | "progress"
        )
    })
}

/// A `content` value as text: strings and `attr()` concatenated. `none`,
/// `normal` and anything unsupported (counters, quotes, images) give no box.
fn content_text(values: &[Value], doc: &Document, owner: NodeId) -> Option<String> {
    let mut text = String::new();
    for value in values {
        match value {
            Value::Str(s) => text.push_str(s),
            Value::Function { name, args } if name.eq_ignore_ascii_case("attr") => {
                match args.as_slice() {
                    [Value::Keyword(attribute)] => text.push_str(
                        doc.element(owner)
                            .and_then(|e| e.attr(&attribute.to_ascii_lowercase()))
                            .unwrap_or_default(),
                    ),
                    _ => return None,
                }
            }
            _ => return None,
        }
    }
    (!values.is_empty()).then_some(text)
}

/// What a child inherits for `line-height`: a bare number stays a
/// multiplier of the child's own font size; lengths and percentages are
/// already pixels.
#[derive(Clone, Copy, Debug)]
enum InheritedLineHeight {
    Normal,
    Factor(f32),
    Px(f32),
}

impl InheritedLineHeight {
    fn resolve(self, font_size: f32) -> f32 {
        match self {
            Self::Normal => font_size * 1.2,
            Self::Factor(factor) => font_size * factor,
            Self::Px(px) => px,
        }
    }
}
