//! A hand-built minimal font: three glyphs, a format-4 `cmap` for `A`/`B`.
//!
//! Building the bytes here (rather than embedding a real font) keeps the test
//! deterministic and license-free; the parser only reads table offsets, so the
//! checksums are left zero.

use super::*;
use noonoo_text::Shaper;

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn push_i16(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn head() -> Vec<u8> {
    let mut t = Vec::new();
    push_u32(&mut t, 0x0001_0000); // version
    push_u32(&mut t, 0x0001_0000); // fontRevision
    push_u32(&mut t, 0); // checkSumAdjustment
    push_u32(&mut t, 0x5F0F_3CF5); // magicNumber
    push_u16(&mut t, 0); // flags
    push_u16(&mut t, 1000); // unitsPerEm
    t.extend_from_slice(&[0u8; 8]); // created
    t.extend_from_slice(&[0u8; 8]); // modified
    push_i16(&mut t, 0); // xMin
    push_i16(&mut t, 0); // yMin
    push_i16(&mut t, 1000); // xMax
    push_i16(&mut t, 1000); // yMax
    push_u16(&mut t, 0); // macStyle
    push_u16(&mut t, 8); // lowestRecPPEM
    push_i16(&mut t, 2); // fontDirectionHint
    push_i16(&mut t, 0); // indexToLocFormat
    push_i16(&mut t, 0); // glyphDataFormat
    t
}

fn maxp() -> Vec<u8> {
    let mut t = Vec::new();
    push_u32(&mut t, 0x0000_5000); // version 0.5
    push_u16(&mut t, 3); // numGlyphs
    t
}

fn hhea() -> Vec<u8> {
    let mut t = Vec::new();
    push_u32(&mut t, 0x0001_0000); // version
    push_i16(&mut t, 800); // ascender
    push_i16(&mut t, -200); // descender
    push_i16(&mut t, 0); // lineGap
    push_u16(&mut t, 700); // advanceWidthMax
    push_i16(&mut t, 0); // minLeftSideBearing
    push_i16(&mut t, 0); // minRightSideBearing
    push_i16(&mut t, 0); // xMaxExtent
    push_i16(&mut t, 1); // caretSlopeRise
    push_i16(&mut t, 0); // caretSlopeRun
    push_i16(&mut t, 0); // caretOffset
    for _ in 0..4 {
        push_i16(&mut t, 0); // reserved
    }
    push_i16(&mut t, 0); // metricDataFormat
    push_u16(&mut t, 3); // numberOfHMetrics
    t
}

fn hmtx() -> Vec<u8> {
    let mut t = Vec::new();
    for advance in [500u16, 600, 700] {
        push_u16(&mut t, advance);
        push_i16(&mut t, 0); // leftSideBearing
    }
    t
}

fn cmap() -> Vec<u8> {
    let mut sub = Vec::new();
    push_u16(&mut sub, 4); // format
    push_u16(&mut sub, 32); // length
    push_u16(&mut sub, 0); // language
    push_u16(&mut sub, 4); // segCountX2
    push_u16(&mut sub, 4); // searchRange
    push_u16(&mut sub, 1); // entrySelector
    push_u16(&mut sub, 0); // rangeShift
    push_u16(&mut sub, 0x42); // endCode 'B'
    push_u16(&mut sub, 0xFFFF); // endCode sentinel
    push_u16(&mut sub, 0); // reservedPad
    push_u16(&mut sub, 0x41); // startCode 'A'
    push_u16(&mut sub, 0xFFFF); // startCode sentinel
    push_i16(&mut sub, -0x40); // idDelta: 'A' -> 1, 'B' -> 2
    push_i16(&mut sub, 1); // idDelta sentinel
    push_u16(&mut sub, 0); // idRangeOffset
    push_u16(&mut sub, 0); // idRangeOffset
    assert_eq!(sub.len(), 32);

    let mut t = Vec::new();
    push_u16(&mut t, 0); // version
    push_u16(&mut t, 1); // numTables
    push_u16(&mut t, 3); // platformID (Windows)
    push_u16(&mut t, 1); // encodingID (BMP)
    push_u32(&mut t, 12); // subtable offset
    t.extend_from_slice(&sub);
    t
}

fn build(tables: &[([u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut offset = 12 + 16 * tables.len();
    let mut directory = Vec::new();
    let mut body = Vec::new();
    for (tag, data) in tables {
        let padding = (4 - data.len() % 4) % 4;
        directory.push((*tag, offset, data.len()));
        body.extend_from_slice(data);
        body.extend(std::iter::repeat_n(0u8, padding));
        offset += data.len() + padding;
    }

    let mut out = Vec::new();
    push_u32(&mut out, 0x0001_0000); // sfnt version (TrueType)
    push_u16(&mut out, tables.len() as u16);
    push_u16(&mut out, 0); // searchRange
    push_u16(&mut out, 0); // entrySelector
    push_u16(&mut out, 0); // rangeShift
    for (tag, table_offset, length) in directory {
        out.extend_from_slice(&tag);
        push_u32(&mut out, 0); // checksum (unchecked)
        push_u32(&mut out, table_offset as u32);
        push_u32(&mut out, length as u32);
    }
    out.extend_from_slice(&body);
    out
}

fn minimal_font() -> Vec<u8> {
    build(&[
        (*b"head", head()),
        (*b"maxp", maxp()),
        (*b"hhea", hhea()),
        (*b"hmtx", hmtx()),
        (*b"cmap", cmap()),
    ])
}

#[test]
fn parses_metrics() {
    let font = Font::parse(&minimal_font()).unwrap();
    assert_eq!(font.units_per_em(), 1000);
    assert_eq!(font.glyph_count(), 3);
    assert_eq!(font.ascender(), 800);
    assert_eq!(font.descender(), -200);
    assert_eq!(font.line_gap(), 0);
}

#[test]
fn maps_characters_and_reads_advances() {
    let font = Font::parse(&minimal_font()).unwrap();
    assert_eq!(font.glyph_for_char('A'), Some(1));
    assert_eq!(font.glyph_for_char('B'), Some(2));
    assert_eq!(font.glyph_for_char('z'), None);
    assert_eq!(font.advance(0), 500);
    assert_eq!(font.advance(1), 600);
    assert_eq!(font.advance_of_char('A'), 600);
    // An unmapped character falls back to `.notdef`.
    assert_eq!(font.advance_of_char('z'), 500);
}

#[test]
fn rejects_a_non_font() {
    assert!(Font::parse(b"not a font at all").is_err());
}

#[test]
fn font_shaper_uses_real_metrics() {
    let font = Font::parse(&minimal_font()).unwrap();
    let shaper = FontShaper::new(&font);
    // "AB" = 600 + 700 units, at 20px over a 1000-unit em.
    assert_eq!(shaper.advance("AB", 20.0, false), 26.0);
    // Line height = (800 - (-200) + 0) units, scaled the same way.
    assert_eq!(shaper.line_height(20.0), 20.0);
}
