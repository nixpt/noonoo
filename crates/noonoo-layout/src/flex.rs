//! Flexbox: row and column, grow/shrink, `justify-content`, gaps.
//!
//! v1 does not wrap and places items on the cross-start edge (no
//! `align-items` cross positioning yet); column flex stacks natural heights.

use noonoo_dom::NodeId;
use noonoo_style::{ComputedStyle, Dimension, Display, FlexDirection, JustifyContent};

use crate::{Ctx, resolve};

pub(crate) fn layout_flex_children(
    ctx: &mut Ctx<'_>,
    style: &ComputedStyle,
    children: &[NodeId],
    content_x: f32,
    content_y: f32,
    content_width: f32,
) -> (Vec<usize>, f32) {
    let items: Vec<NodeId> = children
        .iter()
        .copied()
        .filter(|&child| {
            ctx.styles
                .get(child)
                .is_some_and(|s| s.display != Display::None)
        })
        .collect();

    let row = matches!(
        style.flex_direction,
        FlexDirection::Row | FlexDirection::RowReverse
    );

    if !row {
        let mut indices = Vec::new();
        let mut y = content_y;
        for &item in &items {
            let height = ctx.layout_element(item, content_x, y, content_width);
            if let Some(index) = ctx.tree_index(item) {
                indices.push(index);
            }
            y += height + style.row_gap;
        }
        return (indices, (y - content_y).max(0.0));
    }

    let count = items.len();
    if count == 0 {
        return (Vec::new(), 0.0);
    }
    let gap = style.column_gap;

    let mut bases = Vec::with_capacity(count);
    let mut grows = Vec::with_capacity(count);
    let mut shrinks = Vec::with_capacity(count);
    for &item in &items {
        let item_style = ctx.style(item);
        let basis = match item_style.flex_basis {
            Dimension::Auto => None,
            dimension => resolve::dimension(dimension, content_width),
        };
        let base = basis
            .or_else(|| resolve::dimension(item_style.width, content_width))
            .unwrap_or_else(|| {
                let text = ctx.document.text_content(item);
                ctx.measurer
                    .measure(&text, item_style.font_size, item_style.font_weight >= 600)
                    .width
            });
        bases.push(base.max(0.0));
        grows.push(item_style.flex_grow);
        shrinks.push(item_style.flex_shrink);
    }

    let gaps = gap * (count as f32 - 1.0);
    let used: f32 = bases.iter().sum::<f32>() + gaps;
    let free = content_width - used;
    if free > 0.0 {
        let total_grow: f32 = grows.iter().sum();
        if total_grow > 0.0 {
            for (i, base) in bases.iter_mut().enumerate() {
                *base += free * grows[i] / total_grow;
            }
        }
    } else if free < 0.0 {
        let weights: Vec<f32> = (0..count).map(|i| shrinks[i] * bases[i]).collect();
        let total: f32 = weights.iter().sum();
        if total > 0.0 {
            for (i, base) in bases.iter_mut().enumerate() {
                *base = (*base + free * weights[i] / total).max(0.0);
            }
        }
    }

    let remaining = (content_width - (bases.iter().sum::<f32>() + gaps)).max(0.0);
    let (start, extra) = justify_offset(style.justify_content, remaining, count);

    let mut indices = Vec::new();
    let mut x = content_x + start;
    let mut max_height = 0.0_f32;
    for (i, &item) in items.iter().enumerate() {
        ctx.layout_element(item, x, content_y, bases[i]);
        if let Some(index) = ctx.tree_index(item) {
            max_height = max_height.max(ctx.box_height(index));
            indices.push(index);
        }
        x += bases[i] + gap + extra;
    }

    (indices, max_height)
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
