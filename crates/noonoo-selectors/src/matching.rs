//! Selector matching: the `Element` view and the match algorithms over the
//! selector model.

use super::*;
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

    /// The specificity of the most specific selector that styles
    /// `element`'s `::pseudo` (e.g. `"before"`), if any: one whose last
    /// compound names that pseudo-element and otherwise matches `element`.
    pub fn pseudo_matching_specificity<E: Element>(
        &self,
        element: &E,
        ctx: MatchContext,
        pseudo: &str,
    ) -> Option<Specificity> {
        self.selectors
            .iter()
            .filter(|selector| selector.matches_pseudo(element, ctx, pseudo))
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

    /// Whether this selector styles `element`'s `::pseudo`: the last
    /// compound names it, no other compound names one, and the rest
    /// matches `element` as usual.
    pub fn matches_pseudo<E: Element>(&self, element: &E, ctx: MatchContext, pseudo: &str) -> bool {
        let Some((last, rest)) = self.compounds.split_last() else {
            return false;
        };
        let names_it = last
            .pseudo_element
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case(pseudo));
        if !names_it || rest.iter().any(|c| c.pseudo_element.is_some()) {
            return false;
        }
        last.matches_element(element, ctx)
            && self.match_left(self.compounds.len() - 1, element, ctx)
    }

    /// Match `compounds[index]` against `element`, then walk leftward.
    fn match_at<E: Element>(&self, index: usize, element: &E, ctx: MatchContext) -> bool {
        if !self.compounds[index].matches(element, ctx) {
            return false;
        }
        self.match_left(index, element, ctx)
    }

    /// With `compounds[index]` matched by `element`, match the compounds to
    /// its left through their combinators.
    fn match_left<E: Element>(&self, index: usize, element: &E, ctx: MatchContext) -> bool {
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
        self.matches_element(element, ctx)
    }

    /// Whether the type and simple selectors match `element`, setting any
    /// pseudo-element aside.
    fn matches_element<E: Element>(&self, element: &E, ctx: MatchContext) -> bool {
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
