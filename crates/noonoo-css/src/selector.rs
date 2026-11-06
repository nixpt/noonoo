//! Selectors: parsing, specificity, and matching.
//!
//! This is the foundation the cascade stands on. It covers the selector subset
//! a UI-grade engine needs (see `docs/DEPENDENCY-EXIT.md` §2): type, `*`,
//! `.class`, `#id`, attributes with all the match operators, structural and
//! logical pseudo-classes, pseudo-elements, and the four combinators.
//!
//! Matching is generic over [`Element`] — a read-only view of one element — so
//! this crate stays dependency-free and the style/engine layers supply the view
//! over `noonoo-dom`.

use crate::{Token, TokenKind, tokenize};
use noonoo_parse::{ParseError, ParseResult};

/// A CSS specificity, split into its three components.
///
/// Ordering is the CSS priority: more ids wins, then more classes/attributes/
/// pseudo-classes, then more types/pseudo-elements.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default)]
pub struct Specificity {
    /// The count of id selectors.
    pub ids: u32,
    /// The count of class, attribute, and pseudo-class selectors.
    pub classes: u32,
    /// The count of type and pseudo-element selectors.
    pub types: u32,
}

impl Specificity {
    /// The zero specificity.
    pub const ZERO: Specificity = Specificity {
        ids: 0,
        classes: 0,
        types: 0,
    };

    /// Pack into a single integer (`a<<20 | b<<10 | c`).
    pub fn to_u32(self) -> u32 {
        (self.ids.min(0x3ff) << 20) | (self.classes.min(0x3ff) << 10) | self.types.min(0x3ff)
    }
}

impl core::ops::Add for Specificity {
    type Output = Specificity;

    fn add(self, rhs: Self) -> Self::Output {
        Specificity {
            ids: self.ids + rhs.ids,
            classes: self.classes + rhs.classes,
            types: self.types + rhs.types,
        }
    }
}

/// A combinator between two compound selectors.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Combinator {
    /// `A B` — any ancestor.
    Descendant,
    /// `A > B` — the direct parent.
    Child,
    /// `A + B` — the immediately preceding sibling.
    AdjacentSibling,
    /// `A ~ B` — any preceding sibling.
    GeneralSibling,
}

/// The operator in an attribute selector.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AttrOp {
    /// `[a]`.
    Exists,
    /// `[a=v]`.
    Equals,
    /// `[a~=v]`.
    Includes,
    /// `[a|=v]`.
    DashMatch,
    /// `[a^=v]`.
    Prefix,
    /// `[a$=v]`.
    Suffix,
    /// `[a*=v]`.
    Substring,
}

/// An attribute selector.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AttrSelector {
    /// The attribute name (lowercased).
    pub name: String,
    /// The match operator.
    pub op: AttrOp,
    /// The comparison value (absent only for [`AttrOp::Exists`]).
    pub value: Option<String>,
}

/// An `an+b` expression from an `:nth-*` pseudo-class.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Nth {
    /// The `a` coefficient.
    pub a: i32,
    /// The `b` constant.
    pub b: i32,
}

impl Nth {
    /// Whether index `n` (0-based) satisfies `an+b` for some `n >= 0`.
    pub fn matches_index(&self, index: usize) -> bool {
        let index = index as i32;
        if self.a == 0 {
            return index == self.b - 1;
        }
        let n = index + 1 - self.b;
        n % self.a == 0 && n / self.a >= 0
    }
}

/// A pseudo-class.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PseudoClass {
    /// `:first-child`.
    FirstChild,
    /// `:last-child`.
    LastChild,
    /// `:only-child`.
    OnlyChild,
    /// `:nth-child(an+b)`.
    NthChild(Nth),
    /// `:root`.
    Root,
    /// `:not(...)`.
    Not(Box<SelectorList>),
    /// `:is(...)`.
    Is(Box<SelectorList>),
    /// `:hover`.
    Hover,
    /// `:focus`.
    Focus,
    /// `:active`.
    Active,
    /// `:disabled`.
    Disabled,
    /// `:link`.
    Link,
    /// A pseudo-class Noonoo parses but does not evaluate yet.
    Unknown(String),
}

/// A simple (non-combinator) selector within a compound.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SimpleSelector {
    /// `.class`.
    Class(String),
    /// `#id`.
    Id(String),
    /// `[attr…]`.
    Attribute(AttrSelector),
    /// `:pseudo`.
    PseudoClass(PseudoClass),
}

/// The type part of a compound selector.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum TypeSelector {
    /// `*` or omitted.
    #[default]
    Universal,
    /// A named element, lowercased.
    Named(String),
}

