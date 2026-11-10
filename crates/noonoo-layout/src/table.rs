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
//! - `col`/`colgroup` widths (across their `span`) count as a cell's; a
//!   column with a pixel width wants that width, or its minimum if wider,
//!   whatever its cells' max-content;
//! - column widths shrink proportionally when their sum exceeds the table's
//!   content box, or stretch proportionally when the table's width leaves
//!   leftover space — the §17.5.2 distribution;
//! - cells in a row stretch to the row's tallest border-box height;
//! - `border-collapse: collapse` paints a shared edge once: the cell to the
//!   left of (or above) an internal edge drops its right (bottom) border,
//!   and the table's own border handles the frame;
//! - table parts outside a table (cells or rows in a block) are wrapped in
//!   an anonymous shrink-to-fit table by block flow (see `block.rs`);
//! - `border-spacing` (separated borders only) sits before, between and
//!   after the columns and rows; a spanning cell takes the spacing it
//!   covers;
//! - captions, `table-layout: fixed`, and the rest of the
//!   anonymous fixup (stray content inside a table or row) are not
//!   modelled; stray content contributes nothing.

use noonoo_dom::NodeId;
use noonoo_style::{BorderCollapse, ComputedStyle, Dimension, Display, VerticalAlign};

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
    /// The row's own `height` in pixels: the least it is, cells or not.
    height: f32,
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
    let (spacing_x, spacing_y) = spacing(style);
    let declared = declared_columns(ctx, children);
    let demands = column_demands(ctx, &rows, &declared, content_width, spacing_x);
    let gaps = spacing_x * (demands.minimums.len() + 1) as f32;
    let columns = fit_columns(&demands, (content_width - gaps).max(0.0), fill);
    let column_start = |column: usize| column_start(&columns, column, spacing_x);
    let span_width = |column: usize, span: usize| span_width(&columns, column, span, spacing_x);

    // Place the rows. Measure first, so every cell can stretch to its row's
    // height: the measurement is real layout, thrown away.
    let collapse = style.border_collapse == BorderCollapse::Collapse;
    let mut indices = Vec::new();
    let mut cursor_y = content_y + spacing_y;
    for (row_index, row) in rows.iter().enumerate() {
        let mut measured = Vec::with_capacity(row.cells.len());
        for cell in &row.cells {
            let cell_width = span_width(cell.column, cell.span);
            let margin = resolve::margins(&ctx.style(cell.node).margin, content_width);
            let border_box = (cell_width - margin.left - margin.right).max(0.0);
            // The cell fills its column: its own `width` already sized it.
            ctx.flexed_width = Some((cell.node, border_box));
            let height = ctx.measure_height(
                cell.node,
                content_x + column_start(cell.column) + margin.left,
                cursor_y,
                border_box,
                None,
            );
            ctx.flexed_width = None;
            measured.push(height + margin.top + margin.bottom);
        }
        ctx.prune_pending();
        let row_height = measured.iter().copied().fold(row.height, f32::max);

        for (cell, height) in row.cells.iter().zip(&measured) {
            let cell_width = span_width(cell.column, cell.span);
            let margin = resolve::margins(&ctx.style(cell.node).margin, content_width);
            let border_box = (cell_width - margin.left - margin.right).max(0.0);
            ctx.flexed_width = Some((cell.node, border_box));
            let _height = ctx.layout_element(
                cell.node,
                content_x + column_start(cell.column) + margin.left,
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
                align_cell_content(ctx, cell.node, index, ctx.style(cell.node).vertical_align);
                indices.push(index);
                if collapse {
                    mask_collapsed_borders(ctx, index, cell, &rows, row_index);
                }
            }
        }
        cursor_y += row_height + spacing_y;
    }

    (indices, cursor_y - content_y)
}

/// Move a laid-out cell's content down within its content box as its
/// `vertical-align` says: `middle` centres it, `bottom` sinks it, anything
/// else (`top`, `baseline` approximated) leaves it at the top. What the
/// content takes is the height its layout gave it (margins it contains
/// included, as a quirks-mode form's). The cell's own box keeps filling
/// the row.
fn align_cell_content(ctx: &mut Ctx<'_>, node: NodeId, index: usize, align: VerticalAlign) {
    let share = match align {
        VerticalAlign::Middle => 0.5,
        VerticalAlign::Bottom => 1.0,
        _ => return,
    };
    let Some(&taken) = ctx.cell_contents.get(&node) else {
        return;
    };
    let cell = &ctx.tree.boxes[index];
    let children = cell.children.clone();
    let free = cell.content_box().height - taken;
    if free > 0.0 {
        for child in children {
            ctx.shift_subtree(child, 0.0, free * share);
        }
    }
}

