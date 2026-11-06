//! Block and inline flow.

use noonoo_dom::{NodeData, NodeId};
use noonoo_geometry::Rect;
use noonoo_style::{Display, Edges};

use crate::{BoxKind, Ctx, LayoutBox};

/// Zero edges, for boxes that carry no box model of their own (text, inline).
fn no_edges() -> Edges<f32> {
    Edges::all(0.0)
}

fn is_display_none(ctx: &Ctx<'_>, node: NodeId) -> bool {
    ctx.styles
        .get(node)
        .is_some_and(|style| style.display == Display::None)
}

/// Lay out the block-and-inline children of a box into `content`.
///
/// Consecutive inline-level children (text and inline elements) are grouped into
/// one inline formatting context (an anonymous block); block-level children are
/// stacked. Returns the placed child indices and the content height.
pub(crate) fn layout_block_children(
    ctx: &mut Ctx<'_>,
    children: &[NodeId],
    content_x: f32,
    content_y: f32,
    content_width: f32,
    font_size: f32,
    bold: bool,
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
                    layout_inline_run(ctx, &run, content_x, y, content_width, font_size, bold);
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

/// Lay out an inline formatting context: text and inline boxes wrapped into
/// lines no wider than `width`.
pub(crate) fn layout_inline_run(
    ctx: &mut Ctx<'_>,
    nodes: &[NodeId],
    x: f32,
    y: f32,
    width: f32,
    font_size: f32,
    bold: bool,
) -> (Vec<usize>, f32) {
    let space = ctx.measurer.measure(" ", font_size, bold).width;
    let mut cursor_x = x;
    let mut line_y = y;
    let mut line_height = font_size * 1.2;
    let mut indices = Vec::new();

    let place = |ctx: &mut Ctx<'_>,
                 node: NodeId,
                 kind: BoxKind,
                 run_width: f32,
                 run_height: f32,
                 lead: f32,
                 indices: &mut Vec<usize>,
                 cursor_x: &mut f32,
                 line_y: &mut f32,
                 line_height: &mut f32| {
        let mut lead = lead;
        if *cursor_x + lead + run_width > x + width && *cursor_x > x {
            *line_y += *line_height;
            *cursor_x = x;
            *line_height = run_height;
            lead = 0.0;
        }
        *cursor_x += lead;
        let rect = Rect::new(*cursor_x, *line_y, run_width, run_height);
        let index = ctx.push(LayoutBox {
            node: Some(node),
            kind,
            rect,
            margin: no_edges(),
            padding: no_edges(),
            border: no_edges(),
            children: Vec::new(),
        });
        indices.push(index);
        *cursor_x += run_width;
        *line_height = line_height.max(run_height);
    };

    for &node in nodes {
        if is_display_none(ctx, node) {
            continue;
        }
        match ctx.document.node(node) {
            Some(NodeData::Text(text)) => {
                let text = text.clone();
                let mut first = true;
                for word in text.split_whitespace() {
                    let metrics = ctx.measurer.measure(word, font_size, bold);
                    let lead = if first { 0.0 } else { space };
                    place(
                        ctx,
                        node,
                        BoxKind::Text,
                        metrics.width,
                        metrics.height,
                        lead,
                        &mut indices,
                        &mut cursor_x,
                        &mut line_y,
                        &mut line_height,
                    );
                    first = false;
                }
            }
            Some(NodeData::Element(_)) => {
                let style = ctx.style(node).clone();
                let text = ctx.document.text_content(node);
                let metrics =
                    ctx.measurer
                        .measure(&text, style.font_size, style.font_weight >= 600);
                place(
                    ctx,
                    node,
                    BoxKind::Inline,
                    metrics.width,
                    metrics.height,
                    0.0,
                    &mut indices,
                    &mut cursor_x,
                    &mut line_y,
                    &mut line_height,
                );
            }
            _ => {}
        }
    }

    let height = (line_y + line_height) - y;
    (indices, height)
}
