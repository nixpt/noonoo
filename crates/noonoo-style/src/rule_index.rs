//! Which rules can match an element, found without testing every rule.
//!
//! Each selector is filed under one key from its rightmost compound — the
//! part that must match the element itself: its `#id`, else a `.class`,
//! else its tag. A selector with none of those is a candidate for every
//! element. An element's candidates are then the rules filed under its id,
//! its classes and its tag, plus the keyless ones; only those are matched.
//! Matching itself is unchanged, so the result is the same as testing every
//! rule. Selectors ending in a pseudo-element never match a real element and
//! are kept apart for `::before`/`::after`.

use std::collections::HashMap;

use noonoo_dom::ElementData;
use noonoo_selectors::{ComplexSelector, SelectorList, SimpleSelector, TypeSelector};

/// Rules filed by the key their selectors need, as indices into the rule list.
///
/// A rule can be filed twice under one key (`.a, p.a`); `candidates`
/// dedupes, and a pseudo rule seen twice re-adds the same declarations.
#[derive(Default, Debug)]
pub(crate) struct RuleIndex {
    by_id: HashMap<String, Vec<usize>>,
    by_class: HashMap<String, Vec<usize>>,
    by_tag: HashMap<String, Vec<usize>>,
    universal: Vec<usize>,
    pseudo: Vec<usize>,
}

/// Where one selector is filed.
enum Bucket<'a> {
    Id(&'a str),
    Class(&'a str),
    Tag(&'a str),
    Universal,
    Pseudo,
}

fn bucket(selector: &ComplexSelector) -> Bucket<'_> {
    let Some(subject) = selector.compounds.last() else {
        return Bucket::Universal;
    };
    if subject.pseudo_element.is_some() {
        return Bucket::Pseudo;
    }
    let id = subject.simples.iter().find_map(|simple| match simple {
        SimpleSelector::Id(id) => Some(id.as_str()),
        _ => None,
    });
    let class = subject.simples.iter().find_map(|simple| match simple {
        SimpleSelector::Class(class) => Some(class.as_str()),
        _ => None,
    });
    match (id, class, &subject.type_selector) {
        (Some(id), _, _) => Bucket::Id(id),
        (None, Some(class), _) => Bucket::Class(class),
        (None, None, TypeSelector::Named(tag)) => Bucket::Tag(tag),
        (None, None, TypeSelector::Universal) => Bucket::Universal,
    }
}

impl RuleIndex {
    /// File every rule's selectors; `selectors[i]` is rule `i`'s list.
    pub(crate) fn new<'a>(selectors: impl IntoIterator<Item = &'a SelectorList>) -> Self {
        let mut index = RuleIndex::default();
        for (rule, list) in selectors.into_iter().enumerate() {
            for selector in &list.selectors {
                let slot = match bucket(selector) {
                    Bucket::Id(id) => index.by_id.entry(id.to_owned()).or_default(),
                    Bucket::Class(class) => index.by_class.entry(class.to_owned()).or_default(),
                    Bucket::Tag(tag) => index.by_tag.entry(tag.to_owned()).or_default(),
                    Bucket::Universal => &mut index.universal,
                    Bucket::Pseudo => &mut index.pseudo,
                };
                slot.push(rule);
            }
        }
        index
    }

    /// The rules that might match `element`, in rule order, each once.
    pub(crate) fn candidates(&self, element: &ElementData, out: &mut Vec<usize>) {
        out.clear();
        out.extend_from_slice(&self.universal);
        if let Some(rules) = element.id().and_then(|id| self.by_id.get(id)) {
            out.extend_from_slice(rules);
        }
        for class in element.classes() {
            if let Some(rules) = self.by_class.get(class) {
                out.extend_from_slice(rules);
            }
        }
        if let Some(rules) = self.by_tag.get(element.name.as_str()) {
            out.extend_from_slice(rules);
        }
        out.sort_unstable();
        out.dedup();
    }

    /// The rules with a selector ending in a pseudo-element, in rule order.
    pub(crate) fn pseudo(&self) -> &[usize] {
        &self.pseudo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(selectors: &[&str]) -> (Vec<SelectorList>, RuleIndex) {
        let lists: Vec<SelectorList> = selectors
            .iter()
            .map(|s| SelectorList::parse(s).unwrap())
            .collect();
        let index = RuleIndex::new(&lists);
        (lists, index)
    }

    fn candidates(index: &RuleIndex, html: &str) -> Vec<usize> {
        let doc = noonoo_html::parse(html);
        let element = doc.get_element_by_id("t").unwrap();
        let mut out = Vec::new();
        index.candidates(doc.element(element).unwrap(), &mut out);
        out
    }

    #[test]
    fn rules_are_filed_by_the_subjects_id_class_or_tag() {
        let (_, index) = index(&[
            "#t",          // 0: id
            ".a",          // 1: class
            "p",           // 2: tag
            "*",           // 3: keyless
            "div .a",      // 4: the subject is `.a`
            ".a > #other", // 5: another id
            "span.b",      // 6: a class wins over the tag
            "[data-x]",    // 7: keyless
            "p::before",   // 8: a pseudo-element
        ]);
        assert_eq!(
            candidates(&index, r#"<p id="t" class="a">x</p>"#),
            vec![0, 1, 2, 3, 4, 7]
        );
        assert_eq!(
            candidates(&index, r#"<span id="t" class="b">x</span>"#),
            vec![0, 3, 6, 7]
        );
        assert_eq!(index.pseudo(), &[8]);
    }

    #[test]
    fn a_list_is_filed_under_each_selector_and_found_once() {
        let (_, index) = index(&[".a, .b, p", "#z, .a"]);
        assert_eq!(
            candidates(&index, r#"<p id="t" class="a b">x</p>"#),
            vec![0, 1]
        );
        assert_eq!(
            candidates(&index, r#"<div id="t">x</div>"#),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn candidates_replace_what_the_buffer_held() {
        let (_, index) = index(&["p", "div"]);
        let doc = noonoo_html::parse(r#"<p id="t">x</p>"#);
        let element = doc.element(doc.get_element_by_id("t").unwrap()).unwrap();
        let mut out = vec![1, 7];
        index.candidates(element, &mut out);
        assert_eq!(out, vec![0]);
    }
}
