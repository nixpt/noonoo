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
//! - a cell's demand is its max-content border-box width: internal table
//!   elements have no margins (CSS 2.1 §17.5), so a cell's are ignored;
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
//! - `border-collapse: collapse` is CSS 2.1's collapsing model, as
//!   Chromium lays it out: a shared edge is one border (`hidden` wins, then
//!   the wider, then the stronger style, then the cell nearer the top
//!   left; the table's own border competes on the outside), a column or
//!   row line is as wide as its widest edge, and each cell (and the table)
//!   takes half of the lines around it as its border; the resolved edges
//!   are painted from [`crate::BorderSegment`]s. Row, row-group and column
//!   borders don't take part, and the table's padding is dropped;
//! - table parts outside a table (cells or rows in a block) are wrapped in
//!   an anonymous shrink-to-fit table by block flow (see `block.rs`);
//! - `border-spacing` (separated borders only) sits before, between and
//!   after the columns and rows; a spanning cell takes the spacing it
//!   covers;
//! - captions, `table-layout: fixed`, and the rest of the
//!   anonymous fixup (stray content inside a table or row) are not
//!   modelled; stray content contributes nothing.

use noonoo_css::Color;
use noonoo_dom::NodeId;
use noonoo_geometry::Rect;
use noonoo_style::{
    BorderCollapse, BorderStyle, ComputedStyle, Dimension, Display, Edges, VerticalAlign,
};

use crate::{BorderSegment, Ctx, float, resolve};

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
    let demands = column_demands(ctx, &rows, &declared, spacing_x);
    let gaps = spacing_x * (demands.minimums.len() + 1) as f32;
    let columns = fit_columns(&demands, (content_width - gaps).max(0.0), fill);
    let column_start = |column: usize| column_start(&columns, column, spacing_x);
    let span_width = |column: usize, span: usize| span_width(&columns, column, span, spacing_x);

    // Place the rows. Measure first, so every cell can stretch to its row's
    // height: the measurement is real layout, thrown away.
    let mut indices = Vec::new();
    let mut cursor_y = content_y + spacing_y;
    let mut row_tops = Vec::with_capacity(rows.len() + 1);
    for row in &rows {
        row_tops.push(cursor_y);
        let mut measured = Vec::with_capacity(row.cells.len());
        for cell in &row.cells {
            // The cell fills its column: its own `width` already sized it.
            let border_box = span_width(cell.column, cell.span);
            ctx.flexed_width = Some((cell.node, border_box));
            let height = ctx.measure_height(
                cell.node,
                content_x + column_start(cell.column),
                cursor_y,
                border_box,
                None,
            );
            ctx.flexed_width = None;
            measured.push(height);
        }
        ctx.prune_pending();
        let row_height = measured.iter().copied().fold(row.height, f32::max);

        for (cell, height) in row.cells.iter().zip(&measured) {
            let border_box = span_width(cell.column, cell.span);
            ctx.flexed_width = Some((cell.node, border_box));
            let _height = ctx.layout_element(
                cell.node,
                content_x + column_start(cell.column),
                cursor_y,
                border_box,
                // The cell stretches to its row's height, so a background
                // fills it even when the neighbours are taller.
                Some(row_height.max(*height)),
            );
            ctx.flexed_width = None;
            if let Some(index) = ctx.tree_index(cell.node) {
                align_cell_content(ctx, cell.node, index, ctx.style(cell.node).vertical_align);
                indices.push(index);
            }
        }
        cursor_y += row_height + spacing_y;
    }
    row_tops.push(cursor_y);
    // A collapsed table is its children's parent (an anonymous table's
    // parent is a block, which has no grid).
    if let Some(table) = children
        .first()
        .and_then(|&child| ctx.document.tree().parent(child))
    {
        record_collapsed(ctx, table, &rows, &columns, &row_tops, content_x, content_y);
    }

    (indices, cursor_y - content_y)
}

