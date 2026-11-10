//! Replaced elements with a loaded image: an `<img>` whose `src` is in the
//! layout's [`Images`](crate::Images) is sized from the image, not its
//! content (CSS 2 §10.3.2 and §10.6.2, simplified).
//!
//! Each axis takes its CSS size (HTML's `width`/`height` attributes arrive
//! as presentational hints), else follows the other axis through the natural ratio, else the
//! natural size. `min-`/`max-` sizes then clamp it, carrying the ratio to
//! an axis that wasn't given.

use noonoo_dom::NodeId;
use noonoo_style::{BoxSizing, ComputedStyle, Dimension, Display, Length};

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

    /// The natural size and ratio of replaced content: a loaded image's
    /// pixels; an outer `<svg>`'s `viewBox` ratio (its `width` and
    /// `height` arrive as CSS sizes); a `<canvas>`'s 300x150 bitmap unless
    /// its attributes size it. `None` when `node` isn't replaced content
    /// noonoo sizes this way.
    fn natural(&self, node: NodeId) -> Option<Natural> {
        if let Some(image) = self.image(node) {
            let (width, height) = (image.width as f32, image.height as f32);
            return Some(Natural {
                width: Some(width),
                height: Some(height),
                ratio: Some(width / height),
            });
        }
        let element = self.document.element(node)?;
        match element.name.as_str() {
            "svg" if !self.inside_svg(node) => Some(Natural {
                width: None,
                height: None,
                ratio: element.attr("viewBox").and_then(view_box_ratio),
            }),
            "canvas" => {
                let pixels = |name: &str, default: f32| {
                    element
                        .attr(name)
                        .and_then(|value| value.trim().parse::<u32>().ok())
                        .map_or(default, |value| value as f32)
                };
                let (width, height) = (pixels("width", 300.0), pixels("height", 150.0));
                Some(Natural {
                    width: Some(width),
                    height: Some(height),
                    ratio: (height > 0.0).then(|| width / height),
                })
            }
            _ => None,
        }
    }

    /// Whether `node` sits inside an `<svg>`: an inner one is SVG content.
    fn inside_svg(&self, node: NodeId) -> bool {
        let mut at = self.document.tree().parent(node);
        while let Some(parent) = at {
            if self
                .document
                .element(parent)
                .is_some_and(|e| e.name == "svg")
            {
                return true;
            }
            at = self.document.tree().parent(parent);
        }
        false
    }

    /// The content-box size of `node` if it's replaced content: an image
    /// with a loaded source, an outer `<svg>` or a `<canvas>`. Widths
    /// resolve against `width_basis` (infinite while measuring a preferred
    /// width, where percentages don't count) and heights against
    /// `height_basis`, when it's definite.
    pub(crate) fn image_size(
        &self,
        node: NodeId,
        style: &ComputedStyle,
        width_basis: f32,
        height_basis: Option<f32>,
    ) -> Option<(f32, f32)> {
        let natural = self.natural(node)?;
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

        // CSS's default sizing: a missing axis follows the other through
        // the ratio, else takes the natural size, else the 300x150 default
        // object size; with a ratio and no size at all, the content fills
        // the available width (and has no intrinsic width).
        let ratio = natural.ratio;
        let natural_width = natural.width.unwrap_or(DEFAULT_WIDTH);
        let natural_height = natural.height.unwrap_or(DEFAULT_HEIGHT);
        let (mut w, mut h) = match (width, height) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, ratio.map_or(natural_height, |ratio| w / ratio)),
            (None, Some(h)) => (ratio.map_or(natural_width, |ratio| h * ratio), h),
            (None, None) => match (natural.width, natural.height, ratio) {
                (None, None, Some(ratio)) if width_basis.is_finite() => {
                    let w = (width_basis - horizontal).max(0.0);
                    (w, w / ratio)
                }
                // Measuring intrinsic widths: it has none, so a shrink-to-fit
                // box around it (a float, an inline-block) is 0 wide, as in
                // Chromium.
                (None, None, Some(_)) => (0.0, 0.0),
                _ => (natural_width, natural_height),
            },
        };
        let clamp = |value: f32, min: Option<f32>, max: Option<f32>| {
            // `min` wins over `max`, as for any box.
            let value = max.map_or(value, |max| value.min(max));
            min.map_or(value, |min| value.max(min))
        };
        let clamped = clamp(w, width_of(&style.min_width), width_of(&style.max_width));
        if clamped != w {
            w = clamped;
            if height.is_none()
                && let Some(ratio) = ratio
            {
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
            if width.is_none()
                && let Some(ratio) = ratio
            {
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
            || self.broken_image(node) == Some(Broken::AltInline)
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

    /// Whether `node` is an `<img>` without its image whose width is a
    /// percentage: with no natural size, its alt text adds nothing to the
    /// intrinsic widths around it, as in Chromium.
    pub(crate) fn is_unsized_fallback(&self, node: NodeId, style: &ComputedStyle) -> bool {
        matches!(style.width, Dimension::Percent(_) | Dimension::Calc { .. })
            && self.document.element(node).is_some_and(|e| e.name == "img")
            && self.image(node).is_none()
    }

    /// How Chromium shows `<img>` `node` once its image fails to load, or
    /// `None` if it isn't such an image (or isn't displayed). Decided from
    /// its own computed style, before [`Ctx::resolve_broken_images`]
    /// adjusts it.
    pub(crate) fn broken_image(&self, node: NodeId) -> Option<Broken> {
        let element = self.document.element(node)?;
        if element.name != "img" || self.image(node).is_some() {
            return None;
        }
        let style = self.styles.get(node)?;
        if style.display == Display::None {
            return None;
        }
        let (width, height) = (
            style.width != Dimension::Auto,
            style.height != Dimension::Auto,
        );
        let alt = element.attr("alt");
        Some(if self.is_quirks() && (width || height) {
            Broken::Square
        } else if alt.is_some_and(|alt| !alt.is_empty()) {
            // A float or an absolutely positioned box is block-level.
            let inline = crate::is_atomic_inline(style.display) || style.display == Display::Inline;
            let out_of_flow = style.float != noonoo_style::Float::None
                || matches!(
                    style.position,
                    noonoo_style::Position::Absolute | noonoo_style::Position::Fixed
                );
            if inline && !out_of_flow {
                Broken::AltInline
            } else {
                Broken::AltBlock
            }
        } else if width && height {
            Broken::Sized
        } else if alt.is_none()
            && element
                .attr("src")
                .is_some_and(|src| !src.trim().is_empty())
        {
            Broken::Icon
        } else {
            Broken::Nothing
        })
    }

    /// Give each `<img>` whose image didn't load the style Chromium lays it
    /// out with once loading fails (see [`Broken`]).
    pub(crate) fn resolve_broken_images(&mut self) {
        let mut stack = vec![self.document.root()];
        let mut broken = Vec::new();
        while let Some(node) = stack.pop() {
            if let Some(kind) = self.broken_image(node) {
                broken.push((node, kind));
            }
            stack.extend(crate::child_nodes(self.document, node));
        }
        for (node, kind) in broken {
            let mut style = Box::new(self.style(node).clone());
            let fixed = |style: &mut ComputedStyle, side: f32| {
                style.width = Dimension::Px(side);
                style.height = Dimension::Px(side);
                style.min_width = Dimension::Auto;
                style.max_width = Dimension::Auto;
                style.min_height = Dimension::Auto;
                style.max_height = Dimension::Auto;
            };
            let icon_room = |style: &mut ComputedStyle| {
                let left = match style.padding.left {
                    Length::Px(px) => px,
                    _ => 0.0,
                };
                style.padding.left = Length::Px(left + BROKEN_ICON);
            };
            match kind {
                Broken::Square => {
                    if style.width == Dimension::Auto {
                        style.width = style.height.clone();
                    } else if style.height == Dimension::Auto {
                        style.height = style.width.clone();
                    }
                }
                Broken::AltInline => {
                    style.width = Dimension::Auto;
                    style.height = Dimension::Auto;
                    style.min_width = Dimension::Auto;
                    style.max_width = Dimension::Auto;
                    style.min_height = Dimension::Auto;
                    style.max_height = Dimension::Auto;
                    icon_room(&mut style);
                }
                // The icon sits inside a width it is given.
                Broken::AltBlock if style.width == Dimension::Auto => icon_room(&mut style),
                Broken::AltBlock => {}
                Broken::Sized => {}
                Broken::Icon => fixed(&mut style, BROKEN_ICON),
                Broken::Nothing => fixed(&mut style, 0.0),
            }
            self.collapsed.insert(node, style);
        }
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

/// The side of the icon Chromium shows for an image that didn't load.
const BROKEN_ICON: f32 = 16.0;

/// How Chromium shows an `<img>` whose image failed to load. In quirks
/// mode a width or height makes it a box of that size; otherwise its alt
/// text shows, or with none a box of its width and height, or an icon.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Broken {
    /// Quirks mode, a width or height given: a box of that size, square
    /// when only one is.
    Square,
    /// Its alt text, inline: a shrink-to-fit box holding the text after a
    /// 16px icon, its sizes (and their attributes' ratio) ignored.
    AltInline,
    /// Its alt text in a block-level box (floated and absolutely positioned
    /// ones too), which keeps its sizes.
    AltBlock,
    /// No alt text, and a width and a height: a box of that size.
    Sized,
    /// No alt attribute, and a source: the 16px icon.
    Icon,
    /// `alt=""`, or no source: an empty box on the baseline.
    Nothing,
}

/// The default object size, for replaced content with no natural size.
const DEFAULT_WIDTH: f32 = 300.0;
const DEFAULT_HEIGHT: f32 = 150.0;

/// Replaced content's natural dimensions, each optional.
struct Natural {
    width: Option<f32>,
    height: Option<f32>,
    ratio: Option<f32>,
}

/// The width-to-height ratio of an SVG `viewBox="min-x min-y width
/// height"`, when both are positive.
fn view_box_ratio(view_box: &str) -> Option<f32> {
    let numbers: Vec<f32> = view_box
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().ok())
        .collect::<Option<_>>()?;
    match numbers[..] {
        [_, _, width, height] if width > 0.0 && height > 0.0 => Some(width / height),
        _ => None,
    }
}
