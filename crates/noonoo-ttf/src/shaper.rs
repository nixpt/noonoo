//! [`Shaper`]s backed by parsed fonts.
//!
//! [`FontShaper`] measures a single face; [`FontSetShaper`] picks the right face
//! from a [`FontSet`] per call, so an element's `font-family`/`font-weight`/
//! `font-style` select the face its text is measured with. Neither *shapes* —
//! ligatures, contextual substitution, and complex scripts remain the borrowed
//! concern behind the trait (`docs/DEPENDENCY-EXIT.md`).

use noonoo_text::{FontRequest, Shaper};

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

/// The advance of `text` in pixels, each character measured with the face
/// [`FontSet::face_for`] (or, for an emoji, [`FontSet::face_for_emoji`])
/// picks. Variation selectors take no space. Pairs kern only within one face.
pub(crate) fn fallback_advance(set: &FontSet, selected: &Font, text: &str, font_size: f32) -> f32 {
    let mut pixels = 0.0;
    let mut previous: Option<(*const Font, u16)> = None;
    for (character, emoji) in crate::emoji::presented(text) {
        let font = set.face_presented(selected, character, emoji);
        let glyph = font.glyph_for_char_or_notdef(character);
        let mut units = i32::from(font.advance(glyph));
        if let Some((face, left)) = previous
            && std::ptr::eq(face, font)
        {
            units += i32::from(font.kern(left, glyph));
        }
        pixels += units as f32 * scale(font, font_size);
        previous = Some((font, glyph));
    }
    pixels
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
    fn advance(&self, text: &str, _request: FontRequest, font_size: f32) -> f32 {
        advance_units(self.font, text) as f32 * scale(self.font, font_size)
    }

    fn line_height(&self, _request: FontRequest, font_size: f32) -> f32 {
        line_height_units(self.font) as f32 * scale(self.font, font_size)
    }
}

/// A [`Shaper`] that selects a face from a [`FontSet`] per call.
///
/// Each request carries the element's family list, weight, and style; a face
/// registered under the empty family name matches every request, which is how
/// a host offers a universal fallback.
pub struct FontSetShaper<'a> {
    set: &'a FontSet<'a>,
}

impl<'a> FontSetShaper<'a> {
    /// A shaper over `set`.
    pub fn new(set: &'a FontSet<'a>) -> Self {
        Self { set }
    }
}

impl Shaper for FontSetShaper<'_> {
    fn advance(&self, text: &str, request: FontRequest, font_size: f32) -> f32 {
        match self
            .set
            .select_best(request.families, request.weight, request.italic)
        {
            Some(font) => fallback_advance(self.set, font, text, font_size),
            // A deterministic fallback, as `MetricShaper` uses.
            None => text.chars().count() as f32 * 0.5 * font_size,
        }
    }

    fn line_height(&self, request: FontRequest, font_size: f32) -> f32 {
        match self
            .set
            .select_best(request.families, request.weight, request.italic)
        {
            Some(font) => line_height_units(font) as f32 * scale(font, font_size),
            None => font_size * 1.2,
        }
    }
}
