//! # noonoo-css
//!
//! The CSS layer: a tokenizer following CSS Syntax Level 3, a top-level rule
//! splitter, and the **selector** model — parsing, specificity, and matching.
//!
//! Scope note: this owns *syntax* and *selector matching*. It does not yet own
//! the cascade (declaration merging, inheritance, computed values), which
//! `noonoo-style` builds on top — see `docs/DEPENDENCY-EXIT.md` (Phase A).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use noonoo_parse::ParseResult;

pub mod selector;

pub use selector::{
    AttrOp, AttrSelector, Combinator, ComplexSelector, CompoundSelector, Element, MatchContext,
    Nth, PseudoClass, SelectorList, SimpleSelector, Specificity, TypeSelector,
};

/// A CSS token kind (CSS Syntax Level 3, abridged).
#[derive(Clone, PartialEq, Debug)]
pub enum TokenKind {
    /// An identifier, e.g. `color`.
    Ident(String),
    /// An identifier immediately followed by `(`, e.g. `rgb(`.
    Function(String),
    /// `@name`.
    AtKeyword(String),
    /// `#value`, with whether it would start an identifier.
    Hash {
        /// The hash text without `#`.
        value: String,
        /// Whether the text is identifier-like (a CSS id would be).
        id: bool,
    },
    /// A quoted string (quotes removed, escapes resolved).
    Str(String),
    /// A string containing an unescaped newline.
    BadStr,
    /// A `url(…)` value (already unquoted).
    Url(String),
    /// A malformed `url(…)`.
    BadUrl,
    /// A single punctuation character otherwise unclassified.
    Delim(char),
    /// A number.
    Number(f64),
    /// A percentage (`50%` → `50.0`).
    Percentage(f64),
    /// A dimension, e.g. `10px`.
    Dimension {
        /// The numeric part.
        value: f64,
        /// The unit.
        unit: String,
    },
    /// One or more whitespace characters.
    Whitespace,
    /// A `/* … */` comment.
    Comment,
    /// `:`.
    Colon,
    /// `;`.
    Semicolon,
    /// `,`.
    Comma,
    /// `{`.
    OpenBrace,
    /// `}`.
    CloseBrace,
    /// `(`.
    OpenParen,
    /// `)`.
    CloseParen,
    /// `[`.
    OpenBracket,
    /// `]`.
    CloseBracket,
    /// `<!--`.
    Cdo,
    /// `-->`.
    Cdc,
}

/// A token with its byte span in the source.
#[derive(Clone, PartialEq, Debug)]
pub struct Token {
    /// What kind of token.
    pub kind: TokenKind,
    /// Start byte offset.
    pub start: usize,
    /// End byte offset.
    pub end: usize,
}

impl Token {
    /// The token's raw source text.
    pub fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }
}

