//! Parsing CSS gradients into [`Gradient`] values.
//!
//! Both `linear-gradient()` and `radial-gradient()` (and their `repeating-`
//! forms) are parsed here, next to the rest of value resolution. Anything that
//! depends on the box's size stays symbolic for `noonoo-paint`:
//!
//! - corner directions (`to top right`) depend on the aspect ratio,
//! - radial sizes (`closest-side`, `farthest-corner`, …) depend on the box,
//! - stop positions may be `<length-percentage>` and may need the CSS implicit
//!   position fix-up.
//!
//! Colours, shapes, and explicit radii are resolved immediately.

use noonoo_css::Color;
use noonoo_css::value::{Length, LengthUnit, Value};

use crate::Viewport;
use crate::apply::dimension;
use crate::computed::{
    Dimension, Gradient, GradientKind, GradientStop, LinearDirection, RadialShape, RadialSize,
};

/// Parse a gradient function, returning `None` for anything invalid.
pub(crate) fn parse(
    values: &[Value],
    font_size: f32,
    root: f32,
    viewport: Viewport,
    current: Color,
) -> Option<Gradient> {
    let [Value::Function { name, args }] = values else {
        return None;
    };
    let lower = name.to_ascii_lowercase();
    let (linear, repeating) = match lower.as_str() {
        "linear-gradient" => (true, false),
        "repeating-linear-gradient" => (true, true),
        "radial-gradient" => (false, false),
        "repeating-radial-gradient" => (false, true),
        _ => return None,
    };

    let mut groups = split_commas(args)?;
    let kind = if linear {
        let direction = if groups.first().is_some_and(|g| !is_stop(g, current)) {
            let direction = linear_direction(groups[0])?;
            groups.remove(0);
            direction
        } else {
            LinearDirection::ToBottom
        };
        GradientKind::Linear { direction }
    } else if groups.first().is_some_and(|g| !is_stop(g, current)) {
        let kind = radial_config(groups[0], font_size, root, viewport)?;
        groups.remove(0);
        kind
    } else {
        GradientKind::Radial {
            shape: RadialShape::Ellipse,
            size: RadialSize::FarthestCorner,
            center: (Dimension::Percent(50.0), Dimension::Percent(50.0)),
        }
    };

    let mut stops = Vec::new();
    for group in groups {
        stops.extend(stop(group, font_size, root, viewport, current)?);
    }
    (stops.len() >= 2).then_some(Gradient {
        kind,
        repeating,
        stops,
    })
}

/// Whether a comma-separated group is a colour stop (it starts with a colour)
/// rather than the gradient's direction/shape configuration.
fn is_stop(group: &[Value], current: Color) -> bool {
    group.first().and_then(|v| color(v, current)).is_some()
}

/// A colour, including `currentcolor`.
fn color(value: &Value, current: Color) -> Option<Color> {
    match value {
        Value::Color(color) => Some(*color),
        Value::Keyword(keyword) if keyword.eq_ignore_ascii_case("currentcolor") => Some(current),
        Value::Keyword(keyword) => Color::named(keyword),
        _ => None,
    }
}

/// One group: `<color> <length-percentage>? <length-percentage>?`, where two
/// positions make two stops (the CSS "double position" syntax).
fn stop(
    group: &[Value],
    font_size: f32,
    root: f32,
    viewport: Viewport,
    current: Color,
) -> Option<Vec<GradientStop>> {
    let color = color(group.first()?, current)?;
    let mut positions = Vec::new();
    for value in &group[1..] {
        let position = dimension(std::slice::from_ref(value), font_size, root, viewport)?;
        if matches!(position, Dimension::Auto) {
            return None;
        }
        positions.push(position);
    }
    match positions.len() {
        0 => Some(vec![GradientStop {
            color,
            position: None,
        }]),
        1 => Some(vec![GradientStop {
            color,
            position: Some(positions.remove(0)),
        }]),
        2 => {
            let second = positions.pop()?;
            let first = positions.pop()?;
            Some(vec![
                GradientStop {
                    color,
                    position: Some(first),
                },
                GradientStop {
                    color,
                    position: Some(second),
                },
            ])
        }
        _ => None,
    }
}

