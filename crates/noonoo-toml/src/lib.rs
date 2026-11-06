//! # noonoo-toml
//!
//! A dependency-free TOML parser producing a [`noonoo_json::Json`] value — a
//! *data* format (`docs/FORMATS.md`): configuration, not markup, so it parses to
//! a value model and sits beside `noonoo-ini` and `noonoo-caison`.
//!
//! Supported: comments, bare and quoted keys, dotted keys, basic (`"…"`, with
//! escapes) and literal (`'…'`) strings, integers and floats, booleans, arrays
//! (including multi-line), inline tables, `[table]`, and `[[array-of-tables]]`.
//!
//! Not yet: multi-line (`"""…"""`) strings, TOML dates/times (kept as strings),
//! and `inf`/`nan`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use noonoo_json::Json;
use noonoo_parse::{Cursor, ParseError, ParseResult};

/// Parse TOML into a [`Json`] object.
pub fn to_json(source: &str) -> ParseResult<Json> {
    let mut cursor = Cursor::new(source);
    let mut root = Json::Object(Vec::new());
    let mut path: Vec<String> = Vec::new();

    loop {
        skip_trivia(&mut cursor);
        if cursor.is_eof() {
            break;
        }

        if cursor.starts_with("[[") {
            cursor.eat_str("[[");
            path = parse_key_path(&mut cursor)?;
            skip_inline_space(&mut cursor);
            if !cursor.eat_str("]]") {
                return Err(cursor.error("expected `]]` to close an array-of-tables header"));
            }
            push_array_table(&mut root, &path)?;
        } else if cursor.peek() == Some('[') {
            cursor.bump();
            path = parse_key_path(&mut cursor)?;
            skip_inline_space(&mut cursor);
            if !cursor.eat(']') {
                return Err(cursor.error("expected `]` to close a table header"));
            }
            ensure_table(&mut root, &path)?;
        } else {
            let mut keys = parse_key_path(&mut cursor)?;
            skip_inline_space(&mut cursor);
            if !cursor.eat('=') {
                return Err(cursor.error("expected `=`"));
            }
            let value = parse_value(&mut cursor)?;

            let mut full = path.clone();
            full.append(&mut keys);
            let key = full.pop().ok_or_else(|| cursor.error("empty key"))?;
            let mut target = &mut root;
            for segment in &full {
                target = descend(target, segment)?;
            }
            insert(target, &key, value)?;
        }
    }

    Ok(root)
}

// --- scanning helpers ------------------------------------------------------

fn skip_inline_space(cursor: &mut Cursor<'_>) {
    cursor.skip_while(|c| c == ' ' || c == '\t');
}

fn skip_trivia(cursor: &mut Cursor<'_>) {
    loop {
        cursor.skip_while(|c| c.is_whitespace());
        if cursor.starts_with("#") {
            cursor.skip_while(|c| c != '\n');
            continue;
        }
        break;
    }
}

/// Keys separated by `.`; stops at `]`, `=`, newline, or comment.
fn parse_key_path(cursor: &mut Cursor<'_>) -> ParseResult<Vec<String>> {
    let mut keys = vec![parse_key(cursor)?];
    loop {
        skip_inline_space(cursor);
        if cursor.eat('.') {
            skip_inline_space(cursor);
            keys.push(parse_key(cursor)?);
        } else {
            break;
        }
    }
    Ok(keys)
}

