//! Resolving computed values (percentages included) against a containing block.

use noonoo_style::{ComputedStyle, Dimension, Edges, Length, LengthOrAuto};

/// Resolve a dimension against a reference length; `auto` is `None`.
pub fn dimension(value: Dimension, reference: f32) -> Option<f32> {
    match value {
        Dimension::Auto => None,
        Dimension::Px(px) => Some(px.max(0.0)),
        Dimension::Percent(percent) => Some((reference * percent / 100.0).max(0.0)),
    }
}

fn length(value: Length, reference: f32) -> f32 {
    match value {
        Length::Px(px) => px,
        Length::Percent(percent) => reference * percent / 100.0,
    }
}

/// Resolve margins against the containing block's width (`auto` → `0`).
pub fn margins(edges: &Edges<LengthOrAuto>, reference: f32) -> Edges<f32> {
    let resolve = |value| dimension(value, reference).unwrap_or(0.0);
    Edges {
        top: resolve(edges.top),
        right: resolve(edges.right),
        bottom: resolve(edges.bottom),
        left: resolve(edges.left),
    }
}

/// Resolve paddings against the containing block's width.
pub fn paddings(edges: &Edges<Length>, reference: f32) -> Edges<f32> {
    Edges {
        top: length(edges.top, reference).max(0.0),
        right: length(edges.right, reference).max(0.0),
        bottom: length(edges.bottom, reference).max(0.0),
        left: length(edges.left, reference).max(0.0),
    }
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
/// Percentages resolve against `reference` — the containing block's width for
/// both axes, a documented approximation (CSS resolves the vertical pair
/// against the containing block's height).
pub fn insets(edges: &Edges<Dimension>, reference: f32) -> Edges<Option<f32>> {
    let resolve = |value: Dimension| match value {
        Dimension::Auto => None,
        Dimension::Px(px) => Some(px),
        Dimension::Percent(percent) => Some(reference * percent / 100.0),
    };
    Edges {
        top: resolve(edges.top),
        right: resolve(edges.right),
        bottom: resolve(edges.bottom),
        left: resolve(edges.left),
    }
}