fn linear_direction(group: &[Value]) -> Option<LinearDirection> {
    match group {
        [Value::Length(length)] => angle(length).map(LinearDirection::Angle),
        [Value::Number(number)] if *number == 0.0 => Some(LinearDirection::Angle(0.0)),
        [Value::Keyword(to), sides @ ..] if to.eq_ignore_ascii_case("to") => {
            let sides: Vec<&str> = sides.iter().map(Value::as_keyword).collect::<Option<_>>()?;
            match sides.as_slice() {
                ["top"] => Some(LinearDirection::ToTop),
                ["right"] => Some(LinearDirection::ToRight),
                ["bottom"] => Some(LinearDirection::ToBottom),
                ["left"] => Some(LinearDirection::ToLeft),
                ["top", "right"] | ["right", "top"] => Some(LinearDirection::ToTopRight),
                ["bottom", "right"] | ["right", "bottom"] => Some(LinearDirection::ToBottomRight),
                ["bottom", "left"] | ["left", "bottom"] => Some(LinearDirection::ToBottomLeft),
                ["top", "left"] | ["left", "top"] => Some(LinearDirection::ToTopLeft),
                _ => None,
            }
        }
        _ => None,
    }
}

pub(crate) fn angle(length: &Length) -> Option<f32> {
    let value = length.value;
    match &length.unit {
        LengthUnit::Other(unit) if unit.eq_ignore_ascii_case("deg") => Some(value),
        LengthUnit::Other(unit) if unit.eq_ignore_ascii_case("grad") => Some(value * 0.9),
        LengthUnit::Other(unit) if unit.eq_ignore_ascii_case("rad") => Some(value.to_degrees()),
        LengthUnit::Other(unit) if unit.eq_ignore_ascii_case("turn") => Some(value * 360.0),
        _ => None,
    }
}

/// `[ <shape> || <size> ]? [ at <position> ]?`
fn radial_config(
    tokens: &[Value],
    font_size: f32,
    root: f32,
    viewport: Viewport,
) -> Option<GradientKind> {
    let at = tokens.iter().position(|value| {
        value
            .as_keyword()
            .is_some_and(|k| k.eq_ignore_ascii_case("at"))
    });
    let (config, center) = match at {
        Some(index) => (
            &tokens[..index],
            position(&tokens[index + 1..], font_size, root, viewport)?,
        ),
        None => (tokens, (Dimension::Percent(50.0), Dimension::Percent(50.0))),
    };

    let mut shape = None;
    let mut size = None;
    let mut radii = Vec::new();
    for token in config {
        if let Some(keyword) = token.as_keyword() {
            match keyword.to_ascii_lowercase().as_str() {
                "circle" => shape = Some(RadialShape::Circle),
                "ellipse" => shape = Some(RadialShape::Ellipse),
                "closest-side" => size = Some(RadialSize::ClosestSide),
                "farthest-side" => size = Some(RadialSize::FarthestSide),
                "closest-corner" => size = Some(RadialSize::ClosestCorner),
                "farthest-corner" => size = Some(RadialSize::FarthestCorner),
                _ => return None,
            }
        } else {
            let radius = dimension(std::slice::from_ref(token), font_size, root, viewport)?;
            if matches!(radius, Dimension::Auto) {
                return None;
            }
            radii.push(radius);
        }
    }
    if radii.len() > 2
        || (size.is_some() && !radii.is_empty())
        || (shape == Some(RadialShape::Circle) && radii.len() == 2)
    {
        return None;
    }

    let shape = shape.unwrap_or(if radii.len() == 2 {
        RadialShape::Ellipse
    } else {
        RadialShape::Circle
    });
    let size = match size {
        Some(size) => size,
        None if !radii.is_empty() => {
            let mut radii = radii.into_iter();
            let x = radii.next()?;
            RadialSize::Explicit { x, y: radii.next() }
        }
        None => RadialSize::FarthestCorner,
    };
    Some(GradientKind::Radial {
        shape,
        size,
        center,
    })
}

