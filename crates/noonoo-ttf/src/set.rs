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
}

impl Default for FontSet<'_> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> FontSet<'a> {
    /// An empty set.
    pub fn new() -> Self {
        Self { faces: Vec::new() }
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

    /// The number of registered faces.
    pub fn len(&self) -> usize {
        self.faces.len()
    }

    /// Whether no faces are registered.
    pub fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }
}
