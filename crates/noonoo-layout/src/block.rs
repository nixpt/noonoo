//! Block and inline flow.

use noonoo_dom::{NodeData, NodeId};
use noonoo_geometry::Rect;
use noonoo_style::{ComputedStyle, Display, Edges, TextAlign, WhiteSpace};

use crate::float;
use crate::margin;
use crate::{BoxKind, Ctx, LayoutBox};

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
        TextAlign::Center => noonoo_text::Align::Center,
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
            // collapsing, so any pending margin applies in full first.
            if let Some(margin) = pending.take() {
                cursor += margin;
            }
            if !run.is_empty() {
                let (mut placed, height) = layout_inline_run(
                    ctx,
                    parent_node,
                    &run,
                    content_x,
                    cursor,
                    content_width,
                    parent_style,
                );
                indices.append(&mut placed);
                cursor += height;
            }
            has_content = true;
            continue;
        }

        let margin = margin::effective_margins(ctx, child, content_width);
        // `clear` sits below the floats it names before anything else happens.
        cursor = cursor.max(ctx.floats().clearance(ctx.style(child).clear, cursor));

        if margin.through {
            // A collapsed-through box adds its margin to the chain and occupies
            // no space; it still gets a (zero-height) box for the tree.
            pending = Some(match pending {
                Some(previous) => margin::collapse(previous, margin.top),
                None => margin.top,
            });
            let available = flow_width(ctx, child, content_width);
            ctx.layout_element(child, content_x + margin.left, cursor, available, None);
            if let Some(index) = ctx.tree_index(child) {
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
            apply_auto_margins(ctx, child, index, content_width);
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

    // Floats count toward the container's auto height even though block
    // siblings flow right past them — but only this container's own floats,
    // not ones inherited from the surrounding formatting context.
    let flow_height = cursor - content_y;
    (
        indices,
        flow_height.max(ctx.floats().depth_since(float_base, content_y)),
    )
}

/// A word or atomic inline box, ready to be wrapped onto a line.
struct Item {
    node: NodeId,
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
}

/// How many of `items` fit on the first line of `width`: greedy breaking at
/// the break opportunities, where an item isn't glued to the one before.
/// A run of glued items is placed whole; the first always fits.
fn first_line_len(items: &[Item], space: f32, width: f32) -> usize {
    // Clusters: (item count, width), split where a break may go.
    let mut clusters: Vec<(usize, f32)> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let gap = if item.spaced { space } else { 0.0 };
        match clusters.last_mut() {
            Some((count, used)) if index > 0 && item.glued => {
                *count += 1;
                *used += gap + item.width;
            }
            _ => clusters.push((1, item.width)),
        }
    }
    // Clusters after the first are separated by the space a break replaces
    // (every unglued item but one with no white space before is spaced).
    let gaps: Vec<f32> = {
        let mut start = 0;
        clusters
            .iter()
            .map(|&(count, _)| {
                let gap = if items[start].spaced { space } else { 0.0 };
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn layout_inline_run(
    ctx: &mut Ctx<'_>,
    parent_node: NodeId,
    nodes: &[NodeId],
    x: f32,
    y: f32,
    width: f32,
    parent_style: &ComputedStyle,
) -> (Vec<usize>, f32) {
    let space = ctx.advance(" ", parent_style);
    let line_height = ctx.line_height_of(parent_style);

    // The run's items, split into groups at each `<br>`: a group starts a
    // fresh line, and an empty group is the blank line between two breaks.
    let mut groups: Vec<Vec<Item>> = vec![Vec::new()];
    let mut had_break = false;
    // Whether white space came since the last item: a space goes before
    // the next one, and the line may break there.
    let mut spaced = false;
    // CSS Text: a line may break on either side of an atomic inline, white
    // space or not. Set after one, for the item that follows.
    let mut after_atomic = false;
    // The inline content in document order, each node with the inline
    // element it sits in (`None` at the top). A worklist, not recursion:
    // deep inline nesting mustn't cost stack per level.
    let mut stack: Vec<(NodeId, Option<NodeId>)> =
        nodes.iter().rev().map(|&node| (node, None)).collect();
    while let Some((node, owner)) = stack.pop() {
        if is_display_none(ctx, node) {
            continue;
        }
        if ctx.is_float(node) {
            float::place(ctx, node, x, width, y);
            continue;
        }
        if owner.is_some() && ctx.is_out_of_flow(node) {
            ctx.pending.push(node);
            continue;
        }
        match ctx.document.node(node) {
            Some(NodeData::Text(text)) => {
                // Text in an inline element is that element's: its style,
                // its box. Loose text is the container's.
                let (style, item_node, kind) = match owner {
                    None => (parent_style, node, BoxKind::Text),
                    Some(element) => (ctx.style(element), element, BoxKind::Inline),
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
                        spaced = false;
                    }
                    if line.starts_with(char::is_whitespace) {
                        spaced = true;
                    }
                    for (index, word) in line.split_whitespace().enumerate() {
                        let first = index == 0;
                        groups
                            .last_mut()
                            .expect("the first group always exists")
                            .push(Item {
                                node: item_node,
                                style_node,
                                kind,
                                text: word.to_owned(),
                                width: ctx.advance(word, style),
                                height,
                                atomic: None,
                                spaced: !first || spaced,
                                glued: if first {
                                    !spaced && !after_atomic
                                } else {
                                    unbreakable
                                },
                            });
                        spaced = false;
                        after_atomic = false;
                    }
                    if line.ends_with(char::is_whitespace) {
                        spaced = true;
                    }
                }
            }
            Some(NodeData::Element(element)) => {
                if element.name == "br" {
                    had_break = true;
                    groups.push(Vec::new());
                    spaced = false;
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
                    groups
                        .last_mut()
                        .expect("the first group always exists")
                        .push(Item {
                            node,
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
                            spaced,
                            glued: false,
                        });
                    spaced = false;
                    after_atomic = true;
                    continue;
                }
                let children = ctx.document.tree().children(node);
                if children.is_empty() {
                    // Replaced content (`<img alt>`, `<input value>`) shows
                    // its placeholder text, like a browser while the source
                    // is missing.
                    let text = element.placeholder_text();
                    if !text.is_empty() {
                        groups
                            .last_mut()
                            .expect("the first group always exists")
                            .push(Item {
                                node,
                                style_node: node,
                                kind: BoxKind::Inline,
                                text: text.to_owned(),
                                width: ctx.advance(text, &style),
                                height: ctx.line_height_of(&style),
                                atomic: None,
                                spaced,
                                glued: !spaced && !after_atomic,
                            });
                        spaced = false;
                        after_atomic = false;
                    }
                    continue;
                }
                stack.extend(children.iter().rev().map(|&child| (child, Some(node))));
            }
            _ => {}
        }
    }

    if groups.iter().all(|group| group.is_empty()) {
        // No content at all — but a lone `<br>` still generates one (empty)
        // line box; that is the classic trick that gives an otherwise empty
        // container a line's height.
        return (Vec::new(), if had_break { line_height } else { 0.0 });
    }

    let alignment = map_align(parent_style.text_align);
    let mut indices = Vec::new();
    let mut cursor_y = y;

    // An empty group between two breaks is a blank line — but only when
    // content follows, so the run's trailing break collapses.
    let last_holding = groups.iter().rposition(|group| !group.is_empty());

    for (group_index, items) in groups.iter().enumerate() {
        if items.is_empty() {
            if last_holding.is_some_and(|last| last > group_index) {
                cursor_y += line_height;
            }
            continue;
        }
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
            let line_len = match parent_style.white_space {
                WhiteSpace::NoWrap | WhiteSpace::Pre => rest.len(),
                _ => first_line_len(rest, space, band_width),
            };
            let line: Vec<usize> = (0..line_len).collect();

            let spaces = rest[1..line_len].iter().filter(|item| item.spaced).count();
            let used: f32 =
                rest[..line_len].iter().map(|item| item.width).sum::<f32>() + space * spaces as f32;
            let free = (band_width - used).max(0.0);
            let offset = noonoo_text::align(alignment, free, spaces + 1);

            let mut cursor_x = band_left + offset.start;
            let mut line_h = line_height;
            // The element run just placed on this line, which the element's
            // next word extends.
            let mut open_run: Option<(NodeId, usize)> = None;
            for (position, &i) in line.iter().enumerate() {
                let item = &items[start + i];
                if position > 0 && item.spaced {
                    cursor_x += space + offset.gap;
                }
                line_h = line_h.max(item.height);
                if let Some(atomic) = item.atomic {
                    ctx.layout_element(
                        item.node,
                        cursor_x + atomic.margin_left,
                        cursor_y + atomic.margin_top,
                        atomic.available,
                        None,
                    );
                    if let Some(index) = ctx.tree_index(item.node) {
                        indices.push(index);
                    }
                    cursor_x += item.width;
                    open_run = None;
                    continue;
                }
                if item.kind == BoxKind::Inline
                    && let Some((run_node, index)) = open_run
                    && run_node == item.node
                {
                    let run = &mut ctx.tree.boxes[index];
                    run.rect.width = cursor_x + item.width - run.rect.x;
                    run.rect.height = run.rect.height.max(item.height);
                    if let Some(text) = run.text.as_mut() {
                        if item.spaced {
                            text.push(' ');
                        }
                        text.push_str(&item.text);
                    }
                    cursor_x += item.width;
                    continue;
                }
                let index = ctx.push(LayoutBox {
                    node: Some(item.node),
                    style_node: item.style_node,
                    kind: item.kind,
                    rect: Rect::new(cursor_x, cursor_y, item.width, item.height),
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
                open_run = (item.kind == BoxKind::Inline).then_some((item.node, index));
                cursor_x += item.width;
            }
            cursor_y += line_h;
            start += line.len();
        }
    }

    // Measuring an atomic inline enqueued its out-of-flow descendants, and
    // placing it enqueued them again.
    ctx.prune_pending();
    (indices, cursor_y - y)
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
        crate::table::layout_table_children(ctx, &anonymous, &run, x, y, width);
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
fn apply_auto_margins(ctx: &mut Ctx<'_>, node: NodeId, index: usize, container_width: f32) {
    use noonoo_style::Dimension;
    let style = ctx.style(node);
    let left = matches!(style.margin.left, Dimension::Auto);
    let right = matches!(style.margin.right, Dimension::Auto);
    if !(left || right) {
        return;
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
    ctx.is_text(node) && ctx.document.text_content(node).trim().is_empty()
}
