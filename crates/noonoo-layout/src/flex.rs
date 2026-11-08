//! Flexbox: row and column, grow/shrink, `justify-content`, gaps, and — for a
//! row — `flex-wrap` (items spill onto additional lines).
//!
//! Rows align items on the cross axis per `align-items`/`align-self`:
//! `stretch` (the default) fills the line height when the item's height is
//! auto — an explicit height wins — and `start`/`center`/`end` position the
//! item (`baseline` behaves as `start`). Column flex stretches items across
//! the container (an auto width fills it) or, for `start`/`center`/`end`,
//! shrink-wraps the item and positions it; a column with a definite height
//! grows and shrinks its items into it (no smaller than their content, as
//! `min-height: auto` does) and justifies them.
//! Line heights come from a measure pass (real layout, discarded) so items
//! can be positioned within them.
//!
//! Loose text is anonymous: a run of text children collapses into one item
//! (`white-space` collapses the way the container styles it), styled by the
//! container. See [`crate::runs`].

use noonoo_dom::NodeId;
use noonoo_style::{
    AlignItems, BoxSizing, ComputedStyle, Dimension, FlexDirection, FlexWrap, JustifyContent,
};

use crate::{Ctx, resolve, runs};

/// One flex item: an element, or an anonymous text run (no margins, no flex
/// factors; sized like flushing inline content).
enum Item {
    Element(NodeId),
    Text { anchor: NodeId, text: String },
}

impl Item {
    fn node(&self) -> NodeId {
        match self {
            Self::Element(node) => *node,
            Self::Text { anchor, .. } => *anchor,
        }
    }

    fn text(&self) -> Option<&str> {
        match self {
            Self::Element(_) => None,
            Self::Text { text, .. } => Some(text),
        }
    }

    fn is_text(&self) -> bool {
        self.text().is_some()
    }
}

pub(crate) fn layout_flex_children(
    ctx: &mut Ctx<'_>,
    parent: NodeId,
    style: &ComputedStyle,
    children: &[NodeId],
    content_x: f32,
    content_y: f32,
    content_width: f32,
) -> (Vec<usize>, f32) {
    // Elements are items directly; runs of loose text merge into one
    // anonymous item (whitespace collapses to single spaces).
    let mut items: Vec<Item> = Vec::new();
    let mut index = 0;
    while index < children.len() {
        let child = children[index];
        if ctx.is_hidden(child) {
            index += 1;
            continue;
        }
        if ctx.is_out_of_flow(child) {
            ctx.pending.push(child);
            index += 1;
            continue;
        }
        if ctx.is_text(child) {
            let mut text = String::new();
            while index < children.len() && ctx.is_text(children[index]) {
                let chunk = ctx.document.text_content(children[index]);
                let chunk = chunk.trim();
                if !chunk.is_empty() {
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    text.push_str(chunk);
                }
                index += 1;
            }
            if !text.is_empty() {
                items.push(Item::Text {
                    anchor: child,
                    text,
                });
            }
            continue;
        }
        items.push(Item::Element(child));
        index += 1;
    }

    if items.is_empty() {
        return (Vec::new(), 0.0);
    }

    let row = matches!(
        style.flex_direction,
        FlexDirection::Row | FlexDirection::RowReverse
    );
    if !row {
        return layout_column(
            ctx,
            parent,
            style,
            &items,
            content_x,
            content_y,
            content_width,
        );
    }
    layout_row(
        ctx,
        parent,
        style,
        &items,
        content_x,
        content_y,
        content_width,
    )
}