/// `<position>`: one or two keywords/lengths, `left`/`center`/`right` first.
fn position(
    tokens: &[Value],
    font_size: f32,
    root: f32,
    viewport: Viewport,
) -> Option<(Dimension, Dimension)> {
    enum Slot {
        Left,
        Right,
        Top,
        Bottom,
        Center,
        Length(Dimension),
    }
    let mut slots = Vec::new();
    for token in tokens {
        slots.push(match token {
            Value::Keyword(keyword) => match keyword.to_ascii_lowercase().as_str() {
                "left" => Slot::Left,
                "right" => Slot::Right,
                "top" => Slot::Top,
                "bottom" => Slot::Bottom,
                "center" => Slot::Center,
                _ => return None,
            },
            _ => Slot::Length(dimension(
                std::slice::from_ref(token),
                font_size,
                root,
                viewport,
            )?),
        });
    }
    let to_dimension = |slot: Slot| {
        Some(match slot {
            Slot::Left | Slot::Top => Dimension::Percent(0.0),
            Slot::Right | Slot::Bottom => Dimension::Percent(100.0),
            Slot::Center => Dimension::Percent(50.0),
            Slot::Length(dimension) => dimension,
        })
    };
    match slots.len() {
        1 => {
            let slot = slots.pop()?;
            match slot {
                Slot::Top | Slot::Bottom => Some((Dimension::Percent(50.0), to_dimension(slot)?)),
                slot => Some((to_dimension(slot)?, Dimension::Percent(50.0))),
            }
        }
        2 => {
            let mut slots = slots.into_iter();
            let mut first = slots.next()?;
            let mut second = slots.next()?;
            let vertical_first = matches!(first, Slot::Top | Slot::Bottom);
            let horizontal_second = matches!(second, Slot::Left | Slot::Right);
            if vertical_first || horizontal_second {
                std::mem::swap(&mut first, &mut second);
            }
            Some((to_dimension(first)?, to_dimension(second)?))
        }
        _ => None,
    }
}

fn split_commas(args: &[Value]) -> Option<Vec<&[Value]>> {
    let mut groups = Vec::new();
    let mut start = 0;
    for (index, value) in args.iter().enumerate() {
        if matches!(value, Value::Comma) {
            groups.push(&args[start..index]);
            start = index + 1;
        }
    }
    groups.push(&args[start..]);
    groups
        .iter()
        .all(|group| !group.is_empty())
        .then_some(groups)
}

#[cfg(test)]
mod tests {
    use super::*;
    use noonoo_css::value::parse_values;

    fn parse_css(source: &str) -> Option<Gradient> {
        let values = parse_values(source).ok()?;
        parse(&values, 16.0, 16.0, Viewport::DEFAULT, Color::BLACK)
    }

    fn stops(gradient: &Gradient) -> Vec<(u8, u8, u8, Option<Dimension>)> {
        gradient
            .stops
            .iter()
            .map(|stop| {
                (
                    stop.color.r,
                    stop.color.g,
                    stop.color.b,
                    stop.position.clone(),
                )
            })
            .collect()
    }

    #[test]
    fn parses_a_linear_gradient() {
        let gradient = parse_css("linear-gradient(red, blue)").unwrap();
        assert_eq!(
            gradient.kind,
            GradientKind::Linear {
                direction: LinearDirection::ToBottom
            }
        );
        assert!(!gradient.repeating);
        assert_eq!(
            stops(&gradient),
            vec![(255, 0, 0, None), (0, 0, 255, None),]
        );
    }

