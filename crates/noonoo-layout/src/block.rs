//! Block and inline flow.

use std::collections::HashMap;

use noonoo_dom::{NodeData, NodeId};
use noonoo_geometry::Rect;
use noonoo_style::{ComputedStyle, Display, Edges, TextAlign, VerticalAlign, WhiteSpace};

use crate::float;
use crate::margin;
use crate::{BoxKind, Ctx, LayoutBox, child_nodes};

pub(crate) fn no_edges() -> Edges<f32> {
    Edges::all(0.0)
}

fn is_display_none(ctx: &Ctx<'_>, node: NodeId) -> bool {
    ctx.styles
        .get(node)
        .is_some_and(|style| style.display == Display::None)
}

pub(crate) fn map_align(align: TextAlign) -> noonoo_text::Align {
    match align {
        TextAlign::Start | TextAlign::Left => noonoo_text::Align::Start,
        TextAlign::End | TextAlign::Right => noonoo_text::Align::End,
        TextAlign::Center | TextAlign::WebkitCenter => noonoo_text::Align::Center,
        TextAlign::Justify => noonoo_text::Align::Justify,
    }
}

/// Lay out the block-and-inline children of `parent_node`, collapsing vertical
/// margins.
///
/// Returns `(child box indices, content height)`. Vertical margins are owned
/// here: each child's border box is positioned from its collapsed leading
/// margin, and a collapsed-through child contributes its margin to the chain
/// without occupying space.
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout_block_children(
    ctx: &mut Ctx<'_>,
    parent_node: NodeId,
    children: &[NodeId],
    content_x: f32,
    content_y: f32,
    content_width: f32,
    parent_style: &ComputedStyle,
    collapse_top: bool,
    collapse_bottom: bool,
) -> (Vec<usize>, f32) {
    let mut indices = Vec::new();
    let mut cursor = content_y;
    let mut pending: Option<f32> = None;
    let mut has_content = false;
    // Only floats registered from here on belong to this container's height.
    let float_base = ctx.floats().len();
    let mut i = 0;

    while i < children.len() {
        let child = children[i];
        if is_display_none(ctx, child) {
            i += 1;
            continue;
        }
        if ctx.is_out_of_flow(child) {
            ctx.pending.push(child);
            i += 1;
            continue;
        }
        if ctx.is_float(child) {
            // A float shapes its formatting context's lines and its container's
            // height, but not the position of block siblings, so it never
            // joins the flow cursor.
            float::place(
                ctx,
                child,
                content_x,
                content_width,
                cursor + pending.unwrap_or(0.0),
            );
            if let Some(index) = ctx.tree_index(child) {
                indices.push(index);
            }
            has_content = true;
            i += 1;
            continue;
        }

        if is_table_part(ctx, child) {
            if let Some(margin) = pending.take() {
                cursor += margin;
            }
            cursor += layout_anonymous_table(
                ctx,
                children,
                &mut i,
                (content_x, cursor, content_width),
                &mut indices,
            );
            has_content = true;
            continue;
        }

        if ctx.is_inline_level(child) {
            let mut run = Vec::new();
            while i < children.len() {
                let candidate = children[i];
                if is_display_none(ctx, candidate) {
                    i += 1;
                    continue;
                }
                if ctx.is_out_of_flow(candidate) {
                    ctx.pending.push(candidate);
                    i += 1;
                    continue;
                }
                if !ctx.is_inline_level(candidate) {
                    break;
                }
                run.push(candidate);
                i += 1;
            }
            // Inline content is a line box between the blocks: it stops
            // collapsing, so the run applies any pending margin in full
            // before its first line. A block inside an inline element at
            // the run's start or end still adjoins the flow's margins.
            // A run of white space alone makes no box: the margins on
            // either side of it still collapse.
            // So does a phantom line (CSS 2 §9.4.2): empty, edgeless
            // inline elements.
            if run.iter().all(|&node| margin::is_phantom(ctx, node)) {
                continue;
            }
            // Its edge blocks, past the phantoms around them.
            let first = run
                .iter()
                .copied()
                .find(|&node| !margin::is_phantom(ctx, node));
            let last = run
                .iter()
                .copied()
                .rfind(|&node| !margin::is_phantom(ctx, node));
            if !run.is_empty() {
                let at_top = !has_content && collapse_top;
                let margins = RunMargins {
                    incoming: pending.take(),
                    collapsed_top: at_top
                        .then(|| first.and_then(|first| margin::edge_block(ctx, first, true)))
                        .flatten(),
                };
                let (mut placed, height, trailing) = layout_inline_run(
                    ctx,
                    parent_node,
                    &run,
                    content_x,
                    cursor,
                    content_width,
                    parent_style,
                    margins,
                );
                indices.append(&mut placed);
                cursor += height;
                // The trailing block's margin stays open only if the margin
                // walk sees it at the run's end too.
                if let Some((margin, block)) = trailing {
                    if last.and_then(|last| margin::edge_block(ctx, last, false)) == Some(block) {
                        pending = Some(margin);
                    } else {
                        cursor += margin;
                    }
                }
            } else if let Some(margin) = pending.take() {
                cursor += margin;
            }
            has_content = true;
            continue;
        }

        let margin = margin::effective_margins(ctx, child, content_width);
        // `clear` sits below the floats it names before anything else happens.
        cursor = cursor.max(ctx.floats().clearance(ctx.style(child).clear, cursor));

        if margin.through {
            // A collapsed-through box adds its margin to the chain and occupies
            // no space; it still gets a (zero-height) box for the tree. Unless
            // the margin leaves through the container's top, the box goes
            // below the margin so far, as if it had a bottom border.
            let at = if !has_content && collapse_top {
                cursor
            } else {
                cursor
                    + match pending {
                        Some(previous) => margin::collapse(previous, margin.leading),
                        None => margin.leading,
                    }
            };
            pending = Some(match pending {
                Some(previous) => margin::collapse(previous, margin.top),
                None => margin.top,
            });
            let available = flow_width(ctx, child, content_width);
            ctx.layout_element(child, content_x + margin.left, at, available, None);
            if let Some(index) = ctx.tree_index(child) {
                apply_auto_margins(ctx, child, index, content_width, parent_style.text_align);
                indices.push(index);
            }
            i += 1;
            continue;
        }

        let gap = match pending {
            Some(previous) => margin::collapse(previous, margin.top),
            None => margin.top,
        };
        if !has_content && collapse_top {
            // The leading margin collapses out of the container entirely, so the
            // first content sits at the container's content top.
        } else {
            cursor += gap;
        }

        let available = flow_width(ctx, child, content_width);
        let height = ctx.layout_element(child, content_x + margin.left, cursor, available, None);
        if let Some(index) = ctx.tree_index(child) {
            apply_auto_margins(ctx, child, index, content_width, parent_style.text_align);
            indices.push(index);
        }
        cursor += height;
        pending = Some(margin.bottom);
        has_content = true;
        i += 1;
    }

    // A trailing margin collapses out of the container (and so out of its
    // height) unless a bottom border or padding pins it inside.
    if let Some(margin) = pending
        && !collapse_bottom
    {
        cursor += margin;
    }

    // A formatting context root's auto height holds its floats, nested ones
    // included (CSS 2 §10.6.7); block siblings flow right past them, and a
    // plain block's height is its in-flow content alone. The root element
    // is the initial context's root.
    let flow_height = cursor - content_y;
    let document = ctx.document;
    let is_root = parent_node == document.root()
        || document.tree().parent(parent_node) == Some(document.root());
    if is_root || ctx.establishes_bfc(parent_node, parent_style) {
        (
            indices,
            flow_height.max(ctx.floats().depth_since(float_base, content_y)),
        )
    } else {
        (indices, flow_height)
    }
}

