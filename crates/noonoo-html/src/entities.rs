//! HTML character references.
//!
//! Text and attribute values arrive as escaped raw text (`&amp;`/`&#233;`),
//! and the references are data the renderer must see decoded — otherwise
//! `&amp;&eacute;&#x1F600;` paints literally (`&amp;&eacute;&#x1F600;` was
//! Finding 1 of `docs/REVIEW-2026-11-07.md`). This module decodes references
//! in the places textual content flows: element text (other than raw-text
//! content) and attribute values. Raw text (`script`, `style`, `textarea`,
//! `title`) keeps theirs: its content is *data*, not markup, and the spec
//! keeps references meaningful only in the text flow.
//!
//! The named table is deliberately the common set a configuration or UI
//! document uses. The full HTML5 table (2231 names) is the kind of borrowed
//! data dependency the project excludes, not a win. Two subset decisions are
//! spec-faithful: a named reference requires its closing `;`, and so does a
//! numeric reference.
//!
//! Numeric references for `U+0000` and surrogates map to the replacement
//! character, and the C1 range (`0x80..0x9F`) maps to its Windows-1252 glyph,
//! mirroring the spec's charset note.

/// The `windows-1252` glyphs for numeric references to C1 (`0x80..0x9F`).
const WINDOWS_C1: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}',
    '\u{0090}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

/// The character an unreachable numeric code maps to.
const REPLACEMENT: char = '\u{FFFD}';

/// The common named references.
const NAMED: &[(&str, &str)] = &[
    ("amp", "&"),
    ("apos", "'"),
    ("lt", "<"),
    ("gt", ">"),
    ("quot", "\""),
    ("nbsp", "\u{00A0}"),
    ("sect", "\u{00A7}"),
    ("para", "\u{00B6}"),
    ("middot", "\u{00B7}"),
    ("copy", "\u{00A9}"),
    ("reg", "\u{00AE}"),
    ("deg", "\u{00B0}"),
    ("pound", "\u{00A3}"),
    ("euro", "\u{20AC}"),
    ("times", "\u{00D7}"),
    ("divide", "\u{00F7}"),
    ("minus", "\u{2212}"),
    ("bull", "\u{2022}"),
    ("dagger", "\u{2020}"),
    ("Dagger", "\u{2021}"),
    ("eacute", "\u{00E9}"),
    ("hellip", "\u{2026}"),
    ("mdash", "\u{2014}"),
    ("ndash", "\u{2013}"),
    ("lsquo", "\u{2018}"),
    ("rsquo", "\u{2019}"),
    ("ldquo", "\u{201C}"),
    ("rdquo", "\u{201D}"),
    ("laquo", "\u{00AB}"),
    ("raquo", "\u{00BB}"),
    ("larr", "\u{2190}"),
    ("rarr", "\u{2192}"),
    ("uarr", "\u{2191}"),
    ("darr", "\u{2193}"),
    ("check", "\u{2713}"),
];

/// Decode every character reference in the slice.
pub(crate) fn decode(source: &str) -> String {
    if !source.contains('&') {
        return source.to_owned();
    }
    let mut out = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let mut position = 0usize;

    while position < bytes.len() {
        let ampersand = match source[position..].find('&') {
            Some(offset) => position + offset,
            None => {
                out.push_str(&source[position..]);
                break;
            }
        };
        out.push_str(&source[position..ampersand]);

        let tail = source.get(ampersand + 1..).unwrap_or("");
        match reference(tail) {
            Some((text, consumed)) => {
                out.push_str(&text);
                position = ampersand + 1 + consumed;
            }
            None => {
                out.push('&');
                position = ampersand + 1;
            }
        }
    }
    out
}

/// Decode one reference — the consumed count **excludes** the `&`.
type Decoded = (String, usize);

/// Try the named table, then numeric.
fn reference(tail: &str) -> Option<Decoded> {
    decode_named(tail).or_else(|| decode_numeric(tail))
}

/// A numeric reference like `&#65;` — or one that refuses the name table —
/// then the literal `&` is kept and the scanner advances one byte.
fn decode_named(tail: &str) -> Option<Decoded> {
    let mut best: Option<(usize, &str)> = None;
    for (name, glyphs) in NAMED {
        if tail.starts_with(name)
            && tail
                .get(name.len()..)
                .is_some_and(|rest| rest.starts_with(';'))
        {
            let consumed = name.len() + 1;
            if best.is_none_or(|(existing, _)| existing < consumed) {
                best = Some((consumed, glyphs));
            }
        }
    }
    best.map(|(consumed, glyphs)| (glyphs.to_owned(), consumed))
}

/// A numeric reference like `&#65;` (decimal) or `&#x41;`.
fn decode_numeric(tail: &str) -> Option<Decoded> {
    let after_hash = tail.strip_prefix('#')?;
    let (hex, digits) = if let Some(rest) = after_hash.strip_prefix('x') {
        (true, rest)
    } else if let Some(rest) = after_hash.strip_prefix('X') {
        (true, rest)
    } else {
        (false, after_hash)
    };

    let terminator = digits.find(';')?;
    let digits = digits.get(..terminator)?;
    if digits.is_empty() {
        return None;
    }
    let code = if hex {
        u32::from_str_radix(digits, 16).ok()?
    } else {
        digits.parse::<u32>().ok()?
    };
    // '#' plus (hex marker?) plus digits, and the `;` the caller checks next.
    let consumed = 2 + usize::from(hex) + digits.len();
    Some((safe_char(code)?.to_string(), consumed))
}

/// The character a numeric reference points to, with the spec's numeric
/// substitutions.
fn safe_char(code: u32) -> Option<char> {
    Some(match code {
        0x00 => REPLACEMENT,
        0xD800..=0xDFFF | 0xFFFE | 0xFFFF => REPLACEMENT,
        0x80..=0x9F => WINDOWS_C1[(code - 0x80) as usize],
        _ => char::from_u32(code)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn unsigned_ampersands_stay_literal() {
        assert_eq!(decode("&L"), "&L");
        assert_eq!(decode("&the &amp; tail"), "&the & tail");
        assert_eq!(decode("&#"), "&#");
        assert_eq!(decode("&#;"), "&#;");
        assert_eq!(decode("&#x"), "&#x");
        assert_eq!(decode("&&"), "&&");
        assert_eq!(decode("a&b"), "a&b");
        // `eacute` without a semicolon stays literal per the subset rule.
        assert_eq!(decode("&eacute"), "&eacute");
    }

    #[test]
    fn numeric_references_decode() {
        assert_eq!(decode("&#65;&#x42;&#X66;"), "ABf");
        // C1 maps to windows-1252 glyphs.
        assert_eq!(decode("&#x80;"), "\u{20AC}");
        assert_eq!(decode("&#0;"), "\u{FFFD}");
        assert_eq!(decode("&#xD800;"), "\u{FFFD}");
        assert_eq!(decode("&#xFFFE;"), "\u{FFFD}");
    }

    #[test]
    fn lowercase_ampersand_combo() {
        assert_eq!(decode("a&&b"), "a&&b");
        assert_eq!(decode("&the&the&the"), "&the&the&the");
    }
}
