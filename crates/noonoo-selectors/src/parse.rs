//! Selector parsing from the CSS token stream.

use super::*;
use noonoo_css::{Token, TokenKind, tokenize};
use noonoo_parse::{ParseError, ParseResult};
// --- parsing ---------------------------------------------------------------

impl SelectorList {
    /// Parse a comma-separated selector list.
    pub fn parse(source: &str) -> ParseResult<SelectorList> {
        let tokens = tokenize(source);
        let mut parser = SelectorParser {
            tokens,
            source,
            pos: 0,
        };
        let list = parser.parse_list()?;
        parser.skip_trivia();
        if !parser.at_end() {
            return Err(parser.error("unexpected trailing tokens in selector"));
        }
        Ok(list)
    }
}

struct SelectorParser<'a> {
    tokens: Vec<Token>,
    source: &'a str,
    pos: usize,
}

impl<'a> SelectorParser<'a> {
    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn peek(&self) -> Option<&TokenKind> {
        self.tokens.get(self.pos).map(|t| &t.kind)
    }

    fn error(&self, message: impl Into<String>) -> ParseError {
        let offset = self
            .tokens
            .get(self.pos)
            .map(|t| t.start)
            .unwrap_or(self.source.len());
        ParseError::new(offset, message)
    }

    fn skip_trivia(&mut self) {
        while matches!(
            self.peek(),
            Some(TokenKind::Whitespace) | Some(TokenKind::Comment)
        ) {
            self.pos += 1;
        }
    }

    /// Skip trivia, reporting whether any was skipped.
    fn skip_ws(&mut self) -> bool {
        let start = self.pos;
        self.skip_trivia();
        self.pos > start
    }

    fn parse_list(&mut self) -> ParseResult<SelectorList> {
        let mut selectors = Vec::new();
        self.skip_trivia();
        loop {
            selectors.push(self.parse_complex()?);
            self.skip_trivia();
            if matches!(self.peek(), Some(TokenKind::Comma)) {
                self.pos += 1;
                self.skip_trivia();
                continue;
            }
            break;
        }
        Ok(SelectorList { selectors })
    }

    fn parse_complex(&mut self) -> ParseResult<ComplexSelector> {
        let mut compounds = vec![self.parse_compound()?];
        let mut combinators = Vec::new();
        loop {
            let had_ws = self.skip_ws();
            match self.peek() {
                None | Some(TokenKind::Comma) | Some(TokenKind::CloseParen) => break,
                Some(TokenKind::Delim('>')) => {
                    self.pos += 1;
                    self.skip_ws();
                    combinators.push(Combinator::Child);
                }
                Some(TokenKind::Delim('+')) => {
                    self.pos += 1;
                    self.skip_ws();
                    combinators.push(Combinator::AdjacentSibling);
                }
                Some(TokenKind::Delim('~')) => {
                    self.pos += 1;
                    self.skip_ws();
                    combinators.push(Combinator::GeneralSibling);
                }
                Some(_) if had_ws => combinators.push(Combinator::Descendant),
                Some(_) => {
                    return Err(self.error("expected a combinator"));
                }
            }
            compounds.push(self.parse_compound()?);
        }
        Ok(ComplexSelector {
            compounds,
            combinators,
        })
    }

    fn parse_compound(&mut self) -> ParseResult<CompoundSelector> {
        let mut compound = CompoundSelector::default();
        let mut consumed = false;

        match self.peek() {
            Some(TokenKind::Ident(name)) => {
                compound.type_selector = TypeSelector::Named(name.to_ascii_lowercase());
                self.pos += 1;
                consumed = true;
            }
            Some(TokenKind::Delim('*')) => {
                self.pos += 1;
                consumed = true;
            }
            _ => {}
        }

        loop {
            match self.peek() {
                Some(TokenKind::Delim('.')) => {
                    self.pos += 1;
                    match self.peek() {
                        Some(TokenKind::Ident(name)) => {
                            compound.simples.push(SimpleSelector::Class(name.clone()));
                            self.pos += 1;
                        }
                        _ => return Err(self.error("expected a class name after `.`")),
                    }
                }
                Some(TokenKind::Hash { value, id: true }) => {
                    compound.simples.push(SimpleSelector::Id(value.clone()));
                    self.pos += 1;
                }
                Some(TokenKind::OpenBracket) => {
                    let attr = self.parse_attribute()?;
                    compound.simples.push(SimpleSelector::Attribute(attr));
                }
                Some(TokenKind::Colon) => {
                    self.pos += 1;
                    if matches!(self.peek(), Some(TokenKind::Colon)) {
                        self.pos += 1;
                        compound.pseudo_element = Some(self.parse_ident_name()?);
                    } else if let Some(TokenKind::Ident(name)) = self.peek()
                        && matches!(
                            name.to_ascii_lowercase().as_str(),
                            "before" | "after" | "first-line" | "first-letter"
                        )
                    {
                        // CSS2's single-colon spelling of these pseudo-elements.
                        compound.pseudo_element = Some(self.parse_ident_name()?);
                    } else {
                        let pseudo = self.parse_pseudo_class()?;
                        compound.simples.push(SimpleSelector::PseudoClass(pseudo));
                    }
                }
                _ => break,
            }
            consumed = true;
        }

        if !consumed {
            return Err(self.error("expected a selector"));
        }
        Ok(compound)
    }

