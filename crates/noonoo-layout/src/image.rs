//! Images as their codec reads them: the bytes, the format and the natural
//! size, read from the header without decoding (decoding is the backend's
//! job; codecs are bought, not owned).
//!
//! [`Images`] is the sizing seam: the host puts each loaded `<img src>` in
//! the store under its `src` as written, layout sizes the element from it,
//! and paint draws it.

use std::collections::HashMap;
use std::sync::Arc;

/// How an [`EncodedImage`]'s bytes are encoded.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImageFormat {
    /// PNG (also how colour emoji fonts store their glyphs).
    Png,
    /// JPEG.
    Jpeg,
    /// GIF (the first frame).
    Gif,
    /// WebP.
    Webp,
}

impl ImageFormat {
    /// The format `data` is in, from its signature.
    pub fn sniff(data: &[u8]) -> Option<Self> {
        if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if data.starts_with(&[0xff, 0xd8, 0xff]) {
            Some(Self::Jpeg)
        } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
            Some(Self::Gif)
        } else if data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
            Some(Self::Webp)
        } else {
            None
        }
    }

    /// The name in the paint command stream's JSON.
    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Gif => "gif",
            Self::Webp => "webp",
        }
    }
}

/// An image as its codec reads it, not yet decoded.
#[derive(Clone, PartialEq, Debug)]
pub struct EncodedImage {
    /// The encoding.
    pub format: ImageFormat,
    /// The encoded bytes, shared without copying.
    pub data: Arc<[u8]>,
}

impl EncodedImage {
    /// A PNG image from its bytes.
    pub fn png(data: impl Into<Arc<[u8]>>) -> Self {
        Self {
            format: ImageFormat::Png,
            data: data.into(),
        }
    }

    /// An image in whichever supported format `data` is, if any.
    pub fn sniff(data: impl Into<Arc<[u8]>>) -> Option<Self> {
        let data = data.into();
        Some(Self {
            format: ImageFormat::sniff(&data)?,
            data,
        })
    }

    /// The natural size in pixels, read from the header; `None` if the
    /// header is cut short or malformed.
    pub fn natural_size(&self) -> Option<(u32, u32)> {
        let data = &self.data[..];
        let size = match self.format {
            ImageFormat::Png => png_size(data),
            ImageFormat::Gif => gif_size(data),
            ImageFormat::Jpeg => jpeg_size(data),
            ImageFormat::Webp => webp_size(data),
        }?;
        (size.0 > 0 && size.1 > 0).then_some(size)
    }
}

fn be16(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes(
        data.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn le16(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_le_bytes(
        data.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn le24(data: &[u8], at: usize) -> Option<u32> {
    let b = data.get(at..at + 3)?;
    Some(u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16)
}

/// The IHDR chunk, which must come first: width and height, big-endian.
fn png_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.get(12..16)? != b"IHDR" {
        return None;
    }
    let word = |at: usize| Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?));
    Some((word(16)?, word(20)?))
}

/// The logical screen size, little-endian.
fn gif_size(data: &[u8]) -> Option<(u32, u32)> {
    Some((le16(data, 6)?, le16(data, 8)?))
}

/// The first start-of-frame marker's size, walking the segments.
fn jpeg_size(data: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    loop {
        // Fill bytes may pad a marker.
        while *data.get(at)? == 0xff && *data.get(at + 1)? == 0xff {
            at += 1;
        }
        if *data.get(at)? != 0xff {
            return None;
        }
        let marker = *data.get(at + 1)?;
        match marker {
            // Standalone markers carry no length.
            0xd0..=0xd9 | 0x01 => at += 2,
            // SOF0-SOF15, except DHT (c4), JPG (c8) and DAC (cc).
            0xc0..=0xcf if !matches!(marker, 0xc4 | 0xc8 | 0xcc) => {
                return Some((be16(data, at + 7)?, be16(data, at + 5)?));
            }
            _ => at += 2 + be16(data, at + 2)? as usize,
        }
    }
}

/// The first chunk: lossy (`VP8 `), lossless (`VP8L`) or extended (`VP8X`).
fn webp_size(data: &[u8]) -> Option<(u32, u32)> {
    match data.get(12..16)? {
        b"VP8 " => {
            // After the frame tag and start code: 14-bit dimensions.
            if data.get(23..26)? != [0x9d, 0x01, 0x2a] {
                return None;
            }
            Some((le16(data, 26)? & 0x3fff, le16(data, 28)? & 0x3fff))
        }
        b"VP8L" => {
            if *data.get(20)? != 0x2f {
                return None;
            }
            let bits = u32::from_le_bytes(data.get(21..25)?.try_into().ok()?);
            Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1))
        }
        b"VP8X" => Some((le24(data, 24)? + 1, le24(data, 27)? + 1)),
        _ => None,
    }
}

/// A loaded image and its natural size.
#[derive(Clone, PartialEq, Debug)]
pub struct Image {
    /// The bytes, for paint.
    pub encoded: EncodedImage,
    /// The natural width in pixels.
    pub width: u32,
    /// The natural height in pixels.
    pub height: u32,
}

impl Image {
    /// An image from its bytes, if they're a supported format with a
    /// readable size.
    pub fn from_bytes(data: impl Into<Arc<[u8]>>) -> Option<Self> {
        let encoded = EncodedImage::sniff(data)?;
        let (width, height) = encoded.natural_size()?;
        Some(Self {
            encoded,
            width,
            height,
        })
    }
}

/// Loaded images by `<img src>`, as written in the document.
#[derive(Clone, Default, Debug)]
pub struct Images {
    by_src: HashMap<String, Image>,
}

