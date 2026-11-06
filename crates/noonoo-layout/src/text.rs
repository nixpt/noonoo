//! Text measurement.
//!
//! Layout needs to measure text; shaping does not belong here. [`TextMeasurer`]
//! is the seam: the engine ships [`HeuristicMeasurer`], and `noonoo-text`
//! (Phase C) plugs real shaping in behind the same trait.

use noonoo_geometry::Size;

/// Measures a run of text for a font size and weight.
pub trait TextMeasurer {
    /// The size of `text` laid out on one line.
    fn measure(&self, text: &str, font_size: f32, bold: bool) -> Size;
}

/// A dependency-free stand-in: an average advance per character and a
/// font-size–relative line height. Not correct typography — just enough to lay
/// boxes out deterministically until real shaping arrives.
#[derive(Debug, Default, Clone, Copy)]
pub struct HeuristicMeasurer;

impl TextMeasurer for HeuristicMeasurer {
    fn measure(&self, text: &str, font_size: f32, bold: bool) -> Size {
        let advance = if bold { 0.55 } else { 0.50 } * font_size;
        let width = text.chars().count() as f32 * advance;
        Size::new(width, font_size * 1.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proportional_to_characters() {
        let measurer = HeuristicMeasurer;
        let one = measurer.measure("a", 10.0, false);
        let two = measurer.measure("aa", 10.0, false);
        assert_eq!(one.height, 12.0);
        assert_eq!(two.width, one.width * 2.0);
        assert!(measurer.measure("a", 10.0, true).width > one.width);
    }
}
