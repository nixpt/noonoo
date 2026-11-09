//! Flexbox: row and column, grow/shrink, `justify-content`, gaps, and — for a
//! row — `flex-wrap` (items spill onto additional lines).
//!
//! A row grows and shrinks its items with CSS 9.7's freeze loop: an item
//! stops at its `min-width`/`max-width`, and one with `min-width: auto` that
//! doesn't clip stops at its automatic minimum (its min-content width, or its
//! specified width if smaller). The decided width replaces a specified one.
//!
//! Rows align items on the cross axis per `align-items`/`align-self`:
//! a single-line row with a definite height has one line of that height;
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
                let chunk = noonoo_text::trim_collapsible(&chunk);
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
            // What shrinking couldn't absorb (inflexible items, floors)
            // still overflows, for `justify-content` to place.
            free = height
                - sizes
                    .iter()
                    .zip(&columns)
                    .map(|(size, column)| size + column.margin_main)
                    .sum::<f32>()
                - gaps;
        }
    }
    let (start, extra) = justify_offset(style.justify_content, free, items.len());

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

/// One row item's main-axis inputs to CSS Flexbox 9.7.
struct RowItem {
    /// The flex base size, border box.
    base: f32,
    /// The base clamped by `min-width`/`max-width`.
    hypothetical: f32,
    minimum: f32,
    maximum: Option<f32>,
    grow: f32,
    shrink: f32,
    /// Left and right margins, `auto` as 0.
    margins: (f32, f32),
    /// Which of those margins are `auto`.
    auto: (bool, bool),
}

impl RowItem {
    fn clamp(&self, size: f32) -> f32 {
        let size = match self.maximum {
            Some(maximum) => size.min(maximum),
            None => size,
        };
        size.max(self.minimum).max(0.0)
    }

    fn outer_margins(&self) -> f32 {
        self.margins.0 + self.margins.1
    }
}