    #[test]
    fn parses_directions_and_angles() {
        assert_eq!(
            parse_css("linear-gradient(to right, red, blue)")
                .unwrap()
                .kind,
            GradientKind::Linear {
                direction: LinearDirection::ToRight
            }
        );
        assert_eq!(
            parse_css("linear-gradient(to top left, red, blue)")
                .unwrap()
                .kind,
            GradientKind::Linear {
                direction: LinearDirection::ToTopLeft
            }
        );
        assert_eq!(
            parse_css("linear-gradient(45deg, red, blue)").unwrap().kind,
            GradientKind::Linear {
                direction: LinearDirection::Angle(45.0)
            }
        );
        assert_eq!(
            parse_css("linear-gradient(0.5turn, red, blue)")
                .unwrap()
                .kind,
            GradientKind::Linear {
                direction: LinearDirection::Angle(180.0)
            }
        );
        let GradientKind::Linear {
            direction: LinearDirection::Angle(radians),
        } = parse_css("linear-gradient(1.5708rad, red, blue)")
            .unwrap()
            .kind
        else {
            panic!("expected an angle");
        };
        assert!((radians - 90.0).abs() < 0.01, "got {radians}");
    }

    #[test]
    fn parses_stop_positions_and_double_positions() {
        let gradient = parse_css("linear-gradient(red 10%, blue 50% 75%, green)").unwrap();
        assert_eq!(
            stops(&gradient),
            vec![
                (255, 0, 0, Some(Dimension::Percent(10.0))),
                (0, 0, 255, Some(Dimension::Percent(50.0))),
                (0, 0, 255, Some(Dimension::Percent(75.0))),
                (0, 128, 0, None),
            ]
        );
    }

    #[test]
    fn parses_radial_configurations() {
        assert_eq!(
            parse_css("radial-gradient(circle, red, blue)")
                .unwrap()
                .kind,
            GradientKind::Radial {
                shape: RadialShape::Circle,
                size: RadialSize::FarthestCorner,
                center: (Dimension::Percent(50.0), Dimension::Percent(50.0)),
            }
        );
        assert_eq!(
            parse_css("radial-gradient(circle closest-side at 20% 30%, red, blue)")
                .unwrap()
                .kind,
            GradientKind::Radial {
                shape: RadialShape::Circle,
                size: RadialSize::ClosestSide,
                center: (Dimension::Percent(20.0), Dimension::Percent(30.0)),
            }
        );
        assert_eq!(
            parse_css("radial-gradient(ellipse 40px 20px at left top, red, blue)")
                .unwrap()
                .kind,
            GradientKind::Radial {
                shape: RadialShape::Ellipse,
                size: RadialSize::Explicit {
                    x: Dimension::Px(40.0),
                    y: Some(Dimension::Px(20.0)),
                },
                center: (Dimension::Percent(0.0), Dimension::Percent(0.0)),
            }
        );
        assert_eq!(
            parse_css("radial-gradient(30px, red, blue)").unwrap().kind,
            GradientKind::Radial {
                shape: RadialShape::Circle,
                size: RadialSize::Explicit {
                    x: Dimension::Px(30.0),
                    y: None,
                },
                center: (Dimension::Percent(50.0), Dimension::Percent(50.0)),
            }
        );
    }

    #[test]
    fn repeating_forms_are_flagged() {
        assert!(
            parse_css("repeating-linear-gradient(red, blue)")
                .unwrap()
                .repeating
        );
        assert!(
            parse_css("repeating-radial-gradient(red, blue)")
                .unwrap()
                .repeating
        );
    }

    #[test]
    fn current_color_is_resolved() {
        let values = parse_values("linear-gradient(currentColor, blue)").unwrap();
        let gradient = parse(&values, 16.0, 16.0, Viewport::DEFAULT, Color::rgb(1, 2, 3)).unwrap();
        assert_eq!(gradient.stops[0].color, Color::rgb(1, 2, 3));
    }

    #[test]
    fn invalid_gradients_are_rejected() {
        assert_eq!(parse_css("linear-gradient(red)"), None, "needs two stops");
        assert_eq!(parse_css("linear-gradient(red 10% 20% 30%, blue)"), None);
        assert_eq!(
            parse_css("radial-gradient(circle 10px 20px, red, blue)"),
            None
        );
        assert_eq!(
            parse_css("radial-gradient(farthest-side 10px, red, blue)"),
            None
        );
        assert_eq!(parse_css("linear-gradient(to nowhere, red, blue)"), None);
    }
}
