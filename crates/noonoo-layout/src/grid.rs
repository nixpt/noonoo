//! Grid layout: a two-dimensional track grid with `fr` units, gaps, explicit
//! and auto placement.
//!
//! v1 subset (each deviation documented):
//!
//! - **Tracks**: `<length-percentage>`, `auto`, and `<fr>` (with `repeat()`).
//!   Columns size against the definite content width: fixed tracks first, then
//!   `fr` tracks share the leftover proportionally; with no `fr` tracks the
//!   leftover stretches `auto` tracks equally (CSS Grid §12.8). `auto` tracks'
//!   base size is the max-content of the single-span items in them.
//! - **Rows** size against content (the container's height is indefinite while
//!   it is being sized): fixed lengths apply; percentage tracks behave as
//!   `auto`; `fr` rows are content-sized, since there is no definite height to
//!   distribute. An item's height is measured by laying it out, then the tree
//!   is truncated back — measurement is real layout, thrown away.
//! - **Placement**: line numbers (`1 / 3`), `span N`, and `auto`, with sparse
//!   row-major auto-placement. Auto-placed items wrap at the *explicit*
//!   column count; explicit placements may extend into implicit tracks sized
//!   by `grid-auto-rows`/`grid-auto-columns`. An item's height distributes
//!   equally across the rows it spans.
//! - **Alignment**: items fill their area on the inline axis and sit at the
//!   area's top on the block axis (`align-items` positioning lands with the
//!   same machinery as flex's cross-axis stretch).

use std::collections::HashSet;

use noonoo_dom::NodeId;
use noonoo_style::{AlignItems, ComputedStyle, GridLine, Track};

use crate::Ctx;
use crate::float;
use crate::resolve;

/// One item's resolved placement in the grid (0-based, half-open spans).
struct Placed {
    node: NodeId,
    column: usize,
    column_span: usize,
    row: usize,
    row_span: usize,
}

