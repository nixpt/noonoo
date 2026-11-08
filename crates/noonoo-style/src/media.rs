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
//! - range features: `width`, `height` (`px`, or `em`/`rem` against the
//!   initial 16px), `aspect-ratio` (`16/9`), `resolution` (`dppx`, `x`,
//!   `dpi`, `dpcm`; a window is 1dppx for now) and
//!   `-webkit-device-pixel-ratio`, each with `min-`/`max-` prefixes or in
//!   level 4 range syntax (`(width >= 600px)`, `(400px <= width < 800px)`);
//! - discrete features, answered for a desktop window whose host states no
//!   preferences: `orientation`, `hover`/`any-hover: hover`,
//!   `pointer`/`any-pointer: fine`, `prefers-color-scheme: light`, and
//!   `no-preference` for `prefers-reduced-motion`, `-contrast` and
//!   `-reduced-transparency`;
//! - an unrecognised feature or value is a failed expression, which fails
//!   the whole alternative, so an unknown condition never matches by
//!   accident.
//!
//! The rest of the media-query grammar (`or`, nested conditions in parens,
//! environment functions) is not supported: a condition outside the
//! subset evaluates false.

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

/// One feature expression: `(feature)`, `(feature: value)`, or a range
/// such as `(width >= 600px)` or `(400px <= width < 800px)`.
fn feature(text: &str, viewport: &Viewport) -> bool {
    let trimmed = text.trim();
    let Some(inner) = trimmed
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
    else {
        return false;
    };
    let inner = inner.trim().to_ascii_lowercase();
    if inner.is_empty() {
        return false;
    }
    if inner.contains(['<', '>', '=']) {
        return range(&inner, viewport);
    }
    match inner.split_once(':') {
        None => boolean(&inner, viewport),
        Some((name, value)) => plain(name.trim(), value.trim(), viewport),
    }
}

/// The answers to the discrete features, for a desktop window whose host
/// states no preferences.
fn discrete(name: &str, viewport: &Viewport) -> Option<&'static str> {
    Some(match name {
        "orientation" if viewport.height >= viewport.width => "portrait",
        "orientation" => "landscape",
        "hover" | "any-hover" => "hover",
        "pointer" | "any-pointer" => "fine",
        "prefers-color-scheme" => "light",
        "prefers-reduced-motion" | "prefers-reduced-transparency" | "prefers-contrast" => {
            "no-preference"
        }
        _ => return None,
    })
}

/// A range feature's value: pixels for sizes, width over height for
/// `aspect-ratio`, dots per pixel for resolutions.
fn actual(name: &str, viewport: &Viewport) -> Option<f32> {
    match name {
        "width" => Some(viewport.width),
        "height" => Some(viewport.height),
        "aspect-ratio" => (viewport.height > 0.0).then(|| viewport.width / viewport.height),
        "resolution" | "-webkit-device-pixel-ratio" => Some(1.0),
        "color" => Some(8.0),
        _ => None,
    }
}

/// `(feature)` alone: true when the feature isn't zero or `none`.
fn boolean(name: &str, viewport: &Viewport) -> bool {
    if let Some(value) = discrete(name, viewport) {
        return value != "none" && value != "no-preference";
    }
    actual(name, viewport).is_some_and(|value| value != 0.0)
}

/// `(feature: value)`, `(min-feature: value)` or `(max-feature: value)`.
fn plain(name: &str, value: &str, viewport: &Viewport) -> bool {
    if let Some(answer) = discrete(name, viewport) {
        return answer == value;
    }
    let (name, compare) = if let Some(rest) = name.strip_prefix("-webkit-min-") {
        (format!("-webkit-{rest}"), Compare::Ge)
    } else if let Some(rest) = name.strip_prefix("-webkit-max-") {
        (format!("-webkit-{rest}"), Compare::Le)
    } else if let Some(rest) = name.strip_prefix("min-") {
        (rest.to_owned(), Compare::Ge)
    } else if let Some(rest) = name.strip_prefix("max-") {
        (rest.to_owned(), Compare::Le)
    } else {
        (name.to_owned(), Compare::Eq)
    };
    match (actual(&name, viewport), parse_value(&name, value)) {
        (Some(actual), Some(expected)) => compare.holds(actual, expected),
        _ => false,
    }
}

#[derive(Clone, Copy)]
enum Compare {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
}

