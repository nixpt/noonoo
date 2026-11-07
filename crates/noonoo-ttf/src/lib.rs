//! # noonoo-ttf
//!
//! A dependency-free **TrueType / OpenType parser**: font metrics and the
//! character map. It reads the sfnt container and the tables that describe how a
//! font *measures* — `head` (units per em), `maxp` (glyph count), `hhea`/`hmtx`
//! (advances and vertical metrics), and `cmap` (character → glyph).
//!
//! This is a **format parser**, in the same spirit as `noonoo-xml` and
//! `noonoo-svg`: small, spec-shaped, and owned. It is deliberately *not* a full
//! shaping engine — ligatures, contextual substitution (GSUB), complex scripts,
//! and BiDi are the 100k+ LOC swamp the project keeps borrowed behind
//! [`noonoo_text::Shaper`]. What this gives that layer is honest numbers: real
//! advance widths, line metrics, and the legacy `kern` pair adjustments, instead
//! of a fixed per-character guess.
//!
//! Metrics tables are shared by TrueType (`glyf`) and CFF/OpenType (`OTTO`), so
//! measurement works for both. TrueType glyph *outlines* (`loca`/`glyf`, simple
//! and composite) are parsed too — [`Font::outline`] returns contours in font
//! units, and [`Outline::flatten`] turns them into polygons for the paint stream.
//! CFF outlines are not parsed.
//!
//! [`OutlineGlyphs`] implements `noonoo_paint::Glyphs`, drawing a run as filled
//! glyph outlines — pair it with [`FontShaper`] over the same font to render
//! vector text through the normal paint stream.
//!
//! ```
//! # fn demo(bytes: &[u8]) -> noonoo_parse::ParseResult<()> {
//! let font = noonoo_ttf::Font::parse(bytes)?;
//! let _em = font.units_per_em();
//! if let Some(glyph) = font.glyph_for_char('A') {
//!     let _advance = font.advance(glyph);
//! }
//! # Ok(()) }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod cmap;
mod glyf;
mod kern;
mod reader;
mod render;
mod shaper;

pub use glyf::{Contour, Outline, Point};
pub use render::OutlineGlyphs;
pub use shaper::FontShaper;

use noonoo_parse::ParseResult;
use reader::{Reader, be_u16};

use cmap::Cmap;

/// A parsed font: metrics and the character map.
pub struct Font {
    data: Vec<u8>,
    units_per_em: u16,
    num_glyphs: u16,
    ascender: i16,
    descender: i16,
    line_gap: i16,
    num_h_metrics: u16,
    hmtx_offset: usize,
    /// Byte offsets into `glyf`, one per glyph plus an end sentinel.
    loca: Vec<u32>,
    glyf_offset: usize,
    /// Legacy pair-kerning adjustments, sorted by `(left, right)`.
    kern: Vec<kern::Pair>,
    cmap: Option<Cmap>,
}

