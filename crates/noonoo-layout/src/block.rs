//! Block and inline flow.

use noonoo_dom::{NodeData, NodeId};
use noonoo_geometry::Rect;
use noonoo_style::{ComputedStyle, Display, Edges, TextAlign, WhiteSpace};

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

/// Lay out the block-and-inline children of a box.
///
/// Consecutive inline-level children (text and inline elements) form one inline
/// formatting context; block-level children are stacked. Returns the placed
/// child indices and the content height.
pub(crate) fn layout_block_children(
    ctx: &mut Ctx<'_>,
    children: &[NodeId],
    content_x: f32,
    content_y: f32,
    content_width: f32,
    parent_style: &ComputedStyle,
) -> (Vec<usize>, f32) {
    let mut indices = Vec::new();
    let mut y = content_y;
    let mut i = 0;

    while i < children.len() {
        let child = children[i];
        if is_display_none(ctx, child) {
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
                if !ctx.is_inline_level(candidate) {
                    break;
                }
                run.push(candidate);
                i += 1;
            }
            if !run.is_empty() {
                let (mut placed, height) =
                    layout_inline_run(ctx, &run, content_x, y, content_width, parent_style);
                indices.append(&mut placed);
                y += height;
            }
        } else {
            let margin_height = ctx.layout_element(child, content_x, y, content_width);
            if let Some(index) = ctx.tree_index(child) {
                indices.push(index);
            }
            y += margin_height;
            i += 1;
        }
    }

    (indices, y - content_y)
}

/// A word or atomic inline box, ready to be wrapped onto a line.
struct Item {
    node: NodeId,
    kind: BoxKind,
    width: f32,
    height: f32,
}

/// Lay out an inline formatting context: text and inline boxes wrapped into
/// lines no wider than `width`, then aligned per the container's `text-align`.
pub(crate) fn layout_inline_run(
    ctx: &mut Ctx<'_>,
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
                        kind: BoxKind::Text,
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
                    kind: BoxKind::Inline,
                    width: ctx
                        .shaper
                        .advance(&content, style.font_size, style.font_weight >= 600),
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
                kind: item.kind,
                rect: Rect::new(cursor_x, cursor_y, item.width, item.height),
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