/// Tokenize a CSS source into a token stream (whitespace and comments included).
pub fn tokenize(source: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut pos = 0;
    while pos < source.len() {
        let start = pos;
        let ch = source[pos..].chars().next().unwrap();

        // CDO / CDC.
        if source[pos..].starts_with("<!--") {
            pos += 4;
            out.push(tok(TokenKind::Cdo, start, pos));
            continue;
        }
        if source[pos..].starts_with("-->") {
            pos += 3;
            out.push(tok(TokenKind::Cdc, start, pos));
            continue;
        }

        // Whitespace.
        if ch.is_whitespace() {
            while pos < source.len() && source[pos..].chars().next().unwrap().is_whitespace() {
                pos += source[pos..].chars().next().unwrap().len_utf8();
            }
            out.push(tok(TokenKind::Whitespace, start, pos));
            continue;
        }

        // Comment.
        if source[pos..].starts_with("/*") {
            pos += 2;
            while pos < source.len() && !source[pos..].starts_with("*/") {
                pos += source[pos..].chars().next().unwrap().len_utf8();
            }
            pos = (pos + 2).min(source.len());
            out.push(tok(TokenKind::Comment, start, pos));
            continue;
        }

        // Strings.
        if ch == '"' || ch == '\'' {
            let (value, end, bad) = consume_string(source, pos, ch);
            pos = end;
            out.push(tok(
                if bad {
                    TokenKind::BadStr
                } else {
                    TokenKind::Str(value)
                },
                start,
                pos,
            ));
            continue;
        }

        // Hash.
        if ch == '#' {
            pos += 1;
            if pos < source.len() && is_ident_char(source[pos..].chars().next().unwrap()) {
                let ident_start = pos;
                let (value, end) = consume_ident(source, pos);
                pos = end;
                out.push(tok(
                    TokenKind::Hash {
                        value,
                        id: would_start_ident(&source[ident_start..]),
                    },
                    start,
                    pos,
                ));
                continue;
            }
            out.push(tok(TokenKind::Delim('#'), start, pos));
            continue;
        }

        // At-keyword.
        if ch == '@' && would_start_ident(&source[pos + 1..]) {
            pos += 1;
            let (name, end) = consume_ident(source, pos);
            pos = end;
            out.push(tok(TokenKind::AtKeyword(name), start, pos));
            continue;
        }

        // Numbers (and dimensions/percentages).
        if is_number_start(source, pos) {
            let (value, end) = consume_number(source, pos);
            pos = end;
            if pos < source.len() && source[pos..].starts_with('%') {
                pos += 1;
                out.push(tok(TokenKind::Percentage(value), start, pos));
            } else if pos < source.len() && would_start_ident(&source[pos..]) {
                let (unit, uend) = consume_ident(source, pos);
                pos = uend;
                out.push(tok(TokenKind::Dimension { value, unit }, start, pos));
            } else {
                out.push(tok(TokenKind::Number(value), start, pos));
            }
            continue;
        }

        // Identifiers / functions / url().
        if would_start_ident(&source[pos..]) {
            let (name, end) = consume_ident(source, pos);
            pos = end;
            if source[pos..].starts_with('(') {
                pos += 1;
                if name.eq_ignore_ascii_case("url") {
                    let (url, end, bad) = consume_url(source, pos);
                    pos = end;
                    out.push(tok(
                        if bad {
                            TokenKind::BadUrl
                        } else {
                            TokenKind::Url(url)
                        },
                        start,
                        pos,
                    ));
                } else {
                    out.push(tok(TokenKind::Function(name), start, pos));
                }
            } else {
                out.push(tok(TokenKind::Ident(name), start, pos));
            }
            continue;
        }

        // Single-character punctuation.
        let byte = bytes[pos];
        let simple = match byte {
            b':' => Some(TokenKind::Colon),
            b';' => Some(TokenKind::Semicolon),
            b',' => Some(TokenKind::Comma),
            b'{' => Some(TokenKind::OpenBrace),
            b'}' => Some(TokenKind::CloseBrace),
            b'(' => Some(TokenKind::OpenParen),
            b')' => Some(TokenKind::CloseParen),
            b'[' => Some(TokenKind::OpenBracket),
            b']' => Some(TokenKind::CloseBracket),
            _ => None,
        };
        pos += 1;
        match simple {
            Some(kind) => out.push(tok(kind, start, pos)),
            None => out.push(tok(TokenKind::Delim(ch), start, pos)),
        }
    }
    out
}

fn tok(kind: TokenKind, start: usize, end: usize) -> Token {
    Token { kind, start, end }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || (ch as u32) >= 0x80
}

fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || (ch as u32) >= 0x80
}

fn would_start_ident(rest: &str) -> bool {
    let mut chars = rest.chars();
    match chars.next() {
        Some(ch) if is_ident_start(ch) => true,
        Some('-') => matches!(chars.next(), Some(c) if is_ident_start(c) || c == '-'),
        _ => false,
    }
}

