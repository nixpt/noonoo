//! Line breaking, alignment, and paragraph layout.

use crate::{Align, Shaper, TextStyle, WhiteSpace};

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

/// Greedy line breaking: `widths` are item advances, `gap` the inter-item space.
pub fn wrap(widths: &[f32], gap: f32, max_width: f32) -> Vec<Vec<usize>> {
    let mut lines: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut used = 0.0;
    for (index, &width) in widths.iter().enumerate() {
        if current.is_empty() {
            used = width;
            current.push(index);
        } else if used + gap + width > max_width {
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
    let line_height = if style.line_height > 0.0 {
        style.line_height
    } else {
        shaper.line_height(style.font_size)
    };

    let words: Vec<(String, f32)> = text
        .split_whitespace()
        .map(|word| {
            (
                word.to_owned(),
                shaper.advance(word, style.font_size, style.bold),
            )
        })
        .collect();
    let widths: Vec<f32> = words.iter().map(|(_, width)| *width).collect();
    let space = shaper.advance(" ", style.font_size, style.bold);

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

    fn style() -> TextStyle {
        TextStyle {
            font_size: 10.0,
            ..TextStyle::default()
        }
    }

    #[test]
    fn wraps_at_the_width() {
        // Each item is 10px; gap 5px. Width 24 fits one, 25 fits two.
        let widths = [10.0, 10.0, 10.0];
        assert_eq!(wrap(&widths, 5.0, 24.0), vec![vec![0], vec![1], vec![2]]);
        assert_eq!(wrap(&widths, 5.0, 25.0), vec![vec![0, 1], vec![2]]);
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
}
