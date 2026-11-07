//! Block and inline flow.

use noonoo_dom::{NodeData, NodeId};
use noonoo_geometry::Rect;
use noonoo_style::{ComputedStyle, Display, Edges, TextAlign, WhiteSpace};

use crate::float;
use crate::margin;
use crate::{BoxKind, Ctx, LayoutBox};

fn no_edges() -> Edges<f32> {
    Edges::all(0.0)
}

fn is_display_none(ctx: &Ctx<'_>, node: NodeId) -> bool {
    ctx.styles
        .get(node)
        .is_some_and(|style| style.display == Display::None)
}

fn map_align(align: TextAlign) -> noonoo_text::Align {
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
            ctx.layout_element(child, content_x + margin.left, cursor, content_width, None);
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

        let height =
            ctx.layout_element(child, content_x + margin.left, cursor, content_width, None);
        if let Some(index) = ctx.tree_index(child) {
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
    width: f32,
    height: f32,
}

/// Lay out an inline formatting context: text and inline boxes wrapped into
/// lines inside the band the container's floats leave free, aligned per the
/// container's `text-align`.
///
/// Float children among the inline nodes are laid out first (at `y`), so the
/// lines that follow wrap around them.
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
    let font = parent_style.font_size;
    let request = parent_style.font_request();
    let space = ctx.shaper.advance(" ", request, font);
    let line_height = if parent_style.line_height > 0.0 {
        parent_style.line_height
    } else {
        ctx.shaper.line_height(request, font)
    };

    let mut items: Vec<Item> = Vec::new();
    for &node in nodes {
        if is_display_none(ctx, node) {
            continue;
        }
        if ctx.is_float(node) {
            float::place(ctx, node, x, width, y);
            continue;
        }
        match ctx.document.node(node) {
            Some(NodeData::Text(text)) => {
                for word in text.split_whitespace() {
                    items.push(Item {
                        node,
                        style_node: parent_node,
                        kind: BoxKind::Text,
                        text: word.to_owned(),
                        width: ctx.shaper.advance(word, request, font),
                        height: line_height,
                    });
                }
            }
            Some(NodeData::Element(_)) => {
                let style = ctx.style(node).clone();
                let content = ctx.document.text_content(node);
                let height = if style.line_height > 0.0 {
                    style.line_height
                } else {
                    ctx.shaper
                        .line_height(style.font_request(), style.font_size)
                };
                items.push(Item {
                    node,
                    style_node: node,
                    kind: BoxKind::Inline,
                    width: ctx
                        .shaper
                        .advance(&content, style.font_request(), style.font_size),
                    text: content,
                    height,
                });
            }
            _ => {}
        }
    }

    if items.is_empty() {
        return (Vec::new(), 0.0);
    }

    let alignment = map_align(parent_style.text_align);
    let mut indices = Vec::new();
    let mut cursor_y = y;
    let mut start = 0;

    // Lines wrap one at a time so each line's band reflects the floats that
    // reach it: a float's bottom frees the width again for later lines. With
    // no floats the band is the full content width, as before. (A line taller
    // than `line_height` is not re-checked against the floats — an accepted
    // approximation.)
    while start < items.len() {
        let (band_left, band_right) = ctx
            .floats()
            .band(x, width, cursor_y, cursor_y + line_height);
        let band_width = (band_right - band_left).max(0.0);
        let widths: Vec<f32> = items[start..].iter().map(|item| item.width).collect();
        let wrapped = match parent_style.white_space {
            WhiteSpace::NoWrap | WhiteSpace::Pre => {
                vec![(0..widths.len()).collect::<Vec<usize>>()]
            }
            _ => noonoo_text::wrap(&widths, space, band_width),
        };
        let line = &wrapped[0];

        let used: f32 = line.iter().map(|&i| items[start + i].width).sum::<f32>()
            + space * line.len().saturating_sub(1) as f32;
        let free = (band_width - used).max(0.0);
        let offset = noonoo_text::align(alignment, free, line.len());

        let mut cursor_x = band_left + offset.start;
        let mut line_h = line_height;
        for (position, &i) in line.iter().enumerate() {
            if position > 0 {
                cursor_x += space + offset.gap;
            }
            let item = &items[start + i];
            line_h = line_h.max(item.height);
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
            cursor_x += item.width;
        }
        cursor_y += line_h;
        start += line.len();
    }

    (indices, cursor_y - y)
}
