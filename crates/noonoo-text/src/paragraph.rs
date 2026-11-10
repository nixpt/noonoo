//! Line breaking, alignment, and paragraph layout.

use crate::{Align, FontRequest, Shaper, TextStyle, WhiteSpace};

/// A positioned run of text within a line.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// The run's text.
    pub text: String,
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Advance width.
    pub width: f32,
    /// Line height.
    pub height: f32,
}

/// One laid-out line.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// Top edge of the line.
    pub y: f32,
    /// Line height.
    pub height: f32,
    /// The used width of the line's content.
    pub width: f32,
    /// The runs on the line.
    pub runs: Vec<Run>,
}

/// A laid-out paragraph.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Paragraph {
    /// The widest line.
    pub width: f32,
    /// The total height.
    pub height: f32,
    /// The lines.
    pub lines: Vec<Line>,
}

/// Where a line's content starts, and the extra gap between items (`justify`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Offset {
    /// Offset of the first item from the line's left edge.
    pub start: f32,
    /// Extra space to insert between consecutive items.
    pub gap: f32,
}

/// The alignment offset and inter-item gap for a line with `free` spare space
/// across `count` items.
pub fn align(align: Align, free: f32, count: usize) -> Offset {
    match align {
        Align::Start => Offset {
            start: 0.0,
            gap: 0.0,
        },
        Align::End => Offset {
            start: free,
            gap: 0.0,
        },
        Align::Center => Offset {
            start: free / 2.0,
            gap: 0.0,
        },
        Align::Justify => Offset {
            start: 0.0,
            gap: if count > 1 {
                free / (count as f32 - 1.0)
            } else {
                0.0
            },
        },
    }
}

/// How far past the line a word may reach and still fit: summing the same
/// widths in another order (as max-content measuring does) can differ by a
/// rounding error, and text sized to its own width must not wrap over it.
const WRAP_TOLERANCE: f32 = 0.01;

/// Whether `c` is CSS's collapsible white space: space, tab, and the line
/// breaks. A no-break space (U+00A0) and the other Unicode spaces are not:
/// they belong to the word they sit in.
pub fn is_collapsible_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')
}

/// A space that doesn't collapse but allows a line break after it: UAX #14
/// class BA's spaces (the en, em and thin spaces, ideographic space, ...).
/// U+2007 FIGURE SPACE is GL, no break.
pub fn is_breaking_space(c: char) -> bool {
    matches!(c, '\u{1680}' | '\u{2000}'..='\u{2006}' | '\u{2008}'..='\u{200a}' | '\u{205f}' | '\u{3000}')
}

/// `word` split at its break opportunities, each piece keeping the
/// character it breaks after: after a breaking space, and after a hyphen
/// between a letter and a letter (UAX #14's HY and BA hyphens), not before
/// a digit, at the word's start or inside a run of hyphens. The pieces sit
/// with no space between them.
pub fn break_pieces(word: &str) -> impl Iterator<Item = &str> {
    BreakPieces { rest: word }
}

/// A hyphen a line may break after: hyphen-minus, U+2010 HYPHEN and the en
/// dash. U+2011 is the no-break hyphen.
fn is_breaking_hyphen(c: char) -> bool {
    matches!(c, '-' | '\u{2010}' | '\u{2013}')
}

struct BreakPieces<'a> {
    rest: &'a str,
}

impl<'a> Iterator for BreakPieces<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.rest.is_empty() {
            return None;
        }
        let mut prev: Option<char> = None;
        let mut chars = self.rest.char_indices().peekable();
        while let Some((at, c)) = chars.next() {
            let next = chars.peek().map(|&(_, next)| next);
            let breaks = is_breaking_space(c)
                || (is_breaking_hyphen(c)
                    && prev.is_some_and(|p| !is_breaking_hyphen(p) && !p.is_whitespace())
                    && next.is_some_and(|n| !n.is_ascii_digit() && !is_breaking_hyphen(n)));
            if breaks {
                let end = at + c.len_utf8();
                let (piece, rest) = self.rest.split_at(end);
                self.rest = rest;
                return Some(piece);
            }
            prev = Some(c);
        }
        Some(std::mem::take(&mut self.rest))
    }
}

