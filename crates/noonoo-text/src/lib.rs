//! # noonoo-text
//!
//! Text layout: line breaking and alignment, over a **shaping seam**.
//!
//! This is Phase C of the dependency-exit plan (`docs/DEPENDENCY-EXIT.md`):
//! we own the line-breaking and alignment (the layout-level text work), while
//! **shaping** — turning a string into positioned glyphs, with fonts, kerning
//! and complex scripts — stays a borrowed concern behind [`Shaper`]. The crate
//! ships [`MetricShaper`] (a dependency-free advance model) so text lays out
//! deterministically today; a real-font metrics shaper (`FontShaper`, over a
//! parsed TrueType/OpenType face) lives in `noonoo-ttf`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod breaks;
mod paragraph;

pub use paragraph::{
    Line, Paragraph, Run, align, break_pieces, collapsible_words, is_breaking_space,
    is_collapsible_space, layout_paragraph, trim_collapsible, wrap,
};

/// Turns text into advances and line metrics.
///
/// The seam the whole crate is built on: line breaking asks a `Shaper` how wide
/// a run is and how tall a line is, and never touches glyphs itself.
pub trait Shaper {
    /// The advance width of `text` on one line, in a face matching `request`.
    fn advance(&self, text: &str, request: FontRequest, font_size: f32) -> f32;

    /// The natural line height for `font_size`, in a face matching `request`.
    fn line_height(&self, request: FontRequest, font_size: f32) -> f32;

    /// The ascent and descent, in pixels, of the face `request` selects at
    /// `font_size`: the content area an inline box wraps, and where its
    /// baseline sits. The default splits the natural line height two
    /// thirds above the baseline and one third below (0.8em and 0.4em of a
    /// 1.2em line), as `noonoo-paint` does with no font to ask.
    fn metrics(&self, request: FontRequest, font_size: f32) -> (f32, f32) {
        let line = self.line_height(request, font_size);
        (line * 2.0 / 3.0, line / 3.0)
    }

    /// The ascent and descent, in pixels, of the faces `text` is drawn
    /// with at `font_size`: the face `request` selects and any fallback a
    /// character needs, each reaching as far as its own metrics. A line of
    /// `normal` height grows to hold a taller fallback face, as browsers'
    /// do. The default is the selected face's [`metrics`](Self::metrics).
    fn text_metrics(&self, text: &str, request: FontRequest, font_size: f32) -> (f32, f32) {
        let _ = text;
        self.metrics(request, font_size)
    }

    /// The face's average and widest character advances at `font_size`,
    /// in pixels, as browsers size text fields by. The default takes a
    /// "0"'s advance for both.
    fn char_widths(&self, request: FontRequest, font_size: f32) -> (f32, f32) {
        let zero = self.advance("0", request, font_size);
        (zero, zero)
    }
}

/// A font request: the families to try, in preference order, plus weight and
/// style.
///
/// This is what an element's computed style asks for; a [`Shaper`] picks the
/// face. An empty `families` list matches any face.
#[derive(Clone, Copy, Debug)]
pub struct FontRequest<'a> {
    /// Family names, most preferred first.
    pub families: &'a [String],
    /// The requested weight (`400` normal, `700` bold).
    pub weight: u16,
    /// Whether italics are requested.
    pub italic: bool,
}

impl FontRequest<'_> {
    /// A request with no family preference at normal weight.
    pub fn any() -> Self {
        Self {
            families: &[],
            weight: 400,
            italic: false,
        }
    }
}

/// A dependency-free stand-in: a fixed advance per character and a
/// font-size–relative line height. Not typography — a deterministic baseline.
#[derive(Debug, Default, Clone, Copy)]
pub struct MetricShaper;

impl Shaper for MetricShaper {
    fn advance(&self, text: &str, request: FontRequest, font_size: f32) -> f32 {
        let factor = if request.weight >= 600 { 0.55 } else { 0.50 };
        text.chars().count() as f32 * factor * font_size
    }

    fn line_height(&self, _request: FontRequest, font_size: f32) -> f32 {
        font_size * 1.2
    }
}

/// Horizontal alignment within a line box.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Align {
    /// Left / start.
    #[default]
    Start,
    /// Right / end.
    End,
    /// Centered.
    Center,
    /// Flush to both edges.
    Justify,
}

/// How whitespace and line breaks are handled.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WhiteSpace {
    /// Collapse and wrap.
    #[default]
    Normal,
    /// Collapse, no wrapping.
    NoWrap,
    /// Preserve, no wrapping.
    Pre,
    /// Preserve and wrap.
    PreWrap,
    /// Collapse but honour newlines.
    PreLine,
}

/// The style inputs line breaking needs.
#[derive(Clone, Copy, Debug)]
pub struct TextStyle<'a> {
    /// Font size, in pixels.
    pub font_size: f32,
    /// Which faces to try, in preference order (empty: any face).
    pub families: &'a [String],
    /// Font weight (`400` normal, `700` bold).
    pub weight: u16,
    /// Whether italics are requested.
    pub italic: bool,
    /// Line height, in pixels (`0` means "ask the shaper").
    pub line_height: f32,
    /// Horizontal alignment.
    pub align: Align,
    /// Whitespace handling.
    pub white_space: WhiteSpace,
}

impl Default for TextStyle<'_> {
    fn default() -> Self {
        Self {
            font_size: 16.0,
            families: &[],
            weight: 400,
            italic: false,
            line_height: 0.0,
            align: Align::Start,
            white_space: WhiteSpace::Normal,
        }
    }
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod char_width_tests {
    use super::*;

    #[test]
    fn the_default_character_widths_are_a_zeros() {
        assert_eq!(
            MetricShaper.char_widths(FontRequest::any(), 16.0),
            (8.0, 8.0)
        );
    }
}

#[cfg(test)]
mod text_metrics_tests {
    use super::*;

    #[test]
    fn a_runs_metrics_default_to_the_selected_faces() {
        let request = FontRequest::any();
        assert_eq!(
            MetricShaper.text_metrics("any text", request, 15.0),
            MetricShaper.metrics(request, 15.0)
        );
    }
}