fn is_number_start(source: &str, pos: usize) -> bool {
    let rest = &source[pos..];
    let mut chars = rest.chars().peekable();
    match chars.next() {
        Some(c) if c.is_ascii_digit() => true,
        Some('.') => matches!(chars.next(), Some(c) if c.is_ascii_digit()),
        Some('+') | Some('-') => match chars.next() {
            Some(c) if c.is_ascii_digit() => true,
            Some('.') => matches!(chars.next(), Some(c) if c.is_ascii_digit()),
            _ => false,
        },
        _ => false,
    }
}

fn consume_ident(source: &str, mut pos: usize) -> (String, usize) {
    let start = pos;
    while pos < source.len() {
        let ch = source[pos..].chars().next().unwrap();
        if is_ident_char(ch) {
            pos += ch.len_utf8();
        } else if ch == '\\' && pos + 1 < source.len() {
            pos += ch.len_utf8();
            pos += source[pos..].chars().next().unwrap().len_utf8();
        } else {
            break;
        }
    }
    // Re-read for escape handling.
    let raw = &source[start..pos];
    (unescape_ident(raw), pos)
}

fn unescape_ident(raw: &str) -> String {
    if !raw.contains('\\') {
        return raw.to_owned();
    }
    let mut out = String::new();
    let mut chars = raw.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn consume_number(source: &str, mut pos: usize) -> (f64, usize) {
    let start = pos;
    if let Some(ch) = source[pos..].chars().next()
        && (ch == '+' || ch == '-')
    {
        pos += 1;
    }
    while pos < source.len() && source[pos..].chars().next().unwrap().is_ascii_digit() {
        pos += 1;
    }
    if pos < source.len() && source[pos..].starts_with('.') {
        pos += 1;
        while pos < source.len() && source[pos..].chars().next().unwrap().is_ascii_digit() {
            pos += 1;
        }
    }
    if pos < source.len() {
        let ch = source[pos..].chars().next().unwrap();
        if ch == 'e' || ch == 'E' {
            let mut probe = pos + 1;
            if probe < source.len()
                && matches!(source[probe..].chars().next(), Some('+') | Some('-'))
            {
                probe += 1;
            }
            if probe < source.len() && source[probe..].chars().next().unwrap().is_ascii_digit() {
                pos = probe;
                while pos < source.len() && source[pos..].chars().next().unwrap().is_ascii_digit() {
                    pos += 1;
                }
            }
        }
    }
    let value = source[start..pos].parse().unwrap_or(0.0);
    (value, pos)
}

fn consume_string(source: &str, mut pos: usize, quote: char) -> (String, usize, bool) {
    pos += quote.len_utf8();
    let mut value = String::new();
    let mut bad = false;
    while pos < source.len() {
        let ch = source[pos..].chars().next().unwrap();
        if ch == quote {
            pos += ch.len_utf8();
            return (value, pos, bad);
        }
        if ch == '\n' {
            bad = true;
            return (value, pos, bad);
        }
        if ch == '\\' {
            pos += 1;
            if pos < source.len() {
                let esc = source[pos..].chars().next().unwrap();
                value.push(esc);
                pos += esc.len_utf8();
            }
            continue;
        }
        value.push(ch);
        pos += ch.len_utf8();
    }
    (value, pos, true)
}

fn consume_url(source: &str, mut pos: usize) -> (String, usize, bool) {
    // Skip leading whitespace.
    while pos < source.len() && source[pos..].chars().next().unwrap().is_whitespace() {
        pos += 1;
    }
    let quote = source[pos..].chars().next();
    if matches!(quote, Some('"') | Some('\'')) {
        let q = quote.unwrap();
        let (value, end, bad) = consume_string(source, pos, q);
        pos = end;
        while pos < source.len() && source[pos..].chars().next().unwrap().is_whitespace() {
            pos += 1;
        }
        if pos < source.len() && source[pos..].starts_with(')') {
            pos += 1;
            return (value, pos, bad);
        }
        return (value, pos, true);
    }
    let start = pos;
    while pos < source.len() && !source[pos..].starts_with(')') {
        if source[pos..].chars().next().unwrap().is_whitespace() {
            break;
        }
        pos += source[pos..].chars().next().unwrap().len_utf8();
    }
    let value = source[start..pos].to_owned();
    while pos < source.len() && source[pos..].chars().next().unwrap().is_whitespace() {
        pos += 1;
    }
    if pos < source.len() && source[pos..].starts_with(')') {
        pos += 1;
        (value, pos, false)
    } else {
        (value, pos, true)
    }
}

// --- stylesheet ------------------------------------------------------------

/// A single CSS declaration.
#[derive(Clone, PartialEq, Debug)]
pub struct Declaration {
    /// The property name.
    pub name: String,
    /// The value, verbatim.
    pub value: String,
    /// Whether the value ended with `!important`.
    pub important: bool,
}

/// A stylesheet rule.
#[derive(Clone, PartialEq, Debug)]
pub enum Rule {
    /// A qualified rule: a selector prelude and its declarations.
    Qualified {
        /// The selector text.
        selector: String,
        /// The declarations in the block.
        declarations: Vec<Declaration>,
    },
    /// An at-rule, e.g. `@media … { … }`. The block is captured verbatim.
    At {
        /// The at-keyword name (without `@`).
        name: String,
        /// The prelude before `{` or `;`.
        prelude: String,
        /// The raw block text, if the at-rule had one.
        block: Option<String>,
    },
}

/// A parsed stylesheet: a flat list of top-level rules.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct Stylesheet {
    /// The top-level rules.
    pub rules: Vec<Rule>,
}