/// A word or atomic inline box, ready to be wrapped onto a line.
struct Item {
    /// The node its box reports: a text run's text node, an atomic inline,
    /// or `None` for a replaced element's placeholder text (its element's
    /// fragment carries the node).
    node: Option<NodeId>,
    style_node: NodeId,
    kind: BoxKind,
    text: String,
    /// The space it takes on the line: for an atomic inline, its margin box.
    width: f32,
    height: f32,
    /// Set for an atomic inline: laid out as a box of its own at placement.
    atomic: Option<Atomic>,
    /// Whether a space goes before it on a line (white space preceded it).
    spaced: bool,
    /// Whether it sticks to the item before: no line break between them.
    glued: bool,
    /// Preserved white space before it (`pre`, `pre-wrap`), tabs expanded,
    /// with its width: drawn as it stands instead of one space.
    preserved: Option<(String, f32)>,
    /// The inline elements it sits in, outermost first.
    chain: Vec<NodeId>,
    /// How many of the innermost of `chain` start with it, and end with it.
    opens: usize,
    closes: usize,
    /// The start edges (margin, border, padding) of the elements it opens,
    /// and the end edges of those it closes: room taken on the line.
    lead: f32,
    trail: f32,
    /// Where the white space before it falls among the inline edges around
    /// it, as the source has it.
    straddle: Straddle,
    /// An empty, edgeless inline element's marker: it gives the element a
    /// zero-width box where it sits, but no line height and no line of
    /// its own.
    phantom: bool,
}

/// Which inline elements a collapsed space between two items falls in, as
/// in the source: `a<span> b</span>` puts it inside the span, `a <span>b`
/// outside, and `<span>a </span>b` inside, at its end. Chromium sizes an
/// inline element's box to the spaces it holds.
#[derive(Clone, Copy, Default, Debug)]
struct Straddle {
    /// How many of the elements the item opens, outermost first, hold the
    /// space.
    opens: usize,
    /// How many of the elements the item before closes, outermost first,
    /// hold the space.
    closes: usize,
    /// The space's width in the font of the text it sits in.
    width: Option<f32>,
}

impl Item {
    /// The room it takes on the line, its elements' edges included.
    fn advance(&self) -> f32 {
        self.lead + self.width + self.trail
    }

    /// The white space before it on a line, `space` being one collapsed space.
    fn gap(&self, space: f32) -> f32 {
        match &self.preserved {
            Some((_, width)) => *width,
            None if self.spaced => self.straddle.width.unwrap_or(space),
            None => 0.0,
        }
    }

    /// The white space a line starting with it keeps: preserved white space
    /// at the start of a line the source began (not a soft wrap).
    fn leading_gap(&self) -> f32 {
        self.preserved.as_ref().map_or(0.0, |(_, width)| *width)
    }
}

/// An inline element's margin, border and padding, resolved against the
/// containing block's `width`.
struct InlineEdges {
    margin: Edges<f32>,
    border: Edges<f32>,
    padding: Edges<f32>,
}

impl InlineEdges {
    fn of(ctx: &Ctx<'_>, node: NodeId, width: f32) -> Self {
        let style = ctx.style(node);
        Self {
            margin: crate::resolve::margins(&style.margin, width),
            border: crate::resolve::borders(style),
            padding: crate::resolve::paddings(&style.padding, width),
        }
    }

    fn start(&self) -> f32 {
        self.margin.left + self.border.left + self.padding.left
    }

    fn end(&self) -> f32 {
        self.margin.right + self.border.right + self.padding.right
    }
}

/// One inline element's stretch of a line: the items it holds there.
struct Fragment {
    element: NodeId,
    /// Its place in its items' element chains.
    depth: usize,
    first: usize,
    last: usize,
}

/// The fragments of `line`'s inline elements, outermost first among those
/// starting at the same item.
fn line_fragments(line: &[Item]) -> Vec<Fragment> {
    let mut fragments: Vec<Fragment> = Vec::new();
    for (position, item) in line.iter().enumerate() {
        for (depth, &element) in item.chain.iter().enumerate() {
            match fragments
                .iter_mut()
                .rev()
                .find(|fragment| fragment.element == element)
            {
                Some(fragment) => fragment.last = position,
                None => fragments.push(Fragment {
                    element,
                    depth,
                    first: position,
                    last: position,
                }),
            }
        }
    }
    fragments
}

/// `line` as (white space, word) pairs in order: each word with the white
/// space before it, and a last pair with no word for trailing white space.
fn words_of(line: &str) -> Vec<(&str, Option<&str>)> {
    let mut pairs = Vec::new();
    let mut rest = line;
    while !rest.is_empty() {
        let word_at = rest
            .find(|c: char| !noonoo_text::is_collapsible_space(c))
            .unwrap_or(rest.len());
        let (white, after) = rest.split_at(word_at);
        if after.is_empty() {
            pairs.push((white, None));
            break;
        }
        let end = after
            .find(noonoo_text::is_collapsible_space)
            .unwrap_or(after.len());
        pairs.push((white, Some(&after[..end])));
        rest = &after[end..];
    }
    pairs
}

