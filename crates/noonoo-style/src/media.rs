//! `@media` prelude evaluation.
//!
//! Finding 4 of `docs/REVIEW-2026-11-07.md`: `noonoo-css` captures at-rules
//! verbatim, and the cascade never looked inside them, so `@media
//! (min-width:100px){…}` never applied. The cascade now evaluates the
//! prelude against the [`Viewport`] it is computing for, and expands
//! matching blocks in place (source order preserved).
//!
//! The condition language is a documented subset:
//!
//! - top-level commas separate alternatives (any may match); `and` combines
//!   the expressions of one alternative;
//! - `not` negates its alternative, and a leading `only` is ignored, as the
//!   spec (§6 legacy syntax) defines both;
//! - media types: `screen` (a window-backed engine) and `all` match, and
//!   everything else — notably `print` — does not;
//! - features: `min-width`, `max-width`, `min-height`, `max-height`, with
//!   `px` values (`0` may be unitless). An unrecognised feature is a failed
//!   expression, which fails the whole alternative, so an unknown condition
//!   never matches by accident.
//!
//! The rest of the media-query grammar (range syntax, `or`, nested
//! conditions in parens, environment functions) is not supported: a
//! condition outside the subset evaluates false.

use crate::Viewport;

/// Whether a `@media` (or `@import`-style) prelude matches `viewport`.
pub(crate) fn matches(prelude: &str, viewport: &Viewport) -> bool {
    // A prelude with no condition is `(all)`.
    if prelude.trim().is_empty() {
        return true;
    }
    prelude
        .split(',')
        .any(|alternative| alternative_matches(alternative, viewport))
}

/// One comma-separated alternative, combining its expressions with `and`.
fn alternative_matches(text: &str, viewport: &Viewport) -> bool {
    let mut rest = text.trim();
    let mut negated = false;
    loop {
        let lower = rest.to_ascii_lowercase();
        if let Some(tail) = lower.strip_prefix("not ") {
            negated = !negated;
            rest = rest[rest.len() - tail.len()..].trim_start();
        } else if let Some(tail) = lower.strip_prefix("only ") {
            rest = rest[rest.len() - tail.len()..].trim_start();
        } else {
            break;
        }
    }

    // Expressions split on `and`; type keywords (`screen`, `all`) are single
    // expressions of the same gate.
    let mut result = true;
    for (index, part) in split_and(rest).iter().enumerate() {
        let part = part.trim();
        if part.is_empty() {
            // Leading/trailing `and`, or nothing at all after `not`.
            return false;
        }
        let matches = if index == 0 {
            type_or_feature(part, viewport)
        } else {
            feature(part, viewport)
        };
        result &= matches;
    }
    result != negated
}

/// Split on case-insensitive `and`.
fn split_and(rest: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut remainder = rest;
    while let Some(at) = find_and(remainder) {
        parts.push(&remainder[..at]);
        remainder = &remainder[at + 3..];
    }
    parts.push(remainder);
    parts
}

/// The case-insensitive byte offset of a standalone `and`.
fn find_and(text: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find("and") {
        let at = from + at;
        let before = text[..at].chars().next_back();
        let after = text[at + 3..].chars().next();
        let is_space = |c: Option<char>| c.is_none_or(|c| c.is_ascii_whitespace());
        if is_space(before) && is_space(after) {
            return Some(at);
        }
        from = at + 3;
    }
    None
}

/// The first expression of an alternative may be a media type.
fn type_or_feature(text: &str, viewport: &Viewport) -> bool {
    let lower = text.to_ascii_lowercase();
    match lower.as_str() {
        "screen" | "all" | "(screen)" | "(all)" => true,
        "print" | "speech" | "tty" | "tv" | "projection" | "handheld" | "braille" | "embossed" => {
            false
        }
        _ => feature(text, viewport),
    }
}