fn parse_key(cursor: &mut Cursor<'_>) -> ParseResult<String> {
    skip_inline_space(cursor);
    match cursor.peek() {
        Some('"') => parse_basic_string(cursor),
        Some('\'') => parse_literal_string(cursor),
        _ => {
            let start = cursor.position();
            cursor.skip_while(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
            let key = cursor.slice_from(start);
            if key.is_empty() {
                return Err(cursor.error("expected a key"));
            }
            Ok(key.to_owned())
        }
    }
}

// --- values ----------------------------------------------------------------

fn parse_value(cursor: &mut Cursor<'_>) -> ParseResult<Json> {
    skip_inline_space(cursor);
    match cursor.peek() {
        Some('"') => parse_basic_string(cursor).map(Json::String),
        Some('\'') => parse_literal_string(cursor).map(Json::String),
        Some('[') => parse_array(cursor),
        Some('{') => parse_inline_table(cursor),
        _ => parse_bare_value(cursor),
    }
}

fn parse_basic_string(cursor: &mut Cursor<'_>) -> ParseResult<String> {
    cursor.bump(); // opening quote
    let mut out = String::new();
    loop {
        match cursor.bump() {
            None => return Err(cursor.error("unterminated string")),
            Some('"') => return Ok(out),
            Some('\\') => match cursor.bump() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                other => {
                    if let Some(c) = other {
                        out.push(c);
                    }
                }
            },
            Some(c) => out.push(c),
        }
    }
}

fn parse_literal_string(cursor: &mut Cursor<'_>) -> ParseResult<String> {
    cursor.bump(); // opening quote
    let start = cursor.position();
    while let Some(c) = cursor.peek() {
        if c == '\'' {
            let text = cursor.slice_from(start).to_owned();
            cursor.bump();
            return Ok(text);
        }
        cursor.bump();
    }
    Err(cursor.error("unterminated literal string"))
}

fn parse_array(cursor: &mut Cursor<'_>) -> ParseResult<Json> {
    cursor.bump(); // '['
    let mut items = Vec::new();
    loop {
        skip_trivia(cursor);
        if cursor.eat(']') {
            return Ok(Json::Array(items));
        }
        items.push(parse_value(cursor)?);
        skip_trivia(cursor);
        if cursor.eat(',') {
            continue;
        }
        skip_trivia(cursor);
        if cursor.eat(']') {
            return Ok(Json::Array(items));
        }
        return Err(cursor.error("expected `,` or `]` in array"));
    }
}

fn parse_inline_table(cursor: &mut Cursor<'_>) -> ParseResult<Json> {
    cursor.bump(); // '{'
    let mut pairs: Vec<(String, Json)> = Vec::new();
    loop {
        skip_inline_space(cursor);
        if cursor.eat('}') {
            return Ok(Json::Object(pairs));
        }
        let key = parse_key(cursor)?;
        skip_inline_space(cursor);
        if !cursor.eat('=') {
            return Err(cursor.error("expected `=` in inline table"));
        }
        let value = parse_value(cursor)?;
        if pairs.iter().any(|(k, _)| k == &key) {
            return Err(cursor.error(format!("duplicate key `{key}`")));
        }
        pairs.push((key, value));
        skip_inline_space(cursor);
        if cursor.eat(',') {
            continue;
        }
        skip_inline_space(cursor);
        if cursor.eat('}') {
            return Ok(Json::Object(pairs));
        }
        return Err(cursor.error("expected `,` or `}` in inline table"));
    }
}

fn parse_bare_value(cursor: &mut Cursor<'_>) -> ParseResult<Json> {
    let start = cursor.position();
    cursor.skip_while(|c| !matches!(c, ',' | ']' | '}' | '\n' | '#'));
    let token = cursor.slice_from(start).trim();
    if token.is_empty() {
        return Err(cursor.error("expected a value"));
    }
    Ok(match token {
        "true" => Json::Bool(true),
        "false" => Json::Bool(false),
        _ => match token.parse::<f64>() {
            Ok(number) => Json::Number(number),
            // Dates, times, and anything else are kept as strings.
            Err(_) => Json::String(token.to_owned()),
        },
    })
}

// --- structural insertion --------------------------------------------------

