//! Vertical margin collapsing (the CSS 2.1 §8.3.1 subset Noonoo models).
//!
//! Margins collapse in the block direction only:
//!
//! - **Adjacent siblings** share one margin — the larger of the two facing
//!   margins (negative margins combine as `max(positive) + min(negative)`).
//! - **A block and its first/last in-flow child** collapse their top/bottom
//!   margins *through* the block when nothing separates them — no border, no
//!   padding, and (for the bottom) an `auto` height.
//! - **An empty block** — no border, padding, height, or in-flow content — has
//!   adjoining top and bottom margins: they collapse into a single margin that
//!   joins the surrounding chain (a *collapsed-through* box). This is
//!   recursive: a block whose only children are themselves collapsed-through is
//!   collapsed-through too.
//!
//! The result is computed structurally, without geometry: [`effective_margins`]
//! walks the leading/trailing chains and resolves the margins a box presents
//! upward. Layout then places border boxes from those values.
//!
//! Not modelled: `min-height` interactions, and clearance from floats.

use std::cell::RefCell;
use std::collections::HashMap;

use noonoo_dom::NodeId;
use noonoo_style::{Dimension, Display};

use crate::resolve;
use crate::{Ctx, child_nodes};

/// Collapse two adjoining margins into one: the largest positive plus the most
/// negative.
pub(crate) fn collapse(first: f32, second: f32) -> f32 {
    first.max(0.0).max(second.max(0.0)) + first.min(0.0).min(second.min(0.0))
}

/// Memoised [`effective_margins`] and [`is_collapse_through`], keyed by node
/// and containing width (as bits). Each block's first and last child is asked
/// for its margins by both edges and again by the parent's layout, so without
/// this the cost doubles at every nesting level.
#[derive(Default)]
pub(crate) struct Cache {
    margins: RefCell<HashMap<(NodeId, u32), VerticalMargins>>,
    through: RefCell<HashMap<(NodeId, u32), bool>>,
}

/// The margins a block box presents to its parent.
#[derive(Clone, Copy)]
pub(crate) struct VerticalMargins {
    /// The effective top margin (already collapsed through the leading chain).
    pub top: f32,
    /// The effective bottom margin.
    pub bottom: f32,
    /// The resolved left margin (horizontal margins never collapse).
    pub left: f32,
    /// A collapsed-through box's own top margin with its children's leading
    /// ones, the bottom margins left out: where its top border edge goes
    /// (CSS 2 §8.3.1). The same as `top` for any other box.
    pub leading: f32,
    /// Whether the box is *collapsed through*: an empty box whose top and bottom
    /// margins are actually one margin passing through it.
    pub through: bool,
}

/// The margins a block box presents to its parent.
pub(crate) fn effective_margins(ctx: &Ctx<'_>, node: NodeId, width: f32) -> VerticalMargins {
    let key = (node, width.to_bits());
    if let Some(&cached) = ctx.margins.margins.borrow().get(&key) {
        return cached;
    }
    let margins = compute_effective_margins(ctx, node, width);
    ctx.margins.margins.borrow_mut().insert(key, margins);
    margins
}

fn compute_effective_margins(ctx: &Ctx<'_>, node: NodeId, width: f32) -> VerticalMargins {
    let style = ctx.style(node);
    let margin = resolve::margins(&style.margin, width);
    let padding = resolve::paddings(&style.padding, width);
    let border = resolve::borders(style);
    // A formatting context root (a clipping box, a cell, an inline-block, a
    // float, ...): its margins never collapse with its children's.
    let clipping = resolve::clips(style) || ctx.establishes_bfc(node, style);
    let top_open = border.top == 0.0 && padding.top == 0.0 && !clipping;
    let bottom_open = border.bottom == 0.0
        && padding.bottom == 0.0
        && matches!(style.height, Dimension::Auto)
        && !clipping
        && !ctx.quirks_fills(node, style);

    let horizontal =
        margin.left + margin.right + padding.left + padding.right + border.left + border.right;
    let content_width = (width - horizontal).max(0.0);

    if top_open && bottom_open && is_collapse_through(ctx, node, width) {
        // Every adjoining margin — the box's own and each (through) child's —
        // collapses into one.
        let mut through = collapse(margin.top, margin.bottom);
        for child in child_nodes(ctx.document, node) {
            if ctx.is_hidden(child) || ctx.is_out_of_flow(child) || is_phantom(ctx, child) {
                continue;
            }
            let child_margin = effective_margins(ctx, child, content_width);
            through = collapse(through, child_margin.top);
            through = collapse(through, child_margin.bottom);
        }
        return VerticalMargins {
            top: through,
            bottom: through,
            left: margin.left,
            leading: leading_margin(ctx, node, margin.top, content_width),
            through: true,
        };
    }

    let top = if top_open {
        leading_margin(ctx, node, margin.top, content_width)
    } else {
        margin.top
    };
    let bottom = if bottom_open {
        trailing_margin(ctx, node, margin.bottom, content_width)
    } else {
        margin.bottom
    };

    VerticalMargins {
        top,
        bottom,
        left: margin.left,
        leading: top,
        through: false,
    }
}