fn layout_column(
    ctx: &mut Ctx<'_>,
    parent: NodeId,
    style: &ComputedStyle,
    items: &[Item],
    content_x: f32,
    content_y: f32,
    content_width: f32,
) -> (Vec<usize>, f32) {
    let gap = style.row_gap;
    // The container's own definite content height, if it has one: what its
    // items grow into, shrink to, and are justified within.
    let container_height = ctx.percent_basis();

    // Each item's cross placement, the width to lay it out at, and its base
    // main size (margins excluded), with how far it may shrink.
    struct Column {
        align: AlignItems,
        available: f32,
        base: f32,
        floor: f32,
        grow: f32,
        shrink: f32,
        margin_main: f32,
    }
    let mut columns = Vec::with_capacity(items.len());
    for item in items {
        let column = match item {
            Item::Text { text, .. } => {
                let height = runs::height(ctx, text, style, content_width);
                Column {
                    align: AlignItems::Stretch,
                    available: content_width,
                    base: height,
                    floor: height,
                    grow: 0.0,
                    shrink: 0.0,
                    margin_main: 0.0,
                }
            }
            Item::Element(node) => {
                let item_style = ctx.style(*node).clone();
                let margin = resolve::margins(&item_style.margin, content_width);
                let align = item_style.align_self.unwrap_or(style.align_items);
                let room = (content_width - margin.left - margin.right).max(0.0);
                // A stretched auto-width item fills the line; any other auto
                // width shrinks to fit its content.
                let available = if align != AlignItems::Stretch
                    && matches!(item_style.width, Dimension::Auto)
                {
                    crate::float::shrink_to_fit(ctx, *node, room)
                } else {
                    room
                };
                let content = ctx.measure_height(*node, content_x, content_y, available, None);
                let basis = match &item_style.flex_basis {
                    Dimension::Auto => None,
                    dimension => match container_height {
                        Some(height) => resolve::dimension(dimension.clone(), height),
                        None => resolve::definite(dimension),
                    },
                };
                // `min-height: auto`: an item that doesn't clip shrinks no
                // smaller than its content. A specified height caps that
                // minimum in CSS; the content can't be measured apart from it
                // here, so such an item may shrink freely.
                let floor = if resolve::clips(&item_style)
                    || !matches!(item_style.height, Dimension::Auto)
                {
                    0.0
                } else {
                    content
                };
                Column {
                    align,
                    available,
                    base: basis.unwrap_or(content),
                    floor,
                    grow: item_style.flex_grow,
                    shrink: item_style.flex_shrink,
                    margin_main: margin.top + margin.bottom,
                }
            }
        };
        columns.push(column);
    }
    ctx.prune_pending();

    let gaps = gap * items.len().saturating_sub(1) as f32;
    let mut sizes: Vec<f32> = columns.iter().map(|column| column.base).collect();
    let mut free = 0.0;
    if let Some(height) = container_height {
        let used: f32 = columns
            .iter()
            .map(|column| column.base + column.margin_main)
            .sum::<f32>()
            + gaps;
        free = height - used;
        if free > 0.0 {
            let total: f32 = columns.iter().map(|column| column.grow).sum();
            if total > 0.0 {
                for (size, column) in sizes.iter_mut().zip(&columns) {
                    *size += free * column.grow / total;
                }
                free = 0.0;
            }
        } else if free < 0.0 {
            let weights: Vec<f32> = columns
                .iter()
                .map(|column| column.shrink * column.base)
                .collect();
            let total: f32 = weights.iter().sum();
            if total > 0.0 {
                for ((size, column), weight) in sizes.iter_mut().zip(&columns).zip(&weights) {
                    *size = (*size + free * weight / total).max(column.floor);
                }
            }
            free = 0.0;
        }
    }
    let (start, extra) = justify_offset(style.justify_content, free.max(0.0), items.len());

    let mut indices = Vec::new();
    let mut y = content_y + start;
    for ((item, column), size) in items.iter().zip(&columns).zip(&sizes) {
        match item {
            Item::Element(node) => {
                let margin = resolve::margins(&ctx.style(*node).margin, content_width);
                // The container decided a height only when it had one to share.
                ctx.flexed = container_height.map(|_| (*node, *size));
                let height = ctx.layout_element(
                    *node,
                    content_x + margin.left,
                    y + margin.top,
                    column.available,
                    None,
                );
                ctx.flexed = None;
                if let Some(index) = ctx.tree_index(*node) {
                    let width = ctx.tree.boxes[index].rect.width;
                    let room = content_width - margin.left - margin.right;
                    let dx = match column.align {
                        AlignItems::Center => (room - width) / 2.0,
                        AlignItems::End => room - width,
                        _ => 0.0,
                    };
                    if dx != 0.0 {
                        ctx.shift_subtree(index, dx, 0.0);
                    }
                    indices.push(index);
                }
                y += margin.top + height + margin.bottom + gap + extra;
            }
            Item::Text { anchor, text } => {
                indices.extend(runs::lines(
                    ctx,
                    *anchor,
                    parent,
                    text,
                    style,
                    content_x,
                    y,
                    content_width,
                ));
                y += size + gap + extra;
            }
        }
    }
    (indices, (y - content_y - gap - extra).max(0.0))
}