impl Stylesheet {
    /// Parse a stylesheet.
    pub fn parse(source: &str) -> ParseResult<Stylesheet> {
        let tokens = tokenize(source);
        let mut rules = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            if is_trivia(&tokens[i].kind) {
                i += 1;
                continue;
            }

            // Collect the prelude up to '{', ';', or end.
            let prelude_start = i;
            while i < tokens.len()
                && !matches!(tokens[i].kind, TokenKind::OpenBrace | TokenKind::Semicolon)
            {
                i += 1;
            }
            let prelude = text_range(source, &tokens[prelude_start..i])
                .trim()
                .to_owned();

            if i < tokens.len() && matches!(tokens[i].kind, TokenKind::Semicolon) {
                i += 1;
                if prelude.starts_with('@') {
                    let (name, rest) = split_at_rule(&prelude);
                    rules.push(Rule::At {
                        name,
                        prelude: rest,
                        block: None,
                    });
                }
                continue;
            }

            if i >= tokens.len() {
                break;
            }

            // tokens[i] is OpenBrace: find the matching close.
            let open = i;
            let mut depth = 0usize;
            let mut close = i;
            while i < tokens.len() {
                match tokens[i].kind {
                    TokenKind::OpenBrace => depth += 1,
                    TokenKind::CloseBrace => {
                        depth -= 1;
                        if depth == 0 {
                            close = i;
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            if depth != 0 {
                return Err(noonoo_parse::ParseError::new(
                    tokens[open].start,
                    "Unclosed block: expected `}`",
                ));
            }

            if prelude.starts_with('@') {
                let (name, rest) = split_at_rule(&prelude);
                let block = text_range(source, &tokens[open + 1..close]).to_owned();
                rules.push(Rule::At {
                    name,
                    prelude: rest,
                    block: Some(block),
                });
            } else {
                let declarations = parse_declarations(source, &tokens[open + 1..close]);
                rules.push(Rule::Qualified {
                    selector: prelude,
                    declarations,
                });
            }
            i = close + 1;
        }
        Ok(Stylesheet { rules })
    }
}

fn split_at_rule(prelude: &str) -> (String, String) {
    let name = prelude.trim_start_matches('@');
    let name_end = name.find(|c: char| c.is_whitespace()).unwrap_or(name.len());
    (
        name[..name_end].to_owned(),
        name[name_end..].trim().to_owned(),
    )
}

fn parse_declarations(source: &str, tokens: &[Token]) -> Vec<Declaration> {
    let mut declarations = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        while i < tokens.len()
            && matches!(
                tokens[i].kind,
                TokenKind::Semicolon | TokenKind::Whitespace | TokenKind::Comment
            )
        {
            i += 1;
        }
        let start = i;
        while i < tokens.len() && !matches!(tokens[i].kind, TokenKind::Semicolon) {
            i += 1;
        }
        let segment = &tokens[start..i];
        if segment.is_empty() {
            continue;
        }
        let Some(colon) = segment
            .iter()
            .position(|t| matches!(t.kind, TokenKind::Colon))
        else {
            continue;
        };
        let name = text_range(source, &segment[..colon]).trim().to_owned();
        let mut value = text_range(source, &segment[colon + 1..]).trim().to_owned();
        let mut important = false;
        if let Some(idx) = value.rfind('!')
            && value[idx + 1..].trim().eq_ignore_ascii_case("important")
        {
            important = true;
            value = value[..idx].trim().to_owned();
        }
        if !name.is_empty() {
            declarations.push(Declaration {
                name,
                value,
                important,
            });
        }
    }
    declarations
}

fn text_range<'a>(source: &'a str, tokens: &[Token]) -> &'a str {
    match (tokens.first(), tokens.last()) {
        (Some(first), Some(last)) => &source[first.start..last.end],
        _ => "",
    }
}

