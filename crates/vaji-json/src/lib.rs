//! # vaji-json
//!
//! A complete, dependency-free JSON parser, value model, and writer
//! (RFC 8259). It exists because the engine needs a JSON value to *project*
//! other formats into — most immediately [`vaji_caison`](#) — and pulling a
//! crate for that would be the kind of incidental dependency the project
//! declines.
//!
//! Object key order is preserved, and numbers are [`f64`]. The writer emits
//! integral floats with a trailing `.0` (so `8080.0` round-trips visibly as a
//! float), which is what the CAISON projection expects.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fmt::Write as _;

use vaji_parse::{Cursor, ParseError, ParseResult};

/// A JSON value.
#[derive(Clone, PartialEq, Debug)]
pub enum Json {
    /// `null`.
    Null,
    /// `true` / `false`.
    Bool(bool),
    /// A number (all JSON numbers are IEEE-754 doubles here).
    Number(f64),
    /// A string.
    String(String),
    /// An array.
    Array(Vec<Json>),
    /// An object, with keys in insertion order.
    Object(Vec<(String, Json)>),
}

impl Json {
    /// Parse a complete JSON document. Trailing non-whitespace is an error.
    pub fn parse(input: &str) -> ParseResult<Json> {
        let mut cursor = Cursor::new(input);
        let value = parse_value(&mut cursor)?;
        skip_ws(&mut cursor);
        if !cursor.is_eof() {
            return Err(cursor.error("trailing characters after JSON value"));
        }
        Ok(value)
    }

    /// The kind name, for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "boolean",
            Self::Number(_) => "number",
            Self::String(_) => "string",
            Self::Array(_) => "array",
            Self::Object(_) => "object",
        }
    }

    /// Borrow this value as an array.
    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Self::Array(items) => Some(items),
            _ => None,
        }
    }

    /// Borrow this value as an object's key/value pairs.
    pub fn as_object(&self) -> Option<&[(String, Json)]> {
        match self {
            Self::Object(pairs) => Some(pairs),
            _ => None,
        }
    }

    /// Look up a key in this object.
    pub fn get(&self, key: &str) -> Option<&Json> {
        self.as_object()?
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    /// Render indented JSON (two spaces per level).
    pub fn to_string_pretty(&self) -> String {
        let mut out = String::new();
        write_pretty(self, 0, &mut out);
        out
    }
}

impl std::fmt::Display for Json {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = String::new();
        write_compact(self, &mut out);
        f.write_str(&out)
    }
}

// --- parsing ---------------------------------------------------------------

fn skip_ws(cursor: &mut Cursor<'_>) {
    cursor.skip_while(|ch| matches!(ch, ' ' | '\t' | '\n' | '\r'));
}

fn parse_value(cursor: &mut Cursor<'_>) -> ParseResult<Json> {
    skip_ws(cursor);
    match cursor.peek() {
        None => Err(cursor.error("unexpected end of input")),
        Some('{') => parse_object(cursor),
        Some('[') => parse_array(cursor),
        Some('"') => Ok(Json::String(parse_string(cursor)?)),
        Some('t') => expect_literal(cursor, "true").map(|()| Json::Bool(true)),
        Some('f') => expect_literal(cursor, "false").map(|()| Json::Bool(false)),
        Some('n') => expect_literal(cursor, "null").map(|()| Json::Null),
        Some(c) if c == '-' || c.is_ascii_digit() => parse_number(cursor),
        Some(c) => Err(cursor.error(format!("unexpected character `{c}`"))),
    }
}

fn expect_literal(cursor: &mut Cursor<'_>, literal: &str) -> ParseResult<()> {
    if cursor.eat_str(literal) {
        Ok(())
    } else {
        Err(cursor.error(format!("expected `{literal}`")))
    }
}