/// Fold `own` with the leading margins of the children.
///
/// A collapsed-through child contributes its margin and the walk continues; the
/// first non-through block contributes its own leading margin and stops; inline
/// content (a line box) stops the collapse with no margin of its own.
fn leading_margin(ctx: &Ctx<'_>, node: NodeId, own: f32, content_width: f32) -> f32 {
    let mut margin = own;
    for child in child_nodes(ctx.document, node) {
        if ctx.is_hidden(child) || ctx.is_out_of_flow(child) || is_phantom(ctx, child) {
            continue;
        }
        if ctx.is_text(child) || ctx.is_inline_level(child) {
            // A block at the very start of an inline element still adjoins.
            if let Some(block) = edge_block(ctx, child, true) {
                margin = collapse(margin, effective_margins(ctx, block, content_width).top);
            }
            break;
        }
        let child_margin = effective_margins(ctx, child, content_width);
        margin = collapse(margin, child_margin.top);
        if !child_margin.through {
            break;
        }
    }
    margin
}

/// The mirror of [`leading_margin`] for the bottom edge.
fn trailing_margin(ctx: &Ctx<'_>, node: NodeId, own: f32, content_width: f32) -> f32 {
    let mut margin = own;
    for child in child_nodes(ctx.document, node).into_iter().rev() {
        if ctx.is_hidden(child) || ctx.is_out_of_flow(child) || is_phantom(ctx, child) {
            continue;
        }
        if ctx.is_text(child) || ctx.is_inline_level(child) {
            if let Some(block) = edge_block(ctx, child, false) {
                margin = collapse(margin, effective_margins(ctx, block, content_width).bottom);
            }
            break;
        }
        let child_margin = effective_margins(ctx, child, content_width);
        margin = collapse(margin, child_margin.bottom);
        if !child_margin.through {
            break;
        }
    }
    margin
}

/// What an inline element's content starts (or ends) with, for margins.
enum Edge {
    /// A block inside it, with nothing on a line before (after) it.
    Block(NodeId),
    /// Nothing at all: white space and empty, edgeless inline elements.
    Empty,
    /// Line content: text, an atomic inline, a break, or an inline's edges.
    Content,
}

/// The block at the start (`from_start`) or end of inline element `node`,
/// when nothing that makes a line comes before (after) it: its margins then
/// adjoin the container's, as CSS 2 §9.2.1.1's anonymous blocks around it
/// are empty.
pub(crate) fn edge_block(ctx: &Ctx<'_>, node: NodeId, from_start: bool) -> Option<NodeId> {
    match inline_edge(ctx, node, from_start) {
        Edge::Block(block) => Some(block),
        Edge::Empty | Edge::Content => None,
    }
}

/// Whether `node` is nothing a line box is made of (CSS 2 §9.4.2): blank
/// collapsible text, or a plain inline element holding only such — no text,
/// no atomic or replaced box, no `<br>`, no padding, border or horizontal
/// margin. A line of phantoms takes no height and margins collapse past it.
pub(crate) fn is_phantom(ctx: &Ctx<'_>, node: NodeId) -> bool {
    if ctx.is_text(node) {
        return ctx.is_collapsible_blank(node);
    }
    ctx.style(node).display == Display::Inline
        && matches!(inline_edge(ctx, node, true), Edge::Empty)
}