fn descend<'a>(node: &'a mut Json, segment: &str) -> ParseResult<&'a mut Json> {
    match node {
        Json::Object(pairs) => {
            if !pairs.iter().any(|(key, _)| key == segment) {
                pairs.push((segment.to_owned(), Json::Object(Vec::new())));
            }
            Ok(&mut pairs.iter_mut().find(|(key, _)| key == segment).unwrap().1)
        }
        Json::Array(items) => {
            let last = items
                .last_mut()
                .ok_or_else(|| ParseError::new(0, "empty array in path"))?;
            descend(last, segment)
        }
        _ => Err(ParseError::new(0, format!("`{segment}` is not a table"))),
    }
}

fn ensure_table(root: &mut Json, path: &[String]) -> ParseResult<()> {
    let mut node = root;
    for segment in path {
        node = descend(node, segment)?;
    }
    Ok(())
}

fn push_array_table(root: &mut Json, path: &[String]) -> ParseResult<()> {
    let mut node = root;
    for segment in &path[..path.len().saturating_sub(1)] {
        node = descend(node, segment)?;
    }
    let name = path
        .last()
        .ok_or_else(|| ParseError::new(0, "empty array header"))?;
    match node {
        Json::Object(pairs) => {
            if !pairs.iter().any(|(key, _)| key == name) {
                pairs.push((name.clone(), Json::Array(Vec::new())));
            }
            let array = &mut pairs.iter_mut().find(|(key, _)| key == name).unwrap().1;
            match array {
                Json::Array(items) => items.push(Json::Object(Vec::new())),
                _ => return Err(ParseError::new(0, format!("`{name}` is not an array"))),
            }
        }
        _ => return Err(ParseError::new(0, "array of tables in a non-table")),
    }
    Ok(())
}

fn insert(node: &mut Json, key: &str, value: Json) -> ParseResult<()> {
    match node {
        Json::Object(pairs) => {
            if pairs.iter().any(|(k, _)| k == key) {
                return Err(ParseError::new(0, format!("duplicate key `{key}`")));
            }
            pairs.push((key.to_owned(), value));
            Ok(())
        }
        Json::Array(items) => {
            let last = items
                .last_mut()
                .ok_or_else(|| ParseError::new(0, "empty array in path"))?;
            insert(last, key, value)
        }
        _ => Err(ParseError::new(0, "insert into a non-table")),
    }
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    fn json(source: &str) -> String {
        to_json(source).unwrap().to_string()
    }

    #[test]
    fn scalars() {
        assert_eq!(json(r#"name = "noonoo""#), r#"{"name":"noonoo"}"#);
        assert_eq!(json("port = 8080"), r#"{"port":8080.0}"#);
        assert_eq!(json("ratio = 1.5"), r#"{"ratio":1.5}"#);
        assert_eq!(json("on = true"), r#"{"on":true}"#);
    }

    #[test]
    fn tables_and_dotted_keys() {
        assert_eq!(
            json("[server]\nhost = \"localhost\"\nport = 8080"),
            r#"{"server":{"host":"localhost","port":8080.0}}"#
        );
        assert_eq!(json("a.b = 1"), r#"{"a":{"b":1.0}}"#);
    }

    #[test]
    fn arrays_and_inline_tables() {
        assert_eq!(json("tags = [1, 2, 3]"), r#"{"tags":[1.0,2.0,3.0]}"#);
        assert_eq!(json("p = { x = 1, y = 2 }"), r#"{"p":{"x":1.0,"y":2.0}}"#);
    }

    #[test]
    fn array_of_tables() {
        assert_eq!(
            json("[[items]]\nname = \"a\"\n[[items]]\nname = \"b\""),
            r#"{"items":[{"name":"a"},{"name":"b"}]}"#
        );
    }

    #[test]
    fn comments_and_strings() {
        assert_eq!(json("# c\ns = \"a\\nb\" # trailing"), "{\"s\":\"a\\nb\"}");
        assert_eq!(json(r#"p = 'C:\path'"#), r#"{"p":"C:\\path"}"#);
    }
}