fn parse_object(cursor: &mut Cursor<'_>) -> ParseResult<Json> {
    cursor.bump(); // '{'
    let mut pairs = Vec::new();
    skip_ws(cursor);
    if cursor.eat('}') {
        return Ok(Json::Object(pairs));
    }
    loop {
        skip_ws(cursor);
        if cursor.peek() != Some('"') {
            return Err(cursor.error("expected a string key"));
        }
        let key = parse_string(cursor)?;
        skip_ws(cursor);
        if !cursor.eat(':') {
            return Err(cursor.error("expected `:` after object key"));
        }
        let value = parse_value(cursor)?;
        pairs.push((key, value));
        skip_ws(cursor);
        if cursor.eat(',') {
            continue;
        }
        if cursor.eat('}') {
            return Ok(Json::Object(pairs));
        }
        return Err(cursor.error("expected `,` or `}` in object"));
    }
}

fn parse_array(cursor: &mut Cursor<'_>) -> ParseResult<Json> {
    cursor.bump(); // '['
    let mut items = Vec::new();
    skip_ws(cursor);
    if cursor.eat(']') {
        return Ok(Json::Array(items));
    }
    loop {
        items.push(parse_value(cursor)?);
        skip_ws(cursor);
        if cursor.eat(',') {
            continue;
        }
        if cursor.eat(']') {
            return Ok(Json::Array(items));
        }
        return Err(cursor.error("expected `,` or `]` in array"));
    }
}

fn parse_string(cursor: &mut Cursor<'_>) -> ParseResult<String> {
    cursor.bump(); // opening quote
    let mut out = String::new();
    loop {
        match cursor.bump() {
            None => return Err(cursor.error("unterminated string")),
            Some('"') => return Ok(out),
            Some('\\') => out.push(parse_escape(cursor)?),
            Some(c) if (c as u32) < 0x20 => {
                return Err(cursor.error("unescaped control character in string"));
            }
            Some(c) => out.push(c),
        }
    }
}

fn parse_escape(cursor: &mut Cursor<'_>) -> ParseResult<char> {
    match cursor.bump() {
        Some('"') => Ok('"'),
        Some('\\') => Ok('\\'),
        Some('/') => Ok('/'),
        Some('b') => Ok('\u{0008}'),
        Some('f') => Ok('\u{000c}'),
        Some('n') => Ok('\n'),
        Some('r') => Ok('\r'),
        Some('t') => Ok('\t'),
        Some('u') => {
            let hi = parse_hex4(cursor)?;
            if (0xD800..=0xDBFF).contains(&hi) {
                // High surrogate: require a following "\uXXXX" low surrogate.
                if !cursor.eat_str("\\u") {
                    return Err(cursor.error("unpaired surrogate escape"));
                }
                let lo = parse_hex4(cursor)?;
                if !(0xDC00..=0xDFFF).contains(&lo) {
                    return Err(cursor.error("invalid low surrogate"));
                }
                let combined = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                char::from_u32(combined).ok_or_else(|| cursor.error("invalid surrogate pair"))
            } else if (0xDC00..=0xDFFF).contains(&hi) {
                Err(cursor.error("unexpected low surrogate"))
            } else {
                char::from_u32(hi).ok_or_else(|| cursor.error("invalid unicode escape"))
            }
        }
        Some(c) => Err(cursor.error(format!("invalid escape `\\{c}`"))),
        None => Err(cursor.error("unterminated escape")),
    }
}

fn parse_hex4(cursor: &mut Cursor<'_>) -> ParseResult<u32> {
    let mut value = 0u32;
    for _ in 0..4 {
        let ch = cursor
            .bump()
            .ok_or_else(|| cursor.error("truncated \\u escape"))?;
        let digit = ch
            .to_digit(16)
            .ok_or_else(|| cursor.error("invalid hex digit in \\u escape"))?;
        value = value * 16 + digit;
    }
    Ok(value)
}

fn parse_number(cursor: &mut Cursor<'_>) -> ParseResult<Json> {
    let start = cursor.position();
    cursor.eat('-');
    // Integer part.
    match cursor.peek() {
        Some('0') => {
            cursor.bump();
        }
        Some(c) if c.is_ascii_digit() => {
            cursor.skip_while(|c| c.is_ascii_digit());
        }
        _ => return Err(cursor.error("invalid number")),
    }
    // Fraction.
    if cursor.eat('.') {
        if !cursor.peek().is_some_and(|c| c.is_ascii_digit()) {
            return Err(cursor.error("expected digit after decimal point"));
        }
        cursor.skip_while(|c| c.is_ascii_digit());
    }
    // Exponent.
    if cursor.peek().is_some_and(|c| c == 'e' || c == 'E') {
        cursor.bump();
        if cursor.peek().is_some_and(|c| c == '+' || c == '-') {
            cursor.bump();
        }
        if !cursor.peek().is_some_and(|c| c.is_ascii_digit()) {
            return Err(cursor.error("expected digit in exponent"));
        }
        cursor.skip_while(|c| c.is_ascii_digit());
    }
    let text = cursor.slice_from(start);
    text.parse::<f64>()
        .map(Json::Number)
        .map_err(|_| ParseError::new(start, format!("invalid number `{text}`")))
}

