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

#![forbid(unsafe_code)]
#![warn(missing_docs)]

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
    /// The attribute name, as written: an HTML element's attributes match
    /// it ignoring ASCII case, a foreign (SVG, MathML) element's exactly.
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
    /// Whether this is an HTML element, whose attribute names match
    /// selectors ignoring ASCII case; a foreign (SVG, MathML) element's
    /// match exactly. HTML unless an implementation says otherwise.
    fn is_html(&self) -> bool {
        true
    }
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

mod matching;
mod parse;
#[cfg(test)]
mod tests;
