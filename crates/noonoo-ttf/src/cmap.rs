//! `cmap` parsing: character code → glyph id.
//!
//! Two subtable formats cover almost every real font: format 4 (BMP, segmented)
//! and format 12 (full Unicode repertoire, grouped). We pick the best available
//! subtable and answer lookups from it.

use noonoo_parse::ParseResult;

use crate::reader::{Reader, be_u16};

/// A usable `cmap` subtable.
pub(crate) enum Cmap {
    /// Format 4: BMP, segmented.
    Format4(Format4),
    /// Format 12: grouped, full repertoire.
    Format12(Format12),
}

impl Cmap {
    /// The glyph for a Unicode code point, if the font maps it.
    pub(crate) fn glyph(&self, code: u32) -> Option<u16> {
        match self {
            Cmap::Format4(table) => (code <= 0xFFFF).then(|| table.glyph(code as u16)).flatten(),
            Cmap::Format12(table) => table.glyph(code),
        }
    }
}

/// A format 4 subtable (`platformID`/`encodingID` of a BMP encoding).
pub(crate) struct Format4 {
    end: Vec<u16>,
    start: Vec<u16>,
    delta: Vec<i16>,
    range_offset: Vec<u16>,
    /// Byte offset, within `subtable`, of the `idRangeOffset` array.
    range_offset_base: usize,
    /// The raw subtable, for `glyphIdArray` indirection.
    subtable: Vec<u8>,
}

impl Format4 {
    fn glyph(&self, code: u16) -> Option<u16> {
        // Binary search for the first segment whose `endCode >= code`.
        let mut lo = 0;
        let mut hi = self.end.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.end[mid] < code {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        let i = lo;
        if i >= self.end.len() || code < self.start[i] {
            return None;
        }

        let glyph = if self.range_offset[i] == 0 {
            (code as i32 + self.delta[i] as i32) as u16
        } else {
            let address = self.range_offset_base
                + 2 * i
                + self.range_offset[i] as usize
                + 2 * (code - self.start[i]) as usize;
            let raw = be_u16(&self.subtable, address)?;
            if raw == 0 {
                return None;
            }
            (raw as i32 + self.delta[i] as i32) as u16
        };
        (glyph != 0).then_some(glyph)
    }
}

/// A format 12 subtable (`(startChar, endChar, startGlyph)` groups, sorted).
pub(crate) struct Format12 {
    groups: Vec<(u32, u32, u32)>,
}

impl Format12 {
    fn glyph(&self, code: u32) -> Option<u16> {
        let mut lo = 0;
        let mut hi = self.groups.len();
        while lo < hi {
            let mid = (lo + hi) / 2;
            let (start, end, _) = self.groups[mid];
            if end < code {
                lo = mid + 1;
            } else if start > code {
                hi = mid;
            } else {
                return Some((self.groups[mid].2 + (code - start)) as u16);
            }
        }
        None
    }
}

/// Parse the `cmap` table at `offset`, returning the best subtable.
pub(crate) fn parse(data: &[u8], offset: usize) -> ParseResult<Option<Cmap>> {
    let mut reader = Reader::new(data);
    reader.seek(offset);
    let _version = reader.u16()?;
    let count = reader.u16()?;

    let mut best: Option<(u32, Cmap)> = None;
    for _ in 0..count {
        let platform = reader.u16()?;
        let encoding = reader.u16()?;
        let sub_offset = reader.u32()? as usize;
        let sub_at = offset + sub_offset;
        let Some(format) = be_u16(data, sub_at) else {
            continue;
        };

        // Prefer full-Unicode, then format 12 over format 4.
        let platform_score: u32 = match (platform, encoding) {
            (3, 10) => 4,
            (0, _) => 3,
            (3, 1) => 2,
            (3, 0) => 1,
            _ => 0,
        };
        if platform_score == 0 {
            continue;
        }
        let score = platform_score * 2 + u32::from(format == 12);

        let parsed = match format {
            4 => parse_format4(data, sub_at).ok().map(Cmap::Format4),
            12 => parse_format12(data, sub_at).ok().map(Cmap::Format12),
            _ => None,
        };
        if let Some(cmap) = parsed
            && best
                .as_ref()
                .is_none_or(|(best_score, _)| score > *best_score)
        {
            best = Some((score, cmap));
        }
    }

    Ok(best.map(|(_, cmap)| cmap))
}

fn parse_format4(data: &[u8], at: usize) -> ParseResult<Format4> {
    let mut reader = Reader::new(data);
    reader.seek(at);
    let _format = reader.u16()?;
    let length = reader.u16()? as usize;
    let _language = reader.u16()?;
    let seg_count = (reader.u16()? / 2) as usize;
    reader.skip(6)?; // searchRange, entrySelector, rangeShift

    let mut end = Vec::with_capacity(seg_count);
    for _ in 0..seg_count {
        end.push(reader.u16()?);
    }
    reader.skip(2)?; // reservedPad

    let mut start = Vec::with_capacity(seg_count);
    for _ in 0..seg_count {
        start.push(reader.u16()?);
    }
    let mut delta = Vec::with_capacity(seg_count);
    for _ in 0..seg_count {
        delta.push(reader.i16()?);
    }
    let mut range_offset = Vec::with_capacity(seg_count);
    for _ in 0..seg_count {
        range_offset.push(reader.u16()?);
    }

    let subtable = data
        .get(at..at + length)
        .ok_or_else(|| reader.error("format 4 subtable runs past the font"))?
        .to_vec();

    Ok(Format4 {
        end,
        start,
        delta,
        range_offset,
        // 14-byte header + endCode + reservedPad + startCode + idDelta.
        range_offset_base: 16 + 6 * seg_count,
        subtable,
    })
}

fn parse_format12(data: &[u8], at: usize) -> ParseResult<Format12> {
    let mut reader = Reader::new(data);
    reader.seek(at);
    let _format = reader.u16()?;
    reader.skip(2)?; // reserved
    let _length = reader.u32()?;
    let _language = reader.u32()?;
    let count = reader.u32()? as usize;

    let mut groups = Vec::with_capacity(count);
    for _ in 0..count {
        let start = reader.u32()?;
        let end = reader.u32()?;
        let glyph = reader.u32()?;
        groups.push((start, end, glyph));
    }
    Ok(Format12 { groups })
}
