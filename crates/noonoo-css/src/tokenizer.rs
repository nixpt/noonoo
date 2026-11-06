//! The CSS tokenizer (CSS Syntax Level 3, abridged).

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
