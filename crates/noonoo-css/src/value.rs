//! The CSS value grammar.
//!
//! Turns a declaration's raw value text into typed components: lengths,
//! percentages, numbers, colours, keywords, strings, URLs, functions, and the
//! `,` / `/` separators that shorthands care about. This is the layer between
//! the tokenizer and the property model.

use crate::{Token, TokenKind, tokenize};
use noonoo_parse::{ParseError, ParseResult};

/// A CSS length unit.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LengthUnit {
    /// `px`.
    Px,
    /// `em`.
    Em,
    /// `rem`.
    Rem,
    /// `ex`.
    Ex,
    /// `ch`.
    Ch,
    /// `vw`.
    Vw,
    /// `vh`.
    Vh,
    /// `vmin`.
    Vmin,
    /// `vmax`.
    Vmax,
    /// `pt`.
    Pt,
    /// `pc`.
    Pc,
    /// `in`.
    In,
    /// `cm`.
    Cm,
    /// `mm`.
    Mm,
    /// `q`.
    Q,
    /// Any other unit, kept verbatim.
    Other(String),
}

impl LengthUnit {
    /// Parse a unit from its text.
    pub fn parse(unit: &str) -> LengthUnit {
        match unit.to_ascii_lowercase().as_str() {
            "px" => Self::Px,
            "em" => Self::Em,
            "rem" => Self::Rem,
            "ex" => Self::Ex,
            "ch" => Self::Ch,
            "vw" => Self::Vw,
            "vh" => Self::Vh,
            "vmin" => Self::Vmin,
            "vmax" => Self::Vmax,
            "pt" => Self::Pt,
            "pc" => Self::Pc,
            "in" => Self::In,
            "cm" => Self::Cm,
            "mm" => Self::Mm,
            "q" => Self::Q,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// A length: a number with a unit.
#[derive(Clone, PartialEq, Debug)]
pub struct Length {
    /// The numeric part.
    pub value: f32,
    /// The unit.
    pub unit: LengthUnit,
}

impl Length {
    /// Create a length.
    pub fn new(value: f32, unit: LengthUnit) -> Self {
        Self { value, unit }
    }

    /// A `px` length.
    pub fn px(value: f32) -> Self {
        Self::new(value, LengthUnit::Px)
    }

    /// Whether this is a zero length in an absolute unit.
    pub fn is_zero(&self) -> bool {
        self.value == 0.0
    }
}

/// An RGBA colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Color {
    /// Red channel.
    pub r: u8,
    /// Green channel.
    pub g: u8,
    /// Blue channel.
    pub b: u8,
    /// Alpha channel (`0` transparent … `255` opaque).
    pub a: u8,
}

impl Color {
    /// Fully transparent black.
    pub const TRANSPARENT: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };
    /// Opaque black.
    pub const BLACK: Color = Color {
        r: 0,
        g: 0,
        b: 0,
        a: 255,
    };
    /// Opaque white.
    pub const WHITE: Color = Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    /// Build an opaque colour from RGB.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// Build an RGBA colour.
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parse a `#rgb` / `#rgba` / `#rrggbb` / `#rrggbbaa` hex colour (without
    /// the leading `#`).
    pub fn from_hex(hex: &str) -> Option<Color> {
        let nibble = |c: char| c.to_digit(16).map(|d| d as u8);
        let digits: Vec<u8> = hex.chars().map(|c| nibble(c).unwrap_or(0)).collect();
        if hex.chars().any(|c| c.to_digit(16).is_none()) {
            return None;
        }
        match digits.len() {
            3 => Some(Color::rgb(digits[0] * 17, digits[1] * 17, digits[2] * 17)),
            4 => Some(Color::rgba(
                digits[0] * 17,
                digits[1] * 17,
                digits[2] * 17,
                digits[3] * 17,
            )),
            6 => Some(Color::rgb(
                digits[0] * 16 + digits[1],
                digits[2] * 16 + digits[3],
                digits[4] * 16 + digits[5],
            )),
            8 => Some(Color::rgba(
                digits[0] * 16 + digits[1],
                digits[2] * 16 + digits[3],
                digits[4] * 16 + digits[5],
                digits[6] * 16 + digits[7],
            )),
            _ => None,
        }
    }

    /// Look up a (small) set of CSS colour keywords.
    pub fn named(name: &str) -> Option<Color> {
        let color = match name.to_ascii_lowercase().as_str() {
            "transparent" => Color::TRANSPARENT,
            "black" => Color::BLACK,
            "white" => Color::WHITE,
            "red" => Color::rgb(255, 0, 0),
            "green" => Color::rgb(0, 128, 0),
            "lime" => Color::rgb(0, 255, 0),
            "blue" => Color::rgb(0, 0, 255),
            "yellow" => Color::rgb(255, 255, 0),
            "orange" => Color::rgb(255, 165, 0),
            "purple" => Color::rgb(128, 0, 128),
            "gray" | "grey" => Color::rgb(128, 128, 128),
            "silver" => Color::rgb(192, 192, 192),
            "maroon" => Color::rgb(128, 0, 0),
            "olive" => Color::rgb(128, 128, 0),
            "teal" => Color::rgb(0, 128, 128),
            "navy" => Color::rgb(0, 0, 128),
            "aqua" | "cyan" => Color::rgb(0, 255, 255),
            "fuchsia" | "magenta" => Color::rgb(255, 0, 255),
            _ => return None,
        };
        Some(color)
    }
}

/// A single CSS component value.
#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    /// An identifier / keyword.
    Keyword(String),
    /// A length.
    Length(Length),
    /// A percentage (`50` for `50%`).
    Percentage(f32),
    /// A unitless number.
    Number(f32),
    /// A colour.
    Color(Color),
    /// A quoted string (quotes removed).
    Str(String),
    /// A `url(…)` value.
    Url(String),
    /// A function value of unknown type.
    Function {
        /// The function name.
        name: String,
        /// Its arguments, as component values.
        args: Vec<Value>,
    },
    /// A parenthesised group of component values, e.g. the `(1px + 2px)` in
    /// `calc((1px + 2px) * 2)`.
    Group(Vec<Value>),
    /// A `,` separator.
    Comma,
    /// A `/` separator.
    Slash,
    /// Any other single-character delimiter (`+`, `*`, …) inside a function.
    Delim(char),
}

impl Value {
    /// Borrow this value as a keyword.
    pub fn as_keyword(&self) -> Option<&str> {
        match self {
            Self::Keyword(name) => Some(name),
            _ => None,
        }
    }

