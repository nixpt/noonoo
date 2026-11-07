//! The legacy `kern` table: pair kerning.
//!
//! This is the pre-OpenType way to nudge specific glyph pairs — small and
//! well-specified, so it is worth owning for the couple hundred words it saves.
//! Two formats exist; this reads the common one (**format 0**, a sorted list of
//! `(left, right, value)` pairs) and ignores the rest, as it ignores the modern
//! `GPOS` table entirely (shaping stays borrowed).
//!
//! Version 1 (`kern` in the Apple header layout) is not read.

use noonoo_parse::ParseResult;

use crate::reader::Reader;

/// A kerning pair, in font units: `(left glyph, right glyph, adjustment)`.
pub(crate) type Pair = (u16, u16, i16);

/// Parse the `kern` table at `offset` into a sorted, de-duplicated-free list of
/// horizontal format-0 pairs.
pub(crate) fn parse(data: &[u8], offset: usize) -> ParseResult<Vec<Pair>> {
    let mut reader = Reader::new(data);
    reader.seek(offset);
    // Version 0 is the Windows/TrueType layout; version 1 (Apple) differs.
    if reader.u16()? != 0 {
        return Ok(Vec::new());
    }
    let table_count = reader.u16()?;

    let mut pairs = Vec::new();
    for _ in 0..table_count {
        let subtable_start = reader.position();
        let _version = reader.u16()?;
        let length = reader.u16()? as usize;
        let coverage = reader.u16()?;

        let format = coverage >> 8;
        let horizontal = coverage & 0x0001 != 0;
        if format == 0 && horizontal {
            let count = reader.u16()?;
            reader.skip(6)?; // searchRange, entrySelector, rangeShift
            for _ in 0..count {
                let left = reader.u16()?;
                let right = reader.u16()?;
                let value = reader.i16()?;
                pairs.push((left, right, value));
            }
        }

        // Move to the next subtable by its declared length, guarding against a
        // zero (or lying) length that would loop.
        let next = subtable_start + length;
        if length == 0 || next <= subtable_start {
            break;
        }
        reader.seek(next);
    }

    pairs.sort_unstable();
    Ok(pairs)
}
