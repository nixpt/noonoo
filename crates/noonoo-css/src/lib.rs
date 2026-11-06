//! # noonoo-css
//!
//! The CSS layer: a tokenizer following CSS Syntax Level 3, a top-level rule
//! splitter, and the value & property model. The **selectors** live in
//! [`noonoo_selectors`], and the cascade in [`noonoo_style`].
//!
//! Scope note: this owns *syntax* and *value/property typing*. It
//! does not yet own the cascade (merging declarations by origin, specificity and
//! order, then inheritance and computed values), which `noonoo-style` builds on
//! top — see `docs/DEPENDENCY-EXIT.md` (Phase A).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use noonoo_parse::ParseResult;

pub mod property;
pub mod tokenizer;
pub mod value;

pub use property::{Declaration, Property, parse_declaration};
pub use tokenizer::{Token, TokenKind, tokenize};
pub use value::{Color, Length, LengthUnit, Value, parse_values};

// --- stylesheet ------------------------------------------------------------

/// A single CSS declaration, as spelled in the source (unparsed).
///
/// The typed form — property + parsed values — is [`crate::property::Declaration`],
/// produced by [`crate::parse_declaration`].
#[derive(Clone, PartialEq, Debug)]
pub struct RawDeclaration {
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
        declarations: Vec<RawDeclaration>,
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
                let declarations = declarations_from_tokens(source, &tokens[open + 1..close]);
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

/// Parse a `name: value; …` declaration list (e.g. an element's `style`
/// attribute) into [`RawDeclaration`]s.
pub fn parse_declarations(source: &str) -> Vec<RawDeclaration> {
    let tokens = tokenize(source);
    declarations_from_tokens(source, &tokens)
}

fn declarations_from_tokens(source: &str, tokens: &[Token]) -> Vec<RawDeclaration> {
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
            declarations.push(RawDeclaration {
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