/// A compound selector: an optional type plus simple selectors.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct CompoundSelector {
    /// The type selector (`*` by default).
    pub type_selector: TypeSelector,
    /// The simple selectors.
    pub simples: Vec<SimpleSelector>,
    /// A `::pseudo-element`, if present (never matches a real element).
    pub pseudo_element: Option<String>,
}

/// A complex selector: compounds joined by combinators.
///
/// `combinators.len() == compounds.len() - 1`.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ComplexSelector {
    /// The compound selectors, left to right.
    pub compounds: Vec<CompoundSelector>,
    /// The combinators between them.
    pub combinators: Vec<Combinator>,
}

/// A comma-separated list of complex selectors.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct SelectorList {
    /// The selectors.
    pub selectors: Vec<ComplexSelector>,
}

/// A read-only view of one element, used for matching.
///
/// Implemented by the style/engine layers over `noonoo-dom`; tests implement it
/// over a real tree. Sibling helpers count **element** siblings only.
pub trait Element: Clone {
    /// The tag name, lowercased.
    fn tag_name(&self) -> String;
    /// The `id` attribute, if any.
    fn id(&self) -> Option<String>;
    /// The classes in the `class` attribute.
    fn classes(&self) -> Vec<String>;
    /// An attribute value.
    fn attr(&self, name: &str) -> Option<String>;
    /// The parent element, if any.
    fn parent(&self) -> Option<Self>;
    /// Preceding element siblings, in document order.
    fn previous_siblings(&self) -> Vec<Self>;
    /// The 0-based index among element siblings.
    fn sibling_index(&self) -> usize;
    /// The number of element siblings, including this element.
    fn sibling_count(&self) -> usize;
}

/// Interactive state a dynamic pseudo-class is evaluated against.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MatchContext {
    /// Whether the element is hovered.
    pub hovered: bool,
    /// Whether the element is focused.
    pub focused: bool,
    /// Whether the element is active.
    pub active: bool,
}

// --- matching --------------------------------------------------------------

impl SelectorList {
    /// Whether any selector in the list matches `element`.
    pub fn matches<E: Element>(&self, element: &E, ctx: MatchContext) -> bool {
        self.matching_specificity(element, ctx).is_some()
    }

    /// The specificity of the most specific matching selector, if any.
    pub fn matching_specificity<E: Element>(
        &self,
        element: &E,
        ctx: MatchContext,
    ) -> Option<Specificity> {
        self.selectors
            .iter()
            .filter(|selector| selector.matches(element, ctx))
            .map(ComplexSelector::specificity)
            .max()
    }

    /// The highest specificity across all selectors, regardless of matching.
    pub fn max_specificity(&self) -> Specificity {
        self.selectors
            .iter()
            .map(ComplexSelector::specificity)
            .max()
            .unwrap_or(Specificity::ZERO)
    }
}

impl ComplexSelector {
    /// Whether this selector matches `element`.
    pub fn matches<E: Element>(&self, element: &E, ctx: MatchContext) -> bool {
        if self.compounds.is_empty() {
            return false;
        }
        self.match_at(self.compounds.len() - 1, element, ctx)
    }

    /// Match `compounds[index]` against `element`, then walk leftward.
    fn match_at<E: Element>(&self, index: usize, element: &E, ctx: MatchContext) -> bool {
        if !self.compounds[index].matches(element, ctx) {
            return false;
        }
        if index == 0 {
            return true;
        }
        match self.combinators[index - 1] {
            Combinator::Descendant => {
                let mut cursor = element.parent();
                while let Some(ancestor) = cursor {
                    if self.match_at(index - 1, &ancestor, ctx) {
                        return true;
                    }
                    cursor = ancestor.parent();
                }
                false
            }
            Combinator::Child => element
                .parent()
                .is_some_and(|parent| self.match_at(index - 1, &parent, ctx)),
            Combinator::AdjacentSibling => element
                .previous_siblings()
                .last()
                .is_some_and(|sibling| self.match_at(index - 1, sibling, ctx)),
            Combinator::GeneralSibling => element
                .previous_siblings()
                .iter()
                .any(|sibling| self.match_at(index - 1, sibling, ctx)),
        }
    }

    /// The specificity of this selector (sum of its compounds).
    pub fn specificity(&self) -> Specificity {
        self.compounds
            .iter()
            .fold(Specificity::ZERO, |acc, compound| {
                acc + compound.specificity()
            })
    }
}

