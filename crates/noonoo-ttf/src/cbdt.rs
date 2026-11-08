//! Colour bitmap glyphs: `CBLC` (where each glyph's bitmap is) and `CBDT`
//! (the bitmaps), as colour emoji fonts like Noto Color Emoji store them.
//!
//! Only the layout those fonts use is read: index subtable format 1 (one
//! 32-bit offset per glyph) and image format 17 (small metrics, then PNG
//! bytes). Other formats give no bitmap, so the glyph falls back to its
//! outline, if it has one. Decoding the PNG is not this crate's job.

/// A glyph's colour bitmap: encoded PNG bytes and where they go.
///
/// Sizes and bearings are pixels at `ppem` pixels per em; scale them by
/// `font_size / ppem` to draw at another size.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ColorBitmap<'a> {
    /// The PNG bytes.
    pub data: &'a [u8],
    /// The strike's pixels per em.
    pub ppem: u16,
    /// The bitmap's width in pixels.
    pub width: u16,
    /// The bitmap's height in pixels.
    pub height: u16,
    /// From the pen to the bitmap's left edge, in pixels.
    pub bearing_x: i16,
    /// From the baseline up to the bitmap's top edge, in pixels.
    pub bearing_y: i16,
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// The bitmap for `glyph` from the largest strike that has one.
///
/// `cblc` and `cbdt` are the tables' offsets in `data`.
pub(crate) fn bitmap(data: &[u8], cblc: usize, cbdt: usize, glyph: u16) -> Option<ColorBitmap<'_>> {
    let sizes = u32_at(data, cblc + 4)? as usize;
    (0..sizes)
        .filter_map(|size| strike(data, cblc, cbdt, cblc + 8 + size * 48, glyph))
        .max_by_key(|bitmap| bitmap.ppem)
}

/// The bitmap for `glyph` in the strike whose `BitmapSize` record is at
/// `record`.
fn strike(
    data: &[u8],
    cblc: usize,
    cbdt: usize,
    record: usize,
    glyph: u16,
) -> Option<ColorBitmap<'_>> {
    let array = cblc + u32_at(data, record)? as usize;
    let subtables = u32_at(data, record + 8)? as usize;
    let ppem = u16::from(*data.get(record + 45)?);
    for entry in 0..subtables {
        let at = array + entry * 8;
        let first = u16_at(data, at)?;
        let last = u16_at(data, at + 2)?;
        if !(first..=last).contains(&glyph) {
            continue;
        }
        let subtable = array + u32_at(data, at + 4)? as usize;
        let index_format = u16_at(data, subtable)?;
        let image_format = u16_at(data, subtable + 2)?;
        let image_data = cbdt + u32_at(data, subtable + 4)? as usize;
        if index_format != 1 || image_format != 17 {
            return None;
        }
        let slot = subtable + 8 + usize::from(glyph - first) * 4;
        let offset = u32_at(data, slot)? as usize;
        let next = u32_at(data, slot + 4)? as usize;
        if next <= offset {
            // An empty slot: the glyph has no bitmap in this strike.
            return None;
        }
        return small_metrics_png(data, image_data + offset, ppem);
    }
    None
}

/// An image-format-17 record: small glyph metrics, a length, PNG bytes.
fn small_metrics_png(data: &[u8], at: usize, ppem: u16) -> Option<ColorBitmap<'_>> {
    let metrics = data.get(at..at + 5)?;
    let length = u32_at(data, at + 5)? as usize;
    let png = data.get(at + 9..at + 9 + length)?;
    Some(ColorBitmap {
        data: png,
        ppem,
        height: u16::from(metrics[0]),
        width: u16::from(metrics[1]),
        bearing_x: i16::from(metrics[2] as i8),
        bearing_y: i16::from(metrics[3] as i8),
    })
}
