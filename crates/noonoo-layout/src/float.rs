//! Floats: `float: left`/`right` and `clear`.
//!
//! A floated box leaves normal flow but shapes its container: line boxes wrap
//! around it, later floats stack beside it, and the container's auto height
//! includes its bottom. This module owns the per-container float list — the
//! registry, the free band a line or float may use at a given height, clear
//! positions, and shrink-to-fit sizing for floats with `width: auto`.
//!
//! The registry lives for one **block formatting context**: every plain block
//! container inside it shares the list (so a paragraph wraps around a float
//! declared in a sibling block), while boxes that establish a new context —
//! clipping boxes, floats, flex containers, out-of-flow boxes — start fresh.
//! A container's *height* only counts floats from its own subtree.

use noonoo_dom::NodeId;
use noonoo_geometry::Rect;
use noonoo_style::{
    BoxSizing, Clear, ComputedStyle, Display, FlexDirection, FlexWrap, Float, WhiteSpace,
};

use crate::resolve;
use crate::{Ctx, child_nodes};

/// One registered float: its side and **margin box**, so collisions and clear
/// positions use the outer edges.
struct Slot {
    side: Float,
    rect: Rect,
}

/// The floats registered so far in one **block formatting context**.
///
/// Every block container inside the context shares one registry — that is what
/// lets a paragraph wrap around a float declared in a sibling block. Containers
/// that establish a new context (clipping boxes, floats, flex, out-of-flow)
/// start a fresh one, so their content ignores the outer floats.
pub(crate) struct Floats {
    slots: Vec<Slot>,
}

impl Floats {
    /// A fresh, empty context.
    pub fn new() -> Self {
        Self { slots: Vec::new() }
    }

    /// How many floats are registered.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    fn push(&mut self, side: Float, rect: Rect) {
        self.slots.push(Slot { side, rect });
    }

    /// The free horizontal band — `(left, right)` — within the box whose
    /// content area starts at `x` and is `width` wide, minus the floats that
    /// overlap the vertical range `[top, bottom)`.
    pub fn band(&self, x: f32, width: f32, top: f32, bottom: f32) -> (f32, f32) {
        let mut left = x;
        let mut right = x + width;
        for slot in &self.slots {
            if slot.rect.y < bottom && slot.rect.bottom() > top {
                match slot.side {
                    Float::Left => left = left.max(slot.rect.right()),
                    Float::Right => right = right.min(slot.rect.x),
                    Float::None => {}
                }
            }
        }
        (left, right)
    }

    /// Where a box with `clear` must start to sit below the relevant floats.
    pub fn clearance(&self, clear: Clear, y: f32) -> f32 {
        let mut bottom = y;
        for slot in &self.slots {
            let relevant = match clear {
                Clear::None => false,
                Clear::Left => slot.side == Float::Left,
                Clear::Right => slot.side == Float::Right,
                Clear::Both => true,
            };
            if relevant {
                bottom = bottom.max(slot.rect.bottom());
            }
        }
        bottom
    }

    /// The lowest bottom edge among floats below `y`, if any.
    fn next_bottom_after(&self, y: f32) -> Option<f32> {
        self.slots
            .iter()
            .map(|slot| slot.rect.bottom())
            .filter(|bottom| *bottom > y + f32::EPSILON)
            .reduce(f32::min)
    }

    /// The bottom of the deepest float registered *after* `base` (this
    /// container's own subtree), relative to `content_y`.
    ///
    /// Ancestors' and earlier siblings' floats shape this container's lines but
    /// must not stretch its height.
    pub fn depth_since(&self, base: usize, content_y: f32) -> f32 {
        self.slots[base..]
            .iter()
            .map(|slot| slot.rect.bottom())
            .fold(0.0f32, |max, bottom| max.max(bottom - content_y))
    }
}

/// Lay out `node` as a float and register it.
///
/// `content_x`/`content_width` are the containing block's content area; `top`
/// is the current flow position (the float never moves up from there). The
/// node's box is pushed into the tree so it paints and hit-tests as a child of
/// the container.
pub(crate) fn place(ctx: &mut Ctx<'_>, node: NodeId, content_x: f32, content_width: f32, top: f32) {
    let style = ctx.style(node).clone();
    let side = style.float;

    let margin = resolve::margins(&style.margin, content_width);
    let (available_for_layout, border_box_width) =
        shrink_wrapped_width(ctx, node, &style, content_width);
    let outer_width = border_box_width + margin.left + margin.right;

    // Drop down past earlier floats until the float fits beside them.
    let mut y = ctx.floats().clearance(style.clear, top);
    loop {
        let (band_left, band_right) =
            ctx.floats()
                .band(content_x, content_width, y, y + f32::EPSILON);
        if band_right - band_left >= outer_width {
            break;
        }
        match ctx.floats().next_bottom_after(y) {
            Some(next) => y = next,
            None => break, // nothing left to dodge: overflow the band
        }
    }
    let (band_left, band_right) = ctx
        .floats()
        .band(content_x, content_width, y, y + f32::EPSILON);
    let border_x = match side {
        Float::Left => band_left + margin.left,
        Float::Right => (band_right - margin.right - border_box_width).max(band_left + margin.left),
        Float::None => band_left,
    };
    let border_y = y + margin.top;

    let height = ctx.layout_element(node, border_x, border_y, available_for_layout, None);
    ctx.floats_mut().push(
        side,
        Rect::new(
            border_x - margin.left,
            border_y - margin.top,
            outer_width,
            height + margin.top + margin.bottom,
        ),
    );
}