impl CompoundSelector {
    /// Whether this compound matches `element`.
    pub fn matches<E: Element>(&self, element: &E, ctx: MatchContext) -> bool {
        if self.pseudo_element.is_some() {
            // A `::before`/`::after` selector targets a pseudo-element, not a
            // real element, so it never matches one.
            return false;
        }
        if let TypeSelector::Named(name) = &self.type_selector
            && &element.tag_name() != name
        {
            return false;
        }
        self.simples
            .iter()
            .all(|simple| simple.matches(element, ctx))
    }

    /// The specificity of this compound.
    pub fn specificity(&self) -> Specificity {
        let mut spec = match &self.type_selector {
            TypeSelector::Named(_) => Specificity {
                types: 1,
                ..Specificity::ZERO
            },
            TypeSelector::Universal => Specificity::ZERO,
        };
        if self.pseudo_element.is_some() {
            spec.types += 1;
        }
        for simple in &self.simples {
            match simple {
                SimpleSelector::Id(_) => spec.ids += 1,
                SimpleSelector::Class(_) | SimpleSelector::Attribute(_) => spec.classes += 1,
                SimpleSelector::PseudoClass(pseudo) => match pseudo {
                    // :not()/:is() take their most specific argument.
                    PseudoClass::Not(inner) | PseudoClass::Is(inner) => {
                        let inner = inner.max_specificity();
                        spec.ids += inner.ids;
                        spec.classes += inner.classes;
                        spec.types += inner.types;
                    }
                    _ => spec.classes += 1,
                },
            }
        }
        spec
    }
}

impl SimpleSelector {
    fn matches<E: Element>(&self, element: &E, ctx: MatchContext) -> bool {
        match self {
            SimpleSelector::Class(class) => element.classes().iter().any(|c| c == class),
            SimpleSelector::Id(id) => element.id().as_deref() == Some(id.as_str()),
            SimpleSelector::Attribute(attr) => attr.matches(element),
            SimpleSelector::PseudoClass(pseudo) => pseudo.matches(element, ctx),
        }
    }
}

impl AttrSelector {
    fn matches<E: Element>(&self, element: &E) -> bool {
        let Some(actual) = element.attr(&self.name) else {
            return false;
        };
        let some_value = self.value.as_deref().unwrap_or("");
        match self.op {
            AttrOp::Exists => true,
            AttrOp::Equals => actual == some_value,
            AttrOp::Includes => actual.split_whitespace().any(|part| part == some_value),
            AttrOp::DashMatch => {
                actual == some_value
                    || (actual.starts_with(some_value)
                        && actual.as_bytes().get(some_value.len()) == Some(&b'-'))
            }
            AttrOp::Prefix => actual.starts_with(some_value),
            AttrOp::Suffix => actual.ends_with(some_value),
            AttrOp::Substring => actual.contains(some_value),
        }
    }
}