/// A table's `border-spacing`, `(horizontal, vertical)`: none when its
/// borders collapse.
fn spacing(style: &ComputedStyle) -> (f32, f32) {
    match style.border_collapse {
        BorderCollapse::Collapse => (0.0, 0.0),
        BorderCollapse::Separate => style.border_spacing,
    }
}

/// A table's max-content content width: its columns' widest demands, or
/// wider when its percentage columns need room for their share.
pub(crate) fn max_content_width(ctx: &Ctx<'_>, node: NodeId) -> f32 {
    let children = crate::child_nodes(ctx.document, node);
    let rows = collect_rows(ctx, &children);
    let declared = declared_columns(ctx, &children);
    let (spacing_x, _) = spacing(ctx.style(node));
    let demands = column_demands(ctx, &rows, &declared, 0.0, spacing_x);
    let columns = demands.maximums.iter().sum::<f32>();
    with_spacing(
        columns.max(widened_by_percents(&demands)),
        demands.maximums.len(),
        spacing_x,
    )
}

/// How wide a table's columns must be for each percentage column to hold
/// its maximum as its share, and the other columns theirs in what the
/// percentages leave, as browsers' auto layout widens a table. When the
/// percentages leave nothing for columns that want room, the table asks
/// for as much as it can get, as browsers do with their maximum width.
fn widened_by_percents(demands: &Demands) -> f32 {
    /// Browsers' cap on a table's width.
    const TABLE_MAX_WIDTH: f32 = 1_000_000.0;
    let (mut widest, mut total, mut others) = (0.0_f32, 0.0_f32, 0.0_f32);
    for (maximum, percent) in demands.maximums.iter().zip(&demands.percents) {
        match *percent {
            Some(percent) if percent > 0.0 => {
                widest = widest.max(maximum / percent);
                total += percent;
            }
            _ => others += maximum,
        }
    }
    if total == 0.0 {
        return 0.0;
    }
    let remaining = 1.0 - total;
    if remaining <= f32::EPSILON {
        return if others > 0.0 {
            TABLE_MAX_WIDTH
        } else {
            widest
        };
    }
    widest.max(others / remaining)
}

/// `columns` wide plus the spacing around `count` columns (none without any).
fn with_spacing(columns: f32, count: usize, spacing_x: f32) -> f32 {
    if count == 0 {
        columns
    } else {
        columns + spacing_x * (count + 1) as f32
    }
}

/// A table's min-content content width: its columns' minimums side by side.
pub(crate) fn min_content_width(ctx: &Ctx<'_>, node: NodeId) -> f32 {
    let children = crate::child_nodes(ctx.document, node);
    let rows = collect_rows(ctx, &children);
    let declared = declared_columns(ctx, &children);
    let (spacing_x, _) = spacing(ctx.style(node));
    let minimums = column_demands(ctx, &rows, &declared, 0.0, spacing_x).minimums;
    with_spacing(minimums.iter().sum(), minimums.len(), spacing_x)
}

/// What the columns ask for.
struct Demands {
    minimums: Vec<f32>,
    maximums: Vec<f32>,
    /// Whether a cell of the column (spanning it alone) has a pixel width:
    /// such a column keeps its width when the table has room to spare.
    fixed: Vec<bool>,
    /// The widest percentage width of a cell spanning the column alone, as
    /// a fraction of the table's content width.
    percents: Vec<Option<f32>>,
}

