//! Resolving computed values (percentages included) against a containing block.

use noonoo_style::{ComputedStyle, Dimension, Edges, Length, LengthOrAuto, Overflow};

/// `px` truncated toward zero to a whole number of 1/64 px, the unit
/// Chromium lays out in (its `LayoutUnit`): a `.9em` margin at 16px is
/// 14.390625px there, and a long list of them drifts without this.
pub(crate) fn snap(px: f32) -> f32 {
    (px * 64.0).trunc() / 64.0
}

/// Resolve a dimension against a reference length; `auto` is `None`.
pub fn dimension(value: Dimension, reference: f32) -> Option<f32> {
    value.resolve(reference).map(|value| snap(value.max(0.0)))
}

/// Resolve a dimension that must not depend on a percentage.
///
/// Used for heights, where the containing block's height is usually unknown
/// while the box is being sized: a percentage — or a math result containing
/// one — behaves as `auto`, per CSS for an indefinite containing block.
pub fn definite(value: &Dimension) -> Option<f32> {
    match value {
        Dimension::Auto | Dimension::Percent(_) => None,
        Dimension::Calc { px, percent } => (percent == &0.0).then_some(snap(*px)),
        Dimension::Extreme { terms, .. } => terms
            .iter()
            .all(|term| term.percent == 0.0)
            .then(|| snap(value.resolve(0.0).unwrap_or(0.0))),
        Dimension::Px(px) => Some(snap(*px)),
    }
}

fn length(value: Length, reference: f32) -> f32 {
    snap(value.resolve(reference))
}

/// Resolve margins against the containing block's width.
///
/// Unlike [`dimension`], margins may be negative, so no clamp is applied;
/// `auto` resolves to zero.
pub fn margins(edges: &Edges<LengthOrAuto>, reference: f32) -> Edges<f32> {
    let resolve = |value: &Dimension| snap(value.resolve(reference).unwrap_or(0.0));
    Edges {
        top: resolve(&edges.top),
        right: resolve(&edges.right),
        bottom: resolve(&edges.bottom),
        left: resolve(&edges.left),
    }
}

/// Resolve paddings against the containing block's width.
pub fn paddings(edges: &Edges<Length>, reference: f32) -> Edges<f32> {
    Edges {
        top: length(edges.top.clone(), reference).max(0.0),
        right: length(edges.right.clone(), reference).max(0.0),
        bottom: length(edges.bottom.clone(), reference).max(0.0),
        left: length(edges.left.clone(), reference).max(0.0),
    }
}

/// Whether a box clips its content: any `overflow` other than `visible`.
pub(crate) fn clips(style: &ComputedStyle) -> bool {
    !matches!(style.overflow_x, Overflow::Visible) || !matches!(style.overflow_y, Overflow::Visible)
}

/// Whether a box scrolls vertically (`overflow-y: auto` / `scroll`).
pub(crate) fn scrollable_y(style: &ComputedStyle) -> bool {
    matches!(style.overflow_y, Overflow::Scroll | Overflow::Auto)
}

/// Whether a box scrolls horizontally (`overflow-x: auto` / `scroll`).
pub(crate) fn scrollable_x(style: &ComputedStyle) -> bool {
    matches!(style.overflow_x, Overflow::Scroll | Overflow::Auto)
}

/// Resolve border widths, collapsing hidden borders to zero.
pub fn borders(style: &ComputedStyle) -> Edges<f32> {
    let width = |width: f32, visible: bool| if visible { width } else { 0.0 };
    Edges {
        top: width(style.border_width.top, style.border_style.top.is_visible()),
        right: width(
            style.border_width.right,
            style.border_style.right.is_visible(),
        ),
        bottom: width(
            style.border_width.bottom,
            style.border_style.bottom.is_visible(),
        ),
        left: width(
            style.border_width.left,
            style.border_style.left.is_visible(),
        ),
    }
}

/// Resolve inset offsets (`top`/`right`/`bottom`/`left`); `auto` is `None`.
///
/// `left`/`right` percentages are of `width`, `top`/`bottom` of `height`
/// (the containing block's size). In-flow relative positioning, which has no
/// containing-block height yet, passes the width for both.
pub fn insets(edges: &Edges<Dimension>, width: f32, height: f32) -> Edges<Option<f32>> {
    Edges {
        top: edges.top.resolve(height).map(snap),
        right: edges.right.resolve(width).map(snap),
        bottom: edges.bottom.resolve(height).map(snap),
        left: edges.left.resolve(width).map(snap),
    }
}