/// An element that draws something of its own, children or not.
fn is_replaced(ctx: &Ctx<'_>, node: NodeId) -> bool {
    ctx.document.element(node).is_some_and(|e| {
        matches!(
            e.name.as_str(),
            "br" | "img"
                | "input"
                | "textarea"
                | "select"
                | "button"
                | "svg"
                | "canvas"
                | "video"
                | "audio"
                | "iframe"
                | "object"
                | "embed"
        )
    })
}

fn inline_edge(ctx: &Ctx<'_>, node: NodeId, from_start: bool) -> Edge {
    if ctx.is_text(node) || crate::is_atomic_inline(ctx.style(node).display) {
        return Edge::Content;
    }
    if is_replaced(ctx, node) {
        return Edge::Content;
    }
    let style = ctx.style(node);
    let padding = resolve::paddings(&style.padding, 0.0);
    let border = resolve::borders(style);
    let margin = resolve::margins(&style.margin, 0.0);
    if padding.left + padding.right + padding.top + padding.bottom != 0.0
        || border.left + border.right + border.top + border.bottom != 0.0
        || margin.left + margin.right != 0.0
    {
        return Edge::Content;
    }
    let mut children = child_nodes(ctx.document, node);
    if !from_start {
        children.reverse();
    }
    for child in children {
        if ctx.is_hidden(child) || ctx.is_out_of_flow(child) || ctx.is_float(child) {
            continue;
        }
        if ctx.is_text(child) {
            if is_phantom(ctx, child) {
                continue;
            }
            return Edge::Content;
        }
        if !ctx.is_inline_level(child) {
            return Edge::Block(child);
        }
        match inline_edge(ctx, child, from_start) {
            Edge::Empty => {}
            edge => return edge,
        }
    }
    Edge::Empty
}

/// Whether a box's top and bottom margins are adjoining: no border, padding,
/// height, or in-flow content other than further collapsed-through blocks.
fn is_collapse_through(ctx: &Ctx<'_>, node: NodeId, width: f32) -> bool {
    let key = (node, width.to_bits());
    if let Some(&cached) = ctx.margins.through.borrow().get(&key) {
        return cached;
    }
    let through = compute_is_collapse_through(ctx, node, width);
    ctx.margins.through.borrow_mut().insert(key, through);
    through
}

fn compute_is_collapse_through(ctx: &Ctx<'_>, node: NodeId, width: f32) -> bool {
    let style = ctx.style(node);
    let padding = resolve::paddings(&style.padding, width);
    let border = resolve::borders(style);
    if border.top != 0.0
        || border.bottom != 0.0
        || padding.top != 0.0
        || padding.bottom != 0.0
        || !matches!(style.height, Dimension::Auto)
        || resolve::clips(style)
        || ctx.establishes_bfc(node, style)
        || is_replaced(ctx, node)
    {
        return false;
    }

    let is_flex = matches!(style.display, Display::Flex | Display::InlineFlex);
    let horizontal = padding.left + padding.right + border.left + border.right;
    let content_width = (width - horizontal).max(0.0);

    for child in child_nodes(ctx.document, node) {
        if ctx.is_hidden(child) || ctx.is_out_of_flow(child) {
            continue;
        }
        // White space between blocks is nothing; in a flex container it
        // makes no item either.
        if is_phantom(ctx, child) {
            continue;
        }
        // A flex item is content, whatever its own size.
        if is_flex {
            return false;
        }
        if ctx.is_text(child) || ctx.is_inline_level(child) {
            return false;
        }
        if !is_collapse_through(ctx, child, content_width) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::collapse;

    #[test]
    fn positive_margins_take_the_larger() {
        assert_eq!(collapse(10.0, 20.0), 20.0);
        assert_eq!(collapse(8.0, 0.0), 8.0);
    }

    #[test]
    fn negative_margins_take_the_most_negative() {
        assert_eq!(collapse(-10.0, -4.0), -10.0);
    }

    #[test]
    fn mixed_margins_sum_positive_and_negative() {
        assert_eq!(collapse(10.0, -4.0), 6.0);
        assert_eq!(collapse(-4.0, 10.0), 6.0);
    }

    #[test]
    fn collapse_is_idempotent_on_equal_values() {
        assert_eq!(collapse(collapse(30.0, 30.0), 30.0), 30.0);
    }
}
