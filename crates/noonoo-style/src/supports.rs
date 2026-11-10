//! `@supports` condition evaluation.
//!
//! A block applies when its condition holds: `(property: value)` holds when
//! the declaration parses, `selector(…)` when the selector list does; `not`,
//! `and`, `or` and parenthesised conditions combine them. Anything else
//! (unknown functions, a malformed condition) is false, as CSS Conditional
//! 3 §6.1 defines for general-enclosed conditions; so are `font-tech()` and
//! `font-format()`, for technologies noonoo's fonts lack.

use noonoo_css::{Property, parse_declaration};
use noonoo_selectors::{PseudoClass, SelectorList, SimpleSelector};

/// Whether an `@supports` prelude holds.
pub(crate) fn holds(prelude: &str) -> bool {
    condition(prelude) == Some(true)
}

/// A `<supports-condition>`'s value, or `None` if it is malformed.
fn condition(text: &str) -> Option<bool> {
    let text = text.trim();
    if let Some(rest) = strip_keyword(text, "not") {
        let (value, rest) = in_parens(rest)?;
        return rest.trim().is_empty().then_some(!value);
    }
    let (mut value, mut rest) = in_parens(text)?;
    let mut joiner: Option<&str> = None;
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            return Some(value);
        }
        let word = ["and", "or"]
            .into_iter()
            .find(|word| strip_keyword(rest, word).is_some())?;
        // `and` and `or` don't mix without parentheses.
        if joiner.is_some_and(|joiner| joiner != word) {
            return None;
        }
        joiner = Some(word);
        let (next, after) = in_parens(strip_keyword(rest, word)?)?;
        value = if word == "and" {
            value && next
        } else {
            value || next
        };
        rest = after;
    }
}

/// `text` after a leading `keyword` (any case) and the white space that
/// must follow it.
fn strip_keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let head = text.get(..keyword.len())?;
    let rest = &text[keyword.len()..];
    (head.eq_ignore_ascii_case(keyword) && rest.starts_with(|c: char| c.is_whitespace()))
        .then_some(rest)
}

/// The value of the `<supports-in-parens>` that starts `text`, and the text
/// after it: a parenthesised condition or declaration, `selector(…)`, or any
/// other function or parenthesised text, which is false.
fn in_parens(text: &str) -> Option<(bool, &str)> {
    let text = text.trim_start();
    let open = text.find('(')?;
    let name = &text[..open];
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    let close = open + closing(&text[open..])?;
    let inner = &text[open + 1..close];
    let rest = &text[close + 1..];
    let value = if name.is_empty() {
        let trimmed = inner.trim_start();
        if trimmed.starts_with('(') || strip_keyword(trimmed, "not").is_some() {
            condition(inner).unwrap_or(false)
        } else {
            declaration(inner)
        }
    } else if name.eq_ignore_ascii_case("selector") {
        SelectorList::parse(inner.trim()).is_ok_and(|list| supported(&list))
    } else {
        false
    };
    Some((value, rest))
}

/// The byte offset of the parenthesis closing the one `text` starts with.
fn closing(text: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (at, c) in text.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(at);
                }
            }
            _ => {}
        }
    }
    None
}

/// Whether `property: value` is a declaration noonoo parses.
fn declaration(text: &str) -> bool {
    let Some((name, value)) = text.split_once(':') else {
        return false;
    };
    let (name, value) = (name.trim(), value.trim());
    !name.is_empty()
        && !value.is_empty()
        && parse_declaration(name, value, false).is_ok_and(|declarations| {
            !declarations.is_empty()
                && declarations
                    .iter()
                    .all(|declaration| !matches!(declaration.property, Property::Unknown(_)))
        })
}

/// Whether every pseudo-class in `list` is one Chromium supports. Those
/// noonoo parses but doesn't evaluate match nothing, which is right for a
/// static page (`:target` with no fragment, `:visited` with no history);
/// a name outside CSS isn't supported at all.
fn supported(list: &SelectorList) -> bool {
    list.selectors
        .iter()
        .flat_map(|selector| &selector.compounds)
        .flat_map(|compound| &compound.simples)
        .all(|simple| match simple {
            SimpleSelector::PseudoClass(PseudoClass::Unknown(name)) => {
                STANDARD_PSEUDO_CLASSES.contains(&name.as_str())
            }
            SimpleSelector::PseudoClass(PseudoClass::Not(inner) | PseudoClass::Is(inner)) => {
                supported(inner)
            }
            _ => true,
        })
}

/// The argument-less pseudo-classes of Selectors 4 and HTML that Chromium
/// supports, beyond those noonoo evaluates.
const STANDARD_PSEUDO_CLASSES: &[&str] = &[
    "any-link",
    "autofill",
    "checked",
    "default",
    "defined",
    "empty",
    "enabled",
    "first-of-type",
    "focus-visible",
    "focus-within",
    "fullscreen",
    "in-range",
    "indeterminate",
    "invalid",
    "last-of-type",
    "modal",
    "only-of-type",
    "optional",
    "out-of-range",
    "paused",
    "picture-in-picture",
    "placeholder-shown",
    "playing",
    "popover-open",
    "read-only",
    "read-write",
    "required",
    "scope",
    "target",
    "user-invalid",
    "user-valid",
    "valid",
    "visited",
];

#[cfg(test)]
mod tests {
    use super::holds;

    #[test]
    fn a_declaration_holds_when_it_parses() {
        assert!(holds("(display: flex)"));
        assert!(holds("( display:grid )"));
        assert!(!holds("(display: frobnicate)"));
        assert!(!holds("(not-a-property: 1px)"));
        assert!(holds("(--custom: 1px)"));
    }

    #[test]
    fn a_selector_holds_when_it_parses() {
        assert!(holds("selector(:target)"));
        assert!(holds("selector(a > b)"));
        assert!(holds("selector(a:visited:not(:focus-within))"));
        assert!(!holds("selector(:frobnicate)"));
        assert!(!holds("selector(:not(:frobnicate))"));
    }

    #[test]
    fn conditions_combine() {
        assert!(holds("not (display: frobnicate)"));
        assert!(!holds("not (display: flex)"));
        assert!(holds("(display: flex) and selector(:target)"));
        assert!(!holds("(display: flex) and (display: frobnicate)"));
        assert!(holds("(display: frobnicate) or (display: flex)"));
        assert!(holds(
            "((display: frobnicate) or (display: grid)) and (color: red)"
        ));
        assert!(holds("NOT (display: frobnicate)"));
    }

    #[test]
    fn font_technologies_noonoo_lacks_are_false() {
        // Chromium renders colour fonts, so it holds there; noonoo doesn't.
        assert!(!holds("font-tech(color-COLRv1)"));
    }

    #[test]
    fn anything_else_is_false() {
        for prelude in [
            "",
            "display: flex",
            "(display: flex) and",
            "(display: flex) and (color: red) or (color: blue)",
            "(display: flex",
            "frobnicate(1)",
        ] {
            assert!(!holds(prelude), "{prelude}");
        }
    }
}
