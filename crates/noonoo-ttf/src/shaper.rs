//! [`Shaper`]s backed by parsed fonts.
//!
//! [`FontShaper`] measures a single face; [`FontSetShaper`] picks the right face
//! from a [`FontSet`] per call (so `font-weight: bold` selects a real bold face
//! when one is registered). Neither *shapes* — ligatures, contextual
//! substitution, and complex scripts remain the borrowed concern behind the
//! trait (`docs/DEPENDENCY-EXIT.md`).

use noonoo_text::Shaper;

use crate::{Font, FontSet};

/// Font units → pixels for this `font_size`.
fn scale(font: &Font, font_size: f32) -> f32 {
    font_size / font.units_per_em().max(1) as f32
}

/// The advance of `text` in font units, including legacy pair kerning.
pub(crate) fn advance_units(font: &Font, text: &str) -> i32 {
    let mut units = 0i32;
    let mut previous = None;
    for character in text.chars() {
        let glyph = font.glyph_for_char_or_notdef(character);
        units += font.advance(glyph) as i32;
        if let Some(left) = previous {
            units += font.kern(left, glyph) as i32;
        }
        previous = Some(glyph);
    }
    units
}

/// The natural line height of a font, in font units.
pub(crate) fn line_height_units(font: &Font) -> i32 {
    (font.ascender() as i32 - font.descender() as i32 + font.line_gap() as i32).max(0)
}

/// A [`Shaper`] over a single [`Font`]'s metrics.
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
}

impl Shaper for FontShaper<'_> {
    fn advance(&self, text: &str, font_size: f32, _bold: bool) -> f32 {
        advance_units(self.font, text) as f32 * scale(self.font, font_size)
    }

    fn line_height(&self, font_size: f32) -> f32 {
        line_height_units(self.font) as f32 * scale(self.font, font_size)
    }
}

/// A [`Shaper`] that selects a face from a [`FontSet`] per call.
///
/// The `bold` flag a line-break passes selects a heavier face (weight 700)
/// when one is registered for the family; otherwise the nearest face is used.
pub struct FontSetShaper<'a> {
    set: &'a FontSet<'a>,
    family: String,
    weight: u16,
    italic: bool,
}

impl<'a> FontSetShaper<'a> {
    /// A shaper over `set` for `family` at `weight`/`italic`.
    pub fn new(set: &'a FontSet<'a>, family: impl Into<String>, weight: u16, italic: bool) -> Self {
        Self {
            set,
            family: family.into(),
            weight,
            italic,
        }
    }

    fn pick(&self, bold: bool) -> Option<&'a Font> {
        let weight = if bold { 700 } else { self.weight };
        self.set.select(&self.family, weight, self.italic)
    }
}

impl Shaper for FontSetShaper<'_> {
    fn advance(&self, text: &str, font_size: f32, bold: bool) -> f32 {
        match self.pick(bold) {
            Some(font) => advance_units(font, text) as f32 * scale(font, font_size),
            // A deterministic fallback, as `MetricShaper` uses.
            None => text.chars().count() as f32 * 0.5 * font_size,
        }
    }

    fn line_height(&self, font_size: f32) -> f32 {
        match self.pick(false) {
            Some(font) => line_height_units(font) as f32 * scale(font, font_size),
            None => font_size * 1.2,
        }
    }
}