fn is_trivia(kind: &TokenKind) -> bool {
    matches!(kind, TokenKind::Whitespace | TokenKind::Comment)
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizes_a_declaration() {
        let tokens = tokenize("color: red;");
        let kinds: Vec<_> = tokens.iter().map(|t| t.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                TokenKind::Ident("color".into()),
                TokenKind::Colon,
                TokenKind::Whitespace,
                TokenKind::Ident("red".into()),
                TokenKind::Semicolon,
            ]
        );
    }

    #[test]
    fn tokenizes_numbers_dimensions_and_hash() {
        let tokens = tokenize("10px 50% #fff rgb(1,2,3)");
        assert!(matches!(tokens[0].kind, TokenKind::Dimension { value, .. } if value == 10.0));
        assert!(
            tokens
                .iter()
                .any(|t| matches!(&t.kind, TokenKind::Percentage(v) if *v == 50.0))
        );
        assert!(
            tokens
                .iter()
                .any(|t| matches!(&t.kind, TokenKind::Hash { value, id: true } if value == "fff"))
        );
        assert!(
            tokens
                .iter()
                .any(|t| matches!(&t.kind, TokenKind::Function(n) if n == "rgb"))
        );
    }

    #[test]
    fn parses_qualified_rule() {
        let sheet = Stylesheet::parse("body { color: red; margin: 0 auto }").unwrap();
        assert_eq!(sheet.rules.len(), 1);
        let Rule::Qualified {
            selector,
            declarations,
        } = &sheet.rules[0]
        else {
            panic!("expected qualified rule");
        };
        assert_eq!(selector, "body");
        assert_eq!(declarations.len(), 2);
        assert_eq!(declarations[0].name, "color");
        assert_eq!(declarations[0].value, "red");
        assert_eq!(declarations[1].value, "0 auto");
    }

    #[test]
    fn parses_at_rule_with_block() {
        let sheet = Stylesheet::parse("@media screen { a { color: blue } }").unwrap();
        let Rule::At {
            name,
            prelude,
            block,
        } = &sheet.rules[0]
        else {
            panic!("expected at-rule");
        };
        assert_eq!(name, "media");
        assert_eq!(prelude, "screen");
        assert!(block.as_deref().unwrap().contains("color: blue"));
    }

    #[test]
    fn detects_important() {
        let sheet = Stylesheet::parse("a { color: red !important }").unwrap();
        let Rule::Qualified { declarations, .. } = &sheet.rules[0] else {
            panic!()
        };
        assert!(declarations[0].important);
        assert_eq!(declarations[0].value, "red");
    }

    #[test]
    fn unclosed_block_errors() {
        assert!(Stylesheet::parse("a { color: red").is_err());
    }
}
