//! Anonymous text runs: sizing and line-box generation for loose text that a
//! grid or flex container wraps into an *anonymous item*.
//!
//! CSS puts loose text in a flex/grid container into one anonymous item per
//! run of text; the item has no DOM element, so its style is the container's.
//! These helpers share that job across containers: one width measure (per
//! `white-space`), one height measure, and one line-box generator, so each
//! layout mode can't drift into its own dialect.

use noonoo_dom::NodeId;
use noonoo_geometry::Rect;
use noonoo_style::{ComputedStyle, Edges, WhiteSpace};

use crate::{BoxKind, Ctx, LayoutBox, block, float};

/// The run's natural width in `style`'s font. `nowrap`/`pre` are unwrapped;
/// other `white-space` values shrink-to-fit within `available`.
pub(crate) fn width(ctx: &Ctx<'_>, text: &str, style: &ComputedStyle, available: f32) -> f32 {
    if matches!(style.white_space, WhiteSpace::NoWrap | WhiteSpace::Pre) {
        float::text_width(ctx, text, style)
    } else {
        float::text_width(ctx, text, style).min(available).max(0.0)
    }
}

/// The run's height wrapped to `available` in `style`'s line height.
pub(crate) fn height(ctx: &Ctx<'_>, text: &str, style: &ComputedStyle, available: f32) -> f32 {
    let request = style.font_request();
    let space = ctx.advance(" ", style);
    let line_height = if style.line_height > 0.0 {
        style.line_height
    } else {
        ctx.shaper.line_height(request, style.font_size)
    };

    // Each preserved line takes at least one line box.
    let pieces = crate::forced_lines(text, style.white_space);
    if pieces.len() > 1 {
        return pieces
            .into_iter()
            .map(|piece| height(ctx, piece, style, available).max(line_height))
            .sum();
    }
    if matches!(style.white_space, WhiteSpace::NoWrap | WhiteSpace::Pre) {
        return line_height;
    }
    let widths: Vec<f32> = text
        .split_whitespace()
        .map(|word| ctx.advance(word, style))
        .collect();
    let lines = noonoo_text::wrap(&widths, space, available);
    lines.len() as f32 * line_height
}

/// Lay a run out at `(x, y)` inside `available_width` as one [`BoxKind::Text`]
/// box per wrapped line, aligned per the container's `text-align`.
///
/// `node` is the run's paint/a11y anchor — the first text child of the run —
/// and `parent` is the container whose style governs the box. Returns the
/// pushed box indices in reading order.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lines(
    ctx: &mut Ctx<'_>,
    node: NodeId,
    parent: NodeId,
    text: &str,
    style: &ComputedStyle,
    x: f32,
    y: f32,
    available_width: f32,
) -> Vec<usize> {
    let request = style.font_request();
    let font_size = style.font_size;
    let space = ctx.advance(" ", style);
    let line_height = if style.line_height > 0.0 {
        style.line_height
    } else {
        ctx.shaper.line_height(request, font_size)
    };
    // Preserved newlines: lay each line out below the last.
    let pieces = crate::forced_lines(text, style.white_space);
    if pieces.len() > 1 {
        let mut indices = Vec::new();
        let mut cursor_y = y;
        for piece in pieces {
            indices.extend(lines(
                ctx,
                node,
                parent,
                piece,
                style,
                x,
                cursor_y,
                available_width,
            ));
            cursor_y += height(ctx, piece, style, available_width).max(line_height);
        }
        return indices;
    }
    let alignment = block::map_align(style.text_align);
    let nowrap = matches!(style.white_space, WhiteSpace::NoWrap | WhiteSpace::Pre);

    let mut indices = Vec::new();
    let mut cursor_y = y;

    if nowrap {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let used = ctx.advance(&text, style);
        push_text_box(
            ctx,
            node,
            parent,
            &text,
            Rect::new(x, cursor_y, used, line_height),
            &mut indices,
        );
        return indices;
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let widths: Vec<f32> = words.iter().map(|word| ctx.advance(word, style)).collect();
    let lines = noonoo_text::wrap(&widths, space, available_width);
    for line in &lines {
        let used: f32 = line.iter().map(|&i| widths[i]).sum::<f32>()
            + space * (line.len().saturating_sub(1)) as f32;
        let free = (available_width - used).max(0.0);
        let offset = noonoo_text::align(alignment, free, line.len());
        let line_text = line.iter().map(|&i| words[i]).collect::<Vec<_>>().join(" ");
        push_text_box(
            ctx,
            node,
            parent,
            &line_text,
            Rect::new(x + offset.start, cursor_y, used, line_height),
            &mut indices,
        );
        cursor_y += line_height;
    }
    indices
}

fn push_text_box(
    ctx: &mut Ctx<'_>,
    node: NodeId,
    parent: NodeId,
    text: &str,
    rect: Rect,
    indices: &mut Vec<usize>,
) {
    let index = ctx.push(LayoutBox {
        node: Some(node),
        style_node: parent,
        kind: BoxKind::Text,
        rect,
        text: Some(text.to_owned()),
        margin: Edges::all(0.0),
        padding: Edges::all(0.0),
        border: Edges::all(0.0),
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
