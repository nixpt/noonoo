//! Matching `:has()`: whether anything inside or after an element matches a
//! relative selector anchored at it.

use super::*;

/// The most compounds a relative selector may have, to fit [`Matched`].
pub(crate) const MAX_COMPOUNDS: usize = 128;

/// Which of a selector's compounds an element matched, with everything to
/// their left back to the anchor: bit `i` for `compounds[i]`.
type Matched = u128;

/// Sibling elements still to visit, and what the ones around them matched.
struct Level<E> {
    elements: std::vec::IntoIter<E>,
    /// What the siblings' parent matched.
    parent: Matched,
    /// What the siblings' ancestors matched.
    ancestors: Matched,
    /// What the sibling before the next one matched.
    previous: Matched,
    /// What the siblings before the next one matched.
    earlier: Matched,
}

impl RelativeSelector {
    /// Whether some element stands in this selector's relation to `anchor`.
    ///
    /// One pass over the elements the first combinator can reach, keeping
    /// which compounds each could be: the cost grows with the tree and the
    /// selector, not with the ways to pair them.
    pub fn matches_from<E: Element>(&self, anchor: &E) -> bool {
        let selector = &self.selector;
        let count = selector.compounds.len();
        if count == 0 || count > MAX_COMPOUNDS || selector.combinators.len() + 1 != count {
            return false;
        }
        let last: Matched = 1 << (count - 1);
        let starts = match self.combinator {
            Combinator::Descendant | Combinator::Child => anchor.children(),
            Combinator::AdjacentSibling | Combinator::GeneralSibling => anchor.next_siblings(),
        };
        let mut levels = vec![Level {
            elements: starts.into_iter(),
            parent: 0,
            ancestors: 0,
            previous: 0,
            earlier: 0,
        }];
        let mut first = true;
        while let Some(level) = levels.last_mut() {
            let Some(element) = level.elements.next() else {
                levels.pop();
                continue;
            };
            let (parent, ancestors) = (level.parent, level.ancestors);
            let (previous, earlier) = (level.previous, level.earlier);
            let top = levels.len() == 1;
            let starts_here = match self.combinator {
                Combinator::Descendant => true,
                Combinator::Child | Combinator::GeneralSibling => top,
                Combinator::AdjacentSibling => first,
            };
            first = false;

            let mut matched: Matched = 0;
            for (i, compound) in selector.compounds.iter().enumerate() {
                let related = if i == 0 {
                    starts_here
                } else {
                    let from = match selector.combinators[i - 1] {
                        Combinator::Descendant => ancestors,
                        Combinator::Child => parent,
                        Combinator::AdjacentSibling => previous,
                        Combinator::GeneralSibling => earlier,
                    };
                    from & (1 << (i - 1)) != 0
                };
                if related && compound.matches(&element, element.state()) {
                    matched |= 1 << i;
                }
            }
            if matched & last != 0 {
                return true;
            }

            if let Some(level) = levels.last_mut() {
                level.previous = matched;
                level.earlier |= matched;
            }
            // Go down only where a match could still start or go on.
            if self.combinator == Combinator::Descendant || (matched | ancestors) != 0 {
                levels.push(Level {
                    elements: element.children().into_iter(),
                    parent: matched,
                    ancestors: ancestors | matched,
                    previous: 0,
                    earlier: 0,
                });
            } else if top && self.combinator == Combinator::AdjacentSibling && earlier == 0 {
                // The next sibling didn't match: no later one can.
                return false;
            }
        }
        false
    }

    /// The specificity of this selector: its combinator adds nothing.
    pub fn specificity(&self) -> Specificity {
        self.selector.specificity()
    }
}