/// Each column's width demands: a cell demands its min-content and
/// max-content border-box widths (plus its margins, as track sizing counts
/// outer boxes); a spanning cell spreads its demands evenly across the
/// columns it covers, less the `spacing_x` between them. A `declared`
/// column width (from `col`/`colgroup`) counts as a cell's. A column with
/// a pixel width wants that width, or its minimum if wider; otherwise a
/// column's maximum is never below its minimum.
fn column_demands(
    ctx: &Ctx<'_>,
    rows: &[Row],
    declared: &[Dimension],
    content_width: f32,
    spacing_x: f32,
) -> Demands {
    let column_count = rows
        .iter()
        .map(|row| row.cells.iter().map(|cell| cell.span).sum::<usize>())
        .max()
        .unwrap_or(0)
        .max(declared.len());
    let mut minimums = vec![0.0_f32; column_count];
    let mut maximums = vec![0.0_f32; column_count];
    let mut fixed_widths: Vec<Option<f32>> = vec![None; column_count];
    let mut percents: Vec<Option<f32>> = vec![None; column_count];
    for (column, width) in declared.iter().enumerate() {
        match *width {
            Dimension::Px(width) => raise(&mut fixed_widths[column], width),
            Dimension::Percent(percent) if percent > 0.0 => {
                raise(&mut percents[column], percent / 100.0);
            }
            _ => {}
        }
    }
    for row in rows {
        for cell in &row.cells {
            let cell_style = ctx.style(cell.node);
            let margin = resolve::margins(&cell_style.margin, content_width);
            let outer = margin.left + margin.right;
            let span = cell.span as f32;
            let inner_gaps = spacing_x * (span - 1.0);
            let minimum = ((float::min_content_of_contents(ctx, cell.node) + outer - inner_gaps)
                / span)
                .max(0.0);
            let maximum =
                ((float::preferred_width(ctx, cell.node) + outer - inner_gaps) / span).max(0.0);
            if cell.span == 1 && cell.column < column_count {
                match cell_style.width {
                    Dimension::Px(_) => raise(&mut fixed_widths[cell.column], maximum),
                    Dimension::Percent(percent) if percent > 0.0 => {
                        raise(&mut percents[cell.column], percent / 100.0);
                    }
                    _ => {}
                }
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
    for ((maximum, &minimum), fixed_width) in maximums.iter_mut().zip(&minimums).zip(&fixed_widths)
    {
        *maximum = fixed_width.unwrap_or(*maximum).max(minimum);
    }
    // Percentages past 100% in total don't count: the columns after the
    // budget runs out lose theirs.
    let mut budget = 1.0_f32;
    for percent in percents.iter_mut().flatten() {
        *percent = percent.min(budget);
        budget -= *percent;
    }
    Demands {
        minimums,
        maximums,
        fixed: fixed_widths.iter().map(Option::is_some).collect(),
        percents,
    }
}

/// Raise `slot` to at least `value`, setting it if unset.
fn raise(slot: &mut Option<f32>, value: f32) {
    *slot = Some(slot.map_or(value, |known| known.max(value)));
}

/// The columns' widths in a content box `target` wide, after CSS 2
/// §17.5.2.2 as browsers implement it. Every column starts at its minimum;
/// then, while there is room:
/// 1. percentage columns grow toward their share of `target` (a table that
///    doesn't `fill` has no share to give);
/// 2. the other columns grow toward their maximums, in proportion to how
///    much more each wants;
/// 3. a table that `fill`s shares what is left among the auto columns (by
///    their widths), or failing those the fixed ones, then the percentage
///    ones, then all of them evenly.
///
/// Narrower than the minimums, the columns keep them and overflow.
fn fit_columns(demands: &Demands, target: f32, fill: bool) -> Vec<f32> {
    let count = demands.minimums.len();
    let mut columns = demands.minimums.clone();
    let mut room = target - columns.iter().sum::<f32>();
    let percent = |column: usize| if fill { demands.percents[column] } else { None };

    // 1. Percentage columns toward their share.
    let goals: Vec<f32> = (0..count)
        .map(|column| match percent(column) {
            Some(share) => (share * target).max(columns[column]),
            None => columns[column],
        })
        .collect();
    grow_toward(&mut columns, &goals, &mut room);

    // 2. The rest toward their maximums.
    let goals: Vec<f32> = (0..count)
        .map(|column| match percent(column) {
            Some(_) => columns[column],
            None => demands.maximums[column].max(columns[column]),
        })
        .collect();
    grow_toward(&mut columns, &goals, &mut room);

    // 3. What is left.
    if fill && room > f32::EPSILON && count > 0 {
        let auto: Vec<bool> = (0..count)
            .map(|column| !demands.fixed[column] && percent(column).is_none())
            .collect();
        let fixed: Vec<bool> = (0..count)
            .map(|column| demands.fixed[column] && percent(column).is_none())
            .collect();
        let percentage: Vec<bool> = (0..count).map(|column| percent(column).is_some()).collect();
        let group = [auto, fixed, percentage]
            .into_iter()
            .find(|group| group.iter().any(|&member| member))
            .unwrap_or_else(|| vec![true; count]);
        let wanted: f32 = (0..count)
            .filter(|&column| group[column])
            .map(|column| columns[column])
            .sum();
        let members = group.iter().filter(|&&member| member).count() as f32;
        for column in 0..count {
            if group[column] {
                columns[column] += if wanted > f32::EPSILON {
                    room * columns[column] / wanted
                } else {
                    room / members
                };
            }
        }
    }
    columns
}

/// Grow `columns` toward `goals` with up to `room`, each by its share of
/// the total wanted when there isn't enough for all.
fn grow_toward(columns: &mut [f32], goals: &[f32], room: &mut f32) {
    if *room <= 0.0 {
        return;
    }
    let wanted: f32 = columns
        .iter()
        .zip(goals)
        .map(|(width, goal)| (goal - width).max(0.0))
        .sum();
    if wanted <= f32::EPSILON {
        return;
    }
    let share = (*room / wanted).min(1.0);
    for (width, goal) in columns.iter_mut().zip(goals) {
        *width += (goal - *width).max(0.0) * share;
    }
    *room -= wanted * share;
}

/// The x of `column`'s left edge inside the table's content box: the
/// columns before it, each with `spacing_x` before it, and its own.
fn column_start(columns: &[f32], column: usize, spacing_x: f32) -> f32 {
    let before = column.min(columns.len());
    columns[..before].iter().sum::<f32>() + spacing_x * (before + 1) as f32
}

/// The total width of `span` columns starting at `column`, with the
/// spacing between them.
fn span_width(columns: &[f32], column: usize, span: usize, spacing_x: f32) -> f32 {
    let start = column.min(columns.len());
    let end = (column + span).min(columns.len());
    let inner = end.saturating_sub(start).saturating_sub(1);
    (columns[start..end].iter().sum::<f32>() + spacing_x * inner as f32).max(0.0)
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

/// The widths `col` and `colgroup` children declare, one per column they
/// span, in column order. A `col` without a width takes its group's; a
/// group without `col`s spans its own `span` columns.
fn declared_columns(ctx: &Ctx<'_>, children: &[NodeId]) -> Vec<Dimension> {
    let mut widths = Vec::new();
    let mut declare = |node: NodeId, width: &Dimension| {
        let span = span_of(ctx, node, "span");
        widths.extend(std::iter::repeat_n(width.clone(), span));
    };
    for &child in children {
        match display_of(ctx, child) {
            Some(Display::TableColumn) => declare(child, &ctx.style(child).width),
            Some(Display::TableColumnGroup) => {
                let group = &ctx.style(child).width;
                let columns: Vec<NodeId> = crate::child_nodes(ctx.document, child)
                    .into_iter()
                    .filter(|&col| display_of(ctx, col) == Some(Display::TableColumn))
                    .collect();
                if columns.is_empty() {
                    declare(child, group);
                }
                for col in columns {
                    let width = &ctx.style(col).width;
                    declare(
                        col,
                        if *width == Dimension::Auto {
                            group
                        } else {
                            width
                        },
                    );
                }
            }
            _ => {}
        }
    }
    widths
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
    let height = resolve::definite(&ctx.style(node).height).unwrap_or(0.0);
    Row { cells, height }
}

fn cell_of(ctx: &Ctx<'_>, node: NodeId, column: usize) -> Cell {
    let span = span_of(ctx, node, "colspan");
    Cell { node, column, span }
}

/// A `colspan` or `span` attribute: 1 when absent, invalid or zero.
fn span_of(ctx: &Ctx<'_>, node: NodeId, attribute: &str) -> usize {
    ctx.document
        .element(node)
        .and_then(|element| element.attr(attribute))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .clamp(1, MAX_SPAN)
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
