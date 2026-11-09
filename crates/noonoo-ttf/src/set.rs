//! A set of font faces, and selection by family, weight, and style.
//!
//! This is the small, ownable slice of "font management": given the faces a host
//! has loaded, pick the one that best matches a requested `family`/`weight`/
//! `italic`. It does **not** enumerate or load system fonts — that is
//! `fontique`'s job, and stays borrowed (`docs/DEPENDENCY-EXIT.md`).

use crate::Font;

/// One registered face.
struct Face<'a> {
    family: String,
    weight: u16,
    italic: bool,
    font: &'a Font,
}

/// A collection of font faces selectable by family, weight, and style.
pub struct FontSet<'a> {
    faces: Vec<Face<'a>>,
    /// Faces tried, in order, for characters the selected face lacks.
    fallbacks: Vec<&'a Font>,
}

impl Default for FontSet<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> FontSet<'a> {
    /// An empty set.
    pub fn new() -> Self {
        Self {
            faces: Vec::new(),
            fallbacks: Vec::new(),
        }
    }

    /// Register `font` as a face of `family` at `weight` (100–900) and style.
    pub fn add(
        &mut self,
        family: impl Into<String>,
        weight: u16,
        italic: bool,
        font: &'a Font,
    ) -> &mut Self {
        self.faces.push(Face {
            family: family.into(),
            weight,
            italic,
            font,
        });
        self
    }

    /// The best face for a request: same family (case-insensitive; an empty
    /// `family` matches any), matching style preferred, then nearest weight.
    ///
    /// Returns `None` when no face matches the family.
    pub fn select(&self, family: &str, weight: u16, italic: bool) -> Option<&'a Font> {
        let family = family.trim();
        self.faces
            .iter()
            .filter(|face| family.is_empty() || face.family.eq_ignore_ascii_case(family))
            .min_by_key(|face| {
                (
                    face.italic != italic,
                    (i32::from(face.weight) - i32::from(weight)).abs(),
                )
            })
            .map(|face| face.font)
    }

    /// The best face for a request across a family *list*: the first family
    /// with any registered face wins, matching style preferred, then nearest
    /// weight (as [`FontSet::select`]). An empty list means "any face"; a
    /// list naming nothing registered gets a face registered under the
    /// empty family, if any.
    pub fn select_best(&self, families: &[String], weight: u16, italic: bool) -> Option<&'a Font> {
        if families.is_empty() {
            return self.select("", weight, italic);
        }
        families
            .iter()
            .find_map(|family| self.select(family, weight, italic))
            .or_else(|| self.select_universal(weight, italic))
    }

    /// The best face registered under the empty family name: the host's
    /// universal fallback, for a family list that names nothing registered.
    fn select_universal(&self, weight: u16, italic: bool) -> Option<&'a Font> {
        self.faces
            .iter()
            .filter(|face| face.family.trim().is_empty())
            .min_by_key(|face| {
                (
                    face.italic != italic,
                    (i32::from(face.weight) - i32::from(weight)).abs(),
                )
            })
            .map(|face| face.font)
    }

    /// Register `font` as a fallback: tried, after any fallback registered
    /// before it, for characters the selected face has no glyph for (symbols,
    /// arrows, other scripts).
    pub fn add_fallback(&mut self, font: &'a Font) -> &mut Self {
        self.fallbacks.push(font);
        self
    }

    /// The face to draw `character` with when `selected` was chosen for the
    /// text: `selected` itself when it has the glyph, else the first fallback
    /// that does, else `selected` (which draws its `.notdef`).
    pub fn face_for(&self, selected: &'a Font, character: char) -> &'a Font {
        if selected.glyph_for_char(character).is_some() {
            return selected;
        }
        self.fallbacks
            .iter()
            .copied()
            .find(|font| font.glyph_for_char(character).is_some())
            .unwrap_or(selected)
    }

    /// The face to draw `character` with when it should be an emoji: the
    /// first of `selected` and the fallbacks with a colour bitmap for it,
    /// else whatever [`FontSet::face_for`] picks.
    pub fn face_for_emoji(&self, selected: &'a Font, character: char) -> &'a Font {
        std::iter::once(selected)
            .chain(self.fallbacks.iter().copied())
            .find(|font| {
                font.glyph_for_char(character)
                    .is_some_and(|glyph| font.color_bitmap(glyph).is_some())
            })
            .unwrap_or_else(|| self.face_for(selected, character))
    }

    /// The face for `character` in the presentation [`crate::emoji`] chose.
    pub(crate) fn face_presented(
        &self,
        selected: &'a Font,
        character: char,
        emoji: bool,
    ) -> &'a Font {
        if emoji {
            self.face_for_emoji(selected, character)
        } else {
            self.face_for(selected, character)
        }
    }

    /// The number of registered faces.
    pub fn len(&self) -> usize {
        self.faces.len()
    }

    /// Whether no faces are registered.
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }
}
