//! Emoji presentation: whether a character should draw as a colour emoji
//! or as plain text (UTS #51).
//!
//! A character with the `Emoji_Presentation` property is an emoji by
//! default; any other is text by default. A variation selector after it
//! overrides that: U+FE0F (VS16) asks for emoji, U+FE0E (VS15) for text.
//! The selectors themselves are neither drawn nor measured.

/// `Emoji_Presentation=Yes`, as inclusive ranges.
///
/// Derived from the single code points that are fully qualified (or emoji
/// components) in Unicode's `emoji-test.txt` 12.0, plus the regional
/// indicators and the emoji added in 13.0–15.1 (in blocks reserved for
/// emoji). A character missing here draws as text unless VS16 follows it.
const EMOJI_PRESENTATION: &[(char, char)] = &[
    ('\u{231A}', '\u{231B}'),
    ('\u{23E9}', '\u{23EC}'),
    ('\u{23F0}', '\u{23F0}'),
    ('\u{23F3}', '\u{23F3}'),
    ('\u{25FD}', '\u{25FE}'),
    ('\u{2614}', '\u{2615}'),
    ('\u{2648}', '\u{2653}'),
    ('\u{267F}', '\u{267F}'),
    ('\u{2693}', '\u{2693}'),
    ('\u{26A1}', '\u{26A1}'),
    ('\u{26AA}', '\u{26AB}'),
    ('\u{26BD}', '\u{26BE}'),
    ('\u{26C4}', '\u{26C5}'),
    ('\u{26CE}', '\u{26CE}'),
    ('\u{26D4}', '\u{26D4}'),
    ('\u{26EA}', '\u{26EA}'),
    ('\u{26F2}', '\u{26F3}'),
    ('\u{26F5}', '\u{26F5}'),
    ('\u{26FA}', '\u{26FA}'),
    ('\u{26FD}', '\u{26FD}'),
    ('\u{2705}', '\u{2705}'),
    ('\u{270A}', '\u{270B}'),
    ('\u{2728}', '\u{2728}'),
    ('\u{274C}', '\u{274C}'),
    ('\u{274E}', '\u{274E}'),
    ('\u{2753}', '\u{2755}'),
    ('\u{2757}', '\u{2757}'),
    ('\u{2795}', '\u{2797}'),
    ('\u{27B0}', '\u{27B0}'),
    ('\u{27BF}', '\u{27BF}'),
    ('\u{2B1B}', '\u{2B1C}'),
    ('\u{2B50}', '\u{2B50}'),
    ('\u{2B55}', '\u{2B55}'),
    ('\u{1F004}', '\u{1F004}'),
    ('\u{1F0CF}', '\u{1F0CF}'),
    ('\u{1F18E}', '\u{1F18E}'),
    ('\u{1F191}', '\u{1F19A}'),
    ('\u{1F1E6}', '\u{1F1FF}'),
    ('\u{1F201}', '\u{1F201}'),
    ('\u{1F21A}', '\u{1F21A}'),
    ('\u{1F22F}', '\u{1F22F}'),
    ('\u{1F232}', '\u{1F236}'),
    ('\u{1F238}', '\u{1F23A}'),
    ('\u{1F250}', '\u{1F251}'),
    ('\u{1F300}', '\u{1F320}'),
    ('\u{1F32D}', '\u{1F335}'),
    ('\u{1F337}', '\u{1F37C}'),
    ('\u{1F37E}', '\u{1F393}'),
    ('\u{1F3A0}', '\u{1F3CA}'),
    ('\u{1F3CF}', '\u{1F3D3}'),
    ('\u{1F3E0}', '\u{1F3F0}'),
    ('\u{1F3F4}', '\u{1F3F4}'),
    ('\u{1F3F8}', '\u{1F43E}'),
    ('\u{1F440}', '\u{1F440}'),
    ('\u{1F442}', '\u{1F4FC}'),
    ('\u{1F4FF}', '\u{1F53D}'),
    ('\u{1F54B}', '\u{1F54E}'),
    ('\u{1F550}', '\u{1F567}'),
    ('\u{1F57A}', '\u{1F57A}'),
    ('\u{1F595}', '\u{1F596}'),
    ('\u{1F5A4}', '\u{1F5A4}'),
    ('\u{1F5FB}', '\u{1F64F}'),
    ('\u{1F680}', '\u{1F6C5}'),
    ('\u{1F6CC}', '\u{1F6CC}'),
    ('\u{1F6D0}', '\u{1F6D2}'),
    ('\u{1F6D5}', '\u{1F6D7}'),
    ('\u{1F6DC}', '\u{1F6DF}'),
    ('\u{1F6EB}', '\u{1F6EC}'),
    ('\u{1F6F4}', '\u{1F6FC}'),
    ('\u{1F7E0}', '\u{1F7EB}'),
    ('\u{1F7F0}', '\u{1F7F0}'),
    ('\u{1F90C}', '\u{1F9FF}'),
    ('\u{1FA70}', '\u{1FAFF}'),
];

/// Text presentation selector (VS15).
const TEXT_SELECTOR: char = '\u{FE0E}';
/// Emoji presentation selector (VS16).
const EMOJI_SELECTOR: char = '\u{FE0F}';

/// Whether `character` is an emoji by default (`Emoji_Presentation`).
pub fn is_default_emoji(character: char) -> bool {
    EMOJI_PRESENTATION
        .binary_search_by(|&(first, last)| {
            if last < character {
                std::cmp::Ordering::Less
            } else if first > character {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// A variation selector (U+FE00–U+FE0F): it picks a glyph variant of the
/// character before it and is never drawn itself.
fn is_variation_selector(character: char) -> bool {
    ('\u{FE00}'..='\u{FE0F}').contains(&character)
}

/// `text`'s characters without variation selectors, each paired with
/// whether it should be drawn as an emoji.
pub(crate) fn presented(text: &str) -> impl Iterator<Item = (char, bool)> + '_ {
    let mut characters = text.chars().peekable();
    std::iter::from_fn(move || {
        loop {
            let character = characters.next()?;
            if is_variation_selector(character) {
                continue;
            }
            let emoji = match characters.peek() {
                Some(&EMOJI_SELECTOR) => true,
                Some(&TEXT_SELECTOR) => false,
                _ => is_default_emoji(character),
            };
            return Some((character, emoji));
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_presentation_follows_the_emoji_data() {
        assert!(is_default_emoji('📦'));
        assert!(is_default_emoji('\u{26A1}')); // ⚡
        assert!(is_default_emoji('\u{1F1FA}')); // regional indicator U
        assert!(is_default_emoji('\u{1FAE0}')); // 🫠, Unicode 14
        assert!(!is_default_emoji('\u{2699}')); // ⚙
        assert!(!is_default_emoji('#'));
        assert!(!is_default_emoji('A'));
    }

    #[test]
    fn selectors_choose_the_presentation_and_vanish() {
        let presented: Vec<(char, bool)> = presented("\u{2699}\u{FE0F}\u{26A1}\u{FE0E}a").collect();
        assert_eq!(
            presented,
            [('\u{2699}', true), ('\u{26A1}', false), ('a', false)]
        );
    }
}