impl Compare {
    /// `actual <op> expected`.
    fn holds(self, actual: f32, expected: f32) -> bool {
        match self {
            Compare::Lt => actual < expected,
            Compare::Le => actual <= expected,
            Compare::Eq => actual == expected,
            Compare::Ge => actual >= expected,
            Compare::Gt => actual > expected,
        }
    }

    /// The same comparison with its sides swapped (`a < b` is `b > a`).
    fn flipped(self) -> Self {
        match self {
            Compare::Lt => Compare::Gt,
            Compare::Le => Compare::Ge,
            Compare::Eq => Compare::Eq,
            Compare::Ge => Compare::Le,
            Compare::Gt => Compare::Lt,
        }
    }
}

/// `name op value`, `value op name`, or `value op name op value`.
fn range(text: &str, viewport: &Viewport) -> bool {
    let mut parts = Vec::new();
    let mut operators = Vec::new();
    let mut rest = text;
    loop {
        let Some(at) = rest.find(['<', '>', '=']) else {
            parts.push(rest.trim());
            break;
        };
        parts.push(rest[..at].trim());
        let tail = &rest[at..];
        let (operator, length) = if tail.starts_with("<=") {
            (Compare::Le, 2)
        } else if tail.starts_with(">=") {
            (Compare::Ge, 2)
        } else if tail.starts_with('<') {
            (Compare::Lt, 1)
        } else if tail.starts_with('>') {
            (Compare::Gt, 1)
        } else {
            (Compare::Eq, 1)
        };
        operators.push(operator);
        rest = &tail[length..];
    }
    let is_name = |part: &str| actual(part, viewport).is_some();
    // Each comparison reads as `actual <op> value` once the name is on the left.
    let check = |name: &str, operator: Compare, value: &str| match (
        actual(name, viewport),
        parse_value(name, value),
    ) {
        (Some(actual), Some(value)) => operator.holds(actual, value),
        _ => false,
    };
    match (parts.as_slice(), operators.as_slice()) {
        ([name, value], [operator]) if is_name(name) => check(name, *operator, value),
        ([value, name], [operator]) if is_name(name) => check(name, operator.flipped(), value),
        ([low, name, high], [first, second]) if is_name(name) => {
            check(name, first.flipped(), low) && check(name, *second, high)
        }
        _ => false,
    }
}

/// A feature's value in the units [`actual`] reports.
fn parse_value(name: &str, value: &str) -> Option<f32> {
    let value = value.trim();
    match name {
        "width" | "height" => parse_length(value),
        "aspect-ratio" => parse_ratio(value),
        "resolution" => parse_resolution(value),
        "-webkit-device-pixel-ratio" | "color" => value.parse().ok(),
        _ => None,
    }
}

/// A length in pixels: `px`, or `em`/`rem` against the initial 16px font
/// size (a media query has no element to inherit from); `0` may be
/// unitless. Other units are unsupported.
fn parse_length(value: &str) -> Option<f32> {
    if value == "0" {
        return Some(0.0);
    }
    let (number, scale) = if let Some(number) = value.strip_suffix("rem") {
        (number, 16.0)
    } else if let Some(number) = value.strip_suffix("em") {
        (number, 16.0)
    } else {
        (value.strip_suffix("px")?, 1.0)
    };
    number
        .trim()
        .parse::<f32>()
        .ok()
        .map(|number| number * scale)
}

/// `width / height` (or a bare number); both sides positive.
fn parse_ratio(value: &str) -> Option<f32> {
    let (width, height) = match value.split_once('/') {
        Some((width, height)) => (
            width.trim().parse::<f32>().ok()?,
            height.trim().parse::<f32>().ok()?,
        ),
        None => (value.parse::<f32>().ok()?, 1.0),
    };
    (width > 0.0 && height > 0.0).then(|| width / height)
}