/// One feature expression: `(feature)` or `(feature: value)`.
fn feature(text: &str, viewport: &Viewport) -> bool {
    let trimmed = text.trim();
    let Some(inner) = trimmed.strip_prefix('(') else {
        return false;
    };
    let Some(inner) = inner.strip_suffix(')') else {
        return false;
    };
    let inner = inner.trim();
    if inner.is_empty() {
        return false;
    }
    // `(feature)` alone is true when the feature applies; unsupported
    // booleans are false.
    let (name, value) = match inner.split_once(':') {
        Some((name, value)) => (name.trim().to_ascii_lowercase(), value.trim()),
        None => {
            return matches!(
                inner.to_ascii_lowercase().as_str(),
                "width" | "height" | "aspect-ratio" | "color"
            );
        }
    };

    let Some(length) = parse_px(value) else {
        // An unparsable value fails the expression, per unknown feature.
        return false;
    };
    match name.as_str() {
        "min-width" => viewport.width >= length,
        "max-width" => viewport.width <= length,
        "min-height" => viewport.height >= length,
        "max-height" => viewport.height <= length,
        _ => false,
    }
}

/// A `px` length (`0` may be unitless). Other units are unsupported.
fn parse_px(value: &str) -> Option<f32> {
    let value = value.trim();
    let number = if value.eq_ignore_ascii_case("0px") {
        "0"
    } else if let Some(stripped) = value.strip_suffix("px") {
        stripped
    } else {
        // Unitless zero is the only valid bare length.
        if value == "0" {
            "0"
        } else {
            return None;
        }
    };
    number.parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(width: f32, height: f32) -> Viewport {
        Viewport { width, height }
    }

    #[test]
    fn simple_width_features() {
        let vp = at(400.0, 600.0);
        assert!(matches("(min-width:100px)", &vp));
        assert!(matches("(min-width: 400px)", &vp));
        assert!(!matches("(min-width: 401px)", &vp));
        assert!(!matches("(max-width: 100px)", &vp));
        assert!(matches("(max-width:400px)", &vp));
    }

    #[test]
    fn height_features() {
        let vp = at(400.0, 600.0);
        assert!(matches("(min-height:600px)", &vp));
        assert!(matches("(max-height: 600px)", &vp));
        assert!(!matches("(max-height:599px)", &vp));
    }

    #[test]
    fn media_types() {
        let vp = at(400.0, 600.0);
        assert!(matches("screen", &vp));
        assert!(matches("all", &vp));
        assert!(!matches("print", &vp));
        assert!(matches("", &vp));
        assert!(matches("  ", &vp));
    }

    #[test]
    fn and_composes_and_not_negates() {
        let vp = at(400.0, 600.0);
        assert!(matches(
            "screen and (min-width:100px) and (max-width:500px)",
            &vp
        ));
        assert!(!matches("(min-width:100px) and (max-width:300px)", &vp));
        assert!(!matches("not screen", &vp));
        assert!(matches("not print", &vp));
        assert!(matches("not screen and (min-width:10000px)", &vp));
        assert!(matches("only screen", &vp));
    }

    #[test]
    fn commas_separate_alternatives() {
        let vp = at(400.0, 600.0);
        assert!(matches("print, screen", &vp));
        assert!(!matches("print, speech", &vp));
        assert!(matches("(max-width:100px), print, (min-width:300px)", &vp));
    }

    #[test]
    fn unsupported_conditions_fail() {
        let vp = at(400.0, 600.0);
        assert!(!matches("(orientation: landscape)", &vp));
        assert!(!matches("(min-width: 10em)", &vp));
        assert!(!matches("(min-width)", &vp));
        assert!(!matches("(min-width: )", &vp));
        // An unknown type name is a failed alternative.
        assert!(!matches("reality", &vp));
        // Bare-and and empty parens are malformed.
        assert!(!matches("screen and", &vp));
        assert!(!matches("screen and ()", &vp));
    }
}
