//! A [`Glyphs`] renderer that draws text as filled glyph outlines.
//!
//! Where the default `noonoo_paint::TextRuns` emits a backend text command, this
//! emits one `FillPolygon` per glyph contour — so a real font renders through the
//! same command stream as every other shape, with no GPU text backend.
//!
//! It advances the pen with the *same* metrics as [`crate::FontShaper`], so a
//! document laid out with a `FontShaper` over a font paints correctly when that
//! same font is handed to `OutlineGlyphs`.

use noonoo_geometry::Point;
use noonoo_paint::{Color, Command, CommandList, Glyphs};

use crate::Font;

/// Draws text as filled outlines from a [`Font`].
pub struct OutlineGlyphs<'a> {
    font: &'a Font,
    /// Quadratic subdivision per curve (more is smoother, at more vertices).
    steps: usize,
}

impl<'a> OutlineGlyphs<'a> {
    /// A renderer over `font`, with a default curve subdivision of 8.
    pub fn new(font: &'a Font) -> Self {
        Self { font, steps: 8 }
    }

    /// Set the number of samples per quadratic curve segment.
    pub fn with_steps(mut self, steps: usize) -> Self {
        self.steps = steps.max(1);
        self
    }

    /// The font being rendered.
    pub fn font(&self) -> &'a Font {
        self.font
    }
}

impl Glyphs for OutlineGlyphs<'_> {
    fn draw(
        &self,
        text: &str,
        (x, baseline): (f32, f32),
        font_size: f32,
        color: Color,
        out: &mut CommandList,
    ) {
        let scale = font_size / self.font.units_per_em().max(1) as f32;
        let mut pen = x;
        for character in text.chars() {
            if let Some(glyph) = self.font.glyph_for_char(character)
                && let Some(outline) = self.font.outline(glyph)
            {
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
            pen += self.font.advance_of_char(character) as f32 * scale;
        }
    }
}