/// Dots per CSS pixel: `dppx`/`x`, `dpi` (96 per pixel) or `dpcm`.
fn parse_resolution(value: &str) -> Option<f32> {
    let (number, per_dppx) = if let Some(number) = value.strip_suffix("dppx") {
        (number, 1.0)
    } else if let Some(number) = value.strip_suffix("dpcm") {
        (number, 96.0 / 2.54)
    } else if let Some(number) = value.strip_suffix("dpi") {
        (number, 96.0)
    } else {
        (value.strip_suffix('x')?, 1.0)
    };
    number
        .trim()
        .parse::<f32>()
        .ok()
        .map(|number| number / per_dppx)
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
    fn orientation_exact_sizes_and_aspect_ratio() {
        let tall = at(400.0, 600.0);
        assert!(matches("(orientation: portrait)", &tall));
        assert!(!matches("(orientation: landscape)", &tall));
        assert!(matches("(orientation: landscape)", &at(800.0, 600.0)));
        assert!(matches("(width: 400px) and (height: 600px)", &tall));
        assert!(!matches("(width: 401px)", &tall));
        assert!(matches("(aspect-ratio: 2/3)", &tall));
        assert!(matches(
            "(min-aspect-ratio: 1/2) and (max-aspect-ratio: 1 / 1)",
            &tall
        ));
        assert!(!matches("(min-aspect-ratio: 16/9)", &tall));
        assert!(!matches("(aspect-ratio: 0/0)", &tall));
        assert!(!matches("(min-aspect-ratio: 0/1)", &tall), "degenerate");
    }

    #[test]
    fn em_and_rem_lengths_use_the_initial_font_size() {
        let vp = at(400.0, 600.0);
        assert!(matches("(min-width: 25em)", &vp));
        assert!(!matches("(min-width: 25.1rem)", &vp));
        assert!(matches("(max-width: 30em)", &vp));
    }

    #[test]
    fn range_syntax() {
        let vp = at(400.0, 600.0);
        assert!(matches("(width >= 400px)", &vp));
        assert!(!matches("(width > 400px)", &vp));
        assert!(matches("(width < 401px)", &vp));
        assert!(matches("(400px <= width)", &vp));
        assert!(matches("(300px <= width)", &vp));
        assert!(!matches("(500px <= width)", &vp));
        assert!(!matches("(400px < width)", &vp));
        assert!(matches("(300px < width <= 400px)", &vp));
        assert!(!matches("(300px < width < 400px)", &vp));
        assert!(matches("(500px > height > 0)", &at(400.0, 100.0)));
        assert!(matches("(height = 600px)", &vp));
        assert!(!matches("(colour >= 1px)", &vp));
        assert!(matches("screen and (width >= 25em)", &vp));
        assert!(!matches("screen and (width >= 30em)", &vp));
    }

    #[test]
    fn interaction_and_preference_features_describe_a_desktop_window() {
        let vp = at(400.0, 600.0);
        assert!(matches("(hover: hover) and (pointer: fine)", &vp));
        assert!(matches("(hover)", &vp));
        assert!(!matches("(hover: none)", &vp));
        assert!(matches("(any-hover: hover) and (any-pointer: fine)", &vp));
        assert!(!matches("(pointer: coarse)", &vp));
        assert!(matches("(prefers-color-scheme: light)", &vp));
        assert!(!matches("(prefers-color-scheme: dark)", &vp));
        assert!(matches("(prefers-reduced-motion: no-preference)", &vp));
        assert!(!matches("(prefers-reduced-motion)", &vp));
        assert!(matches(
            "(min-resolution: 1dppx) and (max-resolution: 96dpi)",
            &vp
        ));
        assert!(!matches("(min-resolution: 2dppx)", &vp));
        assert!(matches("(min-resolution: 95dpi)", &vp));
        assert!(!matches("(min-resolution: 97dpi)", &vp));
        assert!(matches("(min-resolution: 37dpcm)", &vp));
        assert!(!matches("(min-resolution: 39dpcm)", &vp));
        assert!(matches("(color) and (min-color: 1)", &vp));
        assert!(matches("(-webkit-min-device-pixel-ratio: 1)", &vp));
        assert!(!matches("(-webkit-min-device-pixel-ratio: 1.5)", &vp));
    }

    #[test]
    fn unsupported_conditions_fail() {
        let vp = at(400.0, 600.0);
        assert!(!matches("(scan: interlace)", &vp));
        assert!(!matches("(min-width: 10vw)", &vp));
        assert!(!matches("(min-width)", &vp));
        assert!(!matches("(min-width: )", &vp));
        // An unknown type name is a failed alternative.
        assert!(!matches("reality", &vp));
        // Bare-and and empty parens are malformed.
        assert!(!matches("screen and", &vp));
        assert!(!matches("screen and ()", &vp));
    }
}
