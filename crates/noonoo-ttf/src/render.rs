//! A [`Glyphs`] renderer that draws text as filled glyph outlines.
//!
//! Where the default `noonoo_paint::TextRuns` emits a backend text command, this
//! emits one `FillContours` per glyph (its contours filled together, so holes
//! stay open) — so a real font renders through the same command stream as
//! every other shape, with no GPU text backend.
//!
//! Each request selects a face from a [`FontSet`] (family list, weight, style),
//! the same way [`crate::FontSetShaper`] does — so a document laid out with a
//! `FontSetShaper` paints with the very faces it was measured against.

use noonoo_geometry::{Point, Rect};
use noonoo_paint::{Color, Command, CommandList, EncodedImage, Glyphs};
use noonoo_text::FontRequest;

use crate::{Font, FontSet};

/// Draws text as filled outlines, selecting faces from a [`FontSet`].
pub struct OutlineGlyphs<'a> {
    set: &'a FontSet<'a>,
    /// Quadratic subdivision per curve (more is smoother, at more vertices).
    steps: usize,
}

impl<'a> OutlineGlyphs<'a> {
    /// A renderer over `set`, with a default curve subdivision of 8.
    pub fn new(set: &'a FontSet<'a>) -> Self {
        Self { set, steps: 8 }
    }

    /// Set the number of samples per quadratic curve segment.
    pub fn with_steps(mut self, steps: usize) -> Self {
        self.steps = steps.max(1);
        self
    }
}

impl Glyphs for OutlineGlyphs<'_> {
    fn metrics(&self, request: FontRequest, font_size: f32) -> (f32, f32) {
        match self
            .set
            .select_best(request.families, request.weight, request.italic)
        {
            Some(font) => crate::shaper::ascent_descent_px(font, font_size),
            None => (font_size * 0.8, font_size * 0.4),
        }
    }

    fn draw(
        &self,
        text: &str,
        origin: (f32, f32),
        font_size: f32,
        color: Color,
        request: FontRequest,
        out: &mut CommandList,
    ) {
        self.draw_spaced(text, origin, font_size, 0.0, color, request, out);
    }

    fn draw_spaced(
        &self,
        text: &str,
        (x, baseline): (f32, f32),
        font_size: f32,
        letter_spacing: f32,
        color: Color,
        request: FontRequest,
        out: &mut CommandList,
    ) {
        let Some(selected) = self
            .set
            .select_best(request.families, request.weight, request.italic)
        else {
            return;
        };
        let mut pen = x;
        let mut previous: Option<(*const Font, u16)> = None;
        for (character, emoji) in crate::emoji::presented(text) {
            // A character the face lacks comes from a fallback face; an
            // emoji prefers a face that has it in colour.
            let font = self.set.face_presented(selected, character, emoji);
            let scale = font_size / font.units_per_em().max(1) as f32;
            let glyph = font.glyph_for_char_or_notdef(character);
            if let Some((face, left)) = previous
                && std::ptr::eq(face, font)
            {
                pen += font.kern(left, glyph) as f32 * scale;
            }
            if let Some(bitmap) = font.color_bitmap(glyph) {
                // A colour glyph (emoji) is its image, scaled from its
                // strike's size to this one; it ignores the text colour.
                let pixel = font_size / f32::from(bitmap.ppem.max(1));
                out.0.push(Command::DrawImage {
                    rect: Rect::new(
                        pen + f32::from(bitmap.bearing_x) * pixel,
                        baseline - f32::from(bitmap.bearing_y) * pixel,
                        f32::from(bitmap.width) * pixel,
                        f32::from(bitmap.height) * pixel,
                    ),
                    image: EncodedImage::png(bitmap.data),
                });
            } else if let Some(outline) = font.outline(glyph) {
                // All of a glyph's contours fill as one shape, so a hole
                // (wound the other way) stays open.
                let contours: Vec<Vec<Point>> = outline
                    .flatten(self.steps)
                    .into_iter()
                    .map(|contour| {
                        contour
                            .into_iter()
                            .map(|(px, py)| {
                                // Font units (y up) → pixels (y down), baseline at `baseline`.
                                Point::new(pen + px * scale, baseline - py * scale)
                            })
                            .collect::<Vec<Point>>()
                    })
                    .filter(|points| points.len() >= 3)
                    .collect();
                if !contours.is_empty() {
                    out.0.push(Command::FillContours { contours, color });
                }
            }
            pen += font.advance(glyph) as f32 * scale + letter_spacing;
            previous = Some((font, glyph));
        }
    }
}
