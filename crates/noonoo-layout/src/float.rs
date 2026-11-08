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
use noonoo_style::{BoxSizing, Clear, ComputedStyle, Display, FlexDirection, Float};

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
    let specified = resolve::dimension(style.width.clone(), f32::INFINITY);
    let inner = match specified {
        Some(width) => match style.box_sizing {
            BoxSizing::BorderBox => (width - edges).max(0.0),
            BoxSizing::ContentBox => width,
        },
        None => preferred_content(ctx, node),
    };
    inner + edges
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
    if let Some(text) = ctx.placeholder(node) {
        return text_width(ctx, &text, style);
    }
    if matches!(style.display, Display::Flex | Display::InlineFlex) {
        return preferred_flex_content(ctx, node, style);
    }
    let mut widest = 0.0f32;
    let mut run = 0.0f32;
    for child in child_nodes(ctx.document, node) {
        if ctx.is_hidden(child) || ctx.is_out_of_flow(child) {
            continue;
        }
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
    widest.max(run)
}

/// Measure `text`'s unwrapped advance in `style`'s font (whitespace collapsed).
pub(crate) fn text_width(ctx: &Ctx<'_>, text: &str, style: &ComputedStyle) -> f32 {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    ctx.advance(&collapsed, style)
}
