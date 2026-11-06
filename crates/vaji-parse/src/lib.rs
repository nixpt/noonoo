//! # vaji-parse
//!
//! The small, shared foundation every Vaji parser stands on: a byte [`Span`], a
//! positioned [`ParseError`], and a [`Cursor`] over `&str`.
//!
//! It exists so the format crates (`vaji-json`, `vaji-css`, `vaji-caison`,
//! `vaji-html`) agree on *how* they report a position and *how* they walk input,
//! instead of each inventing its own. It has no dependencies and no opinions
//! about any one format.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use core::fmt;

/// A half-open byte range `[start, end)` into the source text.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Span {
    /// Byte offset of the first byte.
    pub start: usize,
    /// Byte offset one past the last byte.
    pub end: usize,
}

impl Span {
    /// Create a span from two byte offsets.
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The length in bytes.
    pub const fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Whether the span is empty.
    pub const fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

/// A parse failure, with the byte offset where it was detected.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ParseError {
    /// Human-readable message.
    pub message: String,
    /// Byte offset into the source.
    pub offset: usize,
}

impl ParseError {
    /// Build an error at `offset`.
    pub fn new(offset: usize, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (at byte {})", self.message, self.offset)
    }
}

impl std::error::Error for ParseError {}

/// The result type used throughout Vaji's parsers.
pub type ParseResult<T> = Result<T, ParseError>;

/// A cursor over `&str` that tracks a byte offset and never splits a `char`.
#[derive(Clone, Debug)]
pub struct Cursor<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    /// Start a cursor at the beginning of `input`.
    pub const fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }

    /// The current byte offset.
    pub const fn position(&self) -> usize {
        self.pos
    }

    /// The full input.
    pub const fn input(&self) -> &'a str {
        self.input
    }

    /// The not-yet-consumed remainder.
    pub fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    /// Whether the cursor is at the end.
    pub const fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// The next character, without consuming it.
    pub fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    /// The character `n` positions ahead, without consuming.
    pub fn peek_at(&self, n: usize) -> Option<char> {
        self.remaining().chars().nth(n)
    }

    /// Consume and return the next character.
    pub fn bump(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    /// Consume `ch` if it is next; report whether it was.
    pub fn eat(&mut self, ch: char) -> bool {
        if self.peek() == Some(ch) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Whether `s` is next.
    pub fn starts_with(&self, s: &str) -> bool {
        self.remaining().starts_with(s)
    }

    /// Consume `s` if it is next; report whether it was.
    pub fn eat_str(&mut self, s: &str) -> bool {
        if self.starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    /// Consume characters while `pred` holds.
    pub fn skip_while(&mut self, mut pred: impl FnMut(char) -> bool) {
        while let Some(ch) = self.peek() {
            if pred(ch) {
                self.bump();
            } else {
                break;
            }
        }
    }

    /// The source consumed since byte offset `start`.
    pub fn slice_from(&self, start: usize) -> &'a str {
        &self.input[start..self.pos]
    }

    /// The span from byte offset `start` to now.
    pub const fn span_from(&self, start: usize) -> Span {
        Span::new(start, self.pos)
    }

    /// Build an error at the current position.
    pub fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError::new(self.pos, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bump_and_peek_are_utf8_safe() {
        let mut c = Cursor::new("aé✓");
        assert_eq!(c.peek(), Some('a'));
        assert_eq!(c.bump(), Some('a'));
        assert_eq!(c.bump(), Some('é'));
        assert_eq!(c.position(), 3); // 'é' is two bytes
        assert_eq!(c.bump(), Some('✓'));
        assert!(c.is_eof());
        assert_eq!(c.bump(), None);
    }

    #[test]
    fn eat_str_and_skip() {
        let mut c = Cursor::new("  /*x*/rest");
        c.skip_while(|ch| ch == ' ');
        assert!(c.eat_str("/*x*/"));
        assert_eq!(c.remaining(), "rest");
        assert!(!c.eat_str("nope"));
    }
}