/// The words of `text` between runs of collapsible white space.
pub fn collapsible_words(text: &str) -> impl Iterator<Item = &str> {
    text.split(is_collapsible_space)
        .filter(|word| !word.is_empty())
}

/// `text` without its leading and trailing collapsible white space.
pub fn trim_collapsible(text: &str) -> &str {
    text.trim_matches(is_collapsible_space)
}

/// Greedy line breaking: `widths` are item advances, `gap` the inter-item space.
pub fn wrap(widths: &[f32], gap: f32, max_width: f32) -> Vec<Vec<usize>> {
    let mut lines: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut used = 0.0;
    for (index, &width) in widths.iter().enumerate() {
        if current.is_empty() {
            used = width;
            current.push(index);
        } else if used + gap + width > max_width + WRAP_TOLERANCE {
            lines.push(std::mem::take(&mut current));
            used = width;
            current.push(index);
        } else {
            used += gap + width;
            current.push(index);
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Lay out a plain-text paragraph.
pub fn layout_paragraph(
    text: &str,
    style: &TextStyle,
    max_width: f32,
    shaper: &dyn Shaper,
) -> Paragraph {
    let request = FontRequest {
        families: style.families,
        weight: style.weight,
        italic: style.italic,
    };
    let line_height = if style.line_height > 0.0 {
        style.line_height
    } else {
        shaper.line_height(request, style.font_size)
    };

    let words: Vec<(String, f32)> = collapsible_words(text)
        .map(|word| {
            (
                word.to_owned(),
                shaper.advance(word, request, style.font_size),
            )
        })
        .collect();
    let widths: Vec<f32> = words.iter().map(|(_, width)| *width).collect();
    let space = shaper.advance(" ", request, style.font_size);

    let line_indices: Vec<Vec<usize>> = if words.is_empty() {
        Vec::new()
    } else {
        match style.white_space {
            WhiteSpace::NoWrap | WhiteSpace::Pre => vec![(0..words.len()).collect()],
            _ => wrap(&widths, space, max_width),
        }
    };

    let mut lines = Vec::new();
    let mut y = 0.0;
    let mut paragraph_width = 0.0_f32;

    for items in &line_indices {
        let used: f32 = items.iter().map(|&i| widths[i]).sum::<f32>()
            + space * items.len().saturating_sub(1) as f32;
        let free = (max_width - used).max(0.0);
        let offset = align(style.align, free, items.len());

        let mut cursor = offset.start;
        let mut runs = Vec::new();
        for (position, &i) in items.iter().enumerate() {
            if position > 0 {
                cursor += space + offset.gap;
            }
            let (word, width) = &words[i];
            runs.push(Run {
                text: word.clone(),
                x: cursor,
                y,
                width: *width,
                height: line_height,
            });
            cursor += *width;
        }

        let line_width = cursor - offset.start;
        paragraph_width = paragraph_width.max(line_width);
        lines.push(Line {
            y,
            height: line_height,
            width: line_width,
            runs,
        });
        y += line_height;
    }

    Paragraph {
        width: paragraph_width,
        height: y,
        lines,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MetricShaper, TextStyle};

    fn style() -> TextStyle<'static> {
        TextStyle {
            font_size: 10.0,
            ..TextStyle::default()
        }
    }

    /// "Focus Address Bar" laid out exactly as wide as its words: summing the
    /// same widths in another order can land a rounding error over the line,
    /// which must not wrap the last word.
    #[test]
    fn a_line_exactly_as_wide_as_its_words_does_not_wrap() {
        let widths: [f32; 3] = [12.39, 51.609, 47.008];
        // Words first, then the spaces: how max-content measures them.
        let measured = widths.iter().sum::<f32>() + 3.12 * 2.0;
        assert_eq!(wrap(&widths, 3.12, measured), vec![vec![0, 1, 2]]);
    }

    #[test]
    fn wraps_at_the_width() {
        // Each item is 10px; gap 5px. Width 24 fits one, 25 fits two.
        let widths = [10.0, 10.0, 10.0];
        assert_eq!(wrap(&widths, 5.0, 24.0), vec![vec![0], vec![1], vec![2]]);
        assert_eq!(wrap(&widths, 5.0, 25.0), vec![vec![0, 1], vec![2]]);
    }

    #[test]
    fn a_no_break_space_joins_words() {
        // CSS's collapsible white space is space, tab and line breaks; a
        // no-break space is part of the word it sits in.
        let paragraph = layout_paragraph("aaaa\u{a0}bbbb cccc", &style(), 55.0, &MetricShaper);
        assert_eq!(paragraph.lines.len(), 2);
        assert_eq!(paragraph.lines[0].runs[0].text, "aaaa\u{a0}bbbb");
        assert_eq!(
            crate::collapsible_words(" a\u{a0} b\t\n").collect::<Vec<_>>(),
            ["a\u{a0}", "b"]
        );
        assert_eq!(crate::trim_collapsible("\u{a0} x \n"), "\u{a0} x");
    }

    #[test]
    fn paragraph_wraps_and_measures() {
        let text = "aaaa bbbb cccc dddd";
        let paragraph = layout_paragraph(text, &style(), 55.0, &MetricShaper);
        // 4-letter word = 20px, space = 5px; 55 fits two words (45px).
        assert_eq!(paragraph.lines.len(), 2);
        assert_eq!(paragraph.lines[0].runs.len(), 2);
        // Two lines at 12px each.
        assert_eq!(paragraph.height, 24.0);
    }

    #[test]
    fn center_shifts_the_line() {
        let mut style = style();
        style.align = Align::Center;
        let paragraph = layout_paragraph("aa", &style, 100.0, &MetricShaper);
        // one 10px run in a 100px line -> starts at 45.
        assert_eq!(paragraph.lines[0].runs[0].x, 45.0);
    }

    #[test]
    fn right_aligns_the_line() {
        let mut style = style();
        style.align = Align::End;
        let paragraph = layout_paragraph("aa", &style, 100.0, &MetricShaper);
        assert_eq!(paragraph.lines[0].runs[0].x, 90.0);
    }

    #[test]
    fn nowrap_keeps_one_line() {
        let mut style = style();
        style.white_space = WhiteSpace::NoWrap;
        let paragraph = layout_paragraph("aaaa bbbb cccc dddd", &style, 20.0, &MetricShaper);
        assert_eq!(paragraph.lines.len(), 1);
    }

    #[test]
    fn a_hyphen_between_letters_is_a_break_opportunity() {
        // UAX #14: a break after a hyphen (HY, BA), as rust-lang.org's
        // "auto-" / "completion" wraps in Chromium; not before a digit, at
        // a word's start, or inside a run of hyphens.
        let pieces = |word| crate::break_pieces(word).collect::<Vec<_>>();
        assert_eq!(pieces("auto-completion"), ["auto-", "completion"]);
        assert_eq!(pieces("multi-editor-ish"), ["multi-", "editor-", "ish"]);
        assert_eq!(
            pieces("a\u{2010}b\u{2013}c"),
            ["a\u{2010}", "b\u{2013}", "c"]
        );
        for whole in ["-flag", "x-1", "a--b", "end-", "-", "a\u{2011}b"] {
            assert_eq!(pieces(whole), [whole], "{whole}");
        }
        assert_eq!(pieces("a\u{2003}b-c"), ["a\u{2003}", "b-", "c"]);
    }
}
