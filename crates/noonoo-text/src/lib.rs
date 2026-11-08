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

mod paragraph;

pub use paragraph::{
    Line, Paragraph, Run, align, collapsible_words, is_collapsible_space, layout_paragraph,
    trim_collapsible, wrap,
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