/// Where `vertical-align` puts an inline-level box.
#[derive(Clone, Copy)]
enum Align {
    /// Raised this far above its parent's baseline (negative lowers).
    Rise(f32),
    /// Its top at the line box's top.
    Top,
    /// Its bottom at the line box's bottom.
    Bottom,
}

/// `style`'s `vertical-align` for a box reaching `above`/`below` its own
/// baseline, in a parent styled `parent` (CSS 2 §10.8.1). `sub` and
/// `super` shift as Blink does (a fifth and a third of the parent's font
/// size, plus a pixel); the x-height `middle` uses is taken as 0.5em.
fn vertical_align(
    ctx: &Ctx<'_>,
    style: &ComputedStyle,
    parent: &ComputedStyle,
    above: f32,
    below: f32,
) -> Align {
    match style.vertical_align {
        VerticalAlign::Baseline => Align::Rise(0.0),
        VerticalAlign::Sub => Align::Rise(-(parent.font_size / 5.0 + 1.0)),
        VerticalAlign::Super => Align::Rise(parent.font_size / 3.0 + 1.0),
        VerticalAlign::TextTop => Align::Rise(ctx.font_metrics(parent).0 - above),
        VerticalAlign::TextBottom => Align::Rise(below - ctx.font_metrics(parent).1),
        VerticalAlign::Middle => Align::Rise((below - above) / 2.0 + parent.font_size * 0.25),
        VerticalAlign::Top => Align::Top,
        VerticalAlign::Bottom => Align::Bottom,
        VerticalAlign::Length(px) => Align::Rise(px),
        VerticalAlign::Percent(percent) => Align::Rise(percent / 100.0 * ctx.line_height_of(style)),
    }
}

/// One step of the inline walk: a node to visit, or the end of an inline
/// element's content.
enum Step {
    Visit(NodeId),
    Exit(NodeId),
}

/// The state of the inline walk between items.
#[derive(Default)]
struct InlineWalk {
    /// Whether white space came since the last item: a space goes before
    /// the next one, and the line may break there.
    spaced: bool,
    /// Whether the last item is a phantom that took a space: white space
    /// right after it collapses into that one, as across any inline
    /// element's edges.
    phantom_spaced: bool,
    /// For that white space: how many open elements had no item yet when it
    /// came, and how many elements closed since.
    space_opening: usize,
    closed_since_space: usize,
    /// ... and its width, in the font of the text it came in.
    space_width: f32,
    /// CSS Text: a line may break on either side of an atomic inline,
    /// white space or not. Set after one, for the item that follows.
    after_atomic: bool,
    /// The open inline elements, outermost first, each with the number of
    /// items placed when it opened.
    chain: Vec<NodeId>,
    entered_at: Vec<usize>,
    /// How many of the innermost open elements no item has opened yet.
    opening: usize,
    /// Preserved white space waiting for the next item, and its width.
    kept: String,
    kept_width: f32,
    /// Columns since the line began, for tab stops.
    column: usize,
    /// Items placed so far, and where the last one is.
    count: usize,
    last: Option<(usize, usize)>,
}

impl InlineWalk {
    /// A forced break: the next line starts fresh.
    fn new_line(&mut self) {
        self.spaced = false;
        self.phantom_spaced = false;
        self.kept.clear();
        self.kept_width = 0.0;
        self.column = 0;
    }

    /// White space in the source, a space `width` wide in its text's font:
    /// the next item gets a space before it.
    fn space(&mut self, width: f32) {
        if !self.spaced && !self.phantom_spaced {
            self.spaced = true;
            self.space_opening = self.opening;
            self.closed_since_space = 0;
            self.space_width = width;
        }
    }

    fn enter(&mut self, element: NodeId) {
        self.chain.push(element);
        self.entered_at.push(self.count);
        self.opening += 1;
    }

    /// Close `element`: the last item inside it ends it, taking its end
    /// edges. An empty element still makes a box, held open by an empty
    /// item: one without edges is a phantom, taking no line height.
    fn exit(&mut self, ctx: &Ctx<'_>, element: NodeId, width: f32, groups: &mut [Vec<Item>]) {
        let entered = self.entered_at.last().copied().unwrap_or(0);
        if self.count == entered {
            // An empty element still marks its place: with edges they take
            // room on the line; without, it's a phantom (CSS 2 §9.4.2).
            let edges = InlineEdges::of(ctx, element, width);
            let phantom = edges.start() + edges.end() == 0.0;
            // An element split around a block isn't empty, only this run
            // of it: it marks nothing here.
            let empty = child_nodes(ctx.document, element)
                .into_iter()
                .all(|child| ctx.is_hidden(child) || margin::is_phantom(ctx, child));
            if !phantom || empty {
                let item = Item {
                    node: None,
                    style_node: element,
                    kind: BoxKind::Text,
                    text: String::new(),
                    width: 0.0,
                    height: ctx.line_height_of(ctx.style(element)),
                    atomic: None,
                    spaced: self.spaced,
                    glued: !self.spaced && !self.after_atomic,
                    preserved: None,
                    chain: Vec::new(),
                    opens: 0,
                    closes: 0,
                    lead: 0.0,
                    trail: 0.0,
                    straddle: Straddle::default(),
                    phantom,
                };
                self.push(ctx, width, groups, item, false);
            }
        }
        self.chain.pop();
        let entered = self.entered_at.pop().unwrap_or(0);
        if self.count == entered {
            self.opening = self.opening.saturating_sub(1);
            return;
        }
        if let Some((group, index)) = self.last
            && let Some(item) = groups.get_mut(group).and_then(|group| group.get_mut(index))
        {
            item.closes += 1;
            item.trail += InlineEdges::of(ctx, element, width).end();
            if self.spaced {
                self.closed_since_space += 1;
            }
        }
    }

