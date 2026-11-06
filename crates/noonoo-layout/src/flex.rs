//! Flexbox: row and column, grow/shrink, `justify-content`, gaps, and — for a
//! row — `flex-wrap` (items spill onto additional lines).
//!
//! v1 places items on the cross-start edge (no `align-items` cross positioning
//! yet); column flex stacks natural heights.

use noonoo_dom::NodeId;
use noonoo_style::{ComputedStyle, Dimension, FlexDirection, FlexWrap, JustifyContent};

use crate::{Ctx, resolve};

pub(crate) fn layout_flex_children(
    ctx: &mut Ctx<'_>,
    style: &ComputedStyle,
    children: &[NodeId],
    content_x: f32,
    content_y: f32,
    content_width: f32,
) -> (Vec<usize>, f32) {
    let mut items: Vec<NodeId> = Vec::new();
    for &child in children {
        if ctx.is_hidden(child) {
            continue;
        }
        if ctx.is_out_of_flow(child) {
            ctx.pending.push(child);
            continue;
        }
        items.push(child);
    }

    if items.is_empty() {
        return (Vec::new(), 0.0);
    }

    let row = matches!(
        style.flex_direction,
        FlexDirection::Row | FlexDirection::RowReverse
    );
    if !row {
        return layout_column(ctx, style, &items, content_x, content_y, content_width);
    }
    layout_row(ctx, style, &items, content_x, content_y, content_width)
}

fn layout_column(
    ctx: &mut Ctx<'_>,
    style: &ComputedStyle,
    items: &[NodeId],
    content_x: f32,
    content_y: f32,
    content_width: f32,
) -> (Vec<usize>, f32) {
    let mut indices = Vec::new();
    let mut y = content_y;
    for &item in items {
        let margin = resolve::margins(&ctx.style(item).margin, content_width);
        let height =
            ctx.layout_element(item, content_x + margin.left, y + margin.top, content_width);
        if let Some(index) = ctx.tree_index(item) {
            indices.push(index);
        }
        y += margin.top + height + margin.bottom + style.row_gap;
    }
    (indices, (y - content_y).max(0.0))
}

fn layout_row(
    ctx: &mut Ctx<'_>,
    style: &ComputedStyle,
    items: &[NodeId],
    content_x: f32,
    content_y: f32,
    content_width: f32,
) -> (Vec<usize>, f32) {
    let count = items.len();
    let gap = style.column_gap;

    // Base main sizes and flex factors.
    let mut bases = Vec::with_capacity(count);
    let mut grows = Vec::with_capacity(count);
    let mut shrinks = Vec::with_capacity(count);
    for &item in items {
        let item_style = ctx.style(item);
        let basis = match item_style.flex_basis {
            Dimension::Auto => None,
            dimension => resolve::dimension(dimension, content_width),
        };
        let base = basis
            .or_else(|| resolve::dimension(item_style.width, content_width))
            .unwrap_or_else(|| {
                let text = ctx.document.text_content(item);
                ctx.shaper
                    .advance(&text, item_style.font_size, item_style.font_weight >= 600)
            });
        bases.push(base.max(0.0));
        grows.push(item_style.flex_grow);
        shrinks.push(item_style.flex_shrink);
    }

    // Partition into lines when wrapping is on.
    let mut lines: Vec<Vec<usize>> = Vec::new();
    if style.flex_wrap == FlexWrap::NoWrap {
        lines.push((0..count).collect());
    } else {
        let mut current: Vec<usize> = Vec::new();
        let mut used = 0.0;
        for (i, &base) in bases.iter().enumerate() {
            let addition = if current.is_empty() { base } else { gap + base };
            if !current.is_empty() && used + addition > content_width {
                lines.push(std::mem::take(&mut current));
                used = base;
            } else {
                used += addition;
            }
            current.push(i);
        }
        if !current.is_empty() {
            lines.push(current);
        }
    }

    let mut indices = Vec::new();
    let mut cursor_y = content_y;

    for line in &lines {
        let n = line.len();
        let line_gaps = gap * n.saturating_sub(1) as f32;
        let mut line_bases: Vec<f32> = line.iter().map(|&i| bases[i]).collect();
        let free = content_width - line_bases.iter().sum::<f32>() - line_gaps;

        if free > 0.0 {
            let total_grow: f32 = line.iter().map(|&i| grows[i]).sum();
            if total_grow > 0.0 {
                for (k, &i) in line.iter().enumerate() {
                    line_bases[k] += free * grows[i] / total_grow;
                }
            }
        } else if free < 0.0 {
            let weights: Vec<f32> = line.iter().map(|&i| shrinks[i] * bases[i]).collect();
            let total: f32 = weights.iter().sum();
            if total > 0.0 {
                for (k, weight) in weights.iter().enumerate() {
                    line_bases[k] = (line_bases[k] + free * weight / total).max(0.0);
                }
            }
        }

        let used: f32 = line_bases.iter().sum::<f32>() + line_gaps;
        let remaining = (content_width - used).max(0.0);
        let (start, extra) = justify_offset(style.justify_content, remaining, n);

        let mut x = content_x + start;
        let mut line_height = 0.0_f32;
        for (k, &i) in line.iter().enumerate() {
            let margin = resolve::margins(&ctx.style(items[i]).margin, content_width);
            ctx.layout_element(
                items[i],
                x + margin.left,
                cursor_y + margin.top,
                line_bases[k],
            );
            if let Some(index) = ctx.tree_index(items[i]) {
                line_height = line_height.max(ctx.box_height(index));
                indices.push(index);
            }
            x += line_bases[k] + gap + extra;
        }
        cursor_y += line_height + style.row_gap;
    }

    (indices, (cursor_y - content_y - style.row_gap).max(0.0))
}

fn justify_offset(justify: JustifyContent, free: f32, count: usize) -> (f32, f32) {
    let n = count as f32;
    match justify {
        JustifyContent::Start => (0.0, 0.0),
        JustifyContent::End => (free, 0.0),
        JustifyContent::Center => (free / 2.0, 0.0),
        JustifyContent::SpaceBetween => (0.0, if count > 1 { free / (n - 1.0) } else { 0.0 }),
        JustifyContent::SpaceAround => (free / (2.0 * n), free / n),
        JustifyContent::SpaceEvenly => {
            let gap = free / (n + 1.0);
            (gap, gap)
        }
    }
}
