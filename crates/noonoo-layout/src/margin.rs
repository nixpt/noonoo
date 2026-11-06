//! Vertical margin collapsing (the CSS 2.1 §8.3.1 subset Noonoo models).
//!
//! Margins collapse in the block direction only:
//!
//! - **Adjacent siblings** share one margin — the larger of the two facing
//!   margins (negative margins combine as `max(positive) + min(negative)`).
//! - **A block and its first/last in-flow child** collapse their top/bottom
//!   margins *through* the block when nothing separates them — no border, no
//!   padding, and (for the bottom) an `auto` height. The collapsed margin is
//!   presented to the block's parent.
//!
//! The result is computed structurally, without geometry: [`effective_margins`]
//! descends the first/last-child chain and resolves the margins a box presents
//! upward. Layout then places border boxes from those values.
//!
//! Not modelled: collapse-through of *empty* blocks (a block with no content and
//! no border/padding), and `min-height` interactions.

use noonoo_dom::NodeId;
use noonoo_style::{Dimension, Display, Edges};

use crate::resolve;
use crate::{Ctx, child_nodes};

/// Collapse two adjoining margins into one: the largest positive plus the most
/// negative.
pub(crate) fn collapse(first: f32, second: f32) -> f32 {
    first.max(0.0).max(second.max(0.0)) + first.min(0.0).min(second.min(0.0))
}

/// The margins a block box presents to its parent.
///
/// `left`/`right` are the ordinary resolved horizontal margins; `top`/`bottom`
/// are the *effective* vertical margins, already collapsed through the box's
/// first/last in-flow child where the rules allow.
pub(crate) fn effective_margins(ctx: &Ctx<'_>, node: NodeId, width: f32) -> Edges<f32> {
    let style = ctx.style(node);
    let margins = resolve::margins(&style.margin, width);

    // A flex container establishes a new formatting context: its margins never
    // collapse with its items.
    if matches!(style.display, Display::Flex | Display::InlineFlex) {
        return margins;
    }

    let padding = resolve::paddings(&style.padding, width);
    let border = resolve::borders(style);
    let top_open = border.top == 0.0 && padding.top == 0.0;
    let bottom_open =
        border.bottom == 0.0 && padding.bottom == 0.0 && matches!(style.height, Dimension::Auto);

    let horizontal =
        margins.left + margins.right + padding.left + padding.right + border.left + border.right;
    let content_width = (width - horizontal).max(0.0);

    let top = if top_open {
        first_block_child(ctx, node)
            .map(|child| {
                collapse(
                    margins.top,
                    effective_margins(ctx, child, content_width).top,
                )
            })
            .unwrap_or(margins.top)
    } else {
        margins.top
    };

    let bottom = if bottom_open {
        last_block_child(ctx, node)
            .map(|child| {
                collapse(
                    margins.bottom,
                    effective_margins(ctx, child, content_width).bottom,
                )
            })
            .unwrap_or(margins.bottom)
    } else {
        margins.bottom
    };

    Edges {
        top,
        right: margins.right,
        bottom,
        left: margins.left,
    }
}

/// The first in-flow child that participates in collapsing: a block-level
/// element recurses; inline content (or nothing) stops the collapse.
fn first_block_child(ctx: &Ctx<'_>, node: NodeId) -> Option<NodeId> {
    for child in child_nodes(ctx.document, node) {
        if ctx.is_hidden(child) || ctx.is_out_of_flow(child) {
            continue;
        }
        return (!ctx.is_text(child) && ctx.is_block_level(child)).then_some(child);
    }
    None
}

fn last_block_child(ctx: &Ctx<'_>, node: NodeId) -> Option<NodeId> {
    for child in child_nodes(ctx.document, node).into_iter().rev() {
        if ctx.is_hidden(child) || ctx.is_out_of_flow(child) {
            continue;
        }
        return (!ctx.is_text(child) && ctx.is_block_level(child)).then_some(child);
    }
    None
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
}