impl Images {
    /// An empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Store `image` for elements whose `src` is `src`.
    pub fn insert(&mut self, src: impl Into<String>, image: Image) {
        self.by_src.insert(src.into(), image);
    }

    /// The image for `src`, if it's loaded.
    pub fn get(&self, src: &str) -> Option<&Image> {
        self.by_src.get(src)
    }

    /// Forget the image for `src`.
    pub fn remove(&mut self, src: &str) -> Option<Image> {
        self.by_src.remove(src)
    }

    /// How many images are stored.
    pub fn len(&self) -> usize {
        self.by_src.len()
    }

    /// Whether the store is empty.
    pub fn is_empty(&self) -> bool {
        self.by_src.is_empty()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A PNG signature and IHDR chunk for a `width` × `height` image.
    pub(crate) fn png(width: u32, height: u32) -> Vec<u8> {
        let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
        data.extend(13u32.to_be_bytes());
        data.extend(b"IHDR");
        data.extend(width.to_be_bytes());
        data.extend(height.to_be_bytes());
        data.extend([8, 6, 0, 0, 0]);
        data
    }

    #[test]
    fn png_and_gif_headers_give_their_size() {
        let image = Image::from_bytes(png(640, 480)).unwrap();
        assert_eq!(
            (image.encoded.format, image.width, image.height),
            (ImageFormat::Png, 640, 480)
        );
        let mut gif = b"GIF89a".to_vec();
        gif.extend([0x2c, 0x01, 0xc8, 0x00, 0, 0, 0]);
        assert_eq!(
            EncodedImage::sniff(gif).unwrap().natural_size(),
            Some((300, 200))
        );
    }

    #[test]
    fn jpeg_size_comes_from_the_first_frame_header() {
        let mut jpeg = vec![0xff, 0xd8];
        // An APP0 segment to skip, then fill bytes, then SOF2 (progressive).
        jpeg.extend([0xff, 0xe0, 0x00, 0x04, 0xaa, 0xbb]);
        // A standalone restart marker carries no length.
        jpeg.extend([0xff, 0xd0]);
        jpeg.extend([
            0xff, 0xff, 0xc2, 0x00, 0x11, 0x08, 0x01, 0x2c, 0x02, 0x80, 0x03,
        ]);
        // A DHT marker (c4) is not a frame header.
        let mut with_dht = vec![0xff, 0xd8, 0xff, 0xc4, 0x00, 0x02];
        with_dht.extend(&jpeg[2..]);
        for data in [jpeg, with_dht] {
            assert_eq!(
                EncodedImage::sniff(data).unwrap().natural_size(),
                Some((640, 300))
            );
        }
    }

    #[test]
    fn webp_size_from_each_chunk_kind() {
        let riff = |chunk: &[u8], body: &[u8]| {
            let mut data = b"RIFF\0\0\0\0WEBP".to_vec();
            data.extend(chunk);
            data.extend([0, 0, 0, 0]);
            data.extend(body);
            data
        };
        let lossy = riff(
            b"VP8 ",
            // The top two bits of each dimension are a scale, not size.
            &[0, 0, 0, 0x9d, 0x01, 0x2a, 0x80, 0xc2, 0xe0, 0x01],
        );
        // 640 × 480 in 14 bits each, minus one.
        let bits: u32 = 639 | (479 << 14);
        let mut lossless_body = vec![0x2f];
        lossless_body.extend(bits.to_le_bytes());
        let lossless = riff(b"VP8L", &lossless_body);
        let extended = riff(b"VP8X", &[0, 0, 0, 0, 0x7f, 0x02, 0x00, 0xdf, 0x01, 0x00]);
        for data in [lossy, lossless, extended] {
            let image = EncodedImage::sniff(data).unwrap();
            assert_eq!(image.format, ImageFormat::Webp);
            assert_eq!(image.natural_size(), Some((640, 480)));
        }
        // Extended sizes are 24 bits: 70 000 wide.
        let wide = riff(b"VP8X", &[0, 0, 0, 0, 0x6f, 0x11, 0x01, 0xdf, 0x01, 0x00]);
        assert_eq!(
            EncodedImage::sniff(wide).unwrap().natural_size(),
            Some((70_000, 480))
        );
    }

    #[test]
    fn unknown_truncated_or_empty_images_have_no_size() {
        assert_eq!(ImageFormat::sniff(b"<svg"), None);
        let mut cut = png(10, 10);
        cut.truncate(20);
        assert_eq!(Image::from_bytes(cut), None);
        assert_eq!(Image::from_bytes(png(0, 10)), None);
        assert_eq!(Image::from_bytes(png(10, 0)), None);
        let mut not_ihdr = png(10, 10);
        not_ihdr[12..16].copy_from_slice(b"IDAT");
        assert_eq!(Image::from_bytes(not_ihdr), None);
        assert_eq!(Image::from_bytes(vec![0xff, 0xd8, 0xff, 0x00]), None);
        assert_eq!(Image::from_bytes(b"GIF89a\x01".to_vec()), None);
    }

    #[test]
    fn the_store_finds_images_by_src() {
        let mut images = Images::new();
        assert!(images.is_empty());
        images.insert("a.png", Image::from_bytes(png(2, 3)).unwrap());
        assert_eq!(images.get("a.png").map(|image| image.height), Some(3));
        assert_eq!(images.len(), 1);
        assert!(images.get("b.png").is_none());
        assert!(images.remove("a.png").is_some() && images.is_empty());
    }
}