impl Font {
    /// Parse a TrueType/OpenType font from its bytes.
    pub fn parse(data: &[u8]) -> ParseResult<Font> {
        let mut reader = Reader::new(data);
        let scaler = reader.u32()?;
        // 0x00010000 TrueType, 'OTTO' CFF/OpenType, 'true' legacy.
        if !matches!(scaler, 0x0001_0000 | 0x4F54_544F | 0x7472_7565) {
            return Err(reader.error("not an sfnt (TrueType/OpenType) font"));
        }
        let num_tables = reader.u16()?;
        reader.skip(6)?; // searchRange, entrySelector, rangeShift

        let mut tables: Vec<([u8; 4], usize, usize)> = Vec::new();
        for _ in 0..num_tables {
            let tag = reader.tag()?;
            let _checksum = reader.u32()?;
            let offset = reader.u32()? as usize;
            let length = reader.u32()? as usize;
            tables.push((tag, offset, length));
        }
        let table = |tag: &[u8; 4]| {
            tables
                .iter()
                .find(|(name, _, _)| name == tag)
                .map(|(_, offset, _)| *offset)
        };

        // `head` — units per em (and a magic-number sanity check).
        let head = table(b"head").ok_or_else(|| reader.error("missing head table"))?;
        reader.seek(head);
        reader.skip(12)?; // version, fontRevision, checkSumAdjustment
        let magic = reader.u32()?;
        if magic != 0x5F0F_3CF5 {
            return Err(reader.error("bad head magic number"));
        }
        reader.skip(2)?; // flags
        let units_per_em = reader.u16()?;

        // `maxp` — glyph count.
        let maxp = table(b"maxp").ok_or_else(|| reader.error("missing maxp table"))?;
        reader.seek(maxp);
        reader.skip(4)?; // version
        let num_glyphs = reader.u16()?;

        // `hhea` — vertical metrics and the number of long metrics.
        let hhea = table(b"hhea").ok_or_else(|| reader.error("missing hhea table"))?;
        reader.seek(hhea);
        reader.skip(4)?; // version
        let ascender = reader.i16()?;
        let descender = reader.i16()?;
        let line_gap = reader.i16()?;
        reader.seek(hhea + 34);
        let num_h_metrics = reader.u16()?;
        if num_h_metrics == 0 {
            return Err(reader.error("hhea declares no horizontal metrics"));
        }

        let hmtx_offset = table(b"hmtx").ok_or_else(|| reader.error("missing hmtx table"))?;

        // `head.indexToLocFormat` chooses the `loca` offset size; `loca` gives
        // each glyph's byte range inside `glyf`.
        reader.seek(head + 50);
        let index_to_loc_format = reader.i16()?;
        let glyf_offset = table(b"glyf").unwrap_or(0);
        let mut loca = Vec::new();
        if let Some(loca_offset) = table(b"loca") {
            let mut loca_reader = Reader::new(data);
            loca_reader.seek(loca_offset);
            for _ in 0..num_glyphs as usize + 1 {
                let offset = if index_to_loc_format == 0 {
                    loca_reader.u16()? as u32 * 2
                } else {
                    loca_reader.u32()?
                };
                loca.push(offset);
            }
        }

        let cmap = match table(b"cmap") {
            Some(offset) => cmap::parse(data, offset)?,
            None => None,
        };
        let kern = match table(b"kern") {
            Some(offset) => kern::parse(data, offset)?,
            None => Vec::new(),
        };

        Ok(Font {
            data: data.to_vec(),
            units_per_em,
            num_glyphs,
            ascender,
            descender,
            line_gap,
            num_h_metrics,
            hmtx_offset,
            loca,
            glyf_offset,
            kern,
            cmap,
        })
    }

    /// The font's design grid (`head.unitsPerEm`), typically 1000 or 2048.
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// The number of glyphs (`maxp.numGlyphs`).
    pub fn glyph_count(&self) -> u16 {
        self.num_glyphs
    }

    /// The typographic ascender, in font units (`hhea`).
    pub fn ascender(&self) -> i16 {
        self.ascender
    }

    /// The typographic descender (negative), in font units.
    pub fn descender(&self) -> i16 {
        self.descender
    }

    /// The line gap, in font units.
    pub fn line_gap(&self) -> i16 {
        self.line_gap
    }

    /// The glyph a character maps to, if the font has a `cmap` entry for it.
    pub fn glyph_for_char(&self, character: char) -> Option<u16> {
        self.cmap
            .as_ref()
            .and_then(|cmap| cmap.glyph(character as u32))
    }

    /// The advance width of `glyph`, in font units.
    ///
    /// Glyphs past the last long metric share its advance, per the `hmtx` rule.
    pub fn advance(&self, glyph: u16) -> u16 {
        let index = (glyph as usize).min(self.num_h_metrics as usize - 1);
        be_u16(&self.data, self.hmtx_offset + index * 4).unwrap_or(0)
    }

    /// The advance width of `character`, in font units, falling back to
    /// `.notdef` (glyph 0) when the font does not map it.
    pub fn advance_of_char(&self, character: char) -> u16 {
        self.glyph_for_char(character)
            .map_or_else(|| self.advance(0), |glyph| self.advance(glyph))
    }

    /// The glyph a character maps to, or `.notdef` (glyph 0) when it does not.
    pub fn glyph_for_char_or_notdef(&self, character: char) -> u16 {
        self.glyph_for_char(character).unwrap_or(0)
    }

    /// The legacy pair-kerning adjustment between two glyphs, in font units.
    ///
    /// Sums every matching format-0 pair (a font with a single `kern` subtable —
    /// the common case — has at most one per pair). Zero when nothing matches, and
    /// zero for a font with no `kern` table.
    pub fn kern(&self, left: u16, right: u16) -> i16 {
        let key = (left, right);
        let start = self.kern.partition_point(|&(l, r, _)| (l, r) < key);
        let mut total = 0i32;
        let mut index = start;
        while index < self.kern.len() && (self.kern[index].0, self.kern[index].1) == key {
            total += self.kern[index].2 as i32;
            index += 1;
        }
        total as i16
    }
}

impl core::fmt::Debug for Font {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Font")
            .field("units_per_em", &self.units_per_em)
            .field("num_glyphs", &self.num_glyphs)
            .field("ascender", &self.ascender)
            .field("descender", &self.descender)
            .field("line_gap", &self.line_gap)
            .finish_non_exhaustive()
    }
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests;
