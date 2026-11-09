//! Table layout — the auto-layout subset a configuration document needs.
//!
//! Finding 3 of `docs/REVIEW-2026-11-07.md` required a starting point for
//! `<table>`: columns sized to their widest cell, `colspan`, and
//! `border-collapse: collapse`. The model below is deliberately simple, in
//! the same spirit as the rest of the layout crate:
//!
//! - rows come from `table-row` children; a `table-row-group`
//!   (`thead`/`tbody`/`tfoot`) is transparent — its rows lay out in its
//!   place, and the group's own box is not pushed;
//! - a cell's demand is its max-content border-box width plus its margins;
//!   a column's width is the widest demand placed on it, and a `colspan`
//!   cell spreads its demand evenly across the columns it covers (an
//!   accepted approximation of the spec's distribution);
//! - column widths shrink proportionally when their sum exceeds the table's
//!   content box, or stretch proportionally when the table's width leaves
//!   leftover space — the §17.5.2 distribution;
//! - cells in a row stretch to the row's tallest border-box height;
//! - `border-collapse: collapse` paints a shared edge once: the cell to the
//!   left of (or above) an internal edge drops its right (bottom) border,
//!   and the table's own border handles the frame;
//! - table parts outside a table (cells or rows in a block) are wrapped in
//!   an anonymous shrink-to-fit table by block flow (see `block.rs`);
//! - `border-spacing`, captions, `table-layout: fixed`, and the rest of the
//!   anonymous fixup (stray content inside a table or row) are not
//!   modelled; stray content contributes nothing.

use noonoo_dom::NodeId;
use noonoo_style::{BorderCollapse, ComputedStyle, Display};

use crate::{Ctx, float, resolve};

/// One cell of one row, with the columns its `colspan` covers.
struct Cell {
    node: NodeId,
    /// The first column the cell occupies (0-based).
    column: usize,
    /// `colspan`; 1 for the common case.
    span: usize,
}

/// The cells of one row, in column order.
#[derive(Default)]
struct Row {
    cells: Vec<Cell>,
}

/// How many columns one `colspan` may claim, so a corrupt or insane
/// attribute cannot explode a row's column walk.
const MAX_SPAN: usize = 1000;

/// Lay the rows out in a table content box `content_width` wide. A table
/// box's columns `fill` that width; an anonymous table's only shrink to
/// fit it.
pub(crate) fn layout_table_children(
    ctx: &mut Ctx<'_>,
    style: &ComputedStyle,
    children: &[NodeId],
    content_x: f32,
    content_y: f32,
    content_width: f32,
    fill: bool,
) -> (Vec<usize>, f32) {
    let rows = collect_rows(ctx, children);
    if rows.is_empty() {
        return (Vec::new(), 0.0);
    }
    let demands = column_demands(ctx, &rows, content_width);
    let columns = fit_columns(&demands, content_width, fill);

    // Place the rows. Measure first, so every cell can stretch to its row's
    // height: the measurement is real layout, thrown away.
    let collapse = style.border_collapse == BorderCollapse::Collapse;
    let mut indices = Vec::new();
    let mut cursor_y = content_y;
    for (row_index, row) in rows.iter().enumerate() {
        let mut measured = Vec::with_capacity(row.cells.len());
        for cell in &row.cells {
            let cell_width = span_width(&columns, cell.column, cell.span);
            let margin = resolve::margins(&ctx.style(cell.node).margin, content_width);
            let border_box = (cell_width - margin.left - margin.right).max(0.0);
            // The cell fills its column: its own `width` already sized it.
            ctx.flexed_width = Some((cell.node, border_box));
            let height = ctx.measure_height(
                cell.node,
                content_x + column_start(&columns, cell.column) + margin.left,
                cursor_y,
                border_box,
                None,
            );
            ctx.flexed_width = None;
            measured.push(height + margin.top + margin.bottom);
        }
        ctx.prune_pending();
        let row_height = measured.iter().copied().fold(0.0_f32, f32::max);

        for (cell, height) in row.cells.iter().zip(&measured) {
            let cell_width = span_width(&columns, cell.column, cell.span);
            let margin = resolve::margins(&ctx.style(cell.node).margin, content_width);
            let border_box = (cell_width - margin.left - margin.right).max(0.0);
            ctx.flexed_width = Some((cell.node, border_box));
            let _height = ctx.layout_element(
                cell.node,
                content_x + column_start(&columns, cell.column) + margin.left,
                cursor_y + margin.top,
                border_box,
                // The cell stretches to its row's height, so a background
                // fills it even when the neighbours are taller.
                Some(
                    (row_height - margin.top - margin.bottom)
                        .max(*height - margin.top - margin.bottom),
                ),
            );
            ctx.flexed_width = None;
            if let Some(index) = ctx.tree_index(cell.node) {
                indices.push(index);
                if collapse {
                    mask_collapsed_borders(ctx, index, cell, &rows, row_index);
                }
            }
        }
        cursor_y += row_height;
    }

    (indices, cursor_y - content_y)
}