/// The width a shrink-wrapped box (a float, an atomic inline) takes in a
/// containing block `content_width` wide: `(available, border_box_width)`,
/// where `available` is what to hand [`Ctx::layout_element`] so it lays the
/// box out at `border_box_width`.
///
/// An explicit width resolves against the containing block; `auto` shrinks to
/// fit the content, capped by the space available.
pub(crate) fn shrink_wrapped_width(
    ctx: &Ctx<'_>,
    node: NodeId,
    style: &ComputedStyle,
    content_width: f32,
) -> (f32, f32) {
    let padding = resolve::paddings(&style.padding, content_width);
    let border = resolve::borders(style);
    let horizontal = padding.left + padding.right + border.left + border.right;
    match resolve::dimension(style.width.clone(), content_width) {
        Some(width) => {
            // Percentages must resolve against the container, so
            // `layout_element` sees the container's width; the absolute value
            // is unaffected.
            (
                content_width,
                match style.box_sizing {
                    BoxSizing::BorderBox => width,
                    BoxSizing::ContentBox => width + horizontal,
                },
            )
        }
        None => {
            // `shrink_to_fit` is already a border-box width, and
            // `layout_element`'s auto branch derives the content width as
            // `available - horizontal`.
            let fit = shrink_to_fit(ctx, node, content_width);
            (fit, fit)
        }
    }
}

/// A float's shrink-to-fit content width: `min(max-content, available)`.
pub(crate) fn shrink_to_fit(ctx: &Ctx<'_>, node: NodeId, available: f32) -> f32 {
    preferred_width(ctx, node).min(available).max(0.0)
}

/// The border-box width a box would take with unwrapped content (max-content).
///
/// It depends only on the subtree and its styles, and walks all of it, so it
/// is remembered for the rest of the layout: grid and shrink-to-fit sizing
/// ask for it at every nesting level.
pub(crate) fn preferred_width(ctx: &Ctx<'_>, node: NodeId) -> f32 {
    if let Some(&width) = ctx.preferred.borrow().get(&node) {
        return width;
    }
    let width = compute_preferred_width(ctx, node);
    ctx.preferred.borrow_mut().insert(node, width);
    width
}

fn compute_preferred_width(ctx: &Ctx<'_>, node: NodeId) -> f32 {
    let style = ctx.style(node);
    // Percentages are unmeasurable without a reference; they count as zero.
    let padding = resolve::paddings(&style.padding, 0.0);
    let border = resolve::borders(style);
    let edges = padding.left + padding.right + border.left + border.right;
    // A percentage width has no reference here either: it counts as auto.
    let specified =
        resolve::dimension(style.width.clone(), f32::INFINITY).filter(|width| width.is_finite());
    let inner = match specified {
        Some(width) => match style.box_sizing {
            BoxSizing::BorderBox => (width - edges).max(0.0),
            BoxSizing::ContentBox => width,
        },
        None => preferred_content(ctx, node),
    };
    // `min-width`/`max-width` clamp it like a laid-out width (min wins).
    let to_border_box = |width: f32| match style.box_sizing {
        BoxSizing::BorderBox => width,
        BoxSizing::ContentBox => width + edges,
    };
    let mut width = inner + edges;
    if let Some(maximum) = resolve::dimension(style.max_width.clone(), f32::INFINITY)
        .filter(|maximum| maximum.is_finite())
    {
        width = width.min(to_border_box(maximum));
    }
    if let Some(minimum) = resolve::dimension(style.min_width.clone(), f32::INFINITY)
        .filter(|minimum| minimum.is_finite())
    {
        width = width.max(to_border_box(minimum));
    }
    width
}