/// Note what a collapsed table and its cells paint: the cells nothing of
/// their own, the table each resolved edge, centred on its grid line and
/// placed against the table's border box (`content_x`/`content_y` less the
/// halves of its outer lines).
fn record_collapsed(
    ctx: &mut Ctx<'_>,
    table: NodeId,
    rows: &[Row],
    columns: &[f32],
    row_tops: &[f32],
    content_x: f32,
    content_y: f32,
) {
    let Some(grid) = ctx.grids.get(&table) else {
        return;
    };
    let lines_x: Vec<f32> = (0..=grid.column_lines.len().saturating_sub(1))
        .map(|line| content_x + columns.iter().take(line).sum::<f32>())
        .collect();
    let origin_x = content_x - grid.column_lines[0] / 2.0;
    let origin_y = content_y - grid.row_lines[0] / 2.0;
    let mut segments = Vec::new();
    for (row, edges) in grid.vertical.iter().enumerate() {
        let (top, bottom) = (row_tops[row], row_tops[row + 1]);
        let (above, below) = (grid.row_lines[row] / 2.0, grid.row_lines[row + 1] / 2.0);
        for (line, edge) in edges.iter().enumerate() {
            if edge.width > 0.0 {
                segments.push(BorderSegment {
                    rect: Rect::new(
                        lines_x[line] - edge.width / 2.0 - origin_x,
                        top - above - origin_y,
                        edge.width,
                        bottom - top + above + below,
                    ),
                    color: edge.color,
                });
            }
        }
    }
    for (line, edges) in grid.horizontal.iter().enumerate() {
        for (column, edge) in edges.iter().enumerate() {
            if edge.width > 0.0 {
                segments.push(BorderSegment {
                    rect: Rect::new(
                        lines_x[column] - origin_x,
                        row_tops[line] - edge.width / 2.0 - origin_y,
                        lines_x[column + 1] - lines_x[column],
                        edge.width,
                    ),
                    color: edge.color,
                });
            }
        }
    }
    ctx.tree.collapsed.insert(table, segments);
    for row in rows {
        for cell in &row.cells {
            ctx.tree.collapsed.insert(cell.node, Vec::new());
        }
    }
}

/// One resolved edge of a collapsed table: its width and the winner's
/// colour.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Edge {
    width: f32,
    color: Color,
}

impl Default for Edge {
    fn default() -> Self {
        Edge {
            width: 0.0,
            color: Color::BLACK,
        }
    }
}

/// A collapsed table's resolved border grid.
pub(crate) struct Grid {
    /// `vertical[row][line]`: the edge on column line `line` in `row`.
    vertical: Vec<Vec<Edge>>,
    /// `horizontal[line][column]`: the edge on row line `line` over
    /// `column`.
    horizontal: Vec<Vec<Edge>>,
    /// Each column line's width: its widest edge.
    column_lines: Vec<f32>,
    /// Each row line's width.
    row_lines: Vec<f32>,
}

/// The cell of `row` covering `column`, if any.
fn covering(row: &Row, column: usize) -> Option<&Cell> {
    row.cells
        .iter()
        .find(|cell| cell.column <= column && column < cell.column + cell.span)
}

/// A collapsed table's grid, and the border each of its cells (and the
/// table) lays out with.
pub(crate) type Resolved = (Grid, Vec<(NodeId, Edges<f32>)>);

/// A border side as a candidate for a collapsed edge.
type Candidate = (f32, BorderStyle, Color);

fn top(style: &ComputedStyle) -> Candidate {
    (
        resolve::borders(style).top,
        style.border_style.top,
        style.border_color.top,
    )
}
fn right(style: &ComputedStyle) -> Candidate {
    (
        resolve::borders(style).right,
        style.border_style.right,
        style.border_color.right,
    )
}
fn bottom(style: &ComputedStyle) -> Candidate {
    (
        resolve::borders(style).bottom,
        style.border_style.bottom,
        style.border_color.bottom,
    )
}
fn left(style: &ComputedStyle) -> Candidate {
    (
        resolve::borders(style).left,
        style.border_style.left,
        style.border_color.left,
    )
}

/// CSS 2.1 §17.6.2.1's ranking of border styles, strongest last.
fn style_rank(style: BorderStyle) -> u8 {
    match style {
        BorderStyle::None | BorderStyle::Hidden => 0,
        BorderStyle::Inset => 1,
        BorderStyle::Groove => 2,
        BorderStyle::Outset => 3,
        BorderStyle::Ridge => 4,
        BorderStyle::Dotted => 5,
        BorderStyle::Dashed => 6,
        BorderStyle::Solid => 7,
        BorderStyle::Double => 8,
    }
}

/// The edge `candidates` collapse to (§17.6.2.1): any `hidden` hides it;
/// otherwise the widest, then the strongest style, then the first listed.
fn collapse_edge(candidates: &[Candidate]) -> Edge {
    if candidates
        .iter()
        .any(|(_, style, _)| *style == BorderStyle::Hidden)
    {
        return Edge::default();
    }
    let mut best: Option<Candidate> = None;
    for &candidate in candidates {
        let wins = best.is_none_or(|(width, style, _)| {
            candidate.0 > width
                || (candidate.0 == width && style_rank(candidate.1) > style_rank(style))
        });
        if wins && candidate.0 > 0.0 {
            best = Some(candidate);
        }
    }
    best.map_or_else(Edge::default, |(width, _, color)| Edge { width, color })
}

