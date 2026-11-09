//! Replaced elements with a loaded image: an `<img>` whose `src` is in the
//! layout's [`Images`](crate::Images) is sized from the image, not its
//! content (CSS 2 §10.3.2 and §10.6.2, simplified).
//!
//! Each axis takes its CSS size (HTML's `width`/`height` attributes arrive
//! as presentational hints), else follows the other axis through the natural ratio, else the
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
        let width = width_of(&style.width);
        let height = height_of(&style.height);

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

    /// The width-to-height ratio of embedded content that hasn't loaded,
    /// from its `width` and `height` attributes: HTML maps them to
    /// `aspect-ratio: auto w / h`, so an auto height follows the content
    /// width before (or without) the media's own size.
    pub(crate) fn attribute_ratio(&self, node: NodeId) -> Option<f32> {
        let element = self.document.element(node)?;
        if !matches!(
            element.name.as_str(),
            "img" | "video" | "canvas" | "iframe" | "embed" | "object"
        ) || self.image(node).is_some()
        {
            return None;
        }
        let number = |name: &str| {
            let value = element.attr(name)?.trim();
            let end = value
                .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                .unwrap_or(value.len());
            value[..end].parse::<f32>().ok().filter(|n| *n > 0.0)
        };
        Some(number("width")? / number("height")?)
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
