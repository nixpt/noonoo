//! Replaced elements with a loaded image: an `<img>` whose `src` is in the
//! layout's [`Images`](crate::Images) is sized from the image, not its
//! content (CSS 2 §10.3.2 and §10.6.2, simplified).
//!
//! Each axis takes its CSS size, else its HTML attribute (a presentational
//! hint), else follows the other axis through the natural ratio, else the
//! natural size. `min-`/`max-` sizes then clamp it, carrying the ratio to
//! an axis that wasn't given.

use noonoo_dom::NodeId;
use noonoo_style::{BoxSizing, ComputedStyle, Dimension};

use crate::image::Image;
use crate::{Ctx, resolve};

impl Ctx<'_> {
    /// The loaded image `node` shows, if it's an `<img>` with one.
    pub(crate) fn image(&self, node: NodeId) -> Option<&Image> {
        let element = self.document.element(node)?;
        if element.name != "img" {
            return None;
        }
        self.images.get(element.attr("src")?)
    }

    /// The content-box size of `node` if it's an image with a loaded
    /// source. Widths resolve against `width_basis` (infinite while
    /// measuring a preferred width, where percentages don't count) and
    /// heights against `height_basis`, when it's definite.
    pub(crate) fn image_size(
        &self,
        node: NodeId,
        style: &ComputedStyle,
        width_basis: f32,
        height_basis: Option<f32>,
    ) -> Option<(f32, f32)> {
        let image = self.image(node)?;
        let element = self.document.element(node)?;
        let padding_basis = if width_basis.is_finite() {
            width_basis
        } else {
            0.0
        };
        let padding = resolve::paddings(&style.padding, padding_basis);
        let border = resolve::borders(style);
        let horizontal = padding.left + padding.right + border.left + border.right;
        let vertical = padding.top + padding.bottom + border.top + border.bottom;
        let content = |value: f32, edges: f32| match style.box_sizing {
            BoxSizing::BorderBox => (value - edges).max(0.0),
            BoxSizing::ContentBox => value,
        };
        let width_of = |value: &Dimension| {
            resolve::dimension(value.clone(), width_basis)
                .filter(|width| width.is_finite())
                .map(|width| content(width, horizontal))
        };
        let height_of = |value: &Dimension| {
            match height_basis {
                Some(basis) => resolve::dimension(value.clone(), basis),
                None => resolve::definite(value),
            }
            .map(|height| content(height, vertical))
        };
        let attribute = |name: &str| element.attr(name).and_then(parse_dimension);
        let width =
            width_of(&style.width).or_else(|| attribute("width").and_then(|w| width_of(&w)));
        let height =
            height_of(&style.height).or_else(|| attribute("height").and_then(|h| height_of(&h)));

        let ratio = image.width as f32 / image.height as f32;
        let (mut w, mut h) = match (width, height) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, w / ratio),
            (None, Some(h)) => (h * ratio, h),
            (None, None) => (image.width as f32, image.height as f32),
        };
        let clamp = |value: f32, min: Option<f32>, max: Option<f32>| {
            // `min` wins over `max`, as for any box.
            let value = max.map_or(value, |max| value.min(max));
            min.map_or(value, |min| value.max(min))
        };
        let clamped = clamp(w, width_of(&style.min_width), width_of(&style.max_width));
        if clamped != w {
            w = clamped;
            if height.is_none() {
                h = w / ratio;
            }
        }
        let clamped = clamp(
            h,
            height_of(&style.min_height),
            height_of(&style.max_height),
        );
        if clamped != h {
            h = clamped;
            if width.is_none() {
                w = h * ratio;
            }
        }
        Some((w, h))
    }

    /// Note the image `node` draws; its box has no content to lay out.
    pub(crate) fn record_image(&mut self, node: NodeId) -> (Vec<usize>, f32) {
        if let Some(image) = self.image(node) {
            let encoded = image.encoded.clone();
            self.tree.images.insert(node, encoded);
        }
        (Vec::new(), 0.0)
    }
}

/// An HTML dimension attribute (`width="40"`, `width="50%"`): a
/// non-negative number, optionally a percentage; trailing junk is ignored.
fn parse_dimension(value: &str) -> Option<Dimension> {
    let value = value.trim_start();
    let digits = value
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(value.len());
    let number: f32 = value[..digits].parse().ok()?;
    if !number.is_finite() {
        return None;
    }
    Some(if value[digits..].starts_with('%') {
        Dimension::Percent(number)
    } else {
        Dimension::Px(number)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimension_attributes_parse_like_html() {
        assert_eq!(parse_dimension("40"), Some(Dimension::Px(40.0)));
        assert_eq!(parse_dimension(" 12.5px"), Some(Dimension::Px(12.5)));
        assert_eq!(parse_dimension("50%"), Some(Dimension::Percent(50.0)));
        assert_eq!(parse_dimension("-4"), None);
        assert_eq!(parse_dimension("auto"), None);
        assert_eq!(parse_dimension(""), None);
    }
}
