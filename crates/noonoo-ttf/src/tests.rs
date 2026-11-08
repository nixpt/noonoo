//! A hand-built minimal font: three glyphs, a format-4 `cmap` for `A`/`B`.
//!
//! Building the bytes here (rather than embedding a real font) keeps the test
//! deterministic and license-free; the parser only reads table offsets, so the
//! checksums are left zero.

use super::*;
use noonoo_text::{FontRequest, Shaper};

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn push_i16(out: &mut Vec<u8>, value: i16) {
    out.extend_from_slice(&value.to_be_bytes());
}
fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn head(units_per_em: u16) -> Vec<u8> {
    let mut t = Vec::new();
    push_u32(&mut t, 0x0001_0000); // version
    push_u32(&mut t, 0x0001_0000); // fontRevision
    push_u32(&mut t, 0); // checkSumAdjustment
    push_u32(&mut t, 0x5F0F_3CF5); // magicNumber
    push_u16(&mut t, 0); // flags
    push_u16(&mut t, units_per_em); // unitsPerEm
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

fn glyf() -> Vec<u8> {
    // glyph 1: a triangle; glyph 2: a composite that references it.
    let mut triangle = Vec::new();
    push_i16(&mut triangle, 1); // numberOfContours
    push_i16(&mut triangle, 0); // xMin
    push_i16(&mut triangle, 0); // yMin
    push_i16(&mut triangle, 700); // xMax
    push_i16(&mut triangle, 700); // yMax
    push_u16(&mut triangle, 2); // endPtsOfContours[0]
    push_u16(&mut triangle, 0); // instructionLength
    triangle.extend_from_slice(&[0x01, 0x01, 0x01]); // on-curve, long deltas
    for dx in [0i16, 700, -700] {
        push_i16(&mut triangle, dx);
    }
    for dy in [0i16, 0, 700] {
        push_i16(&mut triangle, dy);
    }
    while triangle.len() % 2 != 0 {
        triangle.push(0);
    }

    let mut composite = Vec::new();
    push_i16(&mut composite, -1); // numberOfContours (composite)
    push_i16(&mut composite, 0);
    push_i16(&mut composite, 0);
    push_i16(&mut composite, 800);
    push_i16(&mut composite, 700);
    push_u16(&mut composite, 0x0003); // words | xy values
    push_u16(&mut composite, 1); // component glyph
    push_i16(&mut composite, 100); // dx
    push_i16(&mut composite, 0); // dy
    while composite.len() % 2 != 0 {
        composite.push(0);
    }

    let mut t = triangle;
    t.extend_from_slice(&composite);
    t
}

fn loca() -> Vec<u8> {
    // Short format: byte offsets divided by two. glyph 0 is empty.
    let triangle_len = 30u16;
    let composite_len = 18u16;
    let offsets = [0u16, 0, triangle_len, triangle_len + composite_len];
    let mut t = Vec::new();
    for offset in offsets {
        push_u16(&mut t, offset / 2);
    }
    t
}

fn kern() -> Vec<u8> {
    let mut t = Vec::new();
    push_u16(&mut t, 0); // version
    push_u16(&mut t, 1); // nTables
    // One format-0, horizontal subtable with a single pair.
    push_u16(&mut t, 0); // subtable version
    push_u16(&mut t, 20); // length
    push_u16(&mut t, 0x0001); // coverage: format 0, horizontal
    push_u16(&mut t, 1); // nPairs
    push_u16(&mut t, 6); // searchRange
    push_u16(&mut t, 0); // entrySelector
    push_u16(&mut t, 0); // rangeShift
    push_u16(&mut t, 1); // left: glyph 'A'
    push_u16(&mut t, 2); // right: glyph 'B'
    push_i16(&mut t, -100); // value
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
    minimal_font_with_em(1000)
}

/// The same fixture, at a chosen `unitsPerEm`, so two faces are distinguishable.
fn minimal_font_with_em(units_per_em: u16) -> Vec<u8> {
    build(&[
        (*b"head", head(units_per_em)),
        (*b"maxp", maxp()),
        (*b"hhea", hhea()),
        (*b"hmtx", hmtx()),
        (*b"cmap", cmap()),
        (*b"glyf", glyf()),
        (*b"loca", loca()),
        (*b"kern", kern()),
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
    // "AB" = 600 + 700 units, minus the 100-unit kern pair, at 20px over 1000.
    assert_eq!(shaper.advance("AB", FontRequest::any(), 20.0), 24.0);
    // Line height = (800 - (-200) + 0) units, scaled the same way.
    assert_eq!(shaper.line_height(FontRequest::any(), 20.0), 20.0);
}

#[test]
fn reads_a_simple_glyph_outline() {
    let font = Font::parse(&minimal_font()).unwrap();
    let outline = font.outline(1).expect("glyph 1 has an outline");
    assert_eq!(outline.contours.len(), 1);
    let points = &outline.contours[0].points;
    assert_eq!(points.len(), 3);
    assert!(points.iter().all(|point| point.on_curve));

    let polygon = &outline.flatten(4)[0];
    assert_eq!(polygon.len(), 3);
    assert_eq!(polygon[0], (0.0, 0.0));
    assert_eq!(polygon[1], (700.0, 0.0));
    assert_eq!(polygon[2], (0.0, 700.0));
}

#[test]
fn resolves_a_composite_glyph() {
    let font = Font::parse(&minimal_font()).unwrap();
    let outline = font.outline(2).expect("glyph 2 is a composite");
    assert_eq!(outline.contours.len(), 1);
    let points = &outline.contours[0].points;
    // The triangle, shifted by x = 100.
    assert_eq!(points[0].x, 100.0);
    assert_eq!(points[1].x, 800.0);
}

#[test]
fn an_empty_glyph_has_no_outline() {
    let font = Font::parse(&minimal_font()).unwrap();
    assert!(font.outline(0).expect("glyph 0 exists").is_empty());
}

#[test]
fn lays_out_text_as_polygons() {
    let font = Font::parse(&minimal_font()).unwrap();
    // At a 1000px size the scale is 1, so points are font units.
    let polygons = font.text_polygons("AB", 1000.0);
    assert_eq!(polygons.len(), 2);
    assert_eq!(polygons[0][0], (0.0, 0.0));
    // 'B' starts at 'A's advance (600) plus its kerning (-100) plus the
    // composite offset (100).
    assert_eq!(polygons[1][0], (600.0, 0.0));
}

#[test]
fn reads_legacy_kerning() {
    let font = Font::parse(&minimal_font()).unwrap();
    assert_eq!(font.kern(1, 2), -100);
    // Kerning is directional, and unlisted pairs are zero.
    assert_eq!(font.kern(2, 1), 0);
    assert_eq!(font.kern(0, 0), 0);
}

#[test]
fn selects_a_face_by_weight_and_style() {
    let regular = Font::parse(&minimal_font_with_em(1000)).unwrap();
    let bold = Font::parse(&minimal_font_with_em(2000)).unwrap();
    let italic = Font::parse(&minimal_font_with_em(3000)).unwrap();
    let mut set = FontSet::new();
    set.add("Demo", 400, false, &regular);
    set.add("Demo", 700, false, &bold);
    set.add("Demo", 400, true, &italic);
    assert_eq!(set.len(), 3);

    let em = |font: &Font| font.units_per_em();
    // Exact matches.
    assert_eq!(em(set.select("Demo", 400, false).unwrap()), 1000);
    assert_eq!(em(set.select("Demo", 700, false).unwrap()), 2000);
    // Style wins over weight.
    assert_eq!(em(set.select("Demo", 400, true).unwrap()), 3000);
    // Nearest weight.
    assert_eq!(em(set.select("Demo", 500, false).unwrap()), 1000);
    // Case-insensitive family; unknown is `None`; empty matches any.
    assert_eq!(em(set.select("demo", 700, false).unwrap()), 2000);
    assert!(set.select("Other", 400, false).is_none());
    assert!(set.select("", 400, false).is_some());
}

#[test]
fn font_set_shaper_picks_the_bold_face() {
    let regular = Font::parse(&minimal_font_with_em(1000)).unwrap();
    let bold = Font::parse(&minimal_font_with_em(2000)).unwrap();
    let mut set = FontSet::new();
    set.add("Demo", 400, false, &regular);
    set.add("Demo", 700, false, &bold);

    let shaper = FontSetShaper::new(&set);
    let demo = vec!["Demo".to_owned()];
    let regular = FontRequest {
        families: &demo,
        weight: 400,
        italic: false,
    };
    // 1200 units over a 1000-unit em at 20px.
    assert_eq!(shaper.advance("AB", regular, 20.0), 24.0);
    assert_eq!(shaper.line_height(regular, 20.0), 20.0);
    // The same units over a 2000-unit em (the bold face).
    let bold = FontRequest {
        families: &demo,
        weight: 700,
        italic: false,
    };
    assert_eq!(shaper.advance("AB", bold, 20.0), 12.0);

    // No matching family falls back to the deterministic heuristic.
    let missing = vec!["Nope".to_owned()];
    let missing = FontRequest {
        families: &missing,
        weight: 400,
        italic: false,
    };
    assert_eq!(shaper.advance("AB", missing, 20.0), 20.0);
    assert_eq!(shaper.line_height(missing, 20.0), 24.0);
}

#[test]
fn font_set_selects_families_in_order() {
    let regular = Font::parse(&minimal_font_with_em(1000)).unwrap();
    let wide = Font::parse(&minimal_font_with_em(3000)).unwrap();
    let mut set = FontSet::new();
    set.add("Demo", 400, false, &regular);
    set.add("Other", 400, false, &wide);

    // The second family matches when the first has no faces.
    let families = vec!["Missing".to_owned(), "Other".to_owned()];
    assert_eq!(
        set.select_best(&families, 400, false)
            .unwrap()
            .units_per_em(),
        3000
    );
    // An unresolvable list is `None`, not a guess.
    let missing = vec!["A".to_owned(), "B".to_owned()];
    assert!(set.select_best(&missing, 400, false).is_none());
}

#[test]
fn reports_outline_capability_and_bounds() {
    let font = Font::parse(&minimal_font()).unwrap();
    assert!(font.has_outlines());
    assert_eq!(
        font.outline(1).unwrap().bounds(),
        Some((0.0, 0.0, 700.0, 700.0))
    );
    // An empty glyph has no bounds.
    assert_eq!(font.outline(0).unwrap().bounds(), None);
}

#[test]
fn outline_glyphs_emit_glyph_contours() {
    use noonoo_paint::{Color, Command, CommandList, Glyphs};

    let font = Font::parse(&minimal_font()).unwrap();
    let mut list = CommandList::default();
    let mut set = FontSet::new();
    set.add("Demo", 400, false, &font);
    OutlineGlyphs::new(&set).draw(
        "A",
        (0.0, 0.0),
        1000.0,
        Color::BLACK,
        FontRequest::any(),
        &mut list,
    );

    let points = list
        .commands()
        .iter()
        .find_map(|command| match command {
            Command::FillContours { contours, .. } => Some(contours[0].clone()),
            _ => None,
        })
        .expect("a glyph's contours");
    // The triangle maps to screen space (y down) with the baseline at y = 0.
    assert_eq!(
        points,
        vec![
            noonoo_geometry::Point::new(0.0, 0.0),
            noonoo_geometry::Point::new(700.0, 0.0),
            noonoo_geometry::Point::new(0.0, -700.0),
        ]
    );
}

/// Glyph 1 as a ring: a 700-unit square (clockwise, y up) with a 300-unit
/// square hole (counter-clockwise), the way TrueType draws an `o`. Glyph 2
/// is the same composite as in `glyf()`.
fn ring_glyf_and_loca() -> (Vec<u8>, Vec<u8>) {
    let mut ring = Vec::new();
    push_i16(&mut ring, 2); // numberOfContours
    for bound in [0i16, 0, 700, 700] {
        push_i16(&mut ring, bound);
    }
    push_u16(&mut ring, 3); // endPtsOfContours: outer 0..=3
    push_u16(&mut ring, 7); // inner 4..=7
    push_u16(&mut ring, 0); // instructionLength
    ring.extend_from_slice(&[0x01; 8]); // on-curve, long deltas
    // Outer (0,0) (0,700) (700,700) (700,0); inner (200,200) (500,200)
    // (500,500) (200,500).
    for dx in [0i16, 0, 700, 0, -500, 300, 0, -300] {
        push_i16(&mut ring, dx);
    }
    for dy in [0i16, 700, 0, -700, 200, 0, 300, 0] {
        push_i16(&mut ring, dy);
    }
    assert_eq!(ring.len(), 56);

    let composite = glyf()[30..].to_vec();
    let composite_len = composite.len() as u16;
    let mut glyf = ring;
    glyf.extend_from_slice(&composite);

    let mut loca = Vec::new();
    for offset in [0u16, 0, 56, 56 + composite_len] {
        push_u16(&mut loca, offset / 2);
    }
    (glyf, loca)
}

fn ring_font() -> Vec<u8> {
    let (glyf, loca) = ring_glyf_and_loca();
    build(&[
        (*b"head", head(1000)),
        (*b"maxp", maxp()),
        (*b"hhea", hhea()),
        (*b"hmtx", hmtx()),
        (*b"cmap", cmap()),
        (*b"glyf", glyf),
        (*b"loca", loca),
    ])
}

/// A glyph with a hole is one shape: its contours fill together, so the
/// hole's opposite winding cuts it out. As separate polygons, the hole
/// would be filled solid.
#[test]
fn outline_glyphs_fill_each_glyph_as_one_shape() {
    use noonoo_paint::{Color, Command, CommandList, Glyphs};

    let font = Font::parse(&ring_font()).unwrap();
    assert_eq!(font.outline(1).unwrap().contours.len(), 2);
    let mut set = FontSet::new();
    set.add("Demo", 400, false, &font);
    let mut list = CommandList::default();
    OutlineGlyphs::new(&set).draw(
        "A",
        (0.0, 1000.0),
        1000.0,
        Color::BLACK,
        FontRequest::any(),
        &mut list,
    );

    let [Command::FillContours { contours, color }] = list.commands() else {
        panic!("expected one FillContours, got {:?}", list.commands());
    };
    assert_eq!(*color, Color::BLACK);
    assert_eq!(contours.len(), 2);
    assert_eq!(contours[0].len(), 4);
    // Screen space: y down, baseline at 1000.
    assert_eq!(contours[1][0], noonoo_geometry::Point::new(200.0, 800.0));
}

/// `letter-spacing` moves each glyph after the first by the spacing so far.
#[test]
fn outline_glyphs_add_letter_spacing_after_each_glyph() {
    use noonoo_paint::{Color, Command, CommandList, Glyphs};

    let font = Font::parse(&minimal_font()).unwrap();
    let mut set = FontSet::new();
    set.add("Demo", 400, false, &font);
    let glyphs = OutlineGlyphs::new(&set);
    let starts = |spacing: f32| {
        let mut list = CommandList::default();
        glyphs.draw_spaced(
            "AAA",
            (0.0, 0.0),
            1000.0,
            spacing,
            Color::BLACK,
            FontRequest::any(),
            &mut list,
        );
        list.commands()
            .iter()
            .filter_map(|command| match command {
                Command::FillContours { contours, .. } => Some(contours[0][0].x),
                _ => None,
            })
            .collect::<Vec<f32>>()
    };
    let plain = starts(0.0);
    let spaced = starts(50.0);
    assert_eq!(plain.len(), 3);
    for (i, (p, s)) in plain.iter().zip(&spaced).enumerate() {
        assert_eq!(s - p, 50.0 * i as f32);
    }
}