pub(crate) fn layout_grid_children(
    ctx: &mut Ctx<'_>,
    style: &ComputedStyle,
    children: &[NodeId],
    content_x: f32,
    content_y: f32,
    content_width: f32,
) -> (Vec<usize>, f32) {
    let mut items = Vec::new();
    for &child in children {
        // Grid items are elements; loose text would need an anonymous item,
        // which is not modelled yet, so it is dropped.
        if ctx.is_text(child) || ctx.is_hidden(child) {
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

    let column_tracks = if style.grid_template_columns.is_empty() {
        vec![Track::Auto]
    } else {
        style.grid_template_columns.clone()
    };
    let row_tracks_template = style.grid_template_rows.clone();
    let auto_columns = if style.grid_auto_columns.is_empty() {
        vec![Track::Auto]
    } else {
        style.grid_auto_columns.clone()
    };
    let auto_rows = if style.grid_auto_rows.is_empty() {
        vec![Track::Auto]
    } else {
        style.grid_auto_rows.clone()
    };

    // --- placement -----------------------------------------------------------
    let mut occupied: HashSet<(usize, usize)> = HashSet::new();
    let mut placements: Vec<Placed> = Vec::new();
    // Auto-placed items wrap at the explicit column count; implicit columns
    // exist only for explicitly-placed items that reach past it.
    let wrap_columns = column_tracks.len().max(1);
    let (mut column_count, mut row_count) = (column_tracks.len(), row_tracks_template.len());
    let mut cursor = (0usize, 0usize);

    for node in &items {
        let item_style = ctx.style(*node);
        let (column, column_span) = axis(&item_style.grid_column);
        let (row, row_span) = axis(&item_style.grid_row);

        let (column, row) = match (column, row) {
            (Some(column), Some(row)) => {
                // Both axes explicit: placed as asked, overlap permitted.
                (column, row)
            }
            (Some(column), None) => {
                // First row where the column span is free.
                let mut row = 0usize;
                while !free(&occupied, column, column_span, row, row_span) {
                    row += 1;
                }
                (column, row)
            }
            (None, Some(row)) => {
                // First column span free in the named row; a span wider than
                // the explicit grid grows it rather than looping forever.
                let limit = wrap_columns.max(column_span);
                let mut column = 0usize;
                while column + column_span <= limit
                    && !free(&occupied, column, column_span, row, row_span)
                {
                    column += 1;
                }
                (column.min(limit - 1), row)
            }
            (None, None) => {
                // Row-major cursor (sparse): scan forward from the cursor for
                // the first free area, wrapping at the explicit column count.
                let limit = wrap_columns.max(column_span);
                let mut column = cursor.0;
                let mut row = cursor.1;
                loop {
                    if column + column_span > limit {
                        column = 0;
                        row += 1;
                    }
                    if free(&occupied, column, column_span, row, row_span) {
                        break;
                    }
                    column += 1;
                }
                cursor = (column + column_span, row);
                (column, row)
            }
        };
        for c in column..column + column_span {
            for r in row..row + row_span {
                occupied.insert((c, r));
            }
        }
        column_count = column_count.max(column + column_span);
        row_count = row_count.max(row + row_span);
        placements.push(Placed {
            node: *node,
            column,
            column_span,
            row,
            row_span,
        });
    }

    // Implicit tracks beyond the templates, cycling the auto track lists.
    let template_columns = column_tracks.len();
    let mut columns = column_tracks;
    while columns.len() < column_count {
        columns.push(auto_columns[(columns.len() - template_columns) % auto_columns.len()].clone());
    }
    let template_rows = row_tracks_template.len();
    let mut row_tracks = row_tracks_template;
    while row_tracks.len() < row_count {
        row_tracks.push(auto_rows[(row_tracks.len() - template_rows) % auto_rows.len()].clone());
    }

    // --- column sizing (definite: the content width) -------------------------
    let column_gap = style.column_gap;
    let mut widths: Vec<f32> = columns
        .iter()
        .map(|track| match track {
            Track::Auto | Track::Fr(_) => 0.0,
            Track::Length(dimension) => resolve::dimension(dimension.clone(), content_width)
                .unwrap_or(0.0)
                .max(0.0),
        })
        .collect();
    // `auto` tracks size to the max-content of their single-span items
    // (margins included, as track sizing counts the item's outer box).
    for placed in &placements {
        if placed.column_span == 1 && matches!(columns[placed.column], Track::Auto) {
            let margins = resolve::margins(&ctx.style(placed.node).margin, content_width);
            let preferred = float::preferred_width(ctx, placed.node) + margins.left + margins.right;
            widths[placed.column] = widths[placed.column].max(preferred);
        }
    }
    let column_gaps_total = column_gap * (widths.len().saturating_sub(1)) as f32;
    let leftover = (content_width - widths.iter().sum::<f32>() - column_gaps_total).max(0.0);
    let fr_total: f32 = columns
        .iter()
        .filter_map(|track| match track {
            Track::Fr(factor) => Some(*factor),
            _ => None,
        })
        .sum();
    if fr_total > 0.0 {
        for (index, track) in columns.iter().enumerate() {
            if let Track::Fr(factor) = track {
                widths[index] = leftover * factor / fr_total;
            }
        }
    } else {
        // §12.8: leftover space stretches `auto` tracks equally.
        let autos: Vec<usize> = columns
            .iter()
            .enumerate()
            .filter(|(_, track)| matches!(track, Track::Auto))
            .map(|(index, _)| index)
            .collect();
        if !autos.is_empty() && leftover > 0.0 {
            let share = leftover / autos.len() as f32;
            for &index in &autos {
                widths[index] += share;
            }
        }
    }

    let column_x = |column: usize| -> f32 {
        content_x + widths[..column].iter().sum::<f32>() + column_gap * column as f32
    };
    let area_width = |column: usize, span: usize| -> f32 {
        widths[column..column + span].iter().sum::<f32>()
            + column_gap * (span.saturating_sub(1)) as f32
    };

    // --- row sizing (measure: real layout, thrown away) ----------------------
    let mut heights = Vec::with_capacity(placements.len());
    for placed in &placements {
        let margins = resolve::margins(&ctx.style(placed.node).margin, content_width);
        heights.push(ctx.measure_height(
            placed.node,
            column_x(placed.column) + margins.left,
            content_y,
            area_width(placed.column, placed.column_span),
            None,
        ));
    }
    ctx.prune_pending();

    let mut row_heights: Vec<f32> = row_tracks
        .iter()
        .map(|track| match track {
            Track::Length(dimension) => resolve::definite(dimension).unwrap_or(0.0).max(0.0),
            _ => 0.0,
        })
        .collect();
    // An item's height distributes equally across the rows it spans; fixed
    // rows keep their size and let the item overflow.
    for (index, placed) in placements.iter().enumerate() {
        let share = heights[index] / placed.row_span as f32;
        for (height, track) in row_heights[placed.row..]
            .iter_mut()
            .zip(&row_tracks[placed.row..])
            .take(placed.row_span)
        {
            if !matches!(track, Track::Length(_)) {
                *height = (*height).max(share);
            }
        }
    }

    let row_gap = style.row_gap;
    let row_y = |row: usize| -> f32 {
        content_y + row_heights[..row].iter().sum::<f32>() + row_gap * row as f32
    };

    // --- placement -----------------------------------------------------------
    // Each item aligns within its area's height: `stretch` (the default)
    // fills it, the others keep the item's natural height and position it.
    let area_height = |row: usize, span: usize| -> f32 {
        row_heights[row..row + span].iter().sum::<f32>() + row_gap * (span.saturating_sub(1)) as f32
    };
    let mut indices = Vec::new();
    for (index, placed) in placements.iter().enumerate() {
        let item_style = ctx.style(placed.node);
        let align = item_style.align_self.unwrap_or(style.align_items);
        let margins = resolve::margins(&item_style.margin, content_width);
        let area = area_height(placed.row, placed.row_span);
        let (y, definite) = match align {
            AlignItems::Stretch => (row_y(placed.row), Some(area)),
            AlignItems::Start | AlignItems::Baseline => (row_y(placed.row), None),
            AlignItems::Center => (row_y(placed.row) + (area - heights[index]) / 2.0, None),
            AlignItems::End => (row_y(placed.row) + area - heights[index], None),
        };
        ctx.layout_element(
            placed.node,
            column_x(placed.column) + margins.left,
            y + margins.top,
            area_width(placed.column, placed.column_span),
            definite,
        );
        if let Some(index) = ctx.tree_index(placed.node) {
            indices.push(index);
        }
    }

    let grid_height =
        row_heights.iter().sum::<f32>() + row_gap * row_heights.len().saturating_sub(1) as f32;
    (indices, grid_height.max(0.0))
}

/// One axis of a placement: `(start line 0-based, span)`, with `None` for an
/// auto-placed axis.
fn axis(range: &noonoo_style::GridRange) -> (Option<usize>, usize) {
    let span = range.span().unwrap_or(1).max(1) as usize;
    match range.start {
        GridLine::Line(line) => (Some(line as usize - 1), span),
        _ => (None, span),
    }
}

/// Whether every cell of `[column, column + column_span) ×
/// [row, row + row_span)` is unoccupied.
fn free(
    occupied: &HashSet<(usize, usize)>,
    column: usize,
    column_span: usize,
    row: usize,
    row_span: usize,
) -> bool {
    (column..column + column_span)
        .all(|c| (row..row + row_span).all(|r| !occupied.contains(&(c, r))))
}
