//! # noonoo-net
//!
//! Document loading, from whatever scheme the host is allowed to reach. This is
//! Phase E of the dependency-exit plan (`docs/DEPENDENCY-EXIT.md`).
//!
//! What Noonoo **owns**: the loading seam and the `file:` and `data:` schemes —
//! reading bytes, and decoding base64 / percent-encoding, are dependency-free.
//!
//! What stays **borrowed** (behind the `http` feature, not wired): `http(s)`,
//! because TLS is the genuinely hard part and belongs to a borrowed crate rather
//! than a hand-rolled one. The seam is what this crate delivers.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fmt;

/// Why a load failed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LoadError {
    /// The scheme is recognized but not enabled or supported.
    UnsupportedScheme(String),
    /// The resource could not be read.
    Io(String),
    /// The resource was fetched but could not be decoded.
    BadData(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedScheme(scheme) => write!(f, "unsupported scheme: {scheme}"),
            Self::Io(message) => write!(f, "io error: {message}"),
            Self::BadData(message) => write!(f, "bad data: {message}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Load a resource into bytes.
///
/// Accepts `file:` URLs, `data:` URLs, `http(s):` URLs (feature-gated), and a
/// bare filesystem path.
pub fn load(url: &str) -> Result<Vec<u8>, LoadError> {
    if let Some(rest) = url.strip_prefix("file:") {
        load_file(rest)
    } else if let Some(rest) = url.strip_prefix("data:") {
        load_data(rest)
    } else if url.starts_with("http:") || url.starts_with("https:") {
        Err(LoadError::UnsupportedScheme(
            "http(s) loading needs the `http` feature".to_owned(),
        ))
    } else {
        load_file(url)
    }
}

fn load_file(path: &str) -> Result<Vec<u8>, LoadError> {
    // file://host/path → /path (drop the authority).
    let path = path
        .strip_prefix("//")
        .map(|rest| match rest.find('/') {
            Some(index) => &rest[index..],
            None => rest,
        })
        .unwrap_or(path);
    std::fs::read(path).map_err(|error| LoadError::Io(format!("{path}: {error}")))
}

fn load_data(rest: &str) -> Result<Vec<u8>, LoadError> {
    // [<mediatype>][;base64],<data>
    let (meta, data) = rest
        .split_once(',')
        .ok_or_else(|| LoadError::BadData("data: URL is missing `,`".to_owned()))?;
    if meta
        .split(';')
        .any(|part| part.eq_ignore_ascii_case("base64"))
    {
        base64_decode(data).map_err(LoadError::BadData)
    } else {
        Ok(percent_decode(data).into_bytes())
    }
}

fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    fn value(byte: u8) -> Option<u32> {
        match byte {
            b'A'..=b'Z' => Some((byte - b'A') as u32),
            b'a'..=b'z' => Some((byte - b'a' + 26) as u32),
            b'0'..=b'9' => Some((byte - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let mut out = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for &byte in input.as_bytes() {
        if byte == b'=' {
            break;
        }
        if byte.is_ascii_whitespace() {
            continue;
        }
        let value = value(byte).ok_or_else(|| format!("invalid base64 byte `{}`", byte as char))?;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Ok(out)
}

fn percent_decode(input: &str) -> String {
    fn hex(byte: u8) -> Option<u8> {
        (byte as char).to_digit(16).map(|d| d as u8)
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(high * 16 + low);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The version of this crate, taken from the workspace.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_a_file_url() {
        let path = std::env::temp_dir().join("noonoo-net-test.txt");
        std::fs::write(&path, b"hello net").unwrap();
        let url = format!("file://{}", path.display());
        assert_eq!(load(&url).unwrap(), b"hello net");
    }

    #[test]
    fn loads_base64_data() {
        assert_eq!(load("data:text/plain;base64,SGVsbG8=").unwrap(), b"Hello");
    }

    #[test]
    fn loads_percent_encoded_data() {
        assert_eq!(load("data:,a%20b").unwrap(), b"a b");
    }

    #[test]
    fn http_is_reported_as_unsupported() {
        assert!(matches!(
            load("https://example.com"),
            Err(LoadError::UnsupportedScheme(_))
        ));
    }
}