impl PseudoClass {
    fn matches<E: Element>(&self, element: &E, ctx: MatchContext) -> bool {
        match self {
            PseudoClass::FirstChild => element.sibling_index() == 0,
            PseudoClass::LastChild => element.sibling_index() + 1 == element.sibling_count(),
            PseudoClass::OnlyChild => element.sibling_count() == 1,
            PseudoClass::NthChild(nth) => nth.matches_index(element.sibling_index()),
            PseudoClass::Root => element.parent().is_none(),
            PseudoClass::Not(inner) => !inner.matches(element, ctx),
            PseudoClass::Is(inner) => inner.matches(element, ctx),
            PseudoClass::Hover => ctx.hovered,
            PseudoClass::Focus => ctx.focused,
            PseudoClass::Active => ctx.active,
            PseudoClass::Disabled => element.attr("disabled").is_some(),
            PseudoClass::Link => element.tag_name() == "a" && element.attr("href").is_some(),
            PseudoClass::Unknown(_) => false,
        }
    }
}

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
        let name = self.parse_ident_name()?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use noonoo_dom::{Document, NodeId};

    #[derive(Clone, Copy)]
    struct View<'a> {
        doc: &'a Document,
        id: NodeId,
    }

    impl<'a> View<'a> {
        fn element_children(&self, id: NodeId) -> Vec<NodeId> {
            self.doc
                .tree()
                .children(id)
                .iter()
                .copied()
                .filter(|&c| self.doc.element(c).is_some())
                .collect()
        }
    }

    impl<'a> Element for View<'a> {
        fn tag_name(&self) -> String {
            self.doc
                .element(self.id)
                .map(|e| e.name.clone())
                .unwrap_or_default()
        }
        fn id(&self) -> Option<String> {
            self.doc
                .element(self.id)
                .and_then(|e| e.id().map(str::to_owned))
        }
        fn classes(&self) -> Vec<String> {
            self.doc
                .element(self.id)
                .map(|e| e.classes().map(str::to_owned).collect())
                .unwrap_or_default()
        }
        fn attr(&self, name: &str) -> Option<String> {
            self.doc
                .element(self.id)
                .and_then(|e| e.attr(name).map(str::to_owned))
        }
        fn parent(&self) -> Option<Self> {
            let parent = self.doc.tree().parent(self.id)?;
            self.doc.element(parent).is_some().then_some(View {
                doc: self.doc,
                id: parent,
            })
        }
        fn previous_siblings(&self) -> Vec<Self> {
            let Some(parent) = self.doc.tree().parent(self.id) else {
                return Vec::new();
            };
            let mut out = Vec::new();
            for child in self.element_children(parent) {
                if child == self.id {
                    break;
                }
                out.push(View {
                    doc: self.doc,
                    id: child,
                });
            }
            out
        }
        fn sibling_index(&self) -> usize {
            let Some(parent) = self.doc.tree().parent(self.id) else {
                return 0;
            };
            self.element_children(parent)
                .iter()
                .position(|&c| c == self.id)
                .unwrap_or(0)
        }
        fn sibling_count(&self) -> usize {
            let Some(parent) = self.doc.tree().parent(self.id) else {
                return 1;
            };
            self.element_children(parent).len()
        }
    }

    fn sample() -> (Document, NodeId, NodeId) {
        // body > ul#items > li.a, li.b, li.c
        let mut doc = Document::new();
        let root = doc.root();
        let body = doc.create_element("body");
        doc.mutate().append_child(root, body).unwrap();
        let ul = doc.create_element("ul");
        doc.mutate().set_attribute(ul, "id", "items").unwrap();
        doc.mutate().append_child(body, ul).unwrap();
        for class in ["a", "b", "c"] {
            let li = doc.create_element("li");
            doc.mutate().set_attribute(li, "class", class).unwrap();
            doc.mutate().append_child(ul, li).unwrap();
        }
        (doc, body, ul)
    }

    fn view<'a>(doc: &'a Document, id: NodeId) -> View<'a> {
        View { doc, id }
    }

    fn ctx() -> MatchContext {
        MatchContext::default()
    }

    #[test]
    fn parses_and_matches_type_class_id() {
        let (doc, _body, ul) = sample();
        let s = SelectorList::parse("ul#items").unwrap();
        assert!(s.matches(&view(&doc, ul), ctx()));
        let li = doc.get_elements_by_tag_name("li")[1];
        assert!(
            SelectorList::parse("li.b")
                .unwrap()
                .matches(&view(&doc, li), ctx())
        );
        assert!(
            !SelectorList::parse("li.a")
                .unwrap()
                .matches(&view(&doc, li), ctx())
        );
    }

    #[test]
    fn child_and_descendant_combinators() {
        let (doc, _body, ul) = sample();
        let li = doc.get_elements_by_tag_name("li")[0];
        assert!(
            SelectorList::parse("body ul li")
                .unwrap()
                .matches(&view(&doc, li), ctx())
        );
        assert!(
            SelectorList::parse("ul > li")
                .unwrap()
                .matches(&view(&doc, li), ctx())
        );
        // No direct child relationship between body and li.
        assert!(
            !SelectorList::parse("body > li")
                .unwrap()
                .matches(&view(&doc, li), ctx())
        );
        assert!(
            SelectorList::parse("ul")
                .unwrap()
                .matches(&view(&doc, ul), ctx())
        );
    }

    #[test]
    fn sibling_combinators() {
        let (doc, _body, _ul) = sample();
        let lis = doc.get_elements_by_tag_name("li");
        // li.a + li.b matches li.b
        assert!(
            SelectorList::parse("li.a + li.b")
                .unwrap()
                .matches(&view(&doc, lis[1]), ctx())
        );
        // li.a ~ li.c matches li.c
        assert!(
            SelectorList::parse("li.a ~ li.c")
                .unwrap()
                .matches(&view(&doc, lis[2]), ctx())
        );
        assert!(
            !SelectorList::parse("li.c + li.a")
                .unwrap()
                .matches(&view(&doc, lis[0]), ctx())
        );
    }

    #[test]
    fn structural_pseudo_classes() {
        let (doc, _body, _ul) = sample();
        let lis = doc.get_elements_by_tag_name("li");
        assert!(
            SelectorList::parse("li:first-child")
                .unwrap()
                .matches(&view(&doc, lis[0]), ctx())
        );
        assert!(
            SelectorList::parse("li:last-child")
                .unwrap()
                .matches(&view(&doc, lis[2]), ctx())
        );
        assert!(
            !SelectorList::parse("li:first-child")
                .unwrap()
                .matches(&view(&doc, lis[1]), ctx())
        );
        assert!(
            SelectorList::parse("li:nth-child(2)")
                .unwrap()
                .matches(&view(&doc, lis[1]), ctx())
        );
        assert!(
            SelectorList::parse("li:nth-child(odd)")
                .unwrap()
                .matches(&view(&doc, lis[0]), ctx())
        );
        assert!(
            SelectorList::parse("li:nth-child(2n+1)")
                .unwrap()
                .matches(&view(&doc, lis[2]), ctx())
        );
    }

    #[test]
    fn logical_pseudo_classes() {
        let (doc, _body, _ul) = sample();
        let lis = doc.get_elements_by_tag_name("li");
        assert!(
            SelectorList::parse("li:not(.a)")
                .unwrap()
                .matches(&view(&doc, lis[1]), ctx())
        );
        assert!(
            SelectorList::parse("li:is(.a, .c)")
                .unwrap()
                .matches(&view(&doc, lis[2]), ctx())
        );
    }

    #[test]
    fn attribute_selectors() {
        let mut doc = Document::new();
        let root = doc.root();
        let a = doc.create_element("a");
        doc.mutate()
            .set_attribute(a, "href", "https://example.com/x")
            .unwrap();
        doc.mutate()
            .set_attribute(a, "data-role", "nav item")
            .unwrap();
        doc.mutate().append_child(root, a).unwrap();
        let v = view(&doc, a);

        assert!(SelectorList::parse("[href]").unwrap().matches(&v, ctx()));
        assert!(
            SelectorList::parse("a[data-role~=nav]")
                .unwrap()
                .matches(&v, ctx())
        );
        assert!(
            SelectorList::parse("a[href^=https]")
                .unwrap()
                .matches(&v, ctx())
        );
        assert!(
            SelectorList::parse("a[href$=x]")
                .unwrap()
                .matches(&v, ctx())
        );
        assert!(
            SelectorList::parse("a[href*=example]")
                .unwrap()
                .matches(&v, ctx())
        );
        assert!(
            !SelectorList::parse("[lang|=en]")
                .unwrap()
                .matches(&v, ctx())
        );
    }

    #[test]
    fn dynamic_pseudo_classes_use_context() {
        let (doc, _body, _ul) = sample();
        let li = doc.get_elements_by_tag_name("li")[0];
        let hovered = MatchContext {
            hovered: true,
            ..MatchContext::default()
        };
        assert!(
            SelectorList::parse("li:hover")
                .unwrap()
                .matches(&view(&doc, li), hovered)
        );
        assert!(
            !SelectorList::parse("li:hover")
                .unwrap()
                .matches(&view(&doc, li), ctx())
        );
    }

    #[test]
    fn specificity_is_computed() {
        let spec = |s: &str| SelectorList::parse(s).unwrap().max_specificity().to_u32();
        assert!(spec("#id") > spec(".cls"));
        assert!(spec(".cls") > spec("div"));
        assert!(spec("ul li") > spec("li"));
        assert_eq!(spec("*"), 0);
        assert_eq!(
            spec("li.a"),
            Specificity {
                ids: 0,
                classes: 1,
                types: 1
            }
            .to_u32()
        );
        // :not() takes its argument's specificity.
        assert_eq!(
            spec("li:not(.a)"),
            Specificity {
                ids: 0,
                classes: 1,
                types: 1
            }
            .to_u32()
        );
    }

    #[test]
    fn pseudo_element_never_matches_an_element() {
        let (doc, _body, ul) = sample();
        assert!(
            !SelectorList::parse("ul::before")
                .unwrap()
                .matches(&view(&doc, ul), ctx())
        );
    }

    #[test]
    fn unknown_pseudo_class_parses_but_never_matches() {
        let (doc, _body, ul) = sample();
        let s = SelectorList::parse("ul:defined").unwrap();
        assert!(!s.matches(&view(&doc, ul), ctx()));
    }

    #[test]
    fn parse_errors_are_reported() {
        assert!(SelectorList::parse("").is_err());
        assert!(SelectorList::parse("li >").is_err());
        assert!(SelectorList::parse("[href").is_err());
    }
}
