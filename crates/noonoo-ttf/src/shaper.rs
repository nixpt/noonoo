//! A [`Shaper`] backed by a parsed [`Font`].
//!
//! This is the bridge from the font-format layer to the text-layout seam: it
//! answers the two questions line breaking asks — how wide a run is, and how tall
//! a line is — from real metrics instead of a fixed per-character guess.
//!
//! It does **not** shape: ligatures, kerning, and complex scripts remain the
//! borrowed concern behind the trait (`docs/DEPENDENCY-EXIT.md`). `bold` selects
//! nothing here; a real bold face is a font-selection job, not this adapter's.

use noonoo_text::Shaper;

use crate::Font;

/// A [`Shaper`] over a [`Font`]'s metrics.
pub struct FontShaper<'a> {
    font: &'a Font,
}

impl<'a> FontShaper<'a> {
    /// A shaper over `font`.
    pub fn new(font: &'a Font) -> Self {
        Self { font }
    }

    /// The font being measured.
    pub fn font(&self) -> &'a Font {
        self.font
    }

    /// Font units → pixels for this `font_size`.
    fn scale(&self, font_size: f32) -> f32 {
        font_size / self.font.units_per_em().max(1) as f32
    }
}

impl Shaper for FontShaper<'_> {
    fn advance(&self, text: &str, font_size: f32, _bold: bool) -> f32 {
        let units: u32 = text
            .chars()
            .map(|character| self.font.advance_of_char(character) as u32)
            .sum();
        units as f32 * self.scale(font_size)
    }

    fn line_height(&self, font_size: f32) -> f32 {
        let units = (self.font.ascender() as i32 - self.font.descender() as i32
            + self.font.line_gap() as i32)
            .max(0);
        units as f32 * self.scale(font_size)
    }
}