    /// Add `item` inside the open elements, opening those that wait.
    fn push(
        &mut self,
        ctx: &Ctx<'_>,
        width: f32,
        groups: &mut [Vec<Item>],
        mut item: Item,
        atomic: bool,
    ) {
        item.chain = self.chain.clone();
        item.opens = self.opening;
        if !self.kept.is_empty() {
            item.preserved = Some((std::mem::take(&mut self.kept), self.kept_width));
            item.spaced = true;
            self.kept_width = 0.0;
        }
        item.lead = self.chain[self.chain.len() - self.opening..]
            .iter()
            .map(|&element| InlineEdges::of(ctx, element, width).start())
            .sum();
        if self.spaced && item.preserved.is_none() {
            item.straddle = Straddle {
                opens: self.space_opening.min(self.opening),
                closes: self.closed_since_space,
                width: Some(self.space_width),
            };
        }
        self.opening = 0;
        self.phantom_spaced = item.phantom && (self.spaced || self.phantom_spaced);
        self.spaced = false;
        self.after_atomic = atomic;
        let group = groups.len() - 1;
        groups[group].push(item);
        self.last = Some((group, groups[group].len() - 1));
        self.count += 1;
    }
}

/// How many of `items` fit on the first line of `width`: greedy breaking at
/// the break opportunities, where an item isn't glued to the one before.
/// A run of glued items is placed whole; the first always fits.
fn first_line_len(items: &[Item], space: f32, width: f32) -> usize {
    // Clusters: (item count, width), split where a break may go.
    let mut clusters: Vec<(usize, f32)> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let gap = item.gap(space);
        match clusters.last_mut() {
            Some((count, used)) if index > 0 && item.glued => {
                *count += 1;
                *used += gap + item.advance();
            }
            _ => clusters.push((1, item.advance())),
        }
    }
    // Clusters after the first are separated by the space a break replaces
    // (every unglued item but one with no white space before is spaced).
    let gaps: Vec<f32> = {
        let mut start = 0;
        clusters
            .iter()
            .map(|&(count, _)| {
                let gap = items[start].gap(space);
                start += count;
                gap
            })
            .collect()
    };
    let mut used = 0.0;
    let mut taken = 0;
    for (index, &(count, cluster)) in clusters.iter().enumerate() {
        let gap = if index == 0 { 0.0 } else { gaps[index] };
        if index > 0 && used + gap + cluster > width + 0.01 {
            break;
        }
        used += gap + cluster;
        taken += count;
    }
    taken
}

/// How to lay out an atomic inline once the line places it.
#[derive(Clone, Copy)]
struct Atomic {
    /// The width to hand `layout_element` (see `float::shrink_wrapped_width`).
    available: f32,
    margin_left: f32,
    margin_top: f32,
}

/// The vertical margins around an inline run's blocks (blocks inside its
/// inline elements), which collapse with the flow around the run.
#[derive(Default)]
pub(crate) struct RunMargins {
    /// The margin pending before the run, from the blocks above it.
    pub incoming: Option<f32>,
    /// A leading block whose top margin already collapsed into the
    /// container's: it sits at the run's top.
    pub collapsed_top: Option<NodeId>,
}