// --- writing ---------------------------------------------------------------

fn write_compact(value: &Json, out: &mut String) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(true) => out.push_str("true"),
        Json::Bool(false) => out.push_str("false"),
        Json::Number(n) => out.push_str(&format_number(*n)),
        Json::String(s) => write_json_string(s, out),
        Json::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_compact(item, out);
            }
            out.push(']');
        }
        Json::Object(pairs) => {
            out.push('{');
            for (index, (key, item)) in pairs.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_json_string(key, out);
                out.push(':');
                write_compact(item, out);
            }
            out.push('}');
        }
    }
}

fn write_pretty(value: &Json, indent: usize, out: &mut String) {
    match value {
        Json::Array(items) if !items.is_empty() => {
            out.push_str("[\n");
            for (index, item) in items.iter().enumerate() {
                push_indent(indent + 1, out);
                write_pretty(item, indent + 1, out);
                if index + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(indent, out);
            out.push(']');
        }
        Json::Object(pairs) if !pairs.is_empty() => {
            out.push_str("{\n");
            for (index, (key, item)) in pairs.iter().enumerate() {
                push_indent(indent + 1, out);
                write_json_string(key, out);
                out.push_str(": ");
                write_pretty(item, indent + 1, out);
                if index + 1 < pairs.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            push_indent(indent, out);
            out.push('}');
        }
        other => write_compact(other, out),
    }
}

fn push_indent(level: usize, out: &mut String) {
    for _ in 0..level {
        out.push_str("  ");
    }
}

/// Format a number so integral floats keep a visible `.0`.
pub fn format_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 1e16 {
        format!("{n:.1}")
    } else {
        format!("{n}")
    }
}

fn write_json_string(value: &str, out: &mut String) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000c}' => out.push_str("\\f"),
            ch if (ch as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_scalars() {
        assert_eq!(Json::parse("null").unwrap(), Json::Null);
        assert_eq!(Json::parse("true").unwrap(), Json::Bool(true));
        assert_eq!(Json::parse(" -12.5e2 ").unwrap(), Json::Number(-1250.0));
        assert_eq!(
            Json::parse(r#""a\nb""#).unwrap(),
            Json::String("a\nb".into())
        );
    }

    #[test]
    fn parses_nested_and_preserves_key_order() {
        let value = Json::parse(r#"{"b":1,"a":[true,null,"x"]}"#).unwrap();
        let pairs = value.as_object().unwrap();
        assert_eq!(pairs[0].0, "b");
        assert_eq!(pairs[1].0, "a");
        assert_eq!(value.get("a").unwrap().as_array().unwrap().len(), 3);
    }

    #[test]
    fn writes_integral_floats_with_dot_zero() {
        let value = Json::Object(vec![("port".into(), Json::Number(8080.0))]);
        assert_eq!(value.to_string(), r#"{"port":8080.0}"#);
    }

    #[test]
    fn round_trips() {
        let src = r#"{"a":[1,2,{"b":"c"}],"d":null,"e":true}"#;
        let value = Json::parse(src).unwrap();
        assert_eq!(Json::parse(&value.to_string()).unwrap(), value);
    }

    #[test]
    fn handles_surrogate_pairs() {
        let value = Json::parse(r#""\uD83D\uDE00""#).unwrap();
        assert_eq!(value, Json::String("😀".into()));
    }

    #[test]
    fn reports_errors_with_position() {
        let err = Json::parse(r#"{"a":}"#).unwrap_err();
        assert!(err.offset > 0);
        assert!(Json::parse("[1,2").is_err());
        assert!(Json::parse("{} extra").is_err());
    }
}
