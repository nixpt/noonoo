//! Where a line may break inside a word, as Chromium breaks it.
//!
//! Between two printable ASCII characters Chromium consults its own pair
//! table, not UAX #14: it breaks after `?` but not `!`, after a hyphen
//! before a letter, and never at a `/`. [`ASCII`] is that table, measured
//! by laying out every pair in a 1px-wide box in headless Chromium, between
//! letters, between digits and on its own. The pair alone decides, but for
//! a hyphen before a digit (see [`breaks_between`]). Beyond ASCII Chromium
//! asks ICU; the classes below are the ones measured from it. They never
//! break where Chromium doesn't, but miss some breaks it makes: after ASCII
//! punctuation before a non-ASCII letter or symbol (`?é`, `)−`), and around
//! the minus, per mille and degree signs.

/// Bit `b - '!'` of row `a - '!'`: Chromium breaks between `a` and `b`,
/// both printable ASCII.
const ASCII: [u128; 94] = [
    0x00000000040000000400000008000080, // !
    0x00000000040000000400000008000080, // "
    0x00000000040000000400000008000080, // #
    0x00000000000000000000000000000000, // $
    0x00000000040000000400000008000080, // %
    0x00000000040000000400000008000080, // &
    0x00000000000000000000000000000000, // '
    0x00000000000000000000000000000000, // (
    0x00000000040000000400000008000080, // )
    0x00000000040000000400000008000080, // *
    0x00000000040000000400000008000080, // +
    0x00000000040000000400000008000080, // ,
    0x000000002fffffffefffffffb9ff96f6, // -
    0x00000000040000000400000008000080, // .
    0x00000000000000000000000000000000, // /
    0x00000000000000000000000000000000, // 0
    0x00000000000000000000000000000000, // 1
    0x00000000000000000000000000000000, // 2
    0x00000000000000000000000000000000, // 3
    0x00000000000000000000000000000000, // 4
    0x00000000000000000000000000000000, // 5
    0x00000000000000000000000000000000, // 6
    0x00000000000000000000000000000000, // 7
    0x00000000000000000000000000000000, // 8
    0x00000000000000000000000000000000, // 9
    0x00000000040000000400000008000080, // :
    0x00000000040000000400000008000080, // ;
    0x00000000000000000000000000000000, // <
    0x00000000040000000400000008000080, // =
    0x00000000040000000400000008000080, // >
    0x000000002fffffffefffffffb9ff96bc, // ?
    0x00000000000000000000000000000000, // @
    0x00000000000000000000000000000000, // A
    0x00000000000000000000000000000000, // B
    0x00000000000000000000000000000000, // C
    0x00000000000000000000000000000000, // D
    0x00000000000000000000000000000000, // E
    0x00000000000000000000000000000000, // F
    0x00000000000000000000000000000000, // G
    0x00000000000000000000000000000000, // H
    0x00000000000000000000000000000000, // I
    0x00000000000000000000000000000000, // J
    0x00000000000000000000000000000000, // K
    0x00000000000000000000000000000000, // L
    0x00000000000000000000000000000000, // M
    0x00000000000000000000000000000000, // N
    0x00000000000000000000000000000000, // O
    0x00000000000000000000000000000000, // P
    0x00000000000000000000000000000000, // Q
    0x00000000000000000000000000000000, // R
    0x00000000000000000000000000000000, // S
    0x00000000000000000000000000000000, // T
    0x00000000000000000000000000000000, // U
    0x00000000000000000000000000000000, // V
    0x00000000000000000000000000000000, // W
    0x00000000000000000000000000000000, // X
    0x00000000000000000000000000000000, // Y
    0x00000000000000000000000000000000, // Z
    0x00000000000000000000000000000000, // [
    0x00000000040000000400000008000080, // \
    0x00000000040000000400000008000080, // ]
    0x00000000000000000000000000000000, // ^
    0x00000000000000000000000000000000, // _
    0x00000000000000000000000000000000, // `
    0x00000000000000000000000000000000, // a
    0x00000000000000000000000000000000, // b
    0x00000000000000000000000000000000, // c
    0x00000000000000000000000000000000, // d
    0x00000000000000000000000000000000, // e
    0x00000000000000000000000000000000, // f
    0x00000000000000000000000000000000, // g
    0x00000000000000000000000000000000, // h
    0x00000000000000000000000000000000, // i
    0x00000000000000000000000000000000, // j
    0x00000000000000000000000000000000, // k
    0x00000000000000000000000000000000, // l
    0x00000000000000000000000000000000, // m
    0x00000000000000000000000000000000, // n
    0x00000000000000000000000000000000, // o
    0x00000000000000000000000000000000, // p
    0x00000000000000000000000000000000, // q
    0x00000000000000000000000000000000, // r
    0x00000000000000000000000000000000, // s
    0x00000000000000000000000000000000, // t
    0x00000000000000000000000000000000, // u
    0x00000000000000000000000000000000, // v
    0x00000000000000000000000000000000, // w
    0x00000000000000000000000000000000, // x
    0x00000000000000000000000000000000, // y
    0x00000000000000000000000000000000, // z
    0x00000000000000000000000000000000, // {
    0x00000000040000000400000008000080, // |
    0x00000000040000000400000008000080, // }
    0x00000000040000000400000008000080, // ~
];