/// Lay out an inline formatting context: text and inline boxes wrapped into
/// lines inside the band the container's floats leave free, aligned per the
/// container's `text-align`.
///
/// Float children among the inline nodes are laid out first (at `y`), so the
/// lines that follow wrap around them.
///
/// A `<br>` element ends the current line: the items collected after it go
/// into a separate group that starts on a fresh line. The approximations, in
/// the spirit of a simplified line-box model (see the crate docs):
///
/// - an empty group between two breaks is one blank line whose height is the
///   container's `line-height` (not the neighbouring lines' own heights);
/// - the run's trailing break generates no extra line (`H<br>` stays one
///   line tall), matching how browsers collapse the collapsed line box;
/// - breaks are part of the flow they appear in, so `white-space: pre` lines
///   from source text and `<br>` breaks do not interact (each just breaks).
///
/// Blocks inside its inline elements are laid out between its lines, their
/// margins collapsing with `margins`. Returns the boxes, the height, and the
/// margin left pending after a trailing block, with that block.
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout_inline_run(
    ctx: &mut Ctx<'_>,
    parent_node: NodeId,
    nodes: &[NodeId],
    x: f32,
    y: f32,
    width: f32,
    parent_style: &ComputedStyle,
    margins: RunMargins,
) -> (Vec<usize>, f32, Option<(f32, NodeId)>) {
    let space = ctx.advance(" ", parent_style);
    let line_height = ctx.line_height_of(parent_style);

    // The run's items, split into groups at each `<br>`: a group starts a
    // fresh line, and an empty group is the blank line between two breaks.
    let mut groups: Vec<Vec<Item>> = vec![Vec::new()];
    let mut had_break = false;
    // Blocks inside inline elements (CSS 2 §9.2.1.1), each with the group
    // that resumes the inline content after it.
    let mut blocks: Vec<(usize, NodeId)> = Vec::new();
    let mut walk = InlineWalk::default();
    // The inline content in document order. A worklist, not recursion:
    // deep inline nesting mustn't cost stack per level. An element's `Exit`
    // follows its content, closing it on the last item it holds.
    let mut stack: Vec<Step> = nodes.iter().rev().map(|&node| Step::Visit(node)).collect();
    while let Some(step) = stack.pop() {
        let node = match step {
            Step::Visit(node) => node,
            Step::Exit(element) => {
                walk.exit(ctx, element, width, &mut groups);
                continue;
            }
        };
        if is_display_none(ctx, node) {
            continue;
        }
        if ctx.is_float(node) {
            float::place(ctx, node, x, width, y);
            continue;
        }
        if !walk.chain.is_empty() && ctx.is_out_of_flow(node) {
            ctx.pending.push(node);
            continue;
        }
        match ctx.document.node(node) {
            Some(NodeData::Text(text)) => {
                // Text in an inline element takes that element's style;
                // loose text is the container's.
                let owner = walk.chain.last().copied();
                let style = match owner {
                    None => parent_style,
                    Some(element) => ctx.style(element),
                };
                let style_node = owner.unwrap_or(parent_node);
                let height = match owner {
                    None => line_height,
                    Some(_) => ctx.line_height_of(style),
                };
                // Inside a `nowrap`/`pre` element the words don't break.
                let unbreakable = owner.is_some()
                    && matches!(style.white_space, WhiteSpace::NoWrap | WhiteSpace::Pre);
                // A preserved newline breaks the line like a `<br>`.
                for (piece, line) in crate::forced_lines(text, style.white_space)
                    .into_iter()
                    .enumerate()
                {
                    if piece > 0 {
                        had_break = true;
                        groups.push(Vec::new());
                        walk.new_line();
                    }
                    let preserve = crate::preserves_spaces(style.white_space);
                    for (index, (white, word)) in words_of(line).into_iter().enumerate() {
                        if !white.is_empty() {
                            walk.space(ctx.advance(" ", style));
                            if preserve {
                                let kept = crate::expand_tabs(white, walk.column);
                                walk.column += kept.chars().count();
                                walk.kept_width += ctx.advance(&kept, style);
                                walk.kept.push_str(&kept);
                            }
                        }
                        let Some(word) = word else { continue };
                        walk.column += word.chars().count();
                        // A breaking space (an em space, ...) ends a piece:
                        // the next may start a line, with no space between.
                        for (part, piece) in noonoo_text::break_pieces(word).enumerate() {
                            let first = index == 0 && part == 0;
                            let spaced = if part == 0 {
                                !first || walk.spaced
                            } else {
                                false
                            };
                            let glued = if first {
                                !walk.spaced && !walk.after_atomic
                            } else {
                                unbreakable
                            };
                            let item = Item {
                                node: Some(node),
                                style_node,
                                kind: BoxKind::Text,
                                text: piece.to_owned(),
                                width: ctx.advance(piece, style),
                                height,
                                atomic: None,
                                spaced,
                                glued,
                                preserved: None,
                                chain: Vec::new(),
                                opens: 0,
                                closes: 0,
                                lead: 0.0,
                                trail: 0.0,
                                straddle: Straddle::default(),
                                phantom: false,
                            };
                            walk.push(ctx, width, &mut groups, item, false);
                        }
                    }
                }
            }
            Some(NodeData::Element(element)) => {
                if element.name == "br" {
                    had_break = true;
                    groups.push(Vec::new());
                    walk.new_line();
                    continue;
                }
                let style = ctx.style(node).clone();
                if crate::is_atomic_inline(style.display) {
                    // A box of its own: shrink-wrapped (or its set width),
                    // measured by a trial layout, placed on the line like a
                    // word by its margin box.
                    let margin = crate::resolve::margins(&style.margin, width);
                    let (available, border_box_width) =
                        float::shrink_wrapped_width(ctx, node, &style, width);
                    let height = ctx.measure_height(node, 0.0, 0.0, available, None);
                    let item = Item {
                        node: Some(node),
                        style_node: node,
                        kind: BoxKind::Inline,
                        text: String::new(),
                        width: margin.left + border_box_width + margin.right,
                        height: margin.top + height + margin.bottom,
                        atomic: Some(Atomic {
                            available,
                            margin_left: margin.left,
                            margin_top: margin.top,
                        }),
                        spaced: walk.spaced,
                        glued: false,
                        preserved: None,
                        chain: Vec::new(),
                        opens: 0,
                        closes: 0,
                        lead: 0.0,
                        trail: 0.0,
                        straddle: Straddle::default(),
                        phantom: false,
                    };
                    walk.push(ctx, width, &mut groups, item, true);
                    continue;
                }
                // A block inside an inline ends the line and sits between
                // the inline content before and after it.
                if !ctx.is_inline_level(node) && !is_table_part(ctx, node) {
                    groups.push(Vec::new());
                    walk.new_line();
                    walk.after_atomic = false;
                    blocks.push((groups.len() - 1, node));
                    continue;
                }
                // An inline element: its content joins the line inside it.
                walk.enter(node);
                stack.push(Step::Exit(node));
                let children = ctx.document.tree().children(node);
                if children.is_empty() {
                    // Replaced content (`<img alt>`, `<input value>`) shows
                    // its placeholder text, like a browser while the source
                    // is missing.
                    let text = element.placeholder_text();
                    if !text.is_empty() {
                        let item = Item {
                            node: None,
                            style_node: node,
                            kind: BoxKind::Text,
                            text: text.to_owned(),
                            width: ctx.advance(text, &style),
                            height: ctx.line_height_of(&style),
                            atomic: None,
                            spaced: walk.spaced,
                            glued: !walk.spaced && !walk.after_atomic,
                            preserved: None,
                            chain: Vec::new(),
                            opens: 0,
                            closes: 0,
                            lead: 0.0,
                            trail: 0.0,
                            straddle: Straddle::default(),
                            phantom: false,
                        };
                        walk.push(ctx, width, &mut groups, item, false);
                    }
                }
                stack.extend(children.iter().rev().map(|&child| Step::Visit(child)));
            }
            _ => {}
        }
    }

    if !had_break && blocks.is_empty() && groups.iter().all(|group| group.is_empty()) {
        let height = margins.incoming.unwrap_or(0.0);
        return (Vec::new(), height, None);
    }

    let alignment = map_align(parent_style.text_align);
    let mut indices = Vec::new();
    let mut cursor_y = y;

    // An empty group a break ended is a blank line: every group is a line
    // but the last, which a trailing break leaves empty. (A lone `<br>`
    // so still gives an otherwise empty container a line's height.)
    let last_group = groups.len() - 1;

    // The margin waiting below the last block: it collapses with the next
    // block's, and a line applies it in full.
    let mut pending: Option<(f32, NodeId)> = None;
    let mut incoming = margins.incoming;
    for (group_index, items) in groups.iter().enumerate() {
        for &(_, block) in blocks.iter().filter(|(before, _)| *before == group_index) {
            let margin = margin::effective_margins(ctx, block, width);
            let above = pending.take().map(|(margin, _)| margin).or(incoming.take());
            let gap = match above {
                Some(above) => margin::collapse(above, margin.top),
                None => margin.top,
            };
            if margins.collapsed_top != Some(block) {
                cursor_y += gap;
            }
            cursor_y = cursor_y.max(ctx.floats().clearance(ctx.style(block).clear, cursor_y));
            let available = flow_width(ctx, block, width);
            let height = ctx.layout_element(block, x + margin.left, cursor_y, available, None);
            if let Some(index) = ctx.tree_index(block) {
                apply_auto_margins(ctx, block, index, width, parent_style.text_align);
                indices.push(index);
            }
            cursor_y += height;
            pending = Some((margin.bottom, block));
        }
        // A line applies the margin above it in full.
        let mut settle = |cursor_y: &mut f32| {
            if let Some(margin) = pending.take().map(|(margin, _)| margin).or(incoming.take()) {
                *cursor_y += margin;
            }
        };
        if items.is_empty() {
            // An empty group a `<br>` ended is a blank line; one a block
            // ended is nothing.
            let ended_by_block = blocks.iter().any(|(before, _)| *before == group_index + 1);
            if !ended_by_block && group_index < last_group {
                settle(&mut cursor_y);
                cursor_y += line_height;
            }
            continue;
        }
        settle(&mut cursor_y);
        let mut start = 0;

        // Lines wrap one at a time so each line's band reflects the floats
        // that reach it: a float's bottom frees the width again for later
        // lines. With no floats the band is the full content width, as
        // before. (A line taller than `line_height` is not re-checked against
        // the floats — an accepted approximation.)
        while start < items.len() {
            let (band_left, band_right) =
                ctx.floats()
                    .band(x, width, cursor_y, cursor_y + line_height);
            let band_width = (band_right - band_left).max(0.0);
            let rest = &items[start..];
            // A line the source began keeps its leading preserved white
            // space (indentation); one a soft wrap began drops it.
            let leading = if start == 0 {
                rest[0].leading_gap()
            } else {
                0.0
            };
            let line_len = match parent_style.white_space {
                WhiteSpace::NoWrap | WhiteSpace::Pre => rest.len(),
                _ => first_line_len(rest, space, band_width - leading),
            };
            let line = &rest[..line_len];

            let spaces = line[1..]
                .iter()
                .filter(|item| item.spaced && item.preserved.is_none())
                .count();
            let used: f32 = leading
                + line.iter().map(Item::advance).sum::<f32>()
                + line[1..].iter().map(|item| item.gap(space)).sum::<f32>();
            let free = (band_width - used).max(0.0);
            let offset = noonoo_text::align(alignment, free, spaces + 1);

            // Where each item's own box starts, inside its elements' edges.
            let mut cursor_x = band_left + offset.start + leading;
            let mut at = Vec::with_capacity(line.len());
            // The space each item's white space takes on this line.
            let mut gaps = Vec::with_capacity(line.len());
            for (position, item) in line.iter().enumerate() {
                let mut gap = 0.0;
                if position > 0 {
                    gap = item.gap(space);
                    if item.spaced && item.preserved.is_none() {
                        gap += offset.gap;
                    }
                }
                // Edges around the space reorder but take the same room, so
                // the item lands in the same place either way.
                cursor_x += gap + item.lead;
                at.push(cursor_x);
                gaps.push(gap);
                cursor_x += item.width + item.trail;
            }

            // The line's baseline (CSS 2 §10.8): every inline box on it —
            // the container's strut, each inline element's, each text
            // run's — reaches its ascent plus half-leading above the
            // baseline and the rest below; the line is the tallest reach
            // up plus the tallest down. Atomic boxes are laid out first, at
            // the line's top, to find their baselines (their last line's,
            // or their bottom margin edge), then moved onto it.
            //
            // Quirks mode's line height quirk: an inline box counts only
            // when it holds text itself (or has padding or a border), so a
            // lone image's line is just the image, and small text in a
            // span doesn't bring back its block's larger strut.
            let quirks = ctx.is_quirks();
            let holds_text = |owner: Option<NodeId>| {
                line.iter().any(|item| {
                    item.atomic.is_none()
                        && !item.text.is_empty()
                        && item.chain.last().copied() == owner
                })
            };
            // A line of phantoms alone (an empty span after a `<br>`) takes
            // no height at all.
            let phantoms_only = line.iter().all(|item| item.phantom);
            let struts = !phantoms_only && (!quirks || holds_text(None));
            let (mut up, mut down) = if struts {
                ctx.inline_box(parent_style, line_height)
            } else {
                (0.0, 0.0)
            };
            // `vertical-align` raises each inline element from its parent's
            // baseline; what it holds rides along. Outer fragments come
            // first, so a parent's rise is known before its children's.
            let fragments = line_fragments(line);
            let mut rises: HashMap<NodeId, f32> = HashMap::new();
            let mut fragment_rises = Vec::with_capacity(fragments.len());
            for fragment in &fragments {
                let chain = &line[fragment.first].chain;
                let parent = fragment.depth.checked_sub(1).map(|d| chain[d]);
                let parent_rise = parent.and_then(|p| rises.get(&p).copied()).unwrap_or(0.0);
                let parent_style = parent.map_or(parent_style, |p| ctx.style(p));
                let style = ctx.style(fragment.element);
                let (above, below) = ctx.inline_box(style, ctx.line_height_of(style));
                let rise = parent_rise
                    + match vertical_align(ctx, style, parent_style, above, below) {
                        Align::Rise(rise) => rise,
                        // Not modelled for inline elements: as `baseline`.
                        Align::Top | Align::Bottom => 0.0,
                    };
                rises.insert(fragment.element, rise);
                fragment_rises.push(rise);
                let substantial = line[fragment.first..=fragment.last]
                    .iter()
                    .any(|item| !item.phantom);
                let counts = substantial && !quirks || holds_text(Some(fragment.element)) || {
                    let edges = InlineEdges::of(ctx, fragment.element, width);
                    edges.start() + edges.end() > 0.0
                        || edges.padding.top + edges.padding.bottom > 0.0
                        || edges.border.top + edges.border.bottom > 0.0
                };
                if counts {
                    up = up.max(above + rise);
                    down = down.max(below - rise);
                }
            }
            let mut atomics: Vec<Option<usize>> = vec![None; line.len()];
            // Each item's reach above its own baseline, and where it sits.
            let mut reach_up = Vec::with_capacity(line.len());
            let mut aligns = Vec::with_capacity(line.len());
            for (position, item) in line.iter().enumerate() {
                let owner = item.chain.last().copied();
                let owner_rise = owner.and_then(|o| rises.get(&o).copied()).unwrap_or(0.0);
                // How far the item reaches into the line box, where that's
                // more than its own box (a fallback face's text).
                let mut reach = None;
                let (above, below, align) = if let Some(atomic) = item.atomic {
                    let node = item.node.expect("an atomic inline has a node");
                    let (_, baseline) = ctx.layout_with_baseline(
                        node,
                        at[position] + atomic.margin_left,
                        cursor_y + atomic.margin_top,
                        atomic.available,
                    );
                    atomics[position] = ctx.tree_index(node);
                    let (above, below) = match baseline {
                        Some(baseline) => {
                            let above = atomic.margin_top + baseline;
                            (above, item.height - above)
                        }
                        None => (item.height, 0.0),
                    };
                    let parent = owner.map_or(parent_style, |o| ctx.style(o));
                    let align = match vertical_align(ctx, ctx.style(node), parent, above, below) {
                        Align::Rise(rise) => Align::Rise(owner_rise + rise),
                        edge => edge,
                    };
                    (above, below, align)
                } else {
                    let style = ctx.style(item.style_node);
                    let (above, below) = ctx.inline_box(style, ctx.line_height_of(style));
                    // A `normal` line holds the faces its text is drawn
                    // with, a taller fallback's included; the text box
                    // itself keeps its selected face's place.
                    if style.line_height.is_none() && !item.text.is_empty() {
                        let (ascent, descent) = ctx.shaper.text_metrics(
                            &item.text,
                            style.font_request(),
                            style.font_size,
                        );
                        reach = Some((above.max(ascent), below.max(descent)));
                    }
                    (above, below, Align::Rise(owner_rise))
                };
                if let Align::Rise(rise) = align
                    && !item.phantom
                {
                    let (above, below) = reach.unwrap_or((above, below));
                    up = up.max(above + rise);
                    down = down.max(below - rise);
                }
                reach_up.push(above);
                aligns.push(align);
            }
            // `top` and `bottom` boxes align with the line box, which grows
            // to hold them.
            for (k, align) in aligns.iter().enumerate() {
                if matches!(align, Align::Top | Align::Bottom) {
                    let height = line[k].height;
                    if height > up + down {
                        down = height - up;
                    }
                }
            }
            let line_h = up + down;
            // How far below the line's top a box reaching `above` over its
            // baseline, raised `rise`, starts (exactly 0 for the one that
            // sets the line's top).
            let drop = |above: f32, rise: f32| up - above - rise;
            let item_drop = |k: usize| match aligns[k] {
                Align::Rise(rise) => drop(reach_up[k], rise),
                Align::Top => 0.0,
                Align::Bottom => line_h - line[k].height,
            };

            // Each inline element's fragment on this line, outermost first so
            // it paints under what it holds, spanning its items and the edges
            // that start or end here.
            for (fragment, rise) in fragments.into_iter().zip(fragment_rises) {
                let element = fragment.element;
                let edges = InlineEdges::of(ctx, element, width);
                let (first, last) = (&line[fragment.first], &line[fragment.last]);
                let opens = fragment.depth >= first.chain.len() - first.opens;
                let closes = fragment.depth >= last.chain.len() - last.closes;
                // The edges of deeper elements opening (closing) at the same
                // item sit inside this fragment.
                let inner_start: f32 = first.chain[fragment.depth + 1..]
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| fragment.depth + 1 + k >= first.chain.len() - first.opens)
                    .map(|(_, &inner)| InlineEdges::of(ctx, inner, width).start())
                    .sum();
                let inner_end: f32 = last.chain[fragment.depth + 1..]
                    .iter()
                    .enumerate()
                    .filter(|(k, _)| fragment.depth + 1 + k >= last.chain.len() - last.closes)
                    .map(|(_, &inner)| InlineEdges::of(ctx, inner, width).end())
                    .sum();
                // An element holding the space before its first item, or
                // after its last, spans it.
                let opened_from = first.chain.len() - first.opens;
                let holds_leading = opens && fragment.depth < opened_from + first.straddle.opens;
                let closed_from = last.chain.len() - last.closes;
                let next = line.get(fragment.last + 1);
                let holds_trailing = closes
                    && next.is_some_and(|next| fragment.depth < closed_from + next.straddle.closes);
                let left = at[fragment.first]
                    - inner_start
                    - if holds_leading {
                        gaps[fragment.first]
                    } else {
                        0.0
                    }
                    - if opens {
                        edges.border.left + edges.padding.left
                    } else {
                        0.0
                    };
                let right = at[fragment.last]
                    + last.width
                    + inner_end
                    + if holds_trailing {
                        gaps[fragment.last + 1]
                    } else {
                        0.0
                    }
                    + if closes {
                        edges.padding.right + edges.border.right
                    } else {
                        0.0
                    };
                let (ascent, descent) = ctx.font_metrics(ctx.style(element));
                let padding = Edges {
                    top: edges.padding.top,
                    bottom: edges.padding.bottom,
                    left: if opens { edges.padding.left } else { 0.0 },
                    right: if closes { edges.padding.right } else { 0.0 },
                };
                let border = Edges {
                    top: edges.border.top,
                    bottom: edges.border.bottom,
                    left: if opens { edges.border.left } else { 0.0 },
                    right: if closes { edges.border.right } else { 0.0 },
                };
                // The content area sits on the baseline; vertical padding
                // and borders reach outside it without making the line
                // taller (CSS 2 §10.6.1).
                let top = cursor_y + drop(ascent, rise) - padding.top - border.top;
                let index = ctx.push(LayoutBox {
                    node: Some(element),
                    style_node: element,
                    kind: BoxKind::Inline,
                    rect: Rect::new(
                        left,
                        top,
                        (right - left).max(0.0),
                        ascent
                            + descent
                            + padding.top
                            + padding.bottom
                            + border.top
                            + border.bottom,
                    ),
                    text: None,
                    margin: no_edges(),
                    padding,
                    border,
                    clips: false,
                    scroll_max_x: 0.0,
                    scroll_max_y: 0.0,
                    scroll_x: 0.0,
                    scroll_y: 0.0,
                    z_tier: 1,
                    z_index: 0,
                    children: Vec::new(),
                });
                indices.push(index);
            }

            // The items: atomic boxes, and text runs. An element's words on
            // the line merge into one run.
            let mut open_run: Option<usize> = None;
            for (position, item) in line.iter().enumerate() {
                // The item's box top: its reach above the baseline.
                let top = cursor_y + item_drop(position);
                if item.atomic.is_some() {
                    if let Some(index) = atomics[position] {
                        ctx.shift_subtree(index, 0.0, item_drop(position));
                        indices.push(index);
                    }
                    open_run = None;
                    continue;
                }
                // An empty element's holder: its fragment is the box.
                if item.node.is_none() && item.text.is_empty() {
                    open_run = None;
                    continue;
                }
                let previous = position.checked_sub(1).map(|p| &line[p]);
                let merges = item.style_node != parent_node
                    && item.lead == 0.0
                    && previous.is_some_and(|previous| {
                        previous.atomic.is_none()
                            && previous.style_node == item.style_node
                            && previous.node == item.node
                            && previous.trail == 0.0
                    });
                if merges && let Some(index) = open_run {
                    let run = &mut ctx.tree.boxes[index];
                    run.rect.width = at[position] + item.width - run.rect.x;
                    run.rect.height = run.rect.height.max(item.height);
                    if let Some(text) = run.text.as_mut() {
                        match &item.preserved {
                            Some((white, _)) => text.push_str(white),
                            None if item.spaced => text.push(' '),
                            None => {}
                        }
                        text.push_str(&item.text);
                    }
                    continue;
                }
                let index = ctx.push(LayoutBox {
                    node: item.node,
                    style_node: item.style_node,
                    kind: item.kind,
                    rect: Rect::new(at[position], top, item.width, item.height),
                    text: Some(item.text.clone()),
                    margin: no_edges(),
                    padding: no_edges(),
                    border: no_edges(),
                    clips: false,
                    scroll_max_x: 0.0,
                    scroll_max_y: 0.0,
                    scroll_x: 0.0,
                    scroll_y: 0.0,
                    z_tier: 1,
                    z_index: 0,
                    children: Vec::new(),
                });
                indices.push(index);
                open_run = Some(index);
            }
            cursor_y += line_h;
            start += line_len;
        }
    }

    // Measuring an atomic inline enqueued its out-of-flow descendants, and
    // placing it enqueued them again.
    ctx.prune_pending();
    if let Some(margin) = incoming {
        cursor_y += margin;
    }
    (indices, cursor_y - y, pending)
}