    fn parse_ident_name(&mut self) -> ParseResult<String> {
        match self.peek() {
            Some(TokenKind::Ident(name)) => {
                let name = name.to_ascii_lowercase();
                self.pos += 1;
                Ok(name)
            }
            _ => Err(self.error("expected an identifier")),
        }
    }

    fn parse_attribute(&mut self) -> ParseResult<AttrSelector> {
        self.pos += 1; // '['
        self.skip_trivia();
        // As written: an HTML element matches it ignoring case, a foreign
        // one exactly.
        let name = match self.peek() {
            Some(TokenKind::Ident(name)) => name.clone(),
            _ => return Err(self.error("expected an identifier")),
        };
        self.pos += 1;
        self.skip_trivia();

        let (op, value) = if matches!(self.peek(), Some(TokenKind::CloseBracket)) {
            (AttrOp::Exists, None)
        } else {
            let op = self.parse_attr_op()?;
            self.skip_trivia();
            let value = match self.peek() {
                Some(TokenKind::Str(value)) | Some(TokenKind::Ident(value)) => value.clone(),
                _ => return Err(self.error("expected an attribute value")),
            };
            self.pos += 1;
            self.skip_trivia();
            (op, Some(value))
        };

        if !matches!(self.peek(), Some(TokenKind::CloseBracket)) {
            return Err(self.error("expected `]` to close an attribute selector"));
        }
        self.pos += 1;
        Ok(AttrSelector { name, op, value })
    }

    fn parse_attr_op(&mut self) -> ParseResult<AttrOp> {
        let first = match self.peek() {
            Some(TokenKind::Delim(ch)) => *ch,
            _ => return Err(self.error("expected an attribute operator")),
        };
        match first {
            '=' => {
                self.pos += 1;
                Ok(AttrOp::Equals)
            }
            '~' => self.consume_eq().map(|()| AttrOp::Includes),
            '|' => self.consume_eq().map(|()| AttrOp::DashMatch),
            '^' => self.consume_eq().map(|()| AttrOp::Prefix),
            '$' => self.consume_eq().map(|()| AttrOp::Suffix),
            '*' => self.consume_eq().map(|()| AttrOp::Substring),
            _ => Err(self.error("unknown attribute operator")),
        }
    }

    fn consume_eq(&mut self) -> ParseResult<()> {
        self.pos += 1;
        if matches!(self.peek(), Some(TokenKind::Delim('='))) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error("expected `=` in attribute operator"))
        }
    }

    fn parse_pseudo_class(&mut self) -> ParseResult<PseudoClass> {
        match self.peek().cloned() {
            Some(TokenKind::Ident(name)) => {
                self.pos += 1;
                Ok(match name.to_ascii_lowercase().as_str() {
                    "first-child" => PseudoClass::FirstChild,
                    "last-child" => PseudoClass::LastChild,
                    "only-child" => PseudoClass::OnlyChild,
                    "root" => PseudoClass::Root,
                    "hover" => PseudoClass::Hover,
                    "focus" => PseudoClass::Focus,
                    "active" => PseudoClass::Active,
                    "disabled" => PseudoClass::Disabled,
                    "link" => PseudoClass::Link,
                    other => PseudoClass::Unknown(other.to_owned()),
                })
            }
            Some(TokenKind::Function(name)) => {
                self.pos += 1;
                let args = self.read_function_args()?;
                match name.to_ascii_lowercase().as_str() {
                    "not" => Ok(PseudoClass::Not(Box::new(SelectorList::parse(&args)?))),
                    "is" | "where" => Ok(PseudoClass::Is(Box::new(SelectorList::parse(&args)?))),
                    "nth-child" => parse_nth(&args)
                        .map(PseudoClass::NthChild)
                        .ok_or_else(|| self.error(format!("invalid nth expression `{args}`"))),
                    other => Err(self.error(format!("unsupported pseudo-class `:{other}()`"))),
                }
            }
            _ => Err(self.error("expected a pseudo-class")),
        }
    }

    /// Read the raw text of a function's arguments and consume the `)`.
    fn read_function_args(&mut self) -> ParseResult<String> {
        let start = self
            .tokens
            .get(self.pos)
            .map(|t| t.start)
            .ok_or_else(|| self.error("unterminated function"))?;
        let mut depth = 1usize;
        while let Some(token) = self.tokens.get(self.pos) {
            match token.kind {
                TokenKind::OpenParen => depth += 1,
                TokenKind::CloseParen => {
                    depth -= 1;
                    if depth == 0 {
                        let args = self.source[start..token.start].trim().to_owned();
                        self.pos += 1;
                        return Ok(args);
                    }
                }
                _ => {}
            }
            self.pos += 1;
        }
        Err(self.error("unterminated function"))
    }
}

/// Parse an `an+b` nth expression (`odd`, `even`, `3`, `2n+1`, `-n+3`).
fn parse_nth(input: &str) -> Option<Nth> {
    let text = input.trim().to_ascii_lowercase().replace(' ', "");
    match text.as_str() {
        "odd" => return Some(Nth { a: 2, b: 1 }),
        "even" => return Some(Nth { a: 2, b: 0 }),
        "" => return None,
        _ => {}
    }
    if let Some((a_part, b_part)) = text.split_once('n') {
        let a = match a_part {
            "" | "+" => 1,
            "-" => -1,
            other => other.parse().ok()?,
        };
        let b = match b_part {
            "" => 0,
            other => other.parse().ok()?,
        };
        Some(Nth { a, b })
    } else {
        Some(Nth {
            a: 0,
            b: text.parse().ok()?,
        })
    }
}