/// A flex container's max-content width: in a row, its items side by side
/// with `column-gap` between them; in a column, the widest item. Adjacent
/// text children form one item, as in [`crate::flex`].
fn preferred_flex_content(ctx: &Ctx<'_>, node: NodeId, style: &ComputedStyle) -> f32 {
    let children = child_nodes(ctx.document, node);
    let mut widths = Vec::new();
    let mut index = 0;
    while index < children.len() {
        let child = children[index];
        if ctx.is_hidden(child) || ctx.is_out_of_flow(child) {
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
                widths.push(text_width(ctx, &text, style));
            }
            continue;
        }
        let margins = resolve::margins(&ctx.style(child).margin, 0.0);
        widths.push(preferred_width(ctx, child) + margins.left + margins.right);
        index += 1;
    }
    match style.flex_direction {
        FlexDirection::Row | FlexDirection::RowReverse => {
            widths.iter().sum::<f32>() + style.column_gap * widths.len().saturating_sub(1) as f32
        }
        FlexDirection::Column | FlexDirection::ColumnReverse => {
            widths.into_iter().fold(0.0, f32::max)
        }
    }
}

/// The max-content width of a box's contents: block children stack (take the
/// widest), inline content flows on one unwrapped line (take the sum).
fn preferred_content(ctx: &Ctx<'_>, node: NodeId) -> f32 {
    let style = ctx.style(node);
    if let Some((width, _)) = ctx.image_size(node, style, f32::INFINITY, None) {
        return width;
    }
    if let Some(width) = ctx.text_field_width(node, style) {
        return width;
    }
    if let Some(text) = ctx.placeholder(node) {
        return text_width(ctx, &text, style);
    }
    if matches!(style.display, Display::Flex | Display::InlineFlex) {
        return preferred_flex_content(ctx, node, style);
    }
    if style.display == Display::Table {
        return crate::table::max_content_width(ctx, node);
    }
    let mut widest = 0.0f32;
    let mut run = 0.0f32;
    // A row's cells, or a run of cells outside a table, sit side by side.
    let mut cells = 0.0f32;
    for child in child_nodes(ctx.document, node) {
        if ctx.is_hidden(child) || ctx.is_out_of_flow(child) {
            continue;
        }
        let is_cell = ctx
            .styles
            .get(child)
            .is_some_and(|child| child.display == Display::TableCell);
        if is_cell {
            let margins = resolve::margins(&ctx.style(child).margin, 0.0);
            cells += preferred_width(ctx, child) + margins.left + margins.right;
            continue;
        }
        if ctx.is_text(child)
            && cells > 0.0
            && noonoo_text::trim_collapsible(&ctx.document.text_content(child)).is_empty()
        {
            continue; // Whitespace between cells.
        }
        widest = widest.max(cells);
        cells = 0.0;
        if ctx.is_text(child) {
            // Text has no style of its own; it inherits the container's.
            let text = ctx.document.text_content(child);
            run += text_width(ctx, &text, style);
        } else if ctx.is_inline_level(child) {
            let margins = resolve::margins(&ctx.style(child).margin, 0.0);
            run += preferred_width(ctx, child) + margins.left + margins.right;
        } else {
            widest = widest.max(run);
            run = 0.0;
            let margins = resolve::margins(&ctx.style(child).margin, 0.0);
            widest = widest.max(preferred_width(ctx, child) + margins.left + margins.right);
        }
    }
    widest.max(run).max(cells)
}

/// The border-box width a box takes when it wraps wherever it can
/// (min-content): its longest word, widest inline box or block child. A
/// flex item with `min-width: auto` shrinks no further than this.
///
/// Remembered for the rest of the layout, like [`preferred_width`].
pub(crate) fn min_content_width(ctx: &Ctx<'_>, node: NodeId) -> f32 {
    if let Some(&width) = ctx.min_content.borrow().get(&node) {
        return width;
    }
    let width = compute_min_content_width(ctx, node);
    ctx.min_content.borrow_mut().insert(node, width);
    width
}

fn compute_min_content_width(ctx: &Ctx<'_>, node: NodeId) -> f32 {
    let style = ctx.style(node);
    let padding = resolve::paddings(&style.padding, 0.0);
    let border = resolve::borders(style);
    let edges = padding.left + padding.right + border.left + border.right;
    let to_border_box = |width: f32| match style.box_sizing {
        BoxSizing::BorderBox => width,
        BoxSizing::ContentBox => width + edges,
    };
    let mut width = match resolve::dimension(style.width.clone(), f32::INFINITY)
        .filter(|width| width.is_finite())
    {
        Some(width) => to_border_box(width),
        None => min_content(ctx, node, style) + edges,
    };
    if let Some(maximum) = resolve::dimension(style.max_width.clone(), f32::INFINITY)
        .filter(|maximum| maximum.is_finite())
    {
        width = width.min(to_border_box(maximum));
    }
    if let Some(minimum) = resolve::dimension(style.min_width.clone(), f32::INFINITY)
        .filter(|minimum| minimum.is_finite())
    {
        width = width.max(to_border_box(minimum));
    }
    width
}

