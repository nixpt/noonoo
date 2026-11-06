//! The fetch seam for [`crate`]: a request/response pair and the `Fetcher` trait
//! that produces responses. Kept separate so hosts can plug their own transport
//! (the shell's `http` backend, a test's static map, …) without touching the
//! attribute/swap logic.

use std::fmt;

/// Why a request failed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HxError(pub String);

impl fmt::Display for HxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for HxError {}

/// An HTTP method.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Method {
    /// `GET`.
    Get,
    /// `POST`.
    Post,
    /// `PUT`.
    Put,
    /// `DELETE`.
    Delete,
}

impl Method {
    /// The method name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
        }
    }
}

/// A request handed to a [`Fetcher`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Request {
    /// The method.
    pub method: Method,
    /// The URL.
    pub url: String,
    /// The request body, if any.
    pub body: Option<String>,
}

/// A response from a [`Fetcher`].
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Response {
    /// The status code.
    pub status: u16,
    /// The response body (HTML, for a swap).
    pub body: String,
}

/// Something that can fetch a [`Request`].
pub trait Fetcher {
    /// Perform the request.
    fn fetch(&self, request: &Request) -> Result<Response, HxError>;
}

/// A `Fetcher` over a fixed set of URLs — the test/offline one.
#[derive(Clone, Default, Debug)]
pub struct StaticFetcher {
    responses: Vec<(String, String)>,
}

impl StaticFetcher {
    /// An empty fetcher.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register `body` for `url` (builder style).
    pub fn with(mut self, url: impl Into<String>, body: impl Into<String>) -> Self {
        self.responses.push((url.into(), body.into()));
        self
    }
}

impl Fetcher for StaticFetcher {
    fn fetch(&self, request: &Request) -> Result<Response, HxError> {
        match self.responses.iter().find(|(url, _)| url == &request.url) {
            Some((_, body)) => Ok(Response {
                status: 200,
                body: body.clone(),
            }),
            None => Ok(Response {
                status: 404,
                body: "not found".to_owned(),
            }),
        }
    }
}

/// A `Fetcher` over `noonoo-net` — `file:` and `data:` URLs, `GET` only.
///
/// `http(s)` needs the borrowed TLS backend and is out of scope here; a host
/// with one implements [`Fetcher`] directly.
#[derive(Clone, Copy, Default, Debug)]
pub struct NetFetcher;

impl Fetcher for NetFetcher {
    fn fetch(&self, request: &Request) -> Result<Response, HxError> {
        if request.method != Method::Get {
            return Err(HxError(format!(
                "NetFetcher is GET-only (got {})",
                request.method.as_str()
            )));
        }
        match noonoo_net::load(&request.url) {
            Ok(bytes) => Ok(Response {
                status: 200,
                body: String::from_utf8_lossy(&bytes).into_owned(),
            }),
            Err(error) => Ok(Response {
                status: 404,
                body: error.to_string(),
            }),
        }
    }
}