/// Whether a line may break between `a` and `b`, `before` being the
/// character before `a` (`None` at the word's start).
pub(crate) fn breaks_between(before: Option<char>, a: char, b: char) -> bool {
    if crate::is_breaking_space(a) {
        return true;
    }
    if a == '\u{200b}' {
        return !matches!(b, '\u{200b}' | '\u{ad}' | '\u{2060}');
    }
    if let (Some(row), Some(column)) = (ascii_index(a), ascii_index(b)) {
        // A hyphen breaks before a digit only after a letter or digit.
        if a == '-' && b.is_ascii_digit() {
            return before.is_some_and(|c| c.is_ascii_alphanumeric());
        }
        return ASCII[row] >> column & 1 == 1;
    }
    if is_long_dash(a) && is_long_dash(b) {
        return false;
    }
    // CJK keeps a per mille or degree sign after it.
    if breaks_around(a) && matches!(b, '\u{b0}' | '\u{2030}') {
        return is_long_dash(a);
    }
    if breaks_after(a) && starts(b) {
        // A hyphen at the word's start breaks only before a digit, an
        // opening mark or what breaks on both sides.
        return !is_hyphen(a)
            || before.is_some_and(|c| !c.is_whitespace())
            || b.is_numeric()
            || breaks_around(b)
            || matches!(
                b,
                '(' | '[' | '{' | '\u{a1}' | '\u{bf}' | '\u{b0}' | '\u{2030}' | '\u{2212}'
            );
    }
    breaks_around(b) && !a.is_whitespace() && !holds_next(a)
}

/// `c`'s row or column in [`ASCII`].
fn ascii_index(c: char) -> Option<usize> {
    matches!(c, '!'..='~').then(|| c as usize - '!' as usize)
}

/// Characters a line breaks after, before a character that may start a
/// line ([`starts`]): hyphens and dashes (not U+2011, the no-break hyphen),
/// the ellipsis, the interrobang and doubled question and exclamation marks,
/// CJK commas and stops, and those that break on both sides.
fn breaks_after(c: char) -> bool {
    is_hyphen(c)
        || matches!(
            c,
            '\u{2026}' | '\u{203d}' | '\u{2047}'
                ..='\u{2049}' | '\u{3001}' | '\u{3002}' | '\u{ff01}' | '\u{ff0c}'
        )
        || breaks_around(c)
}

/// The hyphens a line breaks after: U+2010 HYPHEN, the figure dash and the
/// en dash.
fn is_hyphen(c: char) -> bool {
    matches!(c, '\u{2010}' | '\u{2012}' | '\u{2013}')
}

/// Whether `c` may start a line after one of [`breaks_after`]'s marks: an
/// ASCII letter, digit or opening bracket, or any other character but
/// dashes, the ellipsis, quotes, marks that close a sentence, and the
/// spaces and joiners that don't break.
fn starts(c: char) -> bool {
    if c.is_ascii() {
        return c.is_ascii_alphanumeric() || matches!(c, '(' | '[' | '{');
    }
    !c.is_whitespace()
        && !matches!(
            c,
            '\u{2010}'..='\u{2013}'
                | '\u{2026}'
                | '\u{2018}'..='\u{201d}'
                | '\u{ab}'
                | '\u{bb}'
                | '\u{203d}'
                | '\u{2047}'..='\u{2049}'
                | '\u{a0}'
                | '\u{202f}'
                | '\u{2060}'
                | '\u{200b}'
                | '\u{ad}'
                | '\u{3001}'
                | '\u{3002}'
                | '\u{ff01}'
                | '\u{ff0c}'
        )
}

/// Characters a line breaks before as well as after: the em dash and its
/// two- and three-em forms, and CJK ideographs and kana.
fn breaks_around(c: char) -> bool {
    is_long_dash(c)
        || matches!(
            c,
            '\u{3041}'..='\u{30ff}'
                | '\u{3400}'..='\u{4dbf}'
                | '\u{4e00}'..='\u{9fff}'
                | '\u{f900}'..='\u{faff}'
        )
}

/// The em dash and its two- and three-em forms, which don't break between
/// one another.
fn is_long_dash(c: char) -> bool {
    matches!(c, '\u{2014}' | '\u{2e3a}' | '\u{2e3b}')
}

/// A character a line doesn't break after before an em dash or CJK: an
/// opening bracket or quote, a closing quote, a no-break hyphen or space,
/// a word joiner or soft hyphen, the inverted marks and the minus sign.
fn holds_next(c: char) -> bool {
    matches!(
        c,
        '(' | '[' | '{' | '"' | '\'' | '\u{2018}'
            ..='\u{201d}'
                | '\u{ab}'
                | '\u{bb}'
                | '\u{2011}'
                | '\u{a0}'
                | '\u{202f}'
                | '\u{2060}'
                | '\u{ad}'
                | '\u{a1}'
                | '\u{bf}'
                | '\u{2212}'
    )
}
