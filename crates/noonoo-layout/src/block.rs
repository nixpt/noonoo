//! Block and inline flow.

use noonoo_dom::{NodeData, NodeId};
use noonoo_geometry::Rect;
use noonoo_style::{ComputedStyle, Display, Edges, TextAlign, WhiteSpace};

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
/// Returns `(child box indices, content height, leading margin, trailing
/// margin)`. When `collapse_top` is set the first block child's top margin is
/// *not* applied inside — it is returned as `leading` to collapse with the
/// parent's own top margin. `trailing` is the same for the bottom.
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
) -> (Vec<usize>, f32, f32, f32) {
    let mut indices = Vec::new();
    let mut cursor = content_y;
    let mut pending: Option<f32> = None;
    let mut leading = 0.0;
    let mut has_content = false;
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

        // A block-level child: collapse its leading margin with the pending
        // margin from the previous sibling, then place its border box.
        let margin = margin::effective_margins(ctx, child, content_width);
        let gap = match pending {
            Some(previous) => margin::collapse(previous, margin.top),
            None => margin.top,
        };
        if !has_content && collapse_top {
            // The leading margin collapses out of the container entirely.
            leading = gap;
        } else {
            cursor += gap;
        }

        let height = ctx.layout_element(child, content_x + margin.left, cursor, content_width);
        if let Some(index) = ctx.tree_index(child) {
            indices.push(index);
        }
        cursor += height;
        pending = Some(margin.bottom);
        has_content = true;
        i += 1;
    }

    let trailing = match pending {
        Some(margin) if collapse_bottom => margin,
        Some(margin) => {
            cursor += margin;
            0.0
        }
        None => 0.0,
    };

    (indices, cursor - content_y, leading, trailing)
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
/// lines no wider than `width`, aligned per the container's `text-align`.
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
    let bold = parent_style.font_weight >= 600;
    let space = ctx.shaper.advance(" ", font, bold);
    let line_height = if parent_style.line_height > 0.0 {
        parent_style.line_height
    } else {
        ctx.shaper.line_height(font)
    };

    let mut items: Vec<Item> = Vec::new();
    for &node in nodes {
        if is_display_none(ctx, node) {
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
                        width: ctx.shaper.advance(word, font, bold),
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
                    ctx.shaper.line_height(style.font_size)
                };
                items.push(Item {
                    node,
                    style_node: node,
                    kind: BoxKind::Inline,
                    width: ctx
                        .shaper
                        .advance(&content, style.font_size, style.font_weight >= 600),
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

    let widths: Vec<f32> = items.iter().map(|item| item.width).collect();
    let lines = match parent_style.white_space {
        WhiteSpace::NoWrap | WhiteSpace::Pre => vec![(0..items.len()).collect::<Vec<usize>>()],
        _ => noonoo_text::wrap(&widths, space, width),
    };

    let alignment = map_align(parent_style.text_align);
    let mut indices = Vec::new();
    let mut cursor_y = y;

    for line in &lines {
        let used: f32 = line.iter().map(|&i| items[i].width).sum::<f32>()
            + space * line.len().saturating_sub(1) as f32;
        let free = (width - used).max(0.0);
        let offset = noonoo_text::align(alignment, free, line.len());

        let mut cursor_x = x + offset.start;
        let mut line_h = line_height;
        for (position, &i) in line.iter().enumerate() {
            if position > 0 {
                cursor_x += space + offset.gap;
            }
            let item = &items[i];
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
                children: Vec::new(),
            });
            indices.push(index);
            cursor_x += item.width;
        }
        cursor_y += line_h;
    }

    (indices, cursor_y - y)
}