/// Resolve collapsed table `table`'s border grid, and the border each of
/// its cells (and the table itself) keeps: half of each line around it.
pub(crate) fn resolve_collapsed(ctx: &Ctx<'_>, table: NodeId) -> Option<Resolved> {
    let children = crate::child_nodes(ctx.document, table);
    let rows = collect_rows(ctx, &children);
    if rows.is_empty() {
        return None;
    }
    let table_style = ctx.style(table);
    let columns = rows
        .iter()
        .map(|row| {
            row.cells
                .iter()
                .map(|cell| cell.column + cell.span)
                .max()
                .unwrap_or(0)
        })
        .max()
        .unwrap_or(0);

    let mut vertical = Vec::with_capacity(rows.len());
    for row in &rows {
        let mut edges = Vec::with_capacity(columns + 1);
        for line in 0..=columns {
            let inside = row
                .cells
                .iter()
                .any(|cell| cell.column < line && line < cell.column + cell.span);
            if inside {
                edges.push(Edge::default());
                continue;
            }
            let mut candidates = Vec::with_capacity(3);
            if let Some(cell) = row
                .cells
                .iter()
                .find(|cell| cell.column + cell.span == line)
            {
                candidates.push(right(ctx.style(cell.node)));
            }
            if let Some(cell) = row.cells.iter().find(|cell| cell.column == line) {
                candidates.push(left(ctx.style(cell.node)));
            }
            if line == 0 {
                candidates.push(left(table_style));
            }
            if line == columns {
                candidates.push(right(table_style));
            }
            edges.push(collapse_edge(&candidates));
        }
        vertical.push(edges);
    }
    let mut horizontal = Vec::with_capacity(rows.len() + 1);
    for line in 0..=rows.len() {
        let mut edges = Vec::with_capacity(columns);
        for column in 0..columns {
            let mut candidates = Vec::with_capacity(3);
            if let Some(cell) = line.checked_sub(1).and_then(|r| covering(&rows[r], column)) {
                candidates.push(bottom(ctx.style(cell.node)));
            }
            if let Some(cell) = rows.get(line).and_then(|row| covering(row, column)) {
                candidates.push(top(ctx.style(cell.node)));
            }
            if line == 0 {
                candidates.push(top(table_style));
            }
            if line == rows.len() {
                candidates.push(bottom(table_style));
            }
            edges.push(collapse_edge(&candidates));
        }
        horizontal.push(edges);
    }
    let widest = |edges: &mut dyn Iterator<Item = f32>| edges.fold(0.0_f32, f32::max);
    let column_lines: Vec<f32> = (0..=columns)
        .map(|line| widest(&mut vertical.iter().map(|edges: &Vec<Edge>| edges[line].width)))
        .collect();
    let row_lines: Vec<f32> = horizontal
        .iter()
        .map(|edges| widest(&mut edges.iter().map(|edge| edge.width)))
        .collect();

    let mut borders = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        for cell in &row.cells {
            borders.push((
                cell.node,
                Edges {
                    top: row_lines[index] / 2.0,
                    right: column_lines[cell.column + cell.span] / 2.0,
                    bottom: row_lines[index + 1] / 2.0,
                    left: column_lines[cell.column] / 2.0,
                },
            ));
        }
    }
    borders.push((
        table,
        Edges {
            top: row_lines[0] / 2.0,
            right: column_lines[columns] / 2.0,
            bottom: row_lines[rows.len()] / 2.0,
            left: column_lines[0] / 2.0,
        },
    ));
    Some((
        Grid {
            vertical,
            horizontal,
            column_lines,
            row_lines,
        },
        borders,
    ))
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
    let demands = column_demands(ctx, &rows, &declared, spacing_x);
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
    let minimums = column_demands(ctx, &rows, &declared, spacing_x).minimums;
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
/// max-content border-box widths (a cell has no margins, CSS 2.1 §17.5);
/// a spanning cell spreads its demands evenly across the
/// columns it covers, less the `spacing_x` between them. A `declared`
/// column width (from `col`/`colgroup`) counts as a cell's. A column with
/// a pixel width wants that width, or its minimum if wider; otherwise a
/// column's maximum is never below its minimum.
fn column_demands(ctx: &Ctx<'_>, rows: &[Row], declared: &[Dimension], spacing_x: f32) -> Demands {
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
            let span = cell.span as f32;
            let inner_gaps = spacing_x * (span - 1.0);
            let minimum =
                ((float::min_content_of_contents(ctx, cell.node) - inner_gaps) / span).max(0.0);
            let maximum = ((float::preferred_width(ctx, cell.node) - inner_gaps) / span).max(0.0);
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