/// A table's max-content content width: its columns' widest demands.
pub(crate) fn max_content_width(ctx: &Ctx<'_>, node: NodeId) -> f32 {
    let rows = collect_rows(ctx, &crate::child_nodes(ctx.document, node));
    column_demands(ctx, &rows, 0.0).maximums.iter().sum()
}

/// A table's min-content content width: its columns' minimums side by side.
pub(crate) fn min_content_width(ctx: &Ctx<'_>, node: NodeId) -> f32 {
    let rows = collect_rows(ctx, &crate::child_nodes(ctx.document, node));
    column_demands(ctx, &rows, 0.0).minimums.iter().sum()
}

/// What the columns ask for.
struct Demands {
    minimums: Vec<f32>,
    maximums: Vec<f32>,
    /// Whether a cell of the column (spanning it alone) has a pixel width:
    /// such a column keeps its width when the table has room to spare.
    fixed: Vec<bool>,
}

/// Each column's width demands: a cell demands its min-content and
/// max-content border-box widths (plus its margins, as track sizing counts
/// outer boxes); a spanning cell spreads its demands evenly across the
/// columns it covers. A column's maximum is never below its minimum.
fn column_demands(ctx: &Ctx<'_>, rows: &[Row], content_width: f32) -> Demands {
    let column_count = rows
        .iter()
        .map(|row| row.cells.iter().map(|cell| cell.span).sum::<usize>())
        .max()
        .unwrap_or(0);
    let mut minimums = vec![0.0_f32; column_count];
    let mut maximums = vec![0.0_f32; column_count];
    let mut fixed = vec![false; column_count];
    for row in rows {
        for cell in &row.cells {
            let cell_style = ctx.style(cell.node);
            let margin = resolve::margins(&cell_style.margin, content_width);
            let outer = margin.left + margin.right;
            let span = cell.span as f32;
            let minimum = (float::min_content_of_contents(ctx, cell.node) + outer) / span;
            let maximum = (float::preferred_width(ctx, cell.node) + outer) / span;
            if cell.span == 1
                && matches!(cell_style.width, noonoo_style::Dimension::Px(_))
                && let Some(column) = fixed.get_mut(cell.column)
            {
                *column = true;
            }
            let end = (cell.column + cell.span).min(column_count);
            for column in cell.column..end.max(cell.column) {
                if column < column_count {
                    minimums[column] = minimums[column].max(minimum);
                    maximums[column] = maximums[column].max(maximum);
                }
            }
        }
    }
    for (maximum, &minimum) in maximums.iter_mut().zip(&minimums) {
        *maximum = maximum.max(minimum);
    }
    Demands {
        minimums,
        maximums,
        fixed,
    }
}

/// The columns' widths in a content box `target` wide (CSS 2 §17.5.2.2):
/// - wider than their maximums, a table that `fill`s shares the extra
///   among the columns without a pixel width (all of them, if every one
///   has one) in proportion to their maximums; otherwise they stay at
///   their maximums;
/// - between minimums and maximums, each gets its minimum plus a share of
///   what's left in proportion to how much wider it would like to be;
/// - narrower than the minimums, they keep their minimums and overflow.
fn fit_columns(demands: &Demands, target: f32, fill: bool) -> Vec<f32> {
    let (minimums, maximums) = (&demands.minimums, &demands.maximums);
    let (least, most): (f32, f32) = (minimums.iter().sum(), maximums.iter().sum());
    if target >= most {
        let mut columns = maximums.to_vec();
        if fill {
            let growing: Vec<bool> = if demands.fixed.iter().all(|&fixed| fixed) {
                vec![true; columns.len()]
            } else {
                demands.fixed.iter().map(|&fixed| !fixed).collect()
            };
            let wanted: f32 = columns
                .iter()
                .zip(&growing)
                .filter(|(_, grows)| **grows)
                .map(|(width, _)| width)
                .sum();
            let extra = target - most;
            let count = growing.iter().filter(|&&grows| grows).count().max(1) as f32;
            for (column, grows) in columns.iter_mut().zip(&growing) {
                if *grows {
                    // In proportion to what each wants; evenly if none
                    // wants anything.
                    *column += if wanted > f32::EPSILON {
                        extra * *column / wanted
                    } else {
                        extra / count
                    };
                }
            }
        }
        return columns;
    }
    if target <= least || most - least <= f32::EPSILON {
        return minimums.to_vec();
    }
    let share = (target - least) / (most - least);
    minimums
        .iter()
        .zip(maximums)
        .map(|(&minimum, &maximum)| minimum + (maximum - minimum) * share)
        .collect()
}

