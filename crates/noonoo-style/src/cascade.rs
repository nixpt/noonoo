//! The cascade.
//!
//! Collects rules from the UA sheet, author stylesheets, and inline `style`
//! attributes; matches them against the tree; and resolves a winning value per
//! property by origin, `!important`, specificity, and source order — then
//! applies inheritance and initial values into a [`ComputedStyle`].

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Generated, Interaction, PseudoElement, Styles};
use noonoo_css::value::Value;
use noonoo_css::{
    Declaration, Property, Stylesheet, expand_declaration, parse_declaration, parse_declarations,
    pending_shorthand,
};
use noonoo_dom::{Change, Changes, Document, NodeId};
use noonoo_query::ElementRef;
use noonoo_selectors::{MatchContext, SelectorList};

use crate::Viewport;
use crate::apply::{apply, resolve_font_size};
use crate::computed::*;
use crate::rule_index::RuleIndex;
use crate::ua::{QUIRKS_STYLESHEET, UA_STYLESHEET};
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

#[derive(Debug)]
struct Rule {
    selector: SelectorList,
    declarations: Vec<Declaration>,
    origin: Origin,
    order: u32,
    /// A UA rule for quirks-mode documents only.
    quirks: bool,
}

impl Rule {
    /// Whether the rule applies in `doc` at all, whatever its selector.
    fn applies_in(&self, doc: &Document) -> bool {
        !self.quirks || doc.quirks_mode() == noonoo_dom::QuirksMode::Quirks
    }
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
    let quirks_from = rules.len();
    let quirks = Stylesheet::parse(QUIRKS_STYLESHEET).unwrap_or_default();
    push_sheet(&quirks, Origin::Ua, viewport, &mut rules, &mut order);
    for rule in &mut rules[quirks_from..] {
        rule.quirks = true;
    }
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
            quirks: false,
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
        if !rule.applies_in(doc) {
            continue;
        }
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
        if !rule.applies_in(doc) {
            continue;
        }
        let Some(specificity) = rule.selector.matching_specificity(&view, ctx) else {
            continue;
        };
        add_rule(&mut winners, rule, specificity.to_u32());
    }

    // Presentational attributes beat the UA sheet and lose to every
    // author rule (HTML's rendering section, "presentational hints").
    for (order, (property, value)) in presentational_hints(doc, id, element)
        .into_iter()
        .enumerate()
    {
        let Ok(decls) = parse_declaration(property, &value, false) else {
            continue;
        };
        for decl in decls {
            let key = Key {
                rank: rank(Origin::Ua, false),
                specificity: u32::MAX,
                order: order as u32,
            };
            insert_best(&mut winners, decl.property, key, decl.values);
        }
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

/// HTML's rendering section, quirks mode: a child of `body`, `td` or `th`
/// with no substantial sibling before it loses its default (UA) top
/// margin; a child of `td` or `th` with none after it loses its default
/// bottom margin. Author margins stay.
fn drop_edge_margins(doc: &Document, id: NodeId, winners: &mut HashMap<Property, Candidate>) {
    let Some(parent) = doc.tree().parent(id) else {
        return;
    };
    let Some(container) = doc.element(parent).map(|e| e.name.as_str()) else {
        return;
    };
    let cell = matches!(container, "td" | "th");
    if !cell && container != "body" {
        return;
    }
    // An element, or text that isn't inter-element white space.
    let substantial = |node: NodeId| match doc.node(node) {
        Some(noonoo_dom::NodeData::Element(_)) => true,
        Some(noonoo_dom::NodeData::Text(text)) => !text
            .trim_matches(|c: char| c.is_ascii_whitespace())
            .is_empty(),
        _ => false,
    };
    let siblings = doc.tree().children(parent);
    let Some(at) = siblings.iter().position(|&node| node == id) else {
        return;
    };
    let mut drop_default = |property: Property| {
        if winners
            .get(&property)
            .is_some_and(|winner| winner.key.rank == rank(Origin::Ua, false))
        {
            winners.remove(&property);
        }
    };
    if !siblings[..at].iter().any(|&node| substantial(node)) {
        drop_default(Property::MarginTop);
    }
    if cell && !siblings[at + 1..].iter().any(|&node| substantial(node)) {
        drop_default(Property::MarginBottom);
    }
}

/// The CSS an element's presentational attributes ask for, as
/// `(property, value)` declarations.
fn presentational_hints(
    doc: &Document,
    id: NodeId,
    element: &noonoo_dom::ElementData,
) -> Vec<(&'static str, String)> {
    let mut hints = Vec::new();
    let cell = matches!(element.name.as_str(), "td" | "th");
    if (cell || element.name == "table")
        && let Some(width) = element.attr("width").and_then(dimension_attribute)
    {
        hints.push(("width", width));
    }
    if cell
        && let Some(padding) = owning_table(doc, id)
            .and_then(|table| doc.element(table)?.attr("cellpadding"))
            .and_then(non_negative_integer)
    {
        hints.push(("padding", format!("{padding}px")));
    }
    hints
}

/// The table a cell belongs to: up through its row and row group.
fn owning_table(doc: &Document, cell: NodeId) -> Option<NodeId> {
    let mut node = doc.tree().parent(cell)?;
    loop {
        match doc.element(node)?.name.as_str() {
            "table" => return Some(node),
            "tr" | "tbody" | "thead" | "tfoot" => node = doc.tree().parent(node)?,
            _ => return None,
        }
    }
}

/// HTML's "rules for parsing non-negative integers": leading digits after
/// white space.
fn non_negative_integer(value: &str) -> Option<u32> {
    let value = value.trim_start();
    let end = value
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(value.len());
    value[..end].parse().ok()
}

/// HTML's "rules for parsing dimension values", nonzero: a number, read
/// as pixels, or a percentage.
fn dimension_attribute(value: &str) -> Option<String> {
    let value = value.trim_start();
    let end = value
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(value.len());
    let number: f32 = value[..end].parse().ok()?;
    if !number.is_finite() || number <= 0.0 {
        return None;
    }
    Some(if value[end..].starts_with('%') {
        format!("{number}%")
    } else {
        format!("{number}px")
    })
}

/// The per-element results a child inherits from.
struct Resolved {
    style: ComputedStyle,
    /// Shared with the parent unless this element sets one.
    custom: Arc<CustomProperties>,
    line_height: InheritedLineHeight,
}

/// What a child inherits beyond its parent's computed style, kept per node
/// so one subtree can be restyled without the rest.
#[derive(Clone, Debug, Default)]
pub(crate) struct Inherited {
    custom: Vec<Arc<CustomProperties>>,
    line_heights: Vec<InheritedLineHeight>,
    root_font_size: f32,
    /// The hovered, focused and pressed nodes they were computed for.
    interaction: Interaction,
    /// The [`Cascade`] that computed these styles.
    cascade: u64,
    /// The document's node count when they were computed: generated boxes
    /// take the ids after it.
    nodes: usize,
}

/// A `::before`/`::after` box before its ids are handed out.
struct Pending {
    owner: NodeId,
    pseudo: PseudoElement,
    content: String,
    style: ComputedStyle,
}

/// The stylesheets prepared for styling: the user-agent sheet and the
/// author sheets in cascade order, `@media` rules settled for one viewport,
/// and the rules indexed by what their selectors need.
///
/// Build one when the stylesheets or the viewport change, then
/// [`compute`](Cascade::compute) a document's styles and
/// [`restyle`](Cascade::restyle) them after edits.
#[derive(Debug)]
pub struct Cascade {
    rules: Vec<Rule>,
    index: RuleIndex,
    viewport: Viewport,
    id: u64,
}

static NEXT_CASCADE: AtomicU64 = AtomicU64::new(1);

impl Cascade {
    /// Prepare `stylesheets`, after the user-agent sheet, for `viewport`.
    pub fn new(stylesheets: &[Stylesheet], viewport: Viewport) -> Self {
        let rules = collect_rules(stylesheets, &viewport);
        let index = RuleIndex::new(rules.iter().map(|rule| &rule.selector));
        Self {
            rules,
            index,
            viewport,
            id: NEXT_CASCADE.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// The viewport `@media` rules and viewport units were settled for.
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// Compute every element's style, plus the `::before`/`::after` boxes
    /// that have `content`.
    pub fn compute(&self, doc: &Document, interaction: Interaction) -> Styles {
        let nodes = doc.tree().len();
        let mut styles = Styles {
            entries: vec![None; nodes],
            generated: Vec::new(),
            inherited: Inherited {
                custom: vec![Arc::default(); nodes],
                line_heights: vec![InheritedLineHeight::Normal; nodes],
                root_font_size: 16.0,
                interaction,
                cascade: self.id,
                nodes,
            },
        };
        let mut pass = Pass::new(interaction);
        let mut first_element = true;
        for id in doc.elements() {
            self.style_element(doc, id, &mut styles, &mut pass, &mut first_element);
        }
        place_generated(&mut styles, doc, pass.generated);
        styles
    }

    /// Bring `styles` up to date with the edits in `changes`, restyling
    /// only the elements they can reach. Returns how many times an element
    /// was styled: the work done.
    ///
    /// `styles` should come from this cascade's [`compute`](Cascade::compute)
    /// or `restyle` on `doc`, with `changes` the document's edits since then
    /// ([`Document::take_changes`]). Styles from another cascade, an
    /// overflowed log, or an edit that reaches the root element restyle
    /// everything.
    pub fn restyle(
        &self,
        styles: &mut Styles,
        doc: &Document,
        interaction: Interaction,
        changes: &Changes,
    ) -> usize {
        if changes.overflowed() {
            return self.recompute(styles, doc, interaction);
        }
        self.restyle_for(styles, doc, interaction, changes.iter().copied())
    }

    /// [`restyle`](Cascade::restyle) for edits named one by one.
    ///
    /// A different `interaction` from the one `styles` were computed for
    /// counts as an attribute edit to each node that gained or lost
    /// hover, focus or press.
    ///
    /// No selector looks at a later sibling, a descendant or text, so an
    /// element's style can depend on an edit only if the element is under
    /// the edited node's parent (attributes) or under the edited node
    /// itself (children). Those subtrees are restyled; the rest is kept.
    pub fn restyle_for(
        &self,
        styles: &mut Styles,
        doc: &Document,
        interaction: Interaction,
        changes: impl IntoIterator<Item = Change>,
    ) -> usize {
        if styles.inherited.cascade != self.id {
            return self.recompute(styles, doc, interaction);
        }
        let tree = doc.tree();
        let before = std::mem::replace(&mut styles.inherited.interaction, interaction);
        let moved = [
            (before.hovered, interaction.hovered),
            (before.focused, interaction.focused),
            (before.active, interaction.active),
        ]
        .into_iter()
        .filter(|(old, new)| old != new)
        .flat_map(|(old, new)| [old, new])
        .flatten()
        .map(Change::Attributes);
        // Each change names the node whose descendants it can restyle.
        let mut roots = Vec::new();
        for change in changes.into_iter().chain(moved) {
            let root = match change {
                Change::Attributes(element) => tree.parent(element),
                Change::Children(parent) => Some(parent),
                Change::Text(_) => None,
            };
            let Some(root) = root.filter(|&root| doc.is_connected(root)) else {
                continue;
            };
            roots.push(root);
        }
        if roots.is_empty() {
            return 0;
        }
        // A root inside another's subtree is restyled with it.
        roots.sort_unstable();
        roots.dedup();
        let marked: HashSet<NodeId> = roots.iter().copied().collect();
        roots.retain(|&root| !tree.ancestors(root).iter().any(|a| marked.contains(a)));
        // A root with no style for its children to inherit — the document
        // node, when the root element itself was edited (that reaches `rem`
        // too), or a node whose insertion went unrecorded — starts over.
        // (Past the old node count, an entry is a generated box's.)
        let unstyled =
            |root: &NodeId| root.index() >= styles.inherited.nodes || styles.get(*root).is_none();
        if roots.iter().any(unstyled) {
            return self.recompute(styles, doc, interaction);
        }

        // The generated boxes as they were, before their ids move.
        let old: Vec<Pending> = std::mem::take(&mut styles.generated)
            .into_iter()
            .filter_map(|item| {
                Some(Pending {
                    style: styles.entries.get(item.element.index())?.clone()?,
                    owner: item.owner,
                    pseudo: item.pseudo,
                    content: item.content,
                })
            })
            .collect();
        // Make room for nodes created since.
        let nodes = tree.len();
        let inherited = &mut styles.inherited;
        styles.entries.truncate(inherited.nodes);
        styles.entries.resize(nodes, None);
        inherited.custom.resize(nodes, Arc::default());
        inherited
            .line_heights
            .resize(nodes, InheritedLineHeight::Normal);
        inherited.nodes = nodes;

        let mut pass = Pass::new(interaction);
        let mut restyled = HashSet::new();
        let mut styled = 0;
        for root in roots {
            for id in tree.descendants(root) {
                if doc.element(id).is_some() {
                    self.style_element(doc, id, styles, &mut pass, &mut false);
                    restyled.insert(id);
                    styled += 1;
                }
            }
        }

        // Generated boxes in document order: fresh ones for the restyled
        // owners, the old ones for the rest.
        let generated = if old.is_empty() && pass.generated.is_empty() {
            Vec::new()
        } else {
            let mut fresh = by_owner(pass.generated);
            let mut kept = by_owner(old);
            let mut list = Vec::new();
            for id in doc.elements() {
                let source = if restyled.contains(&id) {
                    &mut fresh
                } else {
                    &mut kept
                };
                list.extend(source.remove(&id).unwrap_or_default());
            }
            list
        };
        place_generated(styles, doc, generated);
        styled
    }

    fn recompute(&self, styles: &mut Styles, doc: &Document, interaction: Interaction) -> usize {
        *styles = self.compute(doc, interaction);
        doc.elements().len()
    }

    /// Style one element from its parent's results, already in `styles`.
    fn style_element(
        &self,
        doc: &Document,
        id: NodeId,
        styles: &mut Styles,
        pass: &mut Pass,
        first_element: &mut bool,
    ) {
        let viewport = self.viewport;
        let parent_id = doc.tree().parent(id);
        let parent = parent_id.and_then(|p| styles.entries[p.index()].as_ref());
        let inherited = &mut styles.inherited;
        let parent_custom = parent_id
            .map(|p| inherited.custom[p.index()].clone())
            .unwrap_or_default();
        let inherited_line = parent_id
            .map(|p| inherited.line_heights[p.index()])
            .unwrap_or(InheritedLineHeight::Normal);

        let mut winners = winners_for(
            doc,
            id,
            &self.rules,
            &self.index,
            &mut pass.candidates,
            pass.interaction,
        );
        if doc.quirks_mode() == noonoo_dom::QuirksMode::Quirks {
            drop_edge_margins(doc, id, &mut winners);
        }
        let mut resolved = resolve(
            &winners,
            parent,
            parent_custom,
            inherited_line,
            &mut inherited.root_font_size,
            first_element,
            viewport,
        );

        if generates_content(doc, id) {
            for pseudo in [PseudoElement::Before, PseudoElement::After] {
                let winners =
                    pseudo_winners_for(doc, id, &self.rules, &self.index, pass.interaction, pseudo);
                let Some(content) = winners
                    .get(&Property::Content)
                    .and_then(|c| computed_values(&Property::Content, &c.values, &resolved.custom))
                    .and_then(|values| content_text(&values, doc, id))
                else {
                    continue;
                };
                let style = resolve(
                    &winners,
                    Some(&resolved.style),
                    resolved.custom.clone(),
                    resolved.line_height,
                    &mut inherited.root_font_size,
                    &mut false,
                    viewport,
                )
                .style;
                pass.generated.push(Pending {
                    owner: id,
                    pseudo,
                    content,
                    style,
                });
            }
        }

        // Blink: a table never takes the `-webkit-` alignments; it resets
        // them to `start`, so `<center>` centres the table, not its text.
        if doc.element(id).is_some_and(|e| e.name == "table")
            && resolved.style.text_align == crate::TextAlign::WebkitCenter
        {
            resolved.style.text_align = crate::TextAlign::Start;
        }

        inherited.custom[id.index()] = resolved.custom;
        inherited.line_heights[id.index()] = resolved.line_height;
        styles.entries[id.index()] = Some(resolved.style);
    }
}

/// Scratch state for one compute or restyle.
struct Pass {
    interaction: Interaction,
    candidates: Vec<usize>,
    generated: Vec<Pending>,
}

impl Pass {
    fn new(interaction: Interaction) -> Self {
        Self {
            interaction,
            candidates: Vec::new(),
            generated: Vec::new(),
        }
    }
}

fn by_owner(boxes: Vec<Pending>) -> HashMap<NodeId, Vec<Pending>> {
    let mut map: HashMap<NodeId, Vec<Pending>> = HashMap::new();
    for item in boxes {
        map.entry(item.owner).or_default().push(item);
    }
    map
}

/// Hand `generated` the ids the document would give out next, so a layout
/// copy that inserts them finds their styles by id.
fn place_generated(styles: &mut Styles, doc: &Document, generated: Vec<Pending>) {
    let nodes = styles.inherited.nodes;
    styles.entries.truncate(nodes);
    styles.entries.resize(nodes + 2 * generated.len(), None);
    styles.generated = Vec::with_capacity(generated.len());
    for (k, item) in generated.into_iter().enumerate() {
        let element = doc.tree().upcoming_id(2 * k);
        styles.entries[element.index()] = Some(item.style);
        styles.generated.push(Generated {
            owner: item.owner,
            pseudo: item.pseudo,
            element,
            text: doc.tree().upcoming_id(2 * k + 1),
            content: item.content,
        });
    }
}

/// Cascade `winners` over what the parent passes down.
#[allow(clippy::too_many_arguments)]
fn resolve(
    winners: &HashMap<Property, Candidate>,
    parent: Option<&ComputedStyle>,
    parent_custom: Arc<CustomProperties>,
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
            Arc::make_mut(&mut props).insert(name.clone(), candidate.values.clone());
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
            _ => match style.line_height {
                Some(px) => InheritedLineHeight::Px(px),
                None => InheritedLineHeight::Normal,
            },
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
    fn resolve(self, font_size: f32) -> Option<f32> {
        match self {
            Self::Normal => None,
            Self::Factor(factor) => Some(font_size * factor),
            Self::Px(px) => Some(px),
        }
    }
}
