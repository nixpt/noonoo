//! CSS escapes in strings and identifiers (CSS Syntax §4.3.7).

use noonoo_css::{TokenKind, tokenize};

fn kinds(source: &str) -> Vec<TokenKind> {
    tokenize(source).into_iter().map(|t| t.kind).collect()
}

#[test]
fn hex_escapes_in_strings_decode() {
    // Up to six hex digits; one whitespace after them is part of the escape.
    assert_eq!(
        kinds(r#""\201C x\"\41 B\00004a""#),
        [TokenKind::Str("\u{201C}x\"ABJ".into())]
    );
}

#[test]
fn invalid_code_points_become_the_replacement_character() {
    assert_eq!(
        kinds(r#""\0|\D800|\110000""#),
        [TokenKind::Str("\u{FFFD}|\u{FFFD}|\u{FFFD}".into())]
    );
}

#[test]
fn an_escaped_newline_in_a_string_continues_the_line() {
    assert_eq!(kinds("\"a\\\nb\""), [TokenKind::Str("ab".into())]);
}

#[test]
fn hex_escapes_in_identifiers_decode() {
    assert_eq!(kinds(r"\31 0px"), [TokenKind::Ident("10px".into())]);
    assert_eq!(kinds(r"a\:b"), [TokenKind::Ident("a:b".into())]);
}