/// Table parts outside a table (CSS 2 §17.2.1): the run of them from
/// `children[*i]`, with the whitespace between, is wrapped in an anonymous
/// shrink-to-fit table at `(x, y)` in `width`, so cells sit side by side in
/// one row. Advances `*i` past the run, appends its boxes to `indices` and
/// returns its height.
///
/// Kept out of [`layout_block_children`], which recurses once per nesting
/// level: the run and the (large) default style in its frame would cost
/// stack at every level, and deep trees overflow it.
#[inline(never)]
fn layout_anonymous_table(
    ctx: &mut Ctx<'_>,
    children: &[NodeId],
    i: &mut usize,
    (x, y, width): (f32, f32, f32),
    indices: &mut Vec<usize>,
) -> f32 {
    let mut run = Vec::new();
    while let Some(&candidate) = children.get(*i) {
        if is_display_none(ctx, candidate) || is_blank_text(ctx, candidate) {
            *i += 1;
            continue;
        }
        if !is_table_part(ctx, candidate) {
            break;
        }
        run.push(candidate);
        *i += 1;
    }
    let anonymous = Box::new(ComputedStyle::default());
    let (mut placed, height) =
        crate::table::layout_table_children(ctx, &anonymous, &run, x, y, width, false);
    indices.append(&mut placed);
    height
}