/// CSS Flexbox 9.7, "resolve the flexible lengths" of one line: grow or
/// shrink the items toward the available space, freezing each that a
/// minimum or maximum clamps and handing what it can't take to the rest.
fn resolve_flexible_lengths(items: &[&RowItem], available: f32) -> Vec<f32> {
    let n = items.len();
    let hypothetical: f32 = items
        .iter()
        .map(|item| item.hypothetical + item.outer_margins())
        .sum();
    let growing = hypothetical < available;
    let mut sizes: Vec<f32> = items.iter().map(|item| item.hypothetical).collect();
    // Inflexible items freeze at their hypothetical size: no factor, or a
    // base the clamp already moved the wrong way.
    let mut frozen: Vec<bool> = items
        .iter()
        .map(|item| {
            if growing {
                item.grow <= 0.0 || item.base > item.hypothetical
            } else {
                item.shrink <= 0.0 || item.base < item.hypothetical
            }
        })
        .collect();
    let free_with = |sizes: &[f32], frozen: &[bool]| {
        available
            - (0..n)
                .map(|k| {
                    let size = if frozen[k] { sizes[k] } else { items[k].base };
                    size + items[k].outer_margins()
                })
                .sum::<f32>()
    };
    let initial = free_with(&sizes, &frozen);
    // Each pass freezes at least one item, so this terminates.
    while frozen.iter().any(|&f| !f) {
        let mut free = free_with(&sizes, &frozen);
        let unfrozen: Vec<usize> = (0..n).filter(|&k| !frozen[k]).collect();
        let factors: f32 = unfrozen
            .iter()
            .map(|&k| {
                if growing {
                    items[k].grow
                } else {
                    items[k].shrink
                }
            })
            .sum();
        // Factors summing under 1 take only that share of the space.
        if factors < 1.0 {
            let share = initial * factors;
            if share.abs() < free.abs() {
                free = share;
            }
        }
        let mut targets = vec![0.0; n];
        if growing {
            for &k in &unfrozen {
                targets[k] = items[k].base + free * items[k].grow / factors;
            }
        } else {
            let scaled: f32 = unfrozen
                .iter()
                .map(|&k| items[k].shrink * items[k].base)
                .sum();
            for &k in &unfrozen {
                let weight = items[k].shrink * items[k].base;
                targets[k] = if scaled > 0.0 {
                    items[k].base + free * weight / scaled
                } else {
                    items[k].base
                };
            }
        }
        let mut violation = 0.0;
        for &k in &unfrozen {
            let clamped = items[k].clamp(targets[k]);
            violation += clamped - targets[k];
            sizes[k] = clamped;
        }
        for &k in &unfrozen {
            let clamped = sizes[k];
            frozen[k] = if violation.abs() <= f32::EPSILON {
                true
            } else if violation > 0.0 {
                clamped > targets[k]
            } else {
                clamped < targets[k]
            };
        }
    }
    sizes
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
    let gap = style.column_gap;

    // Base main sizes and flex factors. An anonymous text item has no margins
    // and no flex factors: its base is its shrink-to-fit width.
    let mut rows = Vec::with_capacity(items.len());
    for item in items {
        let row = if let Some(text) = item.text() {
            let base = runs::width(ctx, text, style, content_width).max(0.0);
            RowItem {
                base,
                hypothetical: base,
                minimum: 0.0,
                maximum: None,
                grow: 0.0,
                shrink: 0.0,
                margins: (0.0, 0.0),
                auto: (false, false),
            }
        } else {
            let node = item.node();
            let item_style = ctx.style(node);
            let basis = match &item_style.flex_basis {
                Dimension::Auto => None,
                dimension => resolve::dimension(dimension.clone(), content_width),
            };
            let base = basis
                .or_else(|| resolve::dimension(item_style.width.clone(), content_width))
                // Auto: the item's max-content border box, unclamped: the
                // clamp comes after, in the hypothetical size.
                .unwrap_or_else(|| crate::float::unclamped_preferred_width(ctx, node))
                .max(0.0);
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
            // `min-width: auto` on a flex item that doesn't clip is its
            // automatic minimum: no narrower than its content can wrap to
            // (nor than a specified width).
            let minimum = match resolve::dimension(item_style.min_width.clone(), content_width) {
                Some(minimum) => to_border_box(minimum),
                None if !resolve::clips(item_style) => {
                    let content = crate::float::min_content_of_contents(ctx, node);
                    match resolve::dimension(item_style.width.clone(), content_width) {
                        Some(width) => content.min(to_border_box(width)),
                        None => content,
                    }
                }
                None => 0.0,
            };
            let maximum =
                resolve::dimension(item_style.max_width.clone(), content_width).map(to_border_box);
            let margin = resolve::margins(&item_style.margin, content_width);
            let auto = (
                matches!(item_style.margin.left, Dimension::Auto),
                matches!(item_style.margin.right, Dimension::Auto),
            );
            let mut row = RowItem {
                base,
                hypothetical: base,
                minimum,
                maximum,
                grow: item_style.flex_grow,
                shrink: item_style.flex_shrink,
                margins: (margin.left, margin.right),
                auto,
            };
            row.hypothetical = row.clamp(base);
            row
        };
        rows.push(row);
    }

    // Partition into lines when wrapping is on, by outer hypothetical size.
    let count = items.len();
    let mut lines: Vec<Vec<usize>> = Vec::new();
    if style.flex_wrap == FlexWrap::NoWrap {
        lines.push((0..count).collect());
    } else {
        let mut current: Vec<usize> = Vec::new();
        let mut used = 0.0;
        for (i, row) in rows.iter().enumerate() {
            let outer = row.hypothetical + row.outer_margins();
            let addition = if current.is_empty() {
                outer
            } else {
                gap + outer
            };
            if !current.is_empty() && used + addition > content_width {
                lines.push(std::mem::take(&mut current));
                used = outer;
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
        let line_rows: Vec<&RowItem> = line.iter().map(|&i| &rows[i]).collect();
        let line_bases = resolve_flexible_lengths(&line_rows, content_width - line_gaps);

        // Free space left over goes to `auto` margins first (CSS Flexbox
        // 9.5 step 12), then to `justify-content`.
        let used: f32 = line_bases.iter().sum::<f32>()
            + line_rows.iter().map(|row| row.outer_margins()).sum::<f32>()
            + line_gaps;
        let remaining = content_width - used;
        let autos = line_rows
            .iter()
            .map(|row| row.auto.0 as usize + row.auto.1 as usize)
            .sum::<usize>();
        let mut margins: Vec<(f32, f32)> = line_rows.iter().map(|row| row.margins).collect();
        let (start, extra) = if autos > 0 && remaining > 0.0 {
            let each = remaining / autos as f32;
            for (k, row) in line_rows.iter().enumerate() {
                if row.auto.0 {
                    margins[k].0 += each;
                }
                if row.auto.1 {
                    margins[k].1 += each;
                }
            }
            (0.0, 0.0)
        } else {
            justify_offset(style.justify_content, remaining, n)
        };

        // Cross sizes first: an item is positioned (and stretched) within the
        // line, so its height must be known before it is placed. The
        // measurement is real layout, thrown away.
        let mut cross = Vec::with_capacity(n);
        let mut measure_x = content_x + start;
        for (k, &i) in line.iter().enumerate() {
            measure_x += margins[k].0;
            let height = match &items[i] {
                Item::Text { text, .. } => runs::height(ctx, text, style, line_bases[k]),
                Item::Element(node) => {
                    ctx.flexed_width = Some((*node, line_bases[k]));
                    let height =
                        ctx.measure_height(*node, measure_x, cursor_y, line_bases[k], None);
                    ctx.flexed_width = None;
                    height
                }
            };
            cross.push(height);
            measure_x += line_bases[k] + margins[k].1 + gap + extra;
        }
        ctx.prune_pending();
        let mut line_height = cross.iter().copied().fold(0.0_f32, f32::max);
        // A single-line container with a definite height has one line of
        // exactly that height (CSS Flexbox 9.4, step 15).
        if style.flex_wrap == FlexWrap::NoWrap
            && let Some(height) = ctx.percent_basis()
        {
            line_height = height;
        }

        let mut x = content_x + start;
        for (k, &i) in line.iter().enumerate() {
            x += margins[k].0;
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
                    ctx.flexed_width = Some((*node, line_bases[k]));
                    ctx.layout_element(*node, x, y + margin.top, line_bases[k], definite);
                    ctx.flexed_width = None;
                    if let Some(index) = ctx.tree_index(*node) {
                        indices.push(index);
                    }
                }
            }
            x += line_bases[k] + margins[k].1 + gap + extra;
        }
        cursor_y += line_height + style.row_gap;
    }

    (indices, (cursor_y - content_y - style.row_gap).max(0.0))
}

/// Where `justify-content` starts the items and the extra space between
/// them. `free` may be negative, the items overflowing: alignment stays
/// unsafe, as browsers default it, so `end` and `center` push the overflow
/// out the start (and the end); the `space-*` values fall back to `start`
/// (`space-between`) or `center` (CSS Box Alignment §5.3).
fn justify_offset(justify: JustifyContent, free: f32, count: usize) -> (f32, f32) {
    let n = count as f32;
    if free < 0.0 {
        return match justify {
            JustifyContent::Start | JustifyContent::SpaceBetween => (0.0, 0.0),
            JustifyContent::End => (free, 0.0),
            JustifyContent::Center | JustifyContent::SpaceAround | JustifyContent::SpaceEvenly => {
                (free / 2.0, 0.0)
            }
        };
    }
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