/// The x of `column`'s left edge inside the table's content box (there is no
/// `border-spacing` in the subset).
fn column_start(columns: &[f32], column: usize) -> f32 {
    columns[..column.min(columns.len())].iter().sum()
}

/// The total width of `span` columns starting at `column`.
fn span_width(columns: &[f32], column: usize, span: usize) -> f32 {
    let start = column.min(columns.len());
    let end = (column + span).min(columns.len());
    columns[start..end].iter().sum::<f32>().max(0.0)
}

/// The rows of the table: `table-row` children, in document order; a
/// `table-row-group` contributes its own rows in place; stray
/// `table-cell` children wrap into one anonymous row at the end.
fn collect_rows(ctx: &Ctx<'_>, children: &[NodeId]) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    for &child in children {
        if ctx.is_hidden(child) || ctx.is_out_of_flow(child) {
            continue;
        }
        match display_of(ctx, child) {
            Some(Display::TableRow) => rows.push(row_of(ctx, child)),
            Some(Display::TableRowGroup) => {
                for grandchild in crate::child_nodes(ctx.document, child) {
                    if display_of(ctx, grandchild) == Some(Display::TableRow) {
                        rows.push(row_of(ctx, grandchild));
                    }
                }
            }
            _ => {}
        }
    }
    // A direct `table-cell` child (no rows in the table) forms one
    // anonymous row; anything else — text, block content — contributes
    // nothing.
    let mut anonymous: Option<Row> = None;
    for &child in children {
        if ctx.is_hidden(child) || ctx.is_out_of_flow(child) {
            continue;
        }
        if display_of(ctx, child) == Some(Display::TableCell) {
            let row = anonymous.get_or_insert_with(Row::default);
            let column = row.cells.iter().map(|cell| cell.span).sum::<usize>();
            row.cells.push(cell_of(ctx, child, column));
        }
    }
    if let Some(row) = anonymous.take_if(|row| !row.cells.is_empty()) {
        rows.push(row);
    }
    rows
}

/// A node's `display`, or `None` for text, which has no computed style:
/// whitespace between table parts is text and takes no part in the table.
fn display_of(ctx: &Ctx<'_>, node: NodeId) -> Option<Display> {
    ctx.styles.get(node).map(|style| style.display)
}

fn row_of(ctx: &Ctx<'_>, node: NodeId) -> Row {
    let mut cells = Vec::new();
    let mut column = 0;
    for child in crate::child_nodes(ctx.document, node) {
        if ctx.is_hidden(child) || display_of(ctx, child) != Some(Display::TableCell) {
            continue;
        }
        let cell = cell_of(ctx, child, column);
        column += cell.span;
        cells.push(cell);
    }
    Row { cells }
}

fn cell_of(ctx: &Ctx<'_>, node: NodeId, column: usize) -> Cell {
    let span = ctx
        .document
        .element(node)
        .and_then(|element| element.attr("colspan"))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .clamp(1, MAX_SPAN);
    Cell { node, column, span }
}

/// In `collapse` mode the cell to the left of (or above) a shared edge drops
/// its right (bottom) border: the neighbour paints its own, so the edge is
/// drawn once. The table's own border handles the frame.
fn mask_collapsed_borders(
    ctx: &mut Ctx<'_>,
    index: usize,
    cell: &Cell,
    rows: &[Row],
    row_index: usize,
) {
    let has_right_neighbour = rows[row_index]
        .cells
        .iter()
        .any(|other| other.column == cell.column + cell.span);
    let has_below_neighbour = rows
        .get(row_index + 1)
        .is_some_and(|next| covers(next, cell.column, cell.span));
    let border = &mut ctx.tree.boxes[index].border;
    if has_right_neighbour {
        border.right = 0.0;
    }
    if has_below_neighbour {
        border.bottom = 0.0;
    }
}

/// Whether a row has any cell overlapping the columns `[column, column + span)`.
fn covers(row: &Row, column: usize, span: usize) -> bool {
    let (start, end) = (column, column + span);
    row.cells
        .iter()
        .any(|other| other.column < end && other.column + other.span > start)
}
