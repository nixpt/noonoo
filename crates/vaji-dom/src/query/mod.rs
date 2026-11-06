//! Queries against the tree.
//!
//! Reads are separated from writes on purpose: a query never mutates, so it is
//! total and panic-free. The selector subset here is deliberately small —
//! compound `tag`, `#id`, `.class` — and expands as the style engine arrives;
//! what matters now is that the *shape* (parse once, match many, return ids) is
//! fixed.

use crate::node::ElementData;

/// A parse failure for a selector.
#[derive(Debug, PartialEq, Eq)]
pub enum SelectorError {
    /// The selector was empty.
    Empty,
    /// The selector uses syntax Vaji does not support yet (combinators, etc.).
    Unsupported(String),
}

/// A simple compound selector: at most one tag, at most one id, any classes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Selector {
    /// The tag name, if any.
    pub tag: Option<String>,
    /// The id, if any.
    pub id: Option<String>,
    /// The required classes.
    pub classes: Vec<String>,
}

impl Selector {
    /// Parse a compound selector such as `div`, `#main`, `.card`, `a.btn`.
    pub fn parse(input: &str) -> Result<Self, SelectorError> {
        let source = input.trim();
        if source.is_empty() {
            return Err(SelectorError::Empty);
        }
        if source.contains(|c: char| c.is_whitespace() || matches!(c, '>' | '+' | '~' | ',')) {
            return Err(SelectorError::Unsupported(source.to_owned()));
        }

        let mut selector = Selector::default();

        let tag_end = source.find(['#', '.']).unwrap_or(source.len());
        if tag_end > 0 {
            selector.tag = Some(source[..tag_end].to_ascii_lowercase());
        }

        let mut rest = &source[tag_end..];
        while !rest.is_empty() {
            let marker = rest.chars().next().unwrap();
            rest = &rest[1..];
            let end = rest.find(['#', '.']).unwrap_or(rest.len());
            let name = &rest[..end];
            if name.is_empty() {
                return Err(SelectorError::Unsupported(source.to_owned()));
            }
            match marker {
                '#' if selector.id.is_none() => selector.id = Some(name.to_owned()),
                '#' => return Err(SelectorError::Unsupported(source.to_owned())),
                '.' => selector.classes.push(name.to_owned()),
                _ => return Err(SelectorError::Unsupported(source.to_owned())),
            }
            rest = &rest[end..];
        }

        Ok(selector)
    }

    /// Whether `element` satisfies this selector.
    pub fn matches(&self, element: &ElementData) -> bool {
        if let Some(tag) = &self.tag
            && &element.name != tag
        {
            return false;
        }
        if let Some(id) = &self.id
            && element.id() != Some(id.as_str())
        {
            return false;
        }
        self.classes.iter().all(|class| element.has_class(class))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn el(name: &str, id: Option<&str>, classes: &[&str]) -> ElementData {
        let mut e = ElementData::new(name);
        if let Some(id) = id {
            e.set_attr("id", id);
        }
        if !classes.is_empty() {
            e.set_attr("class", classes.join(" "));
        }
        e
    }

    #[test]
    fn parses_compounds() {
        assert_eq!(
            Selector::parse("a.btn#go").unwrap(),
            Selector {
                tag: Some("a".into()),
                id: Some("go".into()),
                classes: vec!["btn".into()],
            }
        );
    }

    #[test]
    fn rejects_combinators() {
        assert!(matches!(
            Selector::parse("div span"),
            Err(SelectorError::Unsupported(_))
        ));
        assert_eq!(Selector::parse(""), Err(SelectorError::Empty));
    }

    #[test]
    fn matches_elements() {
        let s = Selector::parse(".card").unwrap();
        assert!(s.matches(&el("div", None, &["card", "wide"])));
        assert!(!s.matches(&el("div", None, &["narrow"])));

        let s = Selector::parse("a#go").unwrap();
        assert!(s.matches(&el("a", Some("go"), &[])));
        assert!(!s.matches(&el("a", Some("stop"), &[])));
    }
}
