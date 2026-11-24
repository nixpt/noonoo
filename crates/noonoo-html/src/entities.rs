//! HTML character references (HTML §13.2.5.72 on).
//!
//! Decoded wherever text flows: element text, the text of `title` and
//! `textarea`, and attribute values. The other raw-text elements (`script`,
//! `style`, ...) keep theirs: their content is data.
//!
//! The named references are the standard's whole table. It can't be cut
//! down: which reference a source without a `;` means depends on every
//! longer name (`&notit;` is `&not` and text, `&notin;` a reference).

use crate::named::NAMED;

/// The `windows-1252` glyphs for numeric references to C1 (`0x80..0x9F`).
const WINDOWS_C1: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

/// Decode the character references in text.
pub(crate) fn decode(source: &str) -> String {
    decode_in(source, false)
}

/// Decode the character references in an attribute's value: a named one
/// without its `;` is left alone before `=` or a letter or digit, as in
/// `?a=1&copy=2`.
pub(crate) fn decode_attribute(source: &str) -> String {
    decode_in(source, true)
}

fn decode_in(source: &str, attribute: bool) -> String {
    let mut out = String::with_capacity(source.len());
    let mut rest = source;
    while let Some(ampersand) = rest.find('&') {
        out.push_str(&rest[..ampersand]);
        let tail = &rest[ampersand + 1..];
        let used = match tail.strip_prefix('#') {
            Some(number) => numeric(number, &mut out).map(|used| used + 1),
            None => named(tail, attribute, &mut out),
        };
        if used.is_none() {
            out.push('&');
        }
        rest = &tail[used.unwrap_or(0)..];
    }
    out.push_str(rest);
    out
}

/// Decode the named reference that `tail` (after the `&`) starts with into
/// `out`: how much of `tail` it took.
fn named(tail: &str, attribute: bool, out: &mut String) -> Option<usize> {
    let bytes = tail.as_bytes();
    // The names that start with the first `length` bytes of `tail`, and the
    // longest that is all of them.
    let mut names = NAMED;
    let mut found = None;
    for (length, byte) in bytes.iter().enumerate() {
        let at = |name: &str| name.as_bytes().get(length).copied();
        let start = names.partition_point(|(name, _)| at(name) < Some(*byte));
        let end = names.partition_point(|(name, _)| at(name) <= Some(*byte));
        names = &names[start..end];
        match names.first() {
            None => break,
            Some((name, text)) if name.len() == length + 1 => found = Some((length + 1, text)),
            Some(_) => {}
        }
    }
    let (used, text) = found?;
    if attribute
        && bytes[used - 1] != b';'
        && bytes
            .get(used)
            .is_some_and(|next| *next == b'=' || next.is_ascii_alphanumeric())
    {
        return None;
    }
    out.push_str(text);
    Some(used)
}

/// Decode the numeric reference that `tail` (after the `&#`) starts with
/// into `out`: how much of `tail` it took.
fn numeric(tail: &str, out: &mut String) -> Option<usize> {
    let (radix, marker) = match tail.as_bytes().first() {
        Some(b'x' | b'X') => (16, 1),
        _ => (10, 0),
    };
    let digits = &tail[marker..];
    let end = digits
        .find(|c: char| !c.is_digit(radix))
        .unwrap_or(digits.len());
    if end == 0 {
        return None;
    }
    let code = digits[..end].chars().fold(0u32, |code, c| {
        code.saturating_mul(radix)
            .saturating_add(c.to_digit(radix).unwrap_or(0))
    });
    out.push(character(code));
    let semicolon = digits[end..].starts_with(';');
    Some(marker + end + usize::from(semicolon))
}