fn layout_row(
    ctx: &mut Ctx<'_>,
    parent: NodeId,
    style: &ComputedStyle,
    items: &[Item],
    content_x: f32,
    content_y: f32,
    content_width: f32,
) -> (Vec<usize>, f32) {
    let count = items.len();
    let gap = style.column_gap;

    // Base main sizes and flex factors. An anonymous text item has no margins
    // and no flex factors: its base is its shrink-to-fit width.
    let mut bases = Vec::with_capacity(count);
    let mut grows = Vec::with_capacity(count);
    let mut shrinks = Vec::with_capacity(count);
    let mut minimums = Vec::with_capacity(count);
    let mut maximums = Vec::with_capacity(count);
    for item in items {
        let text = item.text();
        let base;
        let minimum;
        let maximum;
        let grow;
        let shrink;
        if let Some(text) = text {
            base = runs::width(ctx, text, style, content_width);
            minimum = None;
            maximum = None;
            grow = 0.0;
            shrink = 0.0;
        } else {
            let node = item.node();
            let item_style = ctx.style(node);
            let basis = match &item_style.flex_basis {
                Dimension::Auto => None,
                dimension => resolve::dimension(dimension.clone(), content_width),
            };
            base = basis
                .or_else(|| resolve::dimension(item_style.width.clone(), content_width))
                // Auto: the item's max-content border box.
                .unwrap_or_else(|| crate::float::preferred_width(ctx, node));
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
            minimum =
                resolve::dimension(item_style.min_width.clone(), content_width).map(to_border_box);
            maximum =
                resolve::dimension(item_style.max_width.clone(), content_width).map(to_border_box);
            grow = item_style.flex_grow;
            shrink = item_style.flex_shrink;
        }
        bases.push(base.max(0.0));
        minimums.push(minimum);
        maximums.push(maximum);
        grows.push(grow);
        shrinks.push(shrink);
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
            let height = match &items[i] {
                Item::Text { text, .. } => runs::height(ctx, text, style, line_bases[k]),
                Item::Element(node) => {
                    let margin = resolve::margins(&ctx.style(*node).margin, content_width);
                    ctx.measure_height(
                        *node,
                        measure_x + margin.left,
                        cursor_y,
                        line_bases[k],
                        None,
                    )
                }
            };
            cross.push(height);
            measure_x += line_bases[k] + gap + extra;
        }
        ctx.prune_pending();
        let line_height = cross.iter().copied().fold(0.0_f32, f32::max);

        let mut x = content_x + start;
        for (k, &i) in line.iter().enumerate() {
            let item_align = if items[i].is_text() {
                style.align_items
            } else {
                let item_style = ctx.style(items[i].node());
                item_style.align_self.unwrap_or(style.align_items)
            };
            let (y, definite) = match item_align {
                AlignItems::Stretch => (cursor_y, Some(line_height)),
                AlignItems::Start | AlignItems::Baseline => (cursor_y, None),
                AlignItems::Center => (cursor_y + (line_height - cross[k]) / 2.0, None),
                AlignItems::End => (cursor_y + line_height - cross[k], None),
            };

            match &items[i] {
                Item::Text { anchor, text } => {
                    indices.extend(runs::lines(
                        ctx,
                        *anchor,
                        parent,
                        text,
                        style,
                        x,
                        y,
                        line_bases[k],
                    ));
                }
                Item::Element(node) => {
                    let margin = resolve::margins(&ctx.style(*node).margin, content_width);
                    ctx.layout_element(
                        *node,
                        x + margin.left,
                        y + margin.top,
                        line_bases[k],
                        definite,
                    );
                    if let Some(index) = ctx.tree_index(*node) {
                        indices.push(index);
                    }
                }
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
