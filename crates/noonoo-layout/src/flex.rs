//! Flexbox: row and column, grow/shrink, `justify-content`, gaps, and — for a
//! row — `flex-wrap` (items spill onto additional lines).
//!
//! Rows align items on the cross axis per `align-items`/`align-self`:
//! `stretch` (the default) fills the line height when the item's height is
//! auto — an explicit height wins — and `start`/`center`/`end` position the
//! item (`baseline` behaves as `start`). Column flex stretches items across
//! the container as always; cross positioning on columns is not applied yet.
//! Line heights come from a measure pass (real layout, discarded) so items
//! can be positioned within them.

use noonoo_dom::NodeId;
use noonoo_style::{
    AlignItems, BoxSizing, ComputedStyle, Dimension, FlexDirection, FlexWrap, JustifyContent,
};

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
        let height = ctx.layout_element(
            item,
            content_x + margin.left,
            y + margin.top,
            content_width,
            None,
        );
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
    let mut minimums = Vec::with_capacity(count);
    let mut maximums = Vec::with_capacity(count);
    for &item in items {
        let item_style = ctx.style(item);
        let basis = match &item_style.flex_basis {
            Dimension::Auto => None,
            dimension => resolve::dimension(dimension.clone(), content_width),
        };
        let base = basis
            .or_else(|| resolve::dimension(item_style.width.clone(), content_width))
            .unwrap_or_else(|| {
                let text = ctx.document.text_content(item);
                ctx.shaper
                    .advance(&text, item_style.font_request(), item_style.font_size)
            });
        // `min-width`/`max-width` clamp the flex base before grow/shrink, and
        // share `width`'s box (per `box-sizing`).
        let horizontal = {
            let padding = resolve::paddings(&item_style.padding, content_width);
            let border = resolve::borders(item_style);
            padding.left + padding.right + border.left + border.right
        };
        let to_border_box = |width: f32| match item_style.box_sizing {
            BoxSizing::BorderBox => width,
            BoxSizing::ContentBox => (width + horizontal).max(0.0),
        };
        let minimum =
            resolve::dimension(item_style.min_width.clone(), content_width).map(to_border_box);
        let maximum =
            resolve::dimension(item_style.max_width.clone(), content_width).map(to_border_box);
        bases.push(base.max(0.0));
        minimums.push(minimum);
        maximums.push(maximum);
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
                // CSS 9.7's freeze: grow, then any item clamped by its
                // min/max freezes at the bound and its share re-distributes
                // among the growers that are still free. Each pass freezes at
                // least one item, so this terminates.
                let mut sizes: Vec<f32> = line.iter().map(|&i| bases[i]).collect();
                let mut frozen = vec![false; n];
                loop {
                    let remaining =
                        (content_width - line_gaps - sizes.iter().sum::<f32>()).max(0.0);
                    if remaining <= f32::EPSILON {
                        break;
                    }
                    let growers: Vec<usize> = line
                        .iter()
                        .enumerate()
                        .filter(|&(k, &i)| !frozen[k] && grows[i] > 0.0)
                        .map(|(k, _)| k)
                        .collect();
                    if growers.is_empty() {
                        break;
                    }
                    let total: f32 = growers.iter().map(|&k| grows[line[k]]).sum();
                    if total <= 0.0 {
                        break;
                    }
                    for &k in &growers {
                        sizes[k] += remaining * grows[line[k]] / total;
                    }
                    let mut froze_any = false;
                    for &k in &growers {
                        let i = line[k];
                        if let Some(maximum) = maximums[i]
                            && sizes[k] > maximum
                        {
                            sizes[k] = maximum;
                            frozen[k] = true;
                            froze_any = true;
                        }
                        if let Some(minimum) = minimums[i]
                            && sizes[k] < minimum
                        {
                            sizes[k] = minimum;
                            frozen[k] = true;
                            froze_any = true;
                        }
                    }
                    if !froze_any {
                        break;
                    }
                }
                line_bases = sizes;
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

        // Cross sizes first: an item is positioned (and stretched) within the
        // line, so its height must be known before it is placed. The
        // measurement is real layout, thrown away.
        let mut cross = Vec::with_capacity(n);
        let mut measure_x = content_x + start;
        for (k, &i) in line.iter().enumerate() {
            let margin = resolve::margins(&ctx.style(items[i]).margin, content_width);
            cross.push(ctx.measure_height(
                items[i],
                measure_x + margin.left,
                cursor_y,
                line_bases[k],
                None,
            ));
            measure_x += line_bases[k] + gap + extra;
        }
        ctx.prune_pending();
        let line_height = cross.iter().copied().fold(0.0_f32, f32::max);

        let mut x = content_x + start;
        for (k, &i) in line.iter().enumerate() {
            let item_style = ctx.style(items[i]);
            let align = item_style.align_self.unwrap_or(style.align_items);
            let margin = resolve::margins(&item_style.margin, content_width);
            // `stretch` fills the line (when the item's height is auto);
            // otherwise the item keeps its natural height and is positioned.
            let (y, definite) = match align {
                AlignItems::Stretch => (cursor_y, Some(line_height)),
                AlignItems::Start | AlignItems::Baseline => (cursor_y, None),
                AlignItems::Center => (cursor_y + (line_height - cross[k]) / 2.0, None),
                AlignItems::End => (cursor_y + line_height - cross[k], None),
            };
            ctx.layout_element(
                items[i],
                x + margin.left,
                y + margin.top,
                line_bases[k],
                definite,
            );
            if let Some(index) = ctx.tree_index(items[i]) {
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