/// The width to hand [`Ctx::layout_element`] for an in-flow box in a
/// `container_width` container: an auto width fills it less the box's
/// horizontal margins; a specified one resolves against the container.
pub(crate) fn flow_width(ctx: &Ctx<'_>, node: NodeId, container_width: f32) -> f32 {
    let style = ctx.style(node);
    if matches!(style.width, noonoo_style::Dimension::Auto) {
        let margin = crate::resolve::margins(&style.margin, container_width);
        (container_width - margin.left - margin.right).max(0.0)
    } else {
        container_width
    }
}

/// Auto side margins share out the room a block leaves in its
/// `container_width` container: both auto centre it, one pushes it to the
/// other side (CSS 2 §10.3.3). `index` is its laid-out box, at its left
/// margin. An auto width fills the container and leaves no room, unless
/// `max-width` narrowed it.
/// Move a laid-out block over by its `auto` side margins, or, in a
/// `-webkit-center` container (`container_align`), centre one that has none.
fn apply_auto_margins(
    ctx: &mut Ctx<'_>,
    node: NodeId,
    index: usize,
    container_width: f32,
    container_align: TextAlign,
) {
    use noonoo_style::Dimension;
    let style = ctx.style(node);
    let mut left = matches!(style.margin.left, Dimension::Auto);
    let mut right = matches!(style.margin.right, Dimension::Auto);
    if !(left || right) {
        if container_align != TextAlign::WebkitCenter {
            return;
        }
        (left, right) = (true, true);
    }
    // Auto margins resolve to zero here, so this is the room left over.
    let margins = crate::resolve::margins(&style.margin, container_width);
    let width = ctx.tree.boxes[index].rect.width;
    let free = (container_width - width - margins.left - margins.right).max(0.0);
    let dx = match (left, right) {
        (true, true) => free / 2.0,
        (true, false) => free,
        _ => 0.0,
    };
    if dx > 0.0 {
        ctx.shift_subtree(index, dx, 0.0);
    }
}

/// Whether `node` is a table part (a cell, row or row group), which needs a
/// table around it.
pub(crate) fn is_table_part(ctx: &Ctx<'_>, node: NodeId) -> bool {
    ctx.styles.get(node).is_some_and(|style| {
        matches!(
            style.display,
            Display::TableCell | Display::TableRow | Display::TableRowGroup
        )
    })
}

/// Whether `node` is text with nothing but whitespace.
fn is_blank_text(ctx: &Ctx<'_>, node: NodeId) -> bool {
    ctx.is_text(node) && noonoo_text::trim_collapsible(&ctx.document.text_content(node)).is_empty()
}
