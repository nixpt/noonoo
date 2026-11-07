//! A [`Glyphs`] renderer that draws text as filled glyph outlines.
//!
//! Where the default `noonoo_paint::TextRuns` emits a backend text command, this
//! emits one `FillPolygon` per glyph contour — so a real font renders through the
//! same command stream as every other shape, with no GPU text backend.
//!
//! Each request selects a face from a [`FontSet`] (family list, weight, style),
//! the same way [`crate::FontSetShaper`] does — so a document laid out with a
//! `FontSetShaper` paints with the very faces it was measured against.

use noonoo_geometry::Point;
use noonoo_paint::{Color, Command, CommandList, Glyphs};
use noonoo_text::FontRequest;

use crate::FontSet;

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
    fn draw(
        &self,
        text: &str,
        (x, baseline): (f32, f32),
        font_size: f32,
        color: Color,
        request: FontRequest,
        out: &mut CommandList,
    ) {
        let Some(font) = self
            .set
            .select_best(request.families, request.weight, request.italic)
        else {
            return;
        };
        let scale = font_size / font.units_per_em().max(1) as f32;
        let mut pen = x;
        let mut previous = None;
        for character in text.chars() {
            let glyph = font.glyph_for_char_or_notdef(character);
            if let Some(left) = previous {
                pen += font.kern(left, glyph) as f32 * scale;
            }
            if let Some(outline) = font.outline(glyph) {
                for contour in outline.flatten(self.steps) {
                    let points: Vec<Point> = contour
                        .into_iter()
                        .map(|(px, py)| {
                            // Font units (y up) → pixels (y down), baseline at `baseline`.
                            Point::new(pen + px * scale, baseline - py * scale)
                        })
                        .collect();
                    if points.len() >= 3 {
                        out.0.push(Command::FillPolygon { points, color });
                    }
                }
            }
            pen += font.advance(glyph) as f32 * scale;
            previous = Some(glyph);
        }
    }
}