/// The character a numeric reference points to: the replacement character
/// for what is none.
fn character(code: u32) -> char {
    match code {
        0x00 => char::REPLACEMENT_CHARACTER,
        0x80..=0x9F => WINDOWS_C1[(code - 0x80) as usize],
        _ => char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_and_whole() {
        assert_eq!(NAMED.len(), 2231);
        assert!(NAMED.windows(2).all(|pair| pair[0].0 < pair[1].0));
        for &(name, text) in NAMED {
            assert_eq!(decode(&format!("&{name}")), text, "{name}");
            // Each name without `;` is also there with it.
            let full = format!("{name};");
            assert!(name.ends_with(';') || NAMED.iter().any(|(other, _)| *other == full));
        }
    }

    #[test]
    fn named_entities_decode() {
        assert_eq!(decode("&amp;&eacute;&#x1F600;"), "&é😀");
        assert_eq!(decode("&eacute;"), "é");
        assert_eq!(decode("&lt;&gt;&quot;&apos;"), "<>\"'");
        assert_eq!(decode("&nbsp;"), "\u{00A0}");
        assert_eq!(decode("&hellip;"), "\u{2026}");
        assert_eq!(decode("&mdash;&ndash;"), "\u{2014}\u{2013}");
        assert_eq!(decode("&check;"), "\u{2713}");
        assert_eq!(decode("&pound;"), "\u{00A3}");
        // Two characters, and outside the BMP.
        assert_eq!(decode("&acE;&Kopf;"), "\u{223E}\u{333}\u{1D542}");
    }

    #[test]
    fn the_longest_name_counts() {
        assert_eq!(decode("&copy 1 &copy1 &copy"), "© 1 ©1 ©");
        assert_eq!(decode("&notit; &notin; &not;in;"), "¬it; ∉ ¬in;");
        assert_eq!(decode("&AMP&LT&ampamp;"), "&<&amp;");
        // Only some names work without the semicolon, and case matters.
        assert_eq!(decode("&hellip &Eacute &EACUTE;"), "&hellip É &EACUTE;");
    }

    #[test]
    fn an_attribute_keeps_what_may_be_a_query() {
        assert_eq!(decode_attribute("?a=1&copy=2&lt3"), "?a=1&copy=2&lt3");
        assert_eq!(decode_attribute("&notit;"), "&notit;");
        assert_eq!(decode_attribute("&copy;=&copy &copy"), "©=© ©");
        assert_eq!(decode_attribute("&copy-&copyé"), "©-©é");
        assert_eq!(decode_attribute("&#65=1&#x42c"), "A=1Ь");
    }

    #[test]
    fn unsigned_ampersands_stay_literal() {
        assert_eq!(decode("&L"), "&L");
        assert_eq!(decode("&the &amp; tail"), "&the & tail");
        assert_eq!(decode("&#"), "&#");
        assert_eq!(decode("&#;"), "&#;");
        assert_eq!(decode("&#x"), "&#x");
        assert_eq!(decode("&#xg;"), "&#xg;");
        assert_eq!(decode("&&"), "&&");
        assert_eq!(decode("a&b"), "a&b");
        assert_eq!(decode("&é;"), "&é;");
    }

    #[test]
    fn numeric_references_decode() {
        assert_eq!(decode("&#65;&#x42;&#X66;"), "ABf");
        assert_eq!(decode("&#65&#x42&#X66 &#67z"), "ABf Cz");
        // C1 maps to windows-1252 glyphs.
        assert_eq!(decode("&#x80;"), "\u{20AC}");
        assert_eq!(decode("&#0;"), "\u{FFFD}");
        assert_eq!(decode("&#xD800;"), "\u{FFFD}");
        assert_eq!(
            decode("&#x110000;&#99999999999999999999;"),
            "\u{FFFD}\u{FFFD}"
        );
        // A noncharacter is kept.
        assert_eq!(decode("&#xFFFE;"), "\u{FFFE}");
    }

    #[test]
    fn lowercase_ampersand_combo() {
        assert_eq!(decode("a&&b"), "a&&b");
        assert_eq!(decode("&the&the&the"), "&the&the&the");
    }
}
