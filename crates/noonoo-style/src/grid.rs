//! Parsing `grid-template-*`, `grid-auto-*`, `grid-column`, and `grid-row`.
//!
//! Track lists accept `<length-percentage>`, `auto`, and `<fr>`, with
//! `repeat(<integer>, <track-list>)` expanding in place. Placement accepts
//! line numbers, `span N`, and `auto` (see [`GridLine`]); the two-value
//! `start / end` form splits on the slash.

use noonoo_css::value::{Length, LengthUnit, Value};

use crate::Viewport;
use crate::apply::dimension;
use crate::computed::{Dimension, GridLine, GridRange, Track};

/// Parse a track list: `grid-template-columns` / `grid-template-rows` /
/// `grid-auto-*`. `None` when any track is invalid.
pub(crate) fn tracks(
    values: &[Value],
    font_size: f32,
    root: f32,
    viewport: Viewport,
) -> Option<Vec<Track>> {
    let mut tracks = Vec::new();
    for value in values {
        match value {
            Value::Function { name, args } if name.eq_ignore_ascii_case("repeat") => {
                let (count, pattern) = repeat(args, font_size, root, viewport)?;
                for _ in 0..count {
                    tracks.extend(pattern.iter().cloned());
                }
            }
            _ => tracks.push(track(
                std::slice::from_ref(value),
                font_size,
                root,
                viewport,
            )?),
        }
    }
    (!tracks.is_empty()).then_some(tracks)
}

/// `repeat(<integer>, <track-list>)`.
fn repeat(
    args: &[Value],
    font_size: f32,
    root: f32,
    viewport: Viewport,
) -> Option<(usize, Vec<Track>)> {
    let [Value::Number(count), Value::Comma, rest @ ..] = args else {
        return None;
    };
    let count = *count;
    if !(1.0..=10_000.0).contains(&count) || count.fract() != 0.0 {
        return None;
    }
    let mut pattern = Vec::new();
    for value in rest {
        pattern.push(track(
            std::slice::from_ref(value),
            font_size,
            root,
            viewport,
        )?);
    }
    (!pattern.is_empty()).then_some((count as usize, pattern))
}

/// One track: `auto`, a length/percentage (or math function), or `<fr>`.
fn track(values: &[Value], font_size: f32, root: f32, viewport: Viewport) -> Option<Track> {
    match values {
        [Value::Keyword(keyword)] if keyword.eq_ignore_ascii_case("auto") => Some(Track::Auto),
        [Value::Length(Length { value, unit })] if unit_is(unit, "fr") => {
            (*value >= 0.0).then_some(Track::Fr(*value))
        }
        _ => {
            let dimension = dimension(values, font_size, root, viewport)?;
            match dimension {
                Dimension::Auto => Some(Track::Auto),
                dimension => Some(Track::Length(dimension)),
            }
        }
    }
}

fn unit_is(unit: &LengthUnit, name: &str) -> bool {
    matches!(unit, LengthUnit::Other(other) if other.eq_ignore_ascii_case(name))
}

/// Parse `grid-column` / `grid-row`: `<start> [/ <end>]`.
pub(crate) fn placement(values: &[Value]) -> Option<GridRange> {
    let slash = values
        .iter()
        .position(|value| matches!(value, Value::Slash));
    let (start, end) = match slash {
        // An explicit `/` requires a parseable end side; a dropped one would
        // silently reinterpret the declaration.
        Some(index) => (line(&values[..index])?, line(&values[index + 1..])?),
        None => (line(values)?, GridLine::Auto),
    };
    Some(GridRange { start, end })
}

/// One side of a placement: a line number, `span N`, or `auto`.
fn line(values: &[Value]) -> Option<GridLine> {
    match values {
        [Value::Keyword(keyword)] if keyword.eq_ignore_ascii_case("auto") => Some(GridLine::Auto),
        [Value::Number(number)] => {
            // Negative lines count from the end; not modelled yet.
            ((1.0..=10_000.0).contains(number) && number.fract() == 0.0)
                .then_some(GridLine::Line(*number as u16))
        }
        [Value::Keyword(keyword)] if keyword.eq_ignore_ascii_case("span") => {
            Some(GridLine::Span(1))
        }
        [Value::Keyword(keyword), Value::Number(number)]
            if keyword.eq_ignore_ascii_case("span") =>
        {
            ((1.0..=10_000.0).contains(number) && number.fract() == 0.0)
                .then_some(GridLine::Span(*number as u16))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noonoo_css::value::parse_values;

    fn parse(source: &str) -> Option<Vec<Track>> {
        let values = parse_values(source).ok()?;
        tracks(&values, 16.0, 16.0, Viewport::DEFAULT)
    }

    fn placement_of(source: &str) -> Option<GridRange> {
        let values = parse_values(source).ok()?;
        placement(&values)
    }

    #[test]
    fn parses_track_lists() {
        assert_eq!(
            parse("150px 1fr auto"),
            Some(vec![
                Track::Length(Dimension::Px(150.0)),
                Track::Fr(1.0),
                Track::Auto,
            ])
        );
        assert_eq!(
            parse("50% 2fr"),
            Some(vec![
                Track::Length(Dimension::Percent(50.0)),
                Track::Fr(2.0)
            ])
        );
        assert_eq!(
            parse("calc(50% - 10px)"),
            Some(vec![Track::Length(Dimension::Calc {
                px: -10.0,
                percent: 50.0
            })])
        );
    }

    #[test]
    fn repeat_expands_in_place() {
        assert_eq!(
            parse("repeat(2, 40px 1fr)"),
            Some(vec![
                Track::Length(Dimension::Px(40.0)),
                Track::Fr(1.0),
                Track::Length(Dimension::Px(40.0)),
                Track::Fr(1.0),
            ])
        );
        assert_eq!(parse("repeat(0, 1fr)"), None);
        assert_eq!(parse("repeat(2)"), None);
    }

    #[test]
    fn invalid_tracks_are_rejected() {
        assert_eq!(parse("10px none"), None);
        assert_eq!(parse("1fr -1fr"), None);
    }

    #[test]
    fn placements_parse() {
        assert_eq!(
            placement_of("2"),
            Some(GridRange {
                start: GridLine::Line(2),
                end: GridLine::Auto
            })
        );
        assert_eq!(
            placement_of("1 / 3"),
            Some(GridRange {
                start: GridLine::Line(1),
                end: GridLine::Line(3)
            })
        );
        assert_eq!(
            placement_of("auto / span 2"),
            Some(GridRange {
                start: GridLine::Auto,
                end: GridLine::Span(2)
            })
        );
        assert_eq!(
            placement_of("span 2"),
            Some(GridRange {
                start: GridLine::Span(2),
                end: GridLine::Auto
            })
        );
    }

    #[test]
    fn invalid_placements_are_rejected() {
        assert_eq!(placement_of("-1 / 2"), None);
        assert_eq!(placement_of("1 / span 0"), None);
        assert_eq!(placement_of("left"), None);
    }
}