/// The border box around a box's min-content *contents*, ignoring its own
/// `width` (clamped by `max-width`): a flex item's content size suggestion.
pub(crate) fn min_content_of_contents(ctx: &Ctx<'_>, node: NodeId) -> f32 {
    let style = ctx.style(node);
    let padding = resolve::paddings(&style.padding, 0.0);
    let border = resolve::borders(style);
    let edges = padding.left + padding.right + border.left + border.right;
    let width = min_content(ctx, node, style) + edges;
    match resolve::dimension(style.max_width.clone(), f32::INFINITY)
        .filter(|maximum| maximum.is_finite())
    {
        Some(maximum) => width.min(match style.box_sizing {
            BoxSizing::BorderBox => maximum,
            BoxSizing::ContentBox => maximum + edges,
        }),
        None => width,
    }
}

/// The min-content width of a box's contents. A single-line flex row keeps
/// its items side by side; everything else may break between items.
fn min_content(ctx: &Ctx<'_>, node: NodeId, style: &ComputedStyle) -> f32 {
    if let Some((width, _)) = ctx.image_size(node, style, f32::INFINITY, None) {
        return width;
    }
    if let Some(width) = ctx.text_field_width(node, style) {
        return width;
    }
    if let Some(text) = ctx.placeholder(node) {
        return text_min_width(ctx, &text, style);
    }
    let side_by_side = matches!(style.display, Display::Flex | Display::InlineFlex)
        && matches!(
            style.flex_direction,
            FlexDirection::Row | FlexDirection::RowReverse
        )
        && style.flex_wrap == FlexWrap::NoWrap;
    let mut widths = Vec::new();
    for child in child_nodes(ctx.document, node) {
        if ctx.is_hidden(child) || ctx.is_out_of_flow(child) {
            continue;
        }
        if ctx.is_text(child) {
            // Text has no style of its own; it inherits the container's.
            let text = ctx.document.text_content(child);
            if !noonoo_text::trim_collapsible(&text).is_empty() {
                widths.push(text_min_width(ctx, &text, style));
            }
        } else {
            let margins = resolve::margins(&ctx.style(child).margin, 0.0);
            widths.push(min_content_width(ctx, child) + margins.left + margins.right);
        }
    }
    if side_by_side {
        widths.iter().sum::<f32>() + style.column_gap * widths.len().saturating_sub(1) as f32
    } else {
        widths.into_iter().fold(0.0, f32::max)
    }
}

/// `text`'s min-content width: its longest word, or all of it where it
/// can't wrap.
fn text_min_width(ctx: &Ctx<'_>, text: &str, style: &ComputedStyle) -> f32 {
    let lines = crate::forced_lines(text, style.white_space);
    if lines.len() > 1 {
        return lines
            .into_iter()
            .map(|line| text_min_width(ctx, line, style))
            .fold(0.0, f32::max);
    }
    if matches!(style.white_space, WhiteSpace::NoWrap | WhiteSpace::Pre) {
        return text_width(ctx, text, style);
    }
    noonoo_text::collapsible_words(text)
        .map(|word| ctx.advance(word, style))
        .fold(0.0, f32::max)
}

/// Measure `text`'s unwrapped advance in `style`'s font (whitespace collapsed).
///
/// Wrappable text is measured the way line breaking measures it — words and
/// spaces apart — so text given exactly its max-content width never wraps
/// (a font can kern a joined string narrower than its parts). `nowrap` and
/// `pre` text is laid out as one string, and measured as one.
pub(crate) fn text_width(ctx: &Ctx<'_>, text: &str, style: &ComputedStyle) -> f32 {
    // Preserved newlines: the widest line.
    let lines = crate::forced_lines(text, style.white_space);
    if lines.len() > 1 {
        return lines
            .into_iter()
            .map(|line| text_width(ctx, line, style))
            .fold(0.0, f32::max);
    }
    // Preserved spaces count as they stand (trailing ones hang).
    if crate::preserves_spaces(style.white_space) {
        return ctx.advance(
            crate::expand_tabs(text, 0).trim_end_matches(noonoo_text::is_collapsible_space),
            style,
        );
    }
    let words: Vec<&str> = noonoo_text::collapsible_words(text).collect();
    if matches!(style.white_space, WhiteSpace::NoWrap | WhiteSpace::Pre) {
        return ctx.advance(&words.join(" "), style);
    }
    let space = ctx.advance(" ", style);
    words
        .iter()
        .map(|word| ctx.advance(word, style))
        .sum::<f32>()
        + space * words.len().saturating_sub(1) as f32
}