    /// Borrow this value as a length.
    pub fn as_length(&self) -> Option<&Length> {
        match self {
            Self::Length(length) => Some(length),
            _ => None,
        }
    }

    /// Borrow this value as a colour.
    pub fn as_color(&self) -> Option<Color> {
        match self {
            Self::Color(color) => Some(*color),
            _ => None,
        }
    }

    /// Whether this is a CSS gradient function.
    pub fn is_gradient(&self) -> bool {
        matches!(self, Self::Function { name, .. } if is_gradient_name(name))
    }

    /// Whether this is a `calc()` function.
    pub fn is_calc(&self) -> bool {
        matches!(self, Self::Function { name, .. } if name.eq_ignore_ascii_case("calc"))
    }
}

/// Whether `name` is a CSS gradient function.
pub fn is_gradient_name(name: &str) -> bool {
    [
        "linear-gradient",
        "repeating-linear-gradient",
        "radial-gradient",
        "repeating-radial-gradient",
    ]
    .iter()
    .any(|gradient| name.eq_ignore_ascii_case(gradient))
}

/// Parse a declaration value into its component values.
pub fn parse_values(source: &str) -> ParseResult<Vec<Value>> {
    let tokens = tokenize(source);
    let (values, next) = parse_components(&tokens, 0)?;
    if next != tokens.len() {
        return Err(ParseError::new(
            tokens.get(next).map(|t| t.start).unwrap_or(source.len()),
            "unexpected token in value",
        ));
    }
    Ok(values)
}

fn parse_components(tokens: &[Token], mut pos: usize) -> ParseResult<(Vec<Value>, usize)> {
    let mut values = Vec::new();
    while let Some(token) = tokens.get(pos).cloned() {
        match token.kind {
            TokenKind::Whitespace | TokenKind::Comment => {
                pos += 1;
            }
            TokenKind::CloseParen => break,
            TokenKind::OpenParen => {
                let (inner, next) = parse_components(tokens, pos + 1)?;
                if !matches!(
                    tokens.get(next).map(|t| &t.kind),
                    Some(TokenKind::CloseParen)
                ) {
                    return Err(ParseError::new(token.start, "unclosed parenthesis"));
                }
                values.push(Value::Group(inner));
                pos = next + 1;
            }
            TokenKind::Comma => {
                values.push(Value::Comma);
                pos += 1;
            }
            TokenKind::Delim('/') => {
                values.push(Value::Slash);
                pos += 1;
            }
            TokenKind::Delim(ch) => {
                values.push(Value::Delim(ch));
                pos += 1;
            }
            TokenKind::Ident(name) => {
                values.push(Value::Keyword(name));
                pos += 1;
            }
            TokenKind::Number(value) => {
                values.push(Value::Number(value as f32));
                pos += 1;
            }
            TokenKind::Percentage(value) => {
                values.push(Value::Percentage(value as f32));
                pos += 1;
            }
            TokenKind::Dimension { value, unit } => {
                values.push(Value::Length(Length::new(
                    value as f32,
                    LengthUnit::parse(&unit),
                )));
                pos += 1;
            }
            TokenKind::Hash { value, .. } => {
                match Color::from_hex(&value) {
                    Some(color) => values.push(Value::Color(color)),
                    None => values.push(Value::Keyword(format!("#{value}"))),
                }
                pos += 1;
            }
            TokenKind::Str(text) => {
                values.push(Value::Str(text));
                pos += 1;
            }
            TokenKind::Url(url) => {
                values.push(Value::Url(url));
                pos += 1;
            }
            TokenKind::Function(name) => {
                let (args, next) = parse_components(tokens, pos + 1)?;
                if !matches!(
                    tokens.get(next).map(|t| &t.kind),
                    Some(TokenKind::CloseParen)
                ) {
                    return Err(ParseError::new(token.start, "unclosed function"));
                }
                pos = next + 1;
                if name.eq_ignore_ascii_case("rgb") || name.eq_ignore_ascii_case("rgba") {
                    match color_from_function(&args) {
                        Some(color) => values.push(Value::Color(color)),
                        None => values.push(Value::Function { name, args }),
                    }
                } else {
                    values.push(Value::Function { name, args });
                }
            }
            other => {
                return Err(ParseError::new(
                    token.start,
                    format!("unexpected value token {other:?}"),
                ));
            }
        }
    }
    Ok((values, pos))
}

fn color_from_function(args: &[Value]) -> Option<Color> {
    let parts: Vec<&Value> = args
        .iter()
        .filter(|value| !matches!(value, Value::Comma | Value::Slash))
        .collect();
    let channel = |value: &Value| -> Option<u8> {
        match value {
            Value::Number(n) => Some((*n).clamp(0.0, 255.0).round() as u8),
            Value::Percentage(p) => Some((p / 100.0 * 255.0).clamp(0.0, 255.0).round() as u8),
            _ => None,
        }
    };
    let alpha = |value: &Value| -> Option<u8> {
        match value {
            Value::Number(n) => Some((n * 255.0).clamp(0.0, 255.0).round() as u8),
            Value::Percentage(p) => Some((p / 100.0 * 255.0).clamp(0.0, 255.0).round() as u8),
            _ => None,
        }
    };
    match parts.as_slice() {
        [r, g, b] => Some(Color::rgb(channel(r)?, channel(g)?, channel(b)?)),
        [r, g, b, a] => Some(Color::rgba(
            channel(r)?,
            channel(g)?,
            channel(b)?,
            alpha(a)?,
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lengths_and_numbers() {
        assert_eq!(
            parse_values("10px").unwrap(),
            vec![Value::Length(Length::px(10.0))]
        );
        assert_eq!(
            parse_values("2.5em").unwrap(),
            vec![Value::Length(Length::new(2.5, LengthUnit::Em))]
        );
        assert_eq!(parse_values("50%").unwrap(), vec![Value::Percentage(50.0)]);
        assert_eq!(parse_values("0").unwrap(), vec![Value::Number(0.0)]);
    }

    #[test]
    fn parses_hex_and_named_colors() {
        assert_eq!(
            parse_values("#f00").unwrap(),
            vec![Value::Color(Color::rgb(255, 0, 0))]
        );
        assert_eq!(
            parse_values("#ff000080").unwrap(),
            vec![Value::Color(Color::rgba(255, 0, 0, 128))]
        );
        assert_eq!(
            parse_values("red").unwrap(),
            vec![Value::Keyword("red".into())]
        );
        assert_eq!(
            parse_values("transparent").unwrap(),
            vec![Value::Keyword("transparent".into())]
        );
    }

    #[test]
    fn parses_rgb_functions() {
        assert_eq!(
            parse_values("rgb(1, 2, 3)").unwrap(),
            vec![Value::Color(Color::rgb(1, 2, 3))]
        );
        assert_eq!(
            parse_values("rgba(255, 0, 0, 0.5)").unwrap(),
            vec![Value::Color(Color::rgba(255, 0, 0, 128))]
        );
    }

    #[test]
    fn parses_lists_and_shorthand_separators() {
        assert_eq!(
            parse_values("0 auto").unwrap(),
            vec![Value::Number(0.0), Value::Keyword("auto".into())]
        );
        assert_eq!(
            parse_values("a, b").unwrap(),
            vec![
                Value::Keyword("a".into()),
                Value::Comma,
                Value::Keyword("b".into())
            ]
        );
    }

    #[test]
    fn parses_unknown_functions() {
        let values = parse_values("calc(1px + 2px)").unwrap();
        assert!(matches!(&values[0], Value::Function { name, .. } if name == "calc"));
    }

    #[test]
    fn parses_grouping_inside_functions() {
        let values = parse_values("calc((1px + 2px) * 2)").unwrap();
        let Value::Function { name, args } = &values[0] else {
            panic!("expected a function, got {values:?}");
        };
        assert_eq!(name, "calc");
        assert!(args.iter().any(|value| matches!(value, Value::Group(_))));
    }
}
