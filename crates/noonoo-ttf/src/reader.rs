//! A big-endian byte reader for font tables.

use noonoo_parse::{ParseError, ParseResult};

/// A cursor over a font's bytes.
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub(crate) fn seek(&mut self, pos: usize) {
        self.pos = pos;
    }

    fn take(&mut self, n: usize) -> ParseResult<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .ok_or_else(|| self.error("offset overflow"))?;
        let slice = self
            .data
            .get(self.pos..end)
            .ok_or_else(|| self.error("unexpected end of font"))?;
        self.pos = end;
        Ok(slice)
    }

    pub(crate) fn u16(&mut self) -> ParseResult<u16> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    pub(crate) fn i16(&mut self) -> ParseResult<i16> {
        Ok(self.u16()? as i16)
    }

    pub(crate) fn u32(&mut self) -> ParseResult<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn tag(&mut self) -> ParseResult<[u8; 4]> {
        let b = self.take(4)?;
        Ok([b[0], b[1], b[2], b[3]])
    }

    pub(crate) fn skip(&mut self, n: usize) -> ParseResult<()> {
        self.take(n).map(|_| ())
    }

    pub(crate) fn error(&self, message: impl Into<String>) -> ParseError {
        ParseError::new(self.pos, message)
    }
}

/// Read a big-endian `u16` at an absolute offset, without a cursor.
pub(crate) fn be_u16(data: &[u8], at: usize) -> Option<u16> {
    let b = data.get(at..at + 2)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
}
